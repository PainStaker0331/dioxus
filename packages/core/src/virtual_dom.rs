//! # Virtual DOM Implementation for Rust
//!
//! This module provides the primary mechanics to create a hook-based, concurrent VDOM for Rust.

use crate::{
    any_props::VComponentProps,
    arena::ElementId,
    arena::ElementRef,
    diff::DirtyScope,
    factory::RenderReturn,
    innerlude::{Mutations, Scheduler, SchedulerMsg},
    mutations::Mutation,
    nodes::{Template, TemplateId},
    scheduler::{SuspenseBoundary, SuspenseId},
    scopes::{ScopeId, ScopeState},
    AttributeValue, Element, EventPriority, Scope, SuspenseContext, UiEvent,
};
use futures_util::{pin_mut, StreamExt};
use slab::Slab;
use std::{
    any::Any,
    cell::Cell,
    collections::{BTreeSet, HashMap},
    future::Future,
    rc::Rc,
};

/// A virtual node system that progresses user events and diffs UI trees.
///
/// ## Guide
///
/// Components are defined as simple functions that take [`Scope`] and return an [`Element`].
///
/// ```rust, ignore
/// #[derive(Props, PartialEq)]
/// struct AppProps {
///     title: String
/// }
///
/// fn App(cx: Scope<AppProps>) -> Element {
///     cx.render(rsx!(
///         div {"hello, {cx.props.title}"}
///     ))
/// }
/// ```
///
/// Components may be composed to make complex apps.
///
/// ```rust, ignore
/// fn App(cx: Scope<AppProps>) -> Element {
///     cx.render(rsx!(
///         NavBar { routes: ROUTES }
///         Title { "{cx.props.title}" }
///         Footer {}
///     ))
/// }
/// ```
///
/// To start an app, create a [`VirtualDom`] and call [`VirtualDom::rebuild`] to get the list of edits required to
/// draw the UI.
///
/// ```rust, ignore
/// let mut vdom = VirtualDom::new(App);
/// let edits = vdom.rebuild();
/// ```
///
/// To call listeners inside the VirtualDom, call [`VirtualDom::handle_event`] with the appropriate event data.
///
/// ```rust, ignore
/// vdom.handle_event(event);
/// ```
///
/// While no events are ready, call [`VirtualDom::wait_for_work`] to poll any futures inside the VirtualDom.
///
/// ```rust, ignore
/// vdom.wait_for_work().await;
/// ```
///
/// Once work is ready, call [`VirtualDom::render_with_deadline`] to compute the differences between the previous and
/// current UI trees. This will return a [`Mutations`] object that contains Edits, Effects, and NodeRefs that need to be
/// handled by the renderer.
///
/// ```rust, ignore
/// let mutations = vdom.work_with_deadline(tokio::time::sleep(Duration::from_millis(100)));
///
/// for edit in mutations.edits {
///     real_dom.apply(edit);
/// }
/// ```
///
/// To not wait for suspense while diffing the VirtualDom, call [`VirtualDom::render_immediate`] or pass an immediately
/// ready future to [`VirtualDom::render_with_deadline`].
///
///
/// ## Building an event loop around Dioxus:
///
/// Putting everything together, you can build an event loop around Dioxus by using the methods outlined above.
/// ```rust
/// fn app(cx: Scope) -> Element {
///     cx.render(rsx!{
///         div { "Hello World" }
///     })
/// }
///
/// let dom = VirtualDom::new(app);
///
/// real_dom.apply(dom.rebuild());
///
/// loop {
///     select! {
///         _ = dom.wait_for_work() => {}
///         evt = real_dom.wait_for_event() => dom.handle_event(evt),
///     }
///
///     real_dom.apply(dom.render_immediate());
/// }
/// ```
///
/// ## Waiting for suspense
///
/// Because Dioxus supports suspense, you can use it for server-side rendering, static site generation, and other usecases
/// where waiting on portions of the UI to finish rendering is important. To wait for suspense, use the
/// [`VirtualDom::render_with_deadline`] method:
///
/// ```rust
/// let dom = VirtualDom::new(app);
///
/// let deadline = tokio::time::sleep(Duration::from_millis(100));
/// let edits = dom.render_with_deadline(deadline).await;
/// ```
///
/// ## Use with streaming
///
/// If not all rendering is done by the deadline, it might be worthwhile to stream the rest later. To do this, we
/// suggest rendering with a deadline, and then looping between [`VirtualDom::wait_for_work`] and render_immediate until
/// no suspended work is left.
///
/// ```
/// let dom = VirtualDom::new(app);
///
/// let deadline = tokio::time::sleep(Duration::from_millis(20));
/// let edits = dom.render_with_deadline(deadline).await;
///
/// real_dom.apply(edits);
///
/// while dom.has_suspended_work() {
///    dom.wait_for_work().await;
///    real_dom.apply(dom.render_immediate());
/// }
/// ```
pub struct VirtualDom {
    pub(crate) templates: HashMap<TemplateId, Template<'static>>,
    pub(crate) scopes: Slab<ScopeState>,
    pub(crate) dirty_scopes: BTreeSet<DirtyScope>,
    pub(crate) scheduler: Rc<Scheduler>,

    // Every element is actually a dual reference - one to the template and the other to the dynamic node in that template
    pub(crate) elements: Slab<ElementRef>,

    // While diffing we need some sort of way of breaking off a stream of suspended mutations.
    pub(crate) scope_stack: Vec<ScopeId>,
    pub(crate) collected_leaves: Vec<SuspenseId>,

    // Whenever a suspense tree is finished, we push its boundary onto this stack.
    // When "render_with_deadline" is called, we pop the stack and return the mutations
    pub(crate) finished_fibers: Vec<ScopeId>,

    pub(crate) rx: futures_channel::mpsc::UnboundedReceiver<SchedulerMsg>,
}

impl VirtualDom {
    /// Create a new VirtualDom with a component that does not have special props.
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
    /// ```rust, ignore
    /// fn Example(cx: Scope) -> Element  {
    ///     cx.render(rsx!( div { "hello world" } ))
    /// }
    ///
    /// let dom = VirtualDom::new(Example);
    /// ```
    ///
    /// Note: the VirtualDom is not progressed, you must either "run_with_deadline" or use "rebuild" to progress it.
    pub fn new(app: fn(Scope) -> Element) -> Self {
        Self::new_with_props(app, ())
    }

    /// Create a new VirtualDom with the given properties for the root component.
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
    /// ```rust, ignore
    /// #[derive(PartialEq, Props)]
    /// struct SomeProps {
    ///     name: &'static str
    /// }
    ///
    /// fn Example(cx: Scope<SomeProps>) -> Element  {
    ///     cx.render(rsx!{ div{ "hello {cx.props.name}" } })
    /// }
    ///
    /// let dom = VirtualDom::new(Example);
    /// ```
    ///
    /// Note: the VirtualDom is not progressed on creation. You must either "run_with_deadline" or use "rebuild" to progress it.
    ///
    /// ```rust, ignore
    /// let mut dom = VirtualDom::new_with_props(Example, SomeProps { name: "jane" });
    /// let mutations = dom.rebuild();
    /// ```
    pub fn new_with_props<P>(root: fn(Scope<P>) -> Element, root_props: P) -> Self
    where
        P: 'static,
    {
        let (tx, rx) = futures_channel::mpsc::unbounded();
        let mut dom = Self {
            rx,
            scheduler: Scheduler::new(tx),
            templates: Default::default(),
            scopes: Slab::default(),
            elements: Default::default(),
            scope_stack: Vec::new(),
            dirty_scopes: BTreeSet::new(),
            collected_leaves: Vec::new(),
            finished_fibers: Vec::new(),
        };

        let root = dom.new_scope(Box::into_raw(Box::new(VComponentProps::new(
            root,
            |_, _| unreachable!(),
            root_props,
        ))));

        // The root component is always a suspense boundary for any async children
        // This could be unexpected, so we might rethink this behavior later
        //
        // We *could* just panic if the suspense boundary is not found
        root.provide_context(SuspenseBoundary::new(ScopeId(0)));

        // the root element is always given element ID 0 since it's the container for the entire tree
        dom.elements.insert(ElementRef::null());

        dom
    }

    /// Get the state for any scope given its ID
    ///
    /// This is useful for inserting or removing contexts from a scope, or rendering out its root node
    pub fn get_scope(&self, id: ScopeId) -> Option<&ScopeState> {
        self.scopes.get(id.0)
    }

    /// Get the single scope at the top of the VirtualDom tree that will always be around
    ///
    /// This scope has a ScopeId of 0 and is the root of the tree
    pub fn base_scope(&self) -> &ScopeState {
        self.scopes.get(0).unwrap()
    }

    /// Build the virtualdom with a global context inserted into the base scope
    ///
    /// This is useful for what is essentially dependency injection when building the app
    pub fn with_root_context<T: Clone + 'static>(self, context: T) -> Self {
        self.base_scope().provide_context(context);
        self
    }

    /// Manually mark a scope as requiring a re-render
    ///
    /// Whenever the VirtualDom "works", it will re-render this scope
    pub fn mark_dirty_scope(&mut self, id: ScopeId) {
        let height = self.scopes[id.0].height;
        self.dirty_scopes.insert(DirtyScope { height, id });
    }

    /// Determine whether or not a scope is currently in a suspended state
    ///
    /// This does not mean the scope is waiting on its own futures, just that the tree that the scope exists in is
    /// currently suspended.
    pub fn is_scope_suspended(&self, id: ScopeId) -> bool {
        !self.scopes[id.0]
            .consume_context::<SuspenseContext>()
            .unwrap()
            .waiting_on
            .borrow()
            .is_empty()
    }

    /// Determine if the tree is at all suspended. Used by SSR and other outside mechanisms to determine if the tree is
    /// ready to be rendered.
    pub fn has_suspended_work(&self) -> bool {
        !self.scheduler.leaves.borrow().is_empty()
    }

    /// Call a listener inside the VirtualDom with data from outside the VirtualDom.
    ///
    /// This method will identify the appropriate element. The data must match up with the listener delcared. Note that
    /// this method does not give any indication as to the success of the listener call. If the listener is not found,
    /// nothing will happen.
    ///
    /// It is up to the listeners themselves to mark nodes as dirty.
    ///
    /// If you have multiple events, you can call this method multiple times before calling "render_with_deadline"
    pub fn handle_event(
        &mut self,
        name: &str,
        data: Rc<dyn Any>,
        element: ElementId,
        bubbles: bool,
        _priority: EventPriority,
    ) {
        /*
        ------------------------
        The algorithm works by walking through the list of dynamic attributes, checking their paths, and breaking when
        we find the target path.

        With the target path, we try and move up to the parent until there is no parent.
        Due to how bubbling works, we call the listeners before walking to the parent.

        If we wanted to do capturing, then we would accumulate all the listeners and call them in reverse order.
        ----------------------

        For a visual demonstration, here we present a tree on the left and whether or not a listener is collected on the
        right.

        |           <-- yes (is ascendant)
        | | |       <-- no  (is not direct ascendant)
        | |         <-- yes (is ascendant)
        | | | | |   <--- target element, break early, don't check other listeners
        | | |       <-- no, broke early
        |           <-- no, broke early
        */
        let mut parent_path = self.elements.get(element.0);
        let mut listeners = vec![];

        // We will clone this later. The data itself is wrapped in RC to be used in callbacks if required
        let uievent = UiEvent {
            bubbles: Rc::new(Cell::new(bubbles)),
            data,
        };

        // Loop through each dynamic attribute in this template before moving up to the template's parent.
        while let Some(el_ref) = parent_path {
            // safety: we maintain references of all vnodes in the element slab
            let template = unsafe { &*el_ref.template };
            let target_path = el_ref.path;

            for (idx, attr) in template.dynamic_attrs.iter().enumerate() {
                fn is_path_ascendant(small: &[u8], big: &[u8]) -> bool {
                    small.len() >= big.len() && small == &big[..small.len()]
                }

                let this_path = template.template.attr_paths[idx];

                // listeners are required to be prefixed with "on", but they come back to the virtualdom with that missing
                // we should fix this so that we look for "onclick" instead of "click"
                if &attr.name[2..] == name && is_path_ascendant(&target_path, &this_path) {
                    listeners.push(&attr.value);

                    // Break if the event doesn't bubble anyways
                    if !bubbles {
                        break;
                    }

                    // Break if this is the exact target element.
                    // This means we won't call two listeners with the same name on the same element. This should be
                    // documented, or be rejected from the rsx! macro outright
                    if this_path == target_path {
                        break;
                    }
                }
            }

            // Now that we've accumulated all the parent attributes for the target element, call them in reverse order
            // We check the bubble state between each call to see if the event has been stopped from bubbling
            for listener in listeners.drain(..).rev() {
                if let AttributeValue::Listener(listener) = listener {
                    listener.borrow_mut()(uievent.clone());
                    if !uievent.bubbles.get() {
                        return;
                    }
                }
            }

            parent_path = template.parent.and_then(|id| self.elements.get(id.0));
        }
    }

    /// Wait for the scheduler to have any work.
    ///
    /// This method polls the internal future queue, waiting for suspense nodes, tasks, or other work. This completes when
    /// any work is ready. If multiple scopes are marked dirty from a task or a suspense tree is finished, this method
    /// will exit.
    ///
    /// This method is cancel-safe, so you're fine to discard the future in a select block.
    ///
    /// This lets us poll async tasks and suspended trees during idle periods without blocking the main thread.
    ///
    /// # Example
    ///
    /// ```rust, ignore
    /// let dom = VirtualDom::new(App);
    /// let sender = dom.get_scheduler_channel();
    /// ```
    pub async fn wait_for_work(&mut self) {
        let mut some_msg = None;

        loop {
            match some_msg.take() {
                // If a bunch of messages are ready in a sequence, try to pop them off synchronously
                Some(msg) => match msg {
                    SchedulerMsg::Immediate(id) => self.mark_dirty_scope(id),
                    SchedulerMsg::TaskNotified(task) => self.handle_task_wakeup(task),
                    SchedulerMsg::SuspenseNotified(id) => self.handle_suspense_wakeup(id),
                },

                // If they're not ready, then we should wait for them to be ready
                None => {
                    match self.rx.try_next() {
                        Ok(Some(val)) => some_msg = Some(val),
                        Ok(None) => return,
                        Err(_) => {
                            // If we have any dirty scopes, or finished fiber trees then we should exit
                            if !self.dirty_scopes.is_empty() || !self.finished_fibers.is_empty() {
                                return;
                            }

                            some_msg = self.rx.next().await
                        }
                    }
                }
            }
        }
    }

    /// Performs a *full* rebuild of the virtual dom, returning every edit required to generate the actual dom from scratch.
    ///
    /// The mutations item expects the RealDom's stack to be the root of the application.
    ///
    /// Tasks will not be polled with this method, nor will any events be processed from the event queue. Instead, the
    /// root component will be ran once and then diffed. All updates will flow out as mutations.
    ///
    /// All state stored in components will be completely wiped away.
    ///
    /// Any templates previously registered will remain.
    ///
    /// # Example
    /// ```rust, ignore
    /// static App: Component = |cx|  cx.render(rsx!{ "hello world" });
    ///
    /// let mut dom = VirtualDom::new();
    /// let edits = dom.rebuild();
    ///
    /// apply_edits(edits);
    /// ```
    pub fn rebuild<'a>(&'a mut self) -> Mutations<'a> {
        let mut mutations = Mutations::new(0);

        match unsafe { self.run_scope_extend(ScopeId(0)) } {
            // Rebuilding implies we append the created elements to the root
            RenderReturn::Sync(Some(node)) => {
                let m = self.create_scope(ScopeId(0), &mut mutations, node);
                mutations.push(Mutation::AppendChildren { m });
            }
            // If nothing was rendered, then insert a placeholder element instead
            RenderReturn::Sync(None) => {
                mutations.push(Mutation::CreatePlaceholder { id: ElementId(1) });
                mutations.push(Mutation::AppendChildren { m: 1 });
            }
            RenderReturn::Async(_) => unreachable!("Root scope cannot be an async component"),
        }

        mutations
    }

    /// Render whatever the VirtualDom has ready as fast as possible without requiring an executor to progress
    /// suspended subtrees.
    pub fn render_immediate(&mut self) -> Mutations {
        // Build a waker that won't wake up since our deadline is already expired when it's polled
        let waker = futures_util::task::noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);

        // Now run render with deadline but dont even try to poll any async tasks
        let fut = self.render_with_deadline(std::future::ready(()));
        pin_mut!(fut);

        // The root component is not allowed to be async
        match fut.poll(&mut cx) {
            std::task::Poll::Ready(mutations) => mutations,
            std::task::Poll::Pending => panic!("render_immediate should never return pending"),
        }
    }

    /// Render what you can given the timeline and then move on
    ///
    /// It's generally a good idea to put some sort of limit on the suspense process in case a future is having issues.
    ///
    /// If no suspense trees are present
    pub async fn render_with_deadline<'a>(
        &'a mut self,
        deadline: impl Future<Output = ()>,
    ) -> Mutations<'a> {
        pin_mut!(deadline);

        let mut mutations = Mutations::new(0);

        loop {
            // first, unload any complete suspense trees
            for finished_fiber in self.finished_fibers.drain(..) {
                let scope = &mut self.scopes[finished_fiber.0];
                let context = scope.has_context::<SuspenseContext>().unwrap();

                mutations.extend(context.mutations.borrow_mut().template_mutations.drain(..));
                mutations.extend(context.mutations.borrow_mut().drain(..));

                // TODO: count how many nodes are on the stack?
                mutations.push(Mutation::ReplaceWith {
                    id: context.placeholder.get().unwrap(),
                    m: 1,
                })
            }

            // Next, diff any dirty scopes
            // We choose not to poll the deadline since we complete pretty quickly anyways
            if let Some(dirty) = self.dirty_scopes.iter().next().cloned() {
                self.dirty_scopes.remove(&dirty);

                // if the scope is currently suspended, then we should skip it, ignoring any tasks calling for an update
                if !self.is_scope_suspended(dirty.id) {
                    self.run_scope(dirty.id);
                    self.diff_scope(&mut mutations, dirty.id);
                }
            }

            // Wait for suspense, or a deadline
            if self.dirty_scopes.is_empty() {
                // If there's no pending suspense, then we have no reason to wait
                if self.scheduler.leaves.borrow().is_empty() {
                    return mutations;
                }

                // Poll the suspense leaves in the meantime
                let work = self.wait_for_work();
                pin_mut!(work);

                // If the deadline is exceded (left) then we should return the mutations we have
                use futures_util::future::{select, Either};
                if let Either::Left((_, _)) = select(&mut deadline, work).await {
                    return mutations;
                }
            }
        }
    }
}

impl Drop for VirtualDom {
    fn drop(&mut self) {
        // self.drop_scope(ScopeId(0));
    }
}
