//! # VirtualDOM Implementation for Rust
//! This module provides the primary mechanics to create a hook-based, concurrent VDOM for Rust.
//!
//! In this file, multiple items are defined. This file is big, but should be documented well to
//! navigate the innerworkings of the Dom. We try to keep these main mechanics in this file to limit
//! the possible exposed API surface (keep fields private). This particular implementation of VDOM
//! is extremely efficient, but relies on some unsafety under the hood to do things like manage
//! micro-heaps for components. We are currently working on refactoring the safety out into safe(r)
//! abstractions, but current tests (MIRI and otherwise) show no issues with the current implementation.
//!
//! Included is:
//! - The [`VirtualDom`] itself
//! - The [`Scope`] object for mangning component lifecycle
//! - The [`ActiveFrame`] object for managing the Scope`s microheap
//! - The [`Context`] object for exposing VirtualDOM API to components
//! - The [`NodeFactory`] object for lazyily exposing the `Context` API to the nodebuilder API
//! - The [`Hook`] object for exposing state management in components.
//!
//! This module includes just the barebones for a complete VirtualDOM API.
//! Additional functionality is defined in the respective files.
use futures_util::{Future, StreamExt};
use fxhash::FxHashMap;

use crate::hooks::{SuspendedContext, SuspenseHook};
use crate::innerlude::*;

use std::any::Any;

use std::any::TypeId;
use std::cell::{Ref, RefCell, RefMut};
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet, VecDeque};
use std::pin::Pin;

/// An integrated virtual node system that progresses events and diffs UI trees.
/// Differences are converted into patches which a renderer can use to draw the UI.
///
///
///
///
///
///
///
pub struct VirtualDom {
    /// All mounted components are arena allocated to make additions, removals, and references easy to work with
    /// A generational arena is used to re-use slots of deleted scopes without having to resize the underlying arena.
    ///
    /// This is wrapped in an UnsafeCell because we will need to get mutable access to unique values in unique bump arenas
    /// and rusts's guartnees cannot prove that this is safe. We will need to maintain the safety guarantees manually.
    shared: SharedResources,

    /// The index of the root component
    /// Should always be the first (gen=0, id=0)
    base_scope: ScopeId,

    scheduler: Scheduler,

    // for managing the props that were used to create the dom
    #[doc(hidden)]
    _root_prop_type: std::any::TypeId,

    #[doc(hidden)]
    _root_props: std::pin::Pin<Box<dyn std::any::Any>>,
}

impl VirtualDom {
    /// Create a new instance of the Dioxus Virtual Dom with no properties for the root component.
    ///
    /// This means that the root component must either consumes its own context, or statics are used to generate the page.
    /// The root component can access things like routing in its context.
    ///
    /// As an end-user, you'll want to use the Renderer's "new" method instead of this method.
    /// Directly creating the VirtualDOM is only useful when implementing a new renderer.
    ///
    ///
    /// ```ignore
    /// // Directly from a closure
    ///
    /// let dom = VirtualDom::new(|cx| cx.render(rsx!{ div {"hello world"} }));
    ///
    /// // or pass in...
    ///
    /// let root = |cx| {
    ///     cx.render(rsx!{
    ///         div {"hello world"}
    ///     })
    /// }
    /// let dom = VirtualDom::new(root);
    ///
    /// // or directly from a fn
    ///
    /// fn Example(cx: Context<()>) -> DomTree  {
    ///     cx.render(rsx!{ div{"hello world"} })
    /// }
    ///
    /// let dom = VirtualDom::new(Example);
    /// ```
    pub fn new(root: FC<()>) -> Self {
        Self::new_with_props(root, ())
    }

    /// Start a new VirtualDom instance with a dependent cx.
    /// Later, the props can be updated by calling "update" with a new set of props, causing a set of re-renders.
    ///
    /// This is useful when a component tree can be driven by external state (IE SSR) but it would be too expensive
    /// to toss out the entire tree.
    ///
    /// ```ignore
    /// // Directly from a closure
    ///
    /// let dom = VirtualDom::new(|cx| cx.render(rsx!{ div {"hello world"} }));
    ///
    /// // or pass in...
    ///
    /// let root = |cx| {
    ///     cx.render(rsx!{
    ///         div {"hello world"}
    ///     })
    /// }
    /// let dom = VirtualDom::new(root);
    ///
    /// // or directly from a fn
    ///
    /// fn Example(cx: Context, props: &SomeProps) -> VNode  {
    ///     cx.render(rsx!{ div{"hello world"} })
    /// }
    ///
    /// let dom = VirtualDom::new(Example);
    /// ```
    pub fn new_with_props<P: Properties + 'static>(root: FC<P>, root_props: P) -> Self {
        let components = SharedResources::new();

        let root_props: Pin<Box<dyn Any>> = Box::pin(root_props);
        let props_ptr = root_props.as_ref().downcast_ref::<P>().unwrap() as *const P;

        let link = components.clone();

        let base_scope = components.insert_scope_with_key(move |myidx| {
            let caller = NodeFactory::create_component_caller(root, props_ptr as *const _);
            Scope::new(caller, myidx, None, 0, ScopeChildren(&[]), link)
        });

        Self {
            base_scope,
            _root_props: root_props,
            shared: components,
            scheduler: Scheduler::new(),
            _root_prop_type: TypeId::of::<P>(),
        }
    }

    pub fn launch_in_place(root: FC<()>) -> Self {
        let mut s = Self::new(root);
        s.rebuild_in_place().unwrap();
        s
    }

    /// Creates a new virtualdom and immediately rebuilds it in place, not caring about the RealDom to write into.
    ///
    pub fn launch_with_props_in_place<P: Properties + 'static>(root: FC<P>, root_props: P) -> Self {
        let mut s = Self::new_with_props(root, root_props);
        s.rebuild_in_place().unwrap();
        s
    }

    pub fn base_scope(&self) -> &Scope {
        unsafe { self.shared.get_scope(self.base_scope).unwrap() }
    }

    pub fn get_scope(&self, id: ScopeId) -> Option<&Scope> {
        unsafe { self.shared.get_scope(id) }
    }

    /// Rebuilds the VirtualDOM from scratch, but uses a "dummy" RealDom.
    ///
    /// Used in contexts where a real copy of the  structure doesn't matter, and the VirtualDOM is the source of truth.
    ///
    /// ## Why?
    ///
    /// This method uses the `DebugDom` under the hood - essentially making the VirtualDOM's diffing patches a "no-op".
    ///
    /// SSR takes advantage of this by using Dioxus itself as the source of truth, and rendering from the tree directly.
    pub fn rebuild_in_place(&mut self) -> Result<Vec<DomEdit>> {
        todo!();
        // let mut realdom = DebugDom::new();
        // let mut edits = Vec::new();
        // self.rebuild(&mut realdom, &mut edits)?;
        // Ok(edits)
    }

    /// Performs a *full* rebuild of the virtual dom, returning every edit required to generate the actual dom rom scratch
    ///
    /// The diff machine expects the RealDom's stack to be the root of the application
    ///
    /// Events like garabge collection, application of refs, etc are not handled by this method and can only be progressed
    /// through "run"
    ///
    pub fn rebuild<'s>(&'s mut self) -> Result<Vec<DomEdit<'s>>> {
        let mut edits = Vec::new();
        let mut diff_machine = DiffMachine::new(Mutations::new(), self.base_scope, &self.shared);

        let cur_component = diff_machine
            .get_scope_mut(&self.base_scope)
            .expect("The base scope should never be moved");

        // We run the component. If it succeeds, then we can diff it and add the changes to the dom.
        if cur_component.run_scope().is_ok() {
            let meta = diff_machine.create_vnode(cur_component.frames.fin_head());
            diff_machine.edit_append_children(meta.added_to_stack);
        } else {
            // todo: should this be a hard error?
            log::warn!(
                "Component failed to run succesfully during rebuild.
                This does not result in a failed rebuild, but indicates a logic failure within your app."
            );
        }

        Ok(edits)
    }

    /// Runs the virtualdom immediately, not waiting for any suspended nodes to complete.
    ///
    /// This method will not wait for any suspended tasks, completely skipping over
    pub fn run_immediate<'s>(&'s mut self) -> Result<Mutations<'s>> {
        //

        todo!()
    }

    /// Runs the virtualdom with no time limit.
    ///
    /// If there are pending tasks, they will be progressed before returning. This is useful when rendering an application
    /// that has suspended nodes or suspended tasks. Be warned - any async tasks running forever will prevent this method
    /// from completing. Consider using `run` and specifing a deadline.
    pub async fn run_unbounded<'s>(&'s mut self) -> Result<Mutations<'s>> {
        self.run_with_deadline(async {}).await
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
    /// static App: FC<()> = |cx| rsx!(in cx, div {"hello"} );
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
    pub async fn run_with_deadline<'s>(
        &'s mut self,
        mut deadline: impl Future<Output = ()>,
    ) -> Result<Mutations<'s>> {
        /*
        Strategy:
        1. Check if there are any UI events in the receiver.
        2. If there are, run the listener and then mark the dirty nodes
        3. If there are dirty nodes to be progressed, do so.
        4. Poll the task queue to see if we can create more dirty scopes.
        5. Resume any current in-flight work if there is some.
        6. While the deadline is not met, progress work, periodically checking the deadline.


        How to choose work:
        - When a scope is marked as dirty, it is given a priority.
        - If a dirty scope chains (borrowed) into children, mark those as dirty as well.
        - When the work loop starts, work on the highest priority scopes first.
        - Work by priority, choosing to pause in-flight work if higher-priority work is ready.



        4. If there are no fibers, then wait for the next event from the receiver. Abort if the deadline is reached.
        5. While processing a fiber, periodically check if we're out of time
        6. If our deadling is reached, then commit our edits to the realdom
        7. Whenever a fiber is finished, immediately commit it. (IE so deadlines can be infinite if unsupported)


        // 1. Check if there are any events in the receiver.
        // 2. If there are, process them and create a new fiber.
        // 3. If there are no events, then choose a fiber to work on.
        // 4. If there are no fibers, then wait for the next event from the receiver. Abort if the deadline is reached.
        // 5. While processing a fiber, periodically check if we're out of time
        // 6. If our deadling is reached, then commit our edits to the realdom
        // 7. Whenever a fiber is finished, immediately commit it. (IE so deadlines can be infinite if unsupported)

        We slice fibers based on time. Each batch of events between frames is its own fiber. This is the simplest way
        to conceptualize what *is* or *isn't* a fiber. IE if a bunch of events occur during a time slice, they all
        get batched together as a single operation of "dirty" scopes.

        This approach is designed around the "diff during rIC and commit during rAF"

        We need to make sure to not call multiple events while the diff machine is borrowing the same scope. Because props
        and listeners hold references to hook data, it is wrong to run a scope that is already being diffed.
        */

        let mut diff_machine = DiffMachine::new(Mutations::new(), self.base_scope, &self.shared);

        // 1. Drain the existing immediates.
        //
        // These are generated by async tasks that we never got a chance to finish.
        // All of these get scheduled with the lowest priority.
        while let Ok(Some(dirty_scope)) = self.shared.immediate_receiver.borrow_mut().try_next() {
            self.scheduler
                .add_dirty_scope(dirty_scope, EventPriority::Low);
        }

        // 2. Drain the event queue, calling whatever listeners need to be called
        //
        while let Ok(Some(trigger)) = self.shared.ui_event_receiver.borrow_mut().try_next() {
            match &trigger.event {
                VirtualEvent::AsyncEvent { .. } => {}

                // This suspense system works, but it's not the most elegant solution.
                // TODO: Replace this system
                VirtualEvent::SuspenseEvent { hook_idx, domnode } => {
                    // Safety: this handler is the only thing that can mutate shared items at this moment in tim
                    let scope = diff_machine.get_scope_mut(&trigger.originator).unwrap();

                    // safety: we are sure that there are no other references to the inner content of suspense hooks
                    let hook = unsafe { scope.hooks.get_mut::<SuspenseHook>(*hook_idx) }.unwrap();

                    let cx = Context { scope, props: &() };
                    let scx = SuspendedContext { inner: cx };

                    // generate the new node!
                    let nodes: Option<VNode> = (&hook.callback)(scx);

                    if let Some(nodes) = nodes {
                        // allocate inside the finished frame - not the WIP frame
                        let nodes = scope.frames.finished_frame().bump.alloc(nodes);

                        // push the old node's root onto the stack
                        let real_id = domnode.get().ok_or(Error::NotMounted)?;
                        diff_machine.edit_push_root(real_id);

                        // push these new nodes onto the diff machines stack
                        let meta = diff_machine.create_vnode(&*nodes);

                        // replace the placeholder with the new nodes we just pushed on the stack
                        diff_machine.edit_replace_with(1, meta.added_to_stack);
                    } else {
                        log::warn!(
                            "Suspense event came through, but there were no generated nodes >:(."
                        );
                    }
                }

                VirtualEvent::ClipboardEvent(_)
                | VirtualEvent::CompositionEvent(_)
                | VirtualEvent::KeyboardEvent(_)
                | VirtualEvent::FocusEvent(_)
                | VirtualEvent::FormEvent(_)
                | VirtualEvent::SelectionEvent(_)
                | VirtualEvent::TouchEvent(_)
                | VirtualEvent::UIEvent(_)
                | VirtualEvent::WheelEvent(_)
                | VirtualEvent::MediaEvent(_)
                | VirtualEvent::AnimationEvent(_)
                | VirtualEvent::TransitionEvent(_)
                | VirtualEvent::ToggleEvent(_)
                | VirtualEvent::MouseEvent(_)
                | VirtualEvent::PointerEvent(_) => {
                    if let Some(scope) = self.shared.get_scope_mut(trigger.originator) {
                        if let Some(element) = trigger.real_node_id {
                            scope.call_listener(trigger.event, element)?;

                            // Drain the immediates into the dirty scopes, setting the appropiate priorities
                            while let Ok(Some(dirty_scope)) =
                                self.shared.immediate_receiver.borrow_mut().try_next()
                            {
                                self.scheduler
                                    .add_dirty_scope(dirty_scope, trigger.priority)
                            }
                        }
                    }
                }
            }
        }

        // 3. Work through the fibers, and wait for any future work to be ready

        // Configure our deadline
        use futures_util::FutureExt;
        let mut deadline_future = deadline.boxed_local();
        let mut is_ready = || -> bool { (&mut deadline_future).now_or_never().is_some() };

        loop {
            if is_ready() {
                break;
            }

            self.scheduler
        }

        Ok(diff_machine.mutations)
    }

    pub fn get_event_sender(&self) -> futures_channel::mpsc::UnboundedSender<EventTrigger> {
        self.shared.ui_event_sender.clone()
    }

    fn get_scope_mut(&mut self, id: ScopeId) -> Option<&mut Scope> {
        unsafe { self.shared.get_scope_mut(id) }
    }
}

// TODO!
// These impls are actually wrong. The DOM needs to have a mutex implemented.
unsafe impl Sync for VirtualDom {}
unsafe impl Send for VirtualDom {}
