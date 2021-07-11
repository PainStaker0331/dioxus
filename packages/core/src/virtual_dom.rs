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

use crate::tasks::TaskQueue;
use crate::{arena::SharedArena, innerlude::*};
use appendlist::AppendList;
use slotmap::DefaultKey;
use slotmap::SlotMap;
use std::any::Any;
use std::cell::RefCell;
use std::pin::Pin;
use std::{any::TypeId, fmt::Debug, rc::Rc};

pub type ScopeIdx = DefaultKey;

/// An integrated virtual node system that progresses events and diffs UI trees.
/// Differences are converted into patches which a renderer can use to draw the UI.
pub struct VirtualDom {
    /// All mounted components are arena allocated to make additions, removals, and references easy to work with
    /// A generational arena is used to re-use slots of deleted scopes without having to resize the underlying arena.
    ///
    /// This is wrapped in an UnsafeCell because we will need to get mutable access to unique values in unique bump arenas
    /// and rusts's guartnees cannot prove that this is safe. We will need to maintain the safety guarantees manually.
    pub components: SharedArena,

    /// The index of the root component
    /// Should always be the first (gen=0, id=0)
    pub base_scope: ScopeIdx,

    /// All components dump their updates into a queue to be processed
    pub(crate) event_queue: EventQueue,

    pub tasks: TaskQueue,

    root_props: std::pin::Pin<Box<dyn std::any::Any>>,

    /// Type of the original cx. This is stored as TypeId so VirtualDom does not need to be generic.
    ///
    /// Whenver props need to be updated, an Error will be thrown if the new props do not
    /// match the props used to create the VirtualDom.
    #[doc(hidden)]
    _root_prop_type: std::any::TypeId,
}

// ======================================
// Public Methods for the VirtualDom
// ======================================
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
    /// fn Example(cx: Context<()>) -> VNode  {
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
        let components = SharedArena::new(SlotMap::new());

        let root_props: Pin<Box<dyn Any>> = Box::pin(root_props);
        let props_ptr = root_props.as_ref().downcast_ref::<P>().unwrap() as *const P;

        // Build a funnel for hooks to send their updates into. The `use_hook` method will call into the update funnel.
        let event_queue = EventQueue::default();
        let _event_queue = event_queue.clone();

        let link = components.clone();

        let tasks = TaskQueue::new();
        let submitter = tasks.new_submitter();

        let base_scope = components
            .with(|arena| {
                arena.insert_with_key(move |myidx| {
                    let event_channel = _event_queue.new_channel(0, myidx);
                    let caller = crate::nodes::create_component_caller(root, props_ptr as *const _);
                    Scope::new(caller, myidx, None, 0, event_channel, link, &[], submitter)
                })
            })
            .unwrap();

        log::debug!("base scope is {:#?}", base_scope);

        Self {
            base_scope,
            event_queue,
            components,
            root_props,
            tasks,
            _root_prop_type: TypeId::of::<P>(),
        }
    }
}

// ======================================
// Private Methods for the VirtualDom
// ======================================
impl VirtualDom {
    /// Rebuilds the VirtualDOM from scratch, but uses a "dummy" RealDom.
    ///
    /// Used in contexts where a real copy of the  structure doesn't matter, and the VirtualDOM is the source of truth.
    ///
    /// ## Why?
    ///
    /// This method uses the `DebugDom` under the hood - essentially making the VirtualDOM's diffing patches a "no-op".
    ///
    /// SSR takes advantage of this by using Dioxus itself as the source of truth, and rendering from the tree directly.
    pub fn rebuild_in_place(&mut self) -> Result<()> {
        let mut realdom = DebugDom::new();
        self.rebuild(&mut realdom)
    }

    /// Performs a *full* rebuild of the virtual dom, returning every edit required to generate the actual dom rom scratch
    /// Currently this doesn't do what we want it to do
    pub fn rebuild<'s, Dom: RealDom<'s>>(&'s mut self, realdom: &mut Dom) -> Result<()> {
        let mut diff_machine = DiffMachine::new(
            realdom,
            &self.components,
            self.base_scope,
            self.event_queue.clone(),
            &self.tasks,
        );

        // Schedule an update and then immediately call it on the root component
        // This is akin to a hook being called from a listener and requring a re-render
        // Instead, this is done on top-level component
        let base = self.components.try_get(self.base_scope)?;

        let update = &base.event_channel;
        update();

        self.progress_completely(&mut diff_machine)?;

        Ok(())
    }
    /// This method is the most sophisticated way of updating the virtual dom after an external event has been triggered.
    ///  
    /// Given a synthetic event, the component that triggered the event, and the index of the callback, this runs the virtual
    /// dom to completion, tagging components that need updates, compressing events together, and finally emitting a single
    /// change list.
    ///
    /// If implementing an external renderer, this is the perfect method to combine with an async event loop that waits on
    /// listeners, something like this:
    ///
    /// ```ignore
    /// while let Ok(event) = receiver.recv().await {
    ///     let edits = self.internal_dom.progress_with_event(event)?;
    ///     for edit in &edits {
    ///         patch_machine.handle_edit(edit);
    ///     }
    /// }
    /// ```
    ///
    /// Note: this method is not async and does not provide suspense-like functionality. It is up to the renderer to provide the
    /// executor and handlers for suspense as show in the example.
    ///
    /// ```ignore
    /// let (sender, receiver) = channel::new();
    /// sender.send(EventTrigger::start());
    ///
    /// let mut dom = VirtualDom::new();
    /// dom.suspense_handler(|event| sender.send(event));
    ///
    /// while let Ok(diffs) = dom.progress_with_event(receiver.recv().await) {
    ///     render(diffs);
    /// }
    ///
    /// ```
    //
    // Developer notes:
    // ----
    // This method has some pretty complex safety guarantees to uphold.
    // We interact with bump arenas, raw pointers, and use UnsafeCell to get a partial borrow of the arena.
    // The final EditList has edits that pull directly from the Bump Arenas which add significant complexity
    // in crafting a 100% safe solution with traditional lifetimes. Consider this method to be internally unsafe
    // but the guarantees provide a safe, fast, and efficient abstraction for the VirtualDOM updating framework.
    //
    // A good project would be to remove all unsafe from this crate and move the unsafety into safer abstractions.
    pub fn progress_with_event<'s, Dom: RealDom<'s>>(
        &'s mut self,
        realdom: &'_ mut Dom,
        trigger: EventTrigger,
    ) -> Result<()> {
        let id = trigger.originator.clone();

        self.components.try_get_mut(id)?.call_listener(trigger)?;

        let mut diff_machine = DiffMachine::new(
            realdom,
            &self.components,
            id,
            self.event_queue.clone(),
            &self.tasks,
        );

        self.progress_completely(&mut diff_machine)?;

        Ok(())
    }

    /// Consume the event queue, descending depth-first.
    /// Only ever run each component once.
    ///
    /// The DiffMachine logs its progress as it goes which might be useful for certain types of renderers.
    pub(crate) fn progress_completely<'a, 'bump, Dom: RealDom<'bump>>(
        &'bump self,
        diff_machine: &'_ mut DiffMachine<'a, 'bump, Dom>,
    ) -> Result<()> {
        // Now, there are events in the queue
        let mut updates = self.event_queue.queue.as_ref().borrow_mut();

        // Order the nodes by their height, we want the nodes with the smallest depth on top
        // This prevents us from running the same component multiple times
        updates.sort_unstable();

        log::debug!("There are: {:#?} updates to be processed", updates.len());

        // Iterate through the triggered nodes (sorted by height) and begin to diff them
        for update in updates.drain(..) {
            log::debug!("Running updates for: {:#?}", update);

            // Make sure this isn't a node we've already seen, we don't want to double-render anything
            // If we double-renderer something, this would cause memory safety issues
            if diff_machine.seen_nodes.contains(&update.idx) {
                continue;
            }

            // Now, all the "seen nodes" are nodes that got notified by running this listener
            diff_machine.seen_nodes.insert(update.idx.clone());

            // Start a new mutable borrow to components
            // We are guaranteeed that this scope is unique because we are tracking which nodes have modified
            let cur_component = self.components.try_get_mut(update.idx).unwrap();

            cur_component.run_scope()?;

            let (old, new) = (cur_component.old_frame(), cur_component.next_frame());
            diff_machine.diff_node(old, new);

            // log::debug!(
            //     "Processing update: {:#?} with height {}",
            //     &update.idx,
            //     cur_height
            // );
        }

        Ok(())
    }

    pub fn base_scope(&self) -> &Scope {
        let idx = self.base_scope;
        self.components.try_get(idx).unwrap()
    }
}

// TODO!
// These impls are actually wrong. The DOM needs to have a mutex implemented.
unsafe impl Sync for VirtualDom {}
unsafe impl Send for VirtualDom {}
