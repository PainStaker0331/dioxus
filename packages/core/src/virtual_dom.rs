//! # VirtualDOM Implementation for Rust
//!
//! This module provides the primary mechanics to create a hook-based, concurrent VDOM for Rust.
//!
//! In this file, multiple items are defined. This file is big, but should be documented well to
//! navigate the inner workings of the Dom. We try to keep these main mechanics in this file to limit
//! the possible exposed API surface (keep fields private). This particular implementation of VDOM
//! is extremely efficient, but relies on some unsafety under the hood to do things like manage
//! micro-heaps for components. We are currently working on refactoring the safety out into safe(r)
//! abstractions, but current tests (MIRI and otherwise) show no issues with the current implementation.
//!
//! Included is:
//! - The [`VirtualDom`] itself
//! - The [`Scope`] object for managing component lifecycle
//! - The [`ActiveFrame`] object for managing the Scope`s microheap
//! - The [`Context`] object for exposing VirtualDOM API to components
//! - The [`NodeFactory`] object for lazily exposing the `Context` API to the nodebuilder API
//!
//! This module includes just the barebones for a complete VirtualDOM API.
//! Additional functionality is defined in the respective files.

use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::innerlude::*;
use std::{any::Any, rc::Rc};

/// An integrated virtual node system that progresses events and diffs UI trees.
///
/// Differences are converted into patches which a renderer can use to draw the UI.
///
/// If you are building an App with Dioxus, you probably won't want to reach for this directly, instead opting to defer
/// to a particular crate's wrapper over the [`VirtualDom`] API.
///
/// Example
/// ```rust
/// static App: FC<()> = |(cx, props)|{
///     cx.render(rsx!{
///         div {
///             "Hello World"
///         }
///     })
/// }
///
/// async fn main() {
///     let mut dom = VirtualDom::new(App);
///     let mut inital_edits = dom.rebuild();
///     initialize_screen(inital_edits);
///
///     loop {
///         let next_frame = TimeoutFuture::new(Duration::from_millis(16));
///         let edits = dom.run_with_deadline(next_frame).await;
///         apply_edits(edits);
///         render_frame();
///     }
/// }
/// ```
pub struct VirtualDom {
    scheduler: Scheduler,

    base_scope: ScopeId,

    root_fc: Box<dyn Any>,

    root_props: Rc<dyn Any + Send>,

    // we need to keep the allocation around, but we don't necessarily use it
    _root_caller: RootCaller,
}

impl VirtualDom {
    /// Create a new VirtualDOM with a component that does not have special props.
    ///
    /// # Description
    ///
    /// Later, the props can be updated by calling "update" with a new set of props, causing a set of re-renders.
    ///
    /// This is useful when a component tree can be driven by external state (IE SSR) but it would be too expensive
    /// to toss out the entire tree.
    ///
    ///
    /// # Example
    /// ```
    /// fn Example(cx: Context<()>) -> DomTree  {
    ///     cx.render(rsx!( div { "hello world" } ))
    /// }
    ///
    /// let dom = VirtualDom::new(Example);
    /// ```
    ///
    /// Note: the VirtualDOM is not progressed, you must either "run_with_deadline" or use "rebuild" to progress it.
    pub fn new(root: FC<()>) -> Self {
        Self::new_with_props(root, ())
    }

    /// Create a new VirtualDOM with the given properties for the root component.
    ///
    /// # Description
    ///
    /// Later, the props can be updated by calling "update" with a new set of props, causing a set of re-renders.
    ///
    /// This is useful when a component tree can be driven by external state (IE SSR) but it would be too expensive
    /// to toss out the entire tree.
    ///
    ///
    /// # Example
    /// ```
    /// #[derive(PartialEq, Props)]
    /// struct SomeProps {
    ///     name: &'static str
    /// }
    ///
    /// fn Example(cx: Context<SomeProps>) -> DomTree  {
    ///     cx.render(rsx!{ div{ "hello {cx.name}" } })
    /// }
    ///
    /// let dom = VirtualDom::new(Example);
    /// ```
    ///
    /// Note: the VirtualDOM is not progressed on creation. You must either "run_with_deadline" or use "rebuild" to progress it.
    ///
    /// ```rust
    /// let mut dom = VirtualDom::new_with_props(Example, SomeProps { name: "jane" });
    /// let mutations = dom.rebuild();
    /// ```
    pub fn new_with_props<P: 'static + Send>(root: FC<P>, root_props: P) -> Self {
        let (sender, receiver) = futures_channel::mpsc::unbounded::<SchedulerMsg>();
        Self::new_with_props_and_scheduler(root, root_props, sender, receiver)
    }

    /// Launch the VirtualDom, but provide your own channel for receiving and sending messages into the scheduler.
    ///
    /// This is useful when the VirtualDom must be driven from outside a thread and it doesn't make sense to wait for the
    /// VirtualDom to be created just to retrieve its channel receiver.
    pub fn new_with_props_and_scheduler<P: 'static + Send>(
        root: FC<P>,
        root_props: P,
        sender: UnboundedSender<SchedulerMsg>,
        receiver: UnboundedReceiver<SchedulerMsg>,
    ) -> Self {
        let root_fc = Box::new(root);

        let root_props: Rc<dyn Any + Send> = Rc::new(root_props);

        let _p = root_props.clone();
        // Safety: this callback is only valid for the lifetime of the root props
        let root_caller: Rc<dyn Fn(&ScopeInner) -> Element> =
            Rc::new(move |scope: &ScopeInner| unsafe {
                let props = _p.downcast_ref::<P>().unwrap();
                std::mem::transmute(root((Context { scope }, props)))
            });

        let scheduler = Scheduler::new(sender, receiver);

        let base_scope = scheduler.pool.insert_scope_with_key(|myidx| {
            ScopeInner::new(
                root_caller.as_ref(),
                myidx,
                None,
                0,
                0,
                scheduler.pool.channel.clone(),
            )
        });

        Self {
            _root_caller: RootCaller(root_caller),
            root_fc,
            base_scope,
            scheduler,
            root_props,
        }
    }

    /// Get the [`Scope`] for the root component.
    ///
    /// This is useful for traversing the tree from the root for heuristics or alternsative renderers that use Dioxus
    /// directly.
    pub fn base_scope(&self) -> &ScopeInner {
        self.scheduler.pool.get_scope(self.base_scope).unwrap()
    }

    /// Get the [`Scope`] for a component given its [`ScopeId`]
    pub fn get_scope(&self, id: ScopeId) -> Option<&ScopeInner> {
        self.scheduler.pool.get_scope(id)
    }

    /// Update the root props of this VirtualDOM.
    ///
    /// This method returns None if the old props could not be removed. The entire VirtualDOM will be rebuilt immediately,
    /// so calling this method will block the main thread until computation is done.
    ///
    /// ## Example
    ///
    /// ```rust
    /// #[derive(Props, PartialEq)]
    /// struct AppProps {
    ///     route: &'static str
    /// }
    /// static App: FC<AppProps> = |(cx, props)|cx.render(rsx!{ "route is {cx.route}" });
    ///
    /// let mut dom = VirtualDom::new_with_props(App, AppProps { route: "start" });
    ///
    /// let mutations = dom.update_root_props(AppProps { route: "end" }).unwrap();
    /// ```
    pub fn update_root_props<P>(&mut self, root_props: P) -> Option<Mutations>
    where
        P: 'static + Send,
    {
        let root_scope = self.scheduler.pool.get_scope_mut(self.base_scope).unwrap();

        // Pre-emptively drop any downstream references of the old props
        root_scope.ensure_drop_safety(&self.scheduler.pool);

        let mut root_props: Rc<dyn Any + Send> = Rc::new(root_props);

        if let Some(props_ptr) = root_props.downcast_ref::<P>().map(|p| p as *const P) {
            // Swap the old props and new props
            std::mem::swap(&mut self.root_props, &mut root_props);

            let root = *self.root_fc.downcast_ref::<FC<P>>().unwrap();

            let root_caller: Box<dyn Fn(&ScopeInner) -> Element> =
                Box::new(move |scope: &ScopeInner| unsafe {
                    let props: &'_ P = &*(props_ptr as *const P);
                    std::mem::transmute(root((Context { scope }, props)))
                });

            root_scope.update_scope_dependencies(&root_caller);

            drop(root_props);

            Some(self.rebuild())
        } else {
            None
        }
    }

    /// Performs a *full* rebuild of the virtual dom, returning every edit required to generate the actual dom from scratch
    ///
    /// The diff machine expects the RealDom's stack to be the root of the application.
    ///
    /// Tasks will not be polled with this method, nor will any events be processed from the event queue. Instead, the
    /// root component will be ran once and then diffed. All updates will flow out as mutations.
    ///
    /// All state stored in components will be completely wiped away.
    ///
    /// # Example
    /// ```
    /// static App: FC<()> = |(cx, props)|cx.render(rsx!{ "hello world" });
    /// let mut dom = VirtualDom::new();
    /// let edits = dom.rebuild();
    ///
    /// apply_edits(edits);
    /// ```
    pub fn rebuild(&mut self) -> Mutations {
        self.scheduler.rebuild(self.base_scope)
    }

    /// Compute a manual diff of the VirtualDOM between states.
    ///
    /// This can be useful when state inside the DOM is remotely changed from the outside, but not propagated as an event.
    ///
    /// In this case, every component will be diffed, even if their props are memoized. This method is intended to be used
    /// to force an update of the DOM when the state of the app is changed outside of the app.
    ///
    ///
    /// # Example
    /// ```rust
    /// #[derive(PartialEq, Props)]
    /// struct AppProps {
    ///     value: Shared<&'static str>,
    /// }
    ///
    /// static App: FC<AppProps> = |(cx, props)|{
    ///     let val = cx.value.borrow();
    ///     cx.render(rsx! { div { "{val}" } })
    /// };
    ///
    /// let value = Rc::new(RefCell::new("Hello"));
    /// let mut dom = VirtualDom::new_with_props(
    ///     App,
    ///     AppProps {
    ///         value: value.clone(),
    ///     },
    /// );
    ///
    /// let _ = dom.rebuild();
    ///
    /// *value.borrow_mut() = "goodbye";
    ///
    /// let edits = dom.diff();
    /// ```
    pub fn diff(&mut self) -> Mutations {
        self.scheduler.hard_diff(self.base_scope)
    }

    /// Runs the virtualdom immediately, not waiting for any suspended nodes to complete.
    ///
    /// This method will not wait for any suspended nodes to complete. If there is no pending work, then this method will
    /// return "None"
    pub fn run_immediate(&mut self) -> Option<Vec<Mutations>> {
        if self.scheduler.has_any_work() {
            Some(self.scheduler.work_sync())
        } else {
            None
        }
    }

    /// Run the virtualdom with a deadline.
    ///
    /// This method will progress async tasks until the deadline is reached. If tasks are completed before the deadline,
    /// and no tasks are pending, this method will return immediately. If tasks are still pending, then this method will
    /// exhaust the deadline working on them.
    ///
    /// This method is useful when needing to schedule the virtualdom around other tasks on the main thread to prevent
    /// "jank". It will try to finish whatever work it has by the deadline to free up time for other work.
    ///
    /// Due to platform differences in how time is handled, this method accepts a future that resolves when the deadline
    /// is exceeded. However, the deadline won't be met precisely, so you might want to build some wiggle room into the
    /// deadline closure manually.
    ///
    /// The deadline is polled before starting to diff components. This strikes a balance between the overhead of checking
    /// the deadline and just completing the work. However, if an individual component takes more than 16ms to render, then
    /// the screen will "jank" up. In debug, this will trigger an alert.
    ///
    /// If there are no in-flight fibers when this method is called, it will await any possible tasks, aborting early if
    /// the provided deadline future resolves.
    ///
    /// For use in the web, it is expected that this method will be called to be executed during "idle times" and the
    /// mutations to be applied during the "paint times" IE "animation frames". With this strategy, it is possible to craft
    /// entirely jank-free applications that perform a ton of work.
    ///
    /// # Example
    ///
    /// ```no_run
    /// static App: FC<()> = |(cx, props)|rsx!(cx, div {"hello"} );
    /// let mut dom = VirtualDom::new(App);
    /// loop {
    ///     let deadline = TimeoutFuture::from_ms(16);
    ///     let mutations = dom.run_with_deadline(deadline).await;
    ///     apply_mutations(mutations);
    /// }
    /// ```
    ///
    /// ## Mutations
    ///
    /// This method returns "mutations" - IE the necessary changes to get the RealDOM to match the VirtualDOM. It also
    /// includes a list of NodeRefs that need to be applied and effects that need to be triggered after the RealDOM has
    /// applied the edits.
    ///
    /// Mutations are the only link between the RealDOM and the VirtualDOM.
    pub fn run_with_deadline(&mut self, deadline: impl FnMut() -> bool) -> Vec<Mutations<'_>> {
        self.scheduler.work_with_deadline(deadline)
    }

    pub fn get_event_sender(&self) -> futures_channel::mpsc::UnboundedSender<SchedulerMsg> {
        self.scheduler.pool.channel.sender.clone()
    }

    /// Waits for the scheduler to have work
    /// This lets us poll async tasks during idle periods without blocking the main thread.
    pub async fn wait_for_work(&mut self) {
        // todo: poll the events once even if there is work to do to prevent starvation
        if self.scheduler.has_any_work() {
            return;
        }

        use futures_util::StreamExt;

        // Wait for any new events if we have nothing to do

        let tasks_fut = self.scheduler.async_tasks.next();
        let scheduler_fut = self.scheduler.receiver.next();

        use futures_util::future::{select, Either};
        match select(tasks_fut, scheduler_fut).await {
            // poll the internal futures
            Either::Left((_id, _)) => {
                //
            }

            // wait for an external event
            Either::Right((msg, _)) => match msg.unwrap() {
                SchedulerMsg::Task(t) => {
                    self.scheduler.handle_task(t);
                }
                SchedulerMsg::Immediate(im) => {
                    self.scheduler.dirty_scopes.insert(im);
                }
                SchedulerMsg::UiEvent(evt) => {
                    self.scheduler.ui_events.push_back(evt);
                }
            },
        }
    }
}

// we never actually use the contents of this root caller
struct RootCaller(Rc<dyn for<'b> Fn(&'b ScopeInner) -> Element<'b> + 'static>);
