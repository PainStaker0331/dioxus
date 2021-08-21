//! This module contains the stateful DiffMachine and all methods to diff VNodes, their properties, and their children.
//!
//! The [`DiffMachine`] calculates the diffs between the old and new frames, updates the new nodes, and generates a set
//! of mutations for the RealDom to apply.
//!
//! ## Notice:
//!
//! The inspiration and code for this module was originally taken from Dodrio (@fitzgen) and then modified to support
//! Components, Fragments, Suspense, SubTree memoization, incremental diffing, cancelation, NodeRefs, and additional
//! batching operations.
//!
//! ## Implementation Details:
//!
//! ### IDs for elements
//! --------------------
//! All nodes are addressed by their IDs. The RealDom provides an imperative interface for making changes to these nodes.
//! We don't necessarily require that DOM changes happen instantly during the diffing process, so the implementor may choose
//! to batch nodes if it is more performant for their application. The element IDs are indicies into the internal element
//! array. The expectation is that implemenetors will use the ID as an index into a Vec of real nodes, allowing for passive
//! garbage collection as the VirtualDOM replaces old nodes.
//!
//! When new vnodes are created through `cx.render`, they won't know which real node they correspond to. During diffing,
//! we always make sure to copy over the ID. If we don't do this properly, the ElementId will be populated incorrectly
//! and brick the user's page.
//!
//! ### Fragment Support
//! --------------------
//! Fragments (nodes without a parent) are supported through a combination of "replace with" and anchor vnodes. Fragments
//! can be particularly challenging when they are empty, so the anchor node lets us "reserve" a spot for the empty
//! fragment to be replaced with when it is no longer empty. This is guaranteed by logic in the NodeFactory - it is
//! impossible to craft a fragment with 0 elements - they must always have at least a single placeholder element. Adding
//! "dummy" nodes _is_ inefficient, but it makes our diffing algorithm faster and the implementation is completely up to
//!  the platform.
//!
//! Other implementations either don't support fragments or use a "child + sibling" pattern to represent them. Our code is
//! vastly simpler and more performant when we can just create a placeholder element while the fragment has no children.
//!
//! ## Subtree Memoization
//! -----------------------
//! We also employ "subtree memoization" which saves us from having to check trees which take no dynamic content. We can
//! detect if a subtree is "static" by checking if its children are "static". Since we dive into the tree depth-first, the
//! calls to "create" propogate this information upwards. Structures like the one below are entirely static:
//! ```rust
//! rsx!( div { class: "hello world", "this node is entirely static" } )
//! ```
//! Because the subtrees won't be diffed, their "real node" data will be stale (invalid), so its up to the reconciler to
//! track nodes created in a scope and clean up all relevant data. Support for this is currently WIP and depends on comp-time
//! hashing of the subtree from the rsx! macro. We do a very limited form of static analysis via static string pointers as
//! a way of short-circuiting the most expensive checks.
//!
//! ## Bloom Filter and Heuristics
//! ------------------------------
//! For all components, we employ some basic heuristics to speed up allocations and pre-size bump arenas. The heuristics are
//! currently very rough, but will get better as time goes on. The information currently tracked includes the size of a
//! bump arena after first render, the number of hooks, and the number of nodes in the tree.
//!
//! ## Garbage Collection
//! ---------------------
//! Dioxus uses a passive garbage collection system to clean up old nodes once the work has been completed. This garabge
//! collection is done internally once the main diffing work is complete. After the "garbage" is collected, Dioxus will then
//! start to re-use old keys for new nodes. This results in a passive memory management system that is very efficient.
//!
//! The IDs used by the key/map are just an index into a vec. This means that Dioxus will drive the key allocation strategy
//! so the client only needs to maintain a simple list of nodes. By default, Dioxus will not manually clean up old nodes
//! for the client. As new nodes are created, old nodes will be over-written.
//!
//! ## Further Reading and Thoughts
//! ----------------------------
//! There are more ways of increasing diff performance here that are currently not implemented.
//! More info on how to improve this diffing algorithm:
//!  - https://hacks.mozilla.org/2019/03/fast-bump-allocated-virtual-doms-with-rust-and-wasm/

use crate::{arena::SharedResources, innerlude::*};
use futures_util::Future;
use fxhash::{FxBuildHasher, FxHashMap, FxHashSet};
use indexmap::IndexSet;
use smallvec::{smallvec, SmallVec};

use std::{
    any::Any, cell::Cell, cmp::Ordering, collections::HashSet, marker::PhantomData, pin::Pin,
};
use DomEdit::*;

/// Our DiffMachine is an iterative tree differ.
///
/// It uses techniques of a stack machine to allow pausing and restarting of the diff algorithm. This
/// was origially implemented using recursive techniques, but Rust lacks the abilty to call async functions recursively,
/// meaning we could not "pause" the original diffing algorithm.
///
/// Instead, we use a traditional stack machine approach to diff and create new nodes. The diff algorithm periodically
/// calls "yield_now" which allows the machine to pause and return control to the caller. The caller can then wait for
/// the next period of idle time, preventing our diff algorithm from blocking the main thread.
///
/// Funnily enough, this stack machine's entire job is to create instructions for another stack machine to execute. It's
/// stack machines all the way down!
pub struct DiffMachine<'bump> {
    vdom: &'bump SharedResources,

    pub mutations: Mutations<'bump>,

    pub nodes_created_stack: SmallVec<[usize; 10]>,

    pub instructions: SmallVec<[DiffInstruction<'bump>; 10]>,

    pub scope_stack: SmallVec<[ScopeId; 5]>,

    pub diffed: FxHashSet<ScopeId>,

    pub seen_scopes: FxHashSet<ScopeId>,
}

/// The stack instructions we use to diff and create new nodes.
///
/// Right now, we insert an instruction for every child node we want to create and diff. This can be less efficient than
/// a custom iterator type - but this is current easier to implement. In the future, let's try interact with the stack less.
#[derive(Debug)]
pub enum DiffInstruction<'a> {
    DiffNode {
        old: &'a VNode<'a>,
        new: &'a VNode<'a>,
    },

    DiffChildren {
        progress: usize,
        old: &'a [VNode<'a>],
        new: &'a [VNode<'a>],
    },

    Create {
        node: &'a VNode<'a>,
    },
    CreateChildren {
        progress: usize,
        children: &'a [VNode<'a>],
    },

    // todo: merge this into the create instruction?
    Append,
    InsertAfter,
    InsertBefore,
    Replace {
        with: usize,
    },
}

impl<'bump> DiffMachine<'bump> {
    pub(crate) fn new(
        edits: Mutations<'bump>,
        cur_scope: ScopeId,
        shared: &'bump SharedResources,
    ) -> Self {
        Self {
            instructions: smallvec![],
            nodes_created_stack: smallvec![],
            mutations: edits,
            scope_stack: smallvec![cur_scope],
            vdom: shared,
            diffed: FxHashSet::default(),
            seen_scopes: FxHashSet::default(),
        }
    }

    pub fn new_headless(shared: &'bump SharedResources) -> Self {
        let edits = Mutations::new();
        let cur_scope = ScopeId(0);
        Self::new(edits, cur_scope, shared)
    }

    //
    pub async fn diff_scope(&mut self, id: ScopeId) -> Result<()> {
        let component = self.get_scope_mut(&id).ok_or_else(|| Error::NotMounted)?;
        let (old, new) = (component.frames.wip_head(), component.frames.fin_head());
        self.diff_node(old, new);
        Ok(())
    }

    /// Progress the diffing for this "fiber"
    ///
    /// This method implements a depth-first iterative tree traversal.
    ///
    /// We do depth-first to maintain high cache locality (nodes were originally generated recursively) and because we
    /// only need a stack (not a queue) of lists
    pub async fn work(&mut self) -> Result<()> {
        // todo: don't move the reused instructions around
        // defer to individual functions so the compiler produces better code
        // large functions tend to be difficult for the compiler to work with
        while let Some(instruction) = self.instructions.last_mut() {
            log::debug!("Handling diff instruction: {:?}", instruction);
            match instruction {
                DiffInstruction::DiffNode { old, new, .. } => {
                    let (old, new) = (*old, *new);
                    self.instructions.pop();
                    self.diff_node(old, new);
                }

                // this is slightly more complicated, we need to find a way to pause our LIS code
                DiffInstruction::DiffChildren { progress, old, new } => {
                    todo!()
                }

                DiffInstruction::Create { node, .. } => {
                    let node = *node;
                    self.instructions.pop();
                    self.create_node(node);
                }

                DiffInstruction::CreateChildren { progress, children } => {
                    if let Some(child) = children.get(*progress) {
                        *progress += 1;
                        self.create_node(child);
                    } else {
                        self.instructions.pop();
                    }
                }

                DiffInstruction::Append => {
                    let many = self.nodes_created_stack.pop().unwrap();
                    self.edit_append_children(many as u32);
                    self.instructions.pop();
                }

                DiffInstruction::Replace { with } => {
                    let with = *with;
                    let many = self.nodes_created_stack.pop().unwrap();
                    self.edit_replace_with(with as u32, many as u32);
                    self.instructions.pop();
                }

                DiffInstruction::InsertAfter => {
                    let n = self.nodes_created_stack.pop().unwrap();
                    self.edit_insert_after(n as u32);
                    self.instructions.pop();
                }

                DiffInstruction::InsertBefore => {
                    let n = self.nodes_created_stack.pop().unwrap();
                    self.edit_insert_before(n as u32);
                    self.instructions.pop();
                }
            };
        }

        Ok(())
    }

    // =================================
    //  Tools for creating new nodes
    // =================================

    fn create_node(&mut self, node: &'bump VNode<'bump>) {
        match node {
            VNode::Text(vtext) => self.create_text_node(vtext),
            VNode::Suspended(suspended) => self.create_suspended_node(suspended),
            VNode::Anchor(anchor) => self.create_anchor_node(anchor),
            VNode::Element(element) => self.create_element_node(element),
            VNode::Fragment(frag) => self.create_fragment_node(frag),
            VNode::Component(component) => self.create_component_node(component),
        }
    }

    fn create_text_node(&mut self, vtext: &'bump VText<'bump>) {
        let real_id = self.vdom.reserve_node();
        self.edit_create_text_node(vtext.text, real_id);
        vtext.dom_id.set(Some(real_id));
        *self.nodes_created_stack.last_mut().unwrap() += 1;
    }

    fn create_suspended_node(&mut self, suspended: &'bump VSuspended) {
        let real_id = self.vdom.reserve_node();
        self.edit_create_placeholder(real_id);
        suspended.node.set(Some(real_id));
        *self.nodes_created_stack.last_mut().unwrap() += 1;
    }

    fn create_anchor_node(&mut self, anchor: &'bump VAnchor) {
        let real_id = self.vdom.reserve_node();
        self.edit_create_placeholder(real_id);
        anchor.dom_id.set(Some(real_id));
        *self.nodes_created_stack.last_mut().unwrap() += 1;
    }

    fn create_element_node(&mut self, element: &'bump VElement<'bump>) {
        let VElement {
            tag_name,
            listeners,
            attributes,
            children,
            namespace,
            dom_id,
            ..
        } = element;

        let real_id = self.vdom.reserve_node();
        self.edit_create_element(tag_name, *namespace, real_id);
        *self.nodes_created_stack.last_mut().unwrap() += 1;
        dom_id.set(Some(real_id));

        let cur_scope = self.current_scope().unwrap();

        listeners.iter().for_each(|listener| {
            self.fix_listener(listener);
            listener.mounted_node.set(Some(real_id));
            self.edit_new_event_listener(listener, cur_scope.clone());
        });

        for attr in *attributes {
            self.edit_set_attribute(attr);
        }

        self.instructions.push(DiffInstruction::Append);

        self.nodes_created_stack.push(0);
        self.instructions.push(DiffInstruction::CreateChildren {
            children,
            progress: 0,
        });
    }

    fn create_fragment_node(&mut self, frag: &'bump VFragment<'bump>) {
        self.instructions.push(DiffInstruction::CreateChildren {
            children: frag.children,
            progress: 0,
        });
    }

    fn create_component_node(&mut self, vcomponent: &'bump VComponent<'bump>) {
        let caller = vcomponent.caller.clone();

        let parent_idx = self.scope_stack.last().unwrap().clone();

        // Insert a new scope into our component list
        let new_idx = self.vdom.insert_scope_with_key(|new_idx| {
            let parent_scope = self.get_scope(&parent_idx).unwrap();
            let height = parent_scope.height + 1;
            Scope::new(
                caller,
                new_idx,
                Some(parent_idx),
                height,
                ScopeChildren(vcomponent.children),
                self.vdom.clone(),
            )
        });

        // Actually initialize the caller's slot with the right address
        vcomponent.ass_scope.set(Some(new_idx));

        if !vcomponent.can_memoize {
            let cur_scope = self.get_scope_mut(&parent_idx).unwrap();
            let extended = vcomponent as *const VComponent;
            let extended: *const VComponent<'static> = unsafe { std::mem::transmute(extended) };
            cur_scope.borrowed_props.borrow_mut().push(extended);
        }

        // TODO:
        //  add noderefs to current noderef list Noderefs
        //  add effects to current effect list Effects

        let new_component = self.get_scope_mut(&new_idx).unwrap();

        // Run the scope for one iteration to initialize it
        match new_component.run_scope() {
            Ok(_g) => {
                // all good, new nodes exist
            }
            Err(err) => {
                // failed to run. this is the first time the component ran, and it failed
                // we manually set its head node to an empty fragment
                panic!("failing components not yet implemented");
            }
        }

        // Take the node that was just generated from running the component
        let nextnode = new_component.frames.fin_head();

        // // Push the new scope onto the stack
        // self.scope_stack.push(new_idx);

        // // Run the creation algorithm with this scope on the stack
        self.instructions
            .push(DiffInstruction::Create { node: nextnode });

        // let meta = self.create_vnode(nextnode);

        // // pop the scope off the stack
        // self.scope_stack.pop();

        // if meta.added_to_stack == 0 {
        //     panic!("Components should *always* generate nodes - even if they fail");
        // }

        // // Finally, insert this scope as a seen node.
        self.seen_scopes.insert(new_idx);
    }

    // =================================
    //  Tools for diffing nodes
    // =================================

    pub fn diff_node(&mut self, old_node: &'bump VNode<'bump>, new_node: &'bump VNode<'bump>) {
        use VNode::*;
        match (old_node, new_node) {
            // Check the most common cases first
            (Text(old), Text(new)) => self.diff_text_nodes(old, new),
            (Element(old), Element(new)) => self.diff_element_nodes(old, new),
            (Component(old), Component(new)) => self.diff_component_nodes(old, new),
            (Fragment(old), Fragment(new)) => self.diff_fragment_nodes(old, new),
            (Anchor(old), Anchor(new)) => new.dom_id.set(old.dom_id.get()),

            (
                Component(_) | Fragment(_) | Text(_) | Element(_) | Anchor(_),
                Component(_) | Fragment(_) | Text(_) | Element(_) | Anchor(_),
            ) => {
                self.replace_and_create_many_with_many([old_node], [new_node]);
            }

            // TODO: these don't properly clean up any data
            (Suspended(old), new) => {
                self.replace_and_create_many_with_many([old_node], [new_node]);
            }

            // a node that was once real is now suspended
            (old, Suspended(_)) => {
                self.replace_and_create_many_with_many([old_node], [new_node]);
            }
        }
    }

    fn diff_text_nodes(&mut self, old: &'bump VText<'bump>, new: &'bump VText<'bump>) {
        let root = old.dom_id.get().unwrap();

        if old.text != new.text {
            self.edit_push_root(root);
            self.edit_set_text(new.text);
            self.edit_pop();
        }

        new.dom_id.set(Some(root));
    }

    fn diff_element_nodes(&mut self, old: &'bump VElement<'bump>, new: &'bump VElement<'bump>) {
        let root = old.dom_id.get().unwrap();

        // If the element type is completely different, the element needs to be re-rendered completely
        // This is an optimization React makes due to how users structure their code
        //
        // This case is rather rare (typically only in non-keyed lists)
        if new.tag_name != old.tag_name || new.namespace != old.namespace {
            todo!();
            // self.replace_node_with_node(root, old_node, new_node);
            return;
        }

        new.dom_id.set(Some(root));

        // Don't push the root if we don't have to
        let mut has_comitted = false;
        let mut please_commit = |edits: &mut Vec<DomEdit>| {
            if !has_comitted {
                has_comitted = true;
                edits.push(PushRoot { id: root.as_u64() });
            }
        };

        // Diff Attributes
        //
        // It's extraordinarily rare to have the number/order of attributes change
        // In these cases, we just completely erase the old set and make a new set
        //
        // TODO: take a more efficient path than this
        if old.attributes.len() == new.attributes.len() {
            for (old_attr, new_attr) in old.attributes.iter().zip(new.attributes.iter()) {
                if old_attr.value != new_attr.value {
                    please_commit(&mut self.mutations.edits);
                    self.edit_set_attribute(new_attr);
                }
            }
        } else {
            // TODO: provide some sort of report on how "good" the diffing was
            please_commit(&mut self.mutations.edits);
            for attribute in old.attributes {
                self.edit_remove_attribute(attribute);
            }
            for attribute in new.attributes {
                self.edit_set_attribute(attribute)
            }
        }

        // Diff listeners
        //
        // It's extraordinarily rare to have the number/order of listeners change
        // In the cases where the listeners change, we completely wipe the data attributes and add new ones
        //
        // We also need to make sure that all listeners are properly attached to the parent scope (fix_listener)
        //
        // TODO: take a more efficient path than this
        let cur_scope: ScopeId = self.scope_stack.last().unwrap().clone();
        if old.listeners.len() == new.listeners.len() {
            for (old_l, new_l) in old.listeners.iter().zip(new.listeners.iter()) {
                if old_l.event != new_l.event {
                    please_commit(&mut self.mutations.edits);
                    self.edit_remove_event_listener(old_l.event);
                    self.edit_new_event_listener(new_l, cur_scope);
                }
                new_l.mounted_node.set(old_l.mounted_node.get());
                self.fix_listener(new_l);
            }
        } else {
            please_commit(&mut self.mutations.edits);
            for listener in old.listeners {
                self.edit_remove_event_listener(listener.event);
            }
            for listener in new.listeners {
                listener.mounted_node.set(Some(root));
                self.edit_new_event_listener(listener, cur_scope);

                // Make sure the listener gets attached to the scope list
                self.fix_listener(listener);
            }
        }

        if has_comitted {
            self.edit_pop();
        }

        self.diff_children(old.children, new.children);
    }

    fn diff_component_nodes(
        &mut self,
        old: &'bump VComponent<'bump>,
        new: &'bump VComponent<'bump>,
    ) {
        let scope_addr = old.ass_scope.get().unwrap();

        // Make sure we're dealing with the same component (by function pointer)
        if old.user_fc == new.user_fc {
            //
            self.scope_stack.push(scope_addr);

            // Make sure the new component vnode is referencing the right scope id
            new.ass_scope.set(Some(scope_addr));

            // make sure the component's caller function is up to date
            let scope = self.get_scope_mut(&scope_addr).unwrap();

            scope.update_scope_dependencies(new.caller.clone(), ScopeChildren(new.children));

            // React doesn't automatically memoize, but we do.
            let compare = old.comparator.unwrap();

            match compare(new) {
                true => {
                    // the props are the same...
                }
                false => {
                    // the props are different...
                    scope.run_scope().unwrap();
                    self.diff_node(scope.frames.wip_head(), scope.frames.fin_head());
                }
            }

            self.scope_stack.pop();

            self.seen_scopes.insert(scope_addr);
        } else {
            todo!();

            // let mut old_iter = RealChildIterator::new(old_node, &self.vdom);
            // let first = old_iter
            //     .next()
            //     .expect("Components should generate a placeholder root");

            // // remove any leftovers
            // for to_remove in old_iter {
            //     self.edit_push_root(to_remove.direct_id());
            //     self.edit_remove();
            // }

            // // seems like we could combine this into a single instruction....
            // self.replace_node_with_node(first.direct_id(), old_node, new_node);

            // // Wipe the old one and plant the new one
            // let old_scope = old.ass_scope.get().unwrap();
            // self.destroy_scopes(old_scope);
        }
    }

    fn diff_fragment_nodes(&mut self, old: &'bump VFragment<'bump>, new: &'bump VFragment<'bump>) {
        // This is the case where options or direct vnodes might be used.
        // In this case, it's faster to just skip ahead to their diff
        if old.children.len() == 1 && new.children.len() == 1 {
            self.diff_node(&old.children[0], &new.children[0]);
            return;
        }

        self.diff_children(old.children, new.children);
    }

    /// Destroy a scope and all of its descendents.
    ///
    /// Calling this will run the destuctors on all hooks in the tree.
    /// It will also add the destroyed nodes to the `seen_nodes` cache to prevent them from being renderered.
    fn destroy_scopes(&mut self, old_scope: ScopeId) {
        let mut nodes_to_delete = vec![old_scope];
        let mut scopes_to_explore = vec![old_scope];

        // explore the scope tree breadth first
        while let Some(scope_id) = scopes_to_explore.pop() {
            // If we're planning on deleting this node, then we don't need to both rendering it
            self.seen_scopes.insert(scope_id);
            let scope = self.get_scope(&scope_id).unwrap();
            for child in scope.descendents.borrow().iter() {
                // Add this node to be explored
                scopes_to_explore.push(child.clone());

                // Also add it for deletion
                nodes_to_delete.push(child.clone());
            }
        }

        // Delete all scopes that we found as part of this subtree
        for node in nodes_to_delete {
            log::debug!("Removing scope {:#?}", node);
            let _scope = self.vdom.try_remove(node).unwrap();
            // do anything we need to do to delete the scope
            // I think we need to run the destructors on the hooks
            // TODO
        }
    }

    // Diff the given set of old and new children.
    //
    // The parent must be on top of the change list stack when this function is
    // entered:
    //
    //     [... parent]
    //
    // the change list stack is in the same state when this function returns.
    //
    // If old no anchors are provided, then it's assumed that we can freely append to the parent.
    //
    // Remember, non-empty lists does not mean that there are real elements, just that there are virtual elements.
    fn diff_children(&mut self, old: &'bump [VNode<'bump>], new: &'bump [VNode<'bump>]) {
        const IS_EMPTY: bool = true;
        const IS_NOT_EMPTY: bool = false;

        match (old.is_empty(), new.is_empty()) {
            (IS_EMPTY, IS_EMPTY) => {}

            // Completely adding new nodes, removing any placeholder if it exists
            (IS_EMPTY, IS_NOT_EMPTY) => {
                todo!();
                // let meta = todo!();
                // let meta = self.create_children(new);
                // let meta = self.create_children(new);
                // self.edit_append_children(meta.added_to_stack);
            }

            // Completely removing old nodes and putting an anchor in its place
            // no anchor (old has nodes) and the new is empty
            // remove all the old nodes
            (IS_NOT_EMPTY, IS_EMPTY) => {
                for node in old {
                    self.remove_vnode(node);
                }
            }

            (IS_NOT_EMPTY, IS_NOT_EMPTY) => {
                let first_old = &old[0];
                let first_new = &new[0];

                match (&first_old, &first_new) {
                    // Anchors can only appear in empty fragments
                    (VNode::Anchor(old_anchor), VNode::Anchor(new_anchor)) => {
                        old_anchor.dom_id.set(new_anchor.dom_id.get());
                    }

                    // Replace the anchor with whatever new nodes are coming down the pipe
                    (VNode::Anchor(anchor), _) => {
                        self.edit_push_root(anchor.dom_id.get().unwrap());
                        let mut added = 0;
                        for el in new {
                            todo!();
                            // let meta = self.create_vnode(el);
                            // added += meta.added_to_stack;
                        }
                        self.edit_replace_with(1, added);
                    }

                    // Replace whatever nodes are sitting there with the anchor
                    (_, VNode::Anchor(anchor)) => {
                        self.replace_and_create_many_with_many(old, [first_new]);
                    }

                    // Use the complex diff algorithm to diff the nodes
                    _ => {
                        let new_is_keyed = new[0].key().is_some();
                        let old_is_keyed = old[0].key().is_some();

                        debug_assert!(
                            new.iter().all(|n| n.key().is_some() == new_is_keyed),
                            "all siblings must be keyed or all siblings must be non-keyed"
                        );
                        debug_assert!(
                            old.iter().all(|o| o.key().is_some() == old_is_keyed),
                            "all siblings must be keyed or all siblings must be non-keyed"
                        );

                        if new_is_keyed && old_is_keyed {
                            self.diff_keyed_children(old, new);
                        } else {
                            self.diff_non_keyed_children(old, new);
                        }
                    }
                }
            }
        }
    }

    // Diffing "keyed" children.
    //
    // With keyed children, we care about whether we delete, move, or create nodes
    // versus mutate existing nodes in place. Presumably there is some sort of CSS
    // transition animation that makes the virtual DOM diffing algorithm
    // observable. By specifying keys for nodes, we know which virtual DOM nodes
    // must reuse (or not reuse) the same physical DOM nodes.
    //
    // This is loosely based on Inferno's keyed patching implementation. However, we
    // have to modify the algorithm since we are compiling the diff down into change
    // list instructions that will be executed later, rather than applying the
    // changes to the DOM directly as we compare virtual DOMs.
    //
    // https://github.com/infernojs/inferno/blob/36fd96/packages/inferno/src/DOM/patching.ts#L530-L739
    //
    // The stack is empty upon entry.
    fn diff_keyed_children(&mut self, old: &'bump [VNode<'bump>], new: &'bump [VNode<'bump>]) {
        if cfg!(debug_assertions) {
            let mut keys = fxhash::FxHashSet::default();
            let mut assert_unique_keys = |children: &'bump [VNode<'bump>]| {
                keys.clear();
                for child in children {
                    let key = child.key();
                    debug_assert!(
                        key.is_some(),
                        "if any sibling is keyed, all siblings must be keyed"
                    );
                    keys.insert(key);
                }
                debug_assert_eq!(
                    children.len(),
                    keys.len(),
                    "keyed siblings must each have a unique key"
                );
            };
            assert_unique_keys(old);
            assert_unique_keys(new);
        }

        // First up, we diff all the nodes with the same key at the beginning of the
        // children.
        //
        // `shared_prefix_count` is the count of how many nodes at the start of
        // `new` and `old` share the same keys.
        //
        // TODO: just inline this
        let shared_prefix_count = match self.diff_keyed_prefix(old, new) {
            KeyedPrefixResult::Finished => return,
            KeyedPrefixResult::MoreWorkToDo(count) => count,
        };

        // Next, we find out how many of the nodes at the end of the children have
        // the same key. We do _not_ diff them yet, since we want to emit the change
        // list instructions such that they can be applied in a single pass over the
        // DOM. Instead, we just save this information for later.
        //
        // `shared_suffix_count` is the count of how many nodes at the end of `new`
        // and `old` share the same keys.
        let shared_suffix_count = old[shared_prefix_count..]
            .iter()
            .rev()
            .zip(new[shared_prefix_count..].iter().rev())
            .take_while(|&(old, new)| old.key() == new.key())
            .count();

        let old_shared_suffix_start = old.len() - shared_suffix_count;
        let new_shared_suffix_start = new.len() - shared_suffix_count;

        // Ok, we now hopefully have a smaller range of children in the middle
        // within which to re-order nodes with the same keys, remove old nodes with
        // now-unused keys, and create new nodes with fresh keys.
        self.diff_keyed_middle(
            &old[shared_prefix_count..old_shared_suffix_start],
            &new[shared_prefix_count..new_shared_suffix_start],
            shared_prefix_count,
            shared_suffix_count,
            old_shared_suffix_start,
        );

        // Finally, diff the nodes at the end of `old` and `new` that share keys.
        let old_suffix = &old[old_shared_suffix_start..];
        let new_suffix = &new[new_shared_suffix_start..];
        debug_assert_eq!(old_suffix.len(), new_suffix.len());
        if !old_suffix.is_empty() {
            self.diff_keyed_suffix(old_suffix, new_suffix, new_shared_suffix_start)
        }
    }

    // Diff the prefix of children in `new` and `old` that share the same keys in
    // the same order.
    //
    // The stack is empty upon entry.
    fn diff_keyed_prefix(
        &mut self,
        old: &'bump [VNode<'bump>],
        new: &'bump [VNode<'bump>],
    ) -> KeyedPrefixResult {
        let mut shared_prefix_count = 0;

        for (old, new) in old.iter().zip(new.iter()) {
            // abort early if we finally run into nodes with different keys
            if old.key() != new.key() {
                break;
            }
            self.diff_node(old, new);
            shared_prefix_count += 1;
        }

        // If that was all of the old children, then create and append the remaining
        // new children and we're finished.
        if shared_prefix_count == old.len() {
            // Load the last element
            let last_node = self.find_last_element(new.last().unwrap()).direct_id();
            self.edit_push_root(last_node);

            // Create the new children and insert them after
            //
            todo!();
            // let meta = self.create_children(&new[shared_prefix_count..]);
            // self.edit_insert_after(meta.added_to_stack);

            return KeyedPrefixResult::Finished;
        }

        // And if that was all of the new children, then remove all of the remaining
        // old children and we're finished.
        if shared_prefix_count == new.len() {
            self.remove_children(&old[shared_prefix_count..]);
            return KeyedPrefixResult::Finished;
        }

        KeyedPrefixResult::MoreWorkToDo(shared_prefix_count)
    }

    // Create the given children and append them to the parent node.
    //
    // The parent node must currently be on top of the change list stack:
    //
    //     [... parent]
    //
    // When this function returns, the change list stack is in the same state.
    pub fn create_and_append_children(&mut self, new: &'bump [VNode<'bump>]) {
        for child in new {
            todo!();
            // let meta = self.create_vnode(child);
            // self.edit_append_children(meta.added_to_stack);
        }
    }

    // The most-general, expensive code path for keyed children diffing.
    //
    // We find the longest subsequence within `old` of children that are relatively
    // ordered the same way in `new` (via finding a longest-increasing-subsequence
    // of the old child's index within `new`). The children that are elements of
    // this subsequence will remain in place, minimizing the number of DOM moves we
    // will have to do.
    //
    // Upon entry to this function, the change list stack must be empty.
    //
    // This function will load the appropriate nodes onto the stack and do diffing in place.
    //
    // Upon exit from this function, it will be restored to that same state.
    fn diff_keyed_middle(
        &mut self,
        old: &'bump [VNode<'bump>],
        mut new: &'bump [VNode<'bump>],
        shared_prefix_count: usize,
        shared_suffix_count: usize,
        old_shared_suffix_start: usize,
    ) {
        // Should have already diffed the shared-key prefixes and suffixes.
        debug_assert_ne!(new.first().map(|n| n.key()), old.first().map(|o| o.key()));
        debug_assert_ne!(new.last().map(|n| n.key()), old.last().map(|o| o.key()));

        // // The algorithm below relies upon using `u32::MAX` as a sentinel
        // // value, so if we have that many new nodes, it won't work. This
        // // check is a bit academic (hence only enabled in debug), since
        // // wasm32 doesn't have enough address space to hold that many nodes
        // // in memory.
        // debug_assert!(new.len() < u32::MAX as usize);

        // Map from each `old` node's key to its index within `old`.
        // IE if the keys were A B C, then we would have (A, 1) (B, 2) (C, 3).
        let mut old_key_to_old_index = old
            .iter()
            .enumerate()
            .map(|(i, o)| (o.key().unwrap(), i))
            .collect::<FxHashMap<_, _>>();

        // The set of shared keys between `new` and `old`.
        let mut shared_keys = FxHashSet::default();
        // let mut to_remove = FxHashSet::default();
        let mut to_add = FxHashSet::default();

        // Map from each index in `new` to the index of the node in `old` that
        // has the same key.
        let mut new_index_to_old_index = new
            .iter()
            .map(|n| {
                let key = n.key().unwrap();
                match old_key_to_old_index.get(&key) {
                    Some(&index) => {
                        shared_keys.insert(key);
                        index
                    }
                    None => {
                        //
                        to_add.insert(key);
                        u32::MAX as usize
                    }
                }
            })
            .collect::<Vec<_>>();

        dbg!(&shared_keys);
        dbg!(&to_add);

        // If none of the old keys are reused by the new children, then we
        // remove all the remaining old children and create the new children
        // afresh.
        if shared_suffix_count == 0 && shared_keys.is_empty() {
            self.replace_and_create_many_with_many(old, new);
            return;
        }

        // // Remove any old children whose keys were not reused in the new
        // // children. Remove from the end first so that we don't mess up indices.
        // for old_child in old.iter().rev() {
        //     if !shared_keys.contains(&old_child.key()) {
        //         self.remove_child(old_child);
        //     }
        // }

        // let old_keyds = old.iter().map(|f| f.key()).collect::<Vec<_>>();
        // let new_keyds = new.iter().map(|f| f.key()).collect::<Vec<_>>();
        // dbg!(old_keyds);
        // dbg!(new_keyds);

        // // If there aren't any more new children, then we are done!
        // if new.is_empty() {
        //     return;
        // }

        // The longest increasing subsequence within `new_index_to_old_index`. This
        // is the longest sequence on DOM nodes in `old` that are relatively ordered
        // correctly within `new`. We will leave these nodes in place in the DOM,
        // and only move nodes that are not part of the LIS. This results in the
        // maximum number of DOM nodes left in place, AKA the minimum number of DOM
        // nodes moved.
        let mut new_index_is_in_lis = FxHashSet::default();
        new_index_is_in_lis.reserve(new_index_to_old_index.len());

        let mut predecessors = vec![0; new_index_to_old_index.len()];
        let mut starts = vec![0; new_index_to_old_index.len()];

        longest_increasing_subsequence::lis_with(
            &new_index_to_old_index,
            &mut new_index_is_in_lis,
            |a, b| a < b,
            &mut predecessors,
            &mut starts,
        );

        dbg!(&new_index_is_in_lis);
        // use the old nodes to navigate the new nodes

        let mut lis_in_order = new_index_is_in_lis.into_iter().collect::<Vec<_>>();
        lis_in_order.sort_unstable();

        dbg!(&lis_in_order);

        // we walk front to back, creating the head node

        // diff the shared, in-place nodes first
        // this makes sure we can rely on their first/last nodes being correct later on
        for id in &lis_in_order {
            let new_node = &new[*id];
            let key = new_node.key().unwrap();
            let old_index = old_key_to_old_index.get(&key).unwrap();
            let old_node = &old[*old_index];
            self.diff_node(old_node, new_node);
        }

        // return the old node from the key
        let load_old_node_from_lsi = |key| -> &VNode {
            let old_index = old_key_to_old_index.get(key).unwrap();
            let old_node = &old[*old_index];
            old_node
        };

        let mut root = None;
        let mut new_iter = new.iter().enumerate();
        for lis_id in &lis_in_order {
            eprintln!("tracking {:?}", lis_id);
            // this is the next milestone node we are working up to
            let new_anchor = &new[*lis_id];
            root = Some(new_anchor);

            let anchor_el = self.find_first_element(new_anchor);
            self.edit_push_root(anchor_el.direct_id());
            // let mut pushed = false;

            'inner: loop {
                let (next_id, next_new) = new_iter.next().unwrap();
                if next_id == *lis_id {
                    // we've reached the milestone, break this loop so we can step to the next milestone
                    // remember: we already diffed this node
                    eprintln!("breaking {:?}", next_id);
                    break 'inner;
                } else {
                    let key = next_new.key().unwrap();
                    eprintln!("found key {:?}", key);
                    if shared_keys.contains(&key) {
                        eprintln!("key is contained {:?}", key);
                        shared_keys.remove(key);
                        // diff the two nodes
                        let old_node = load_old_node_from_lsi(key);
                        self.diff_node(old_node, next_new);

                        // now move all the nodes into the right spot
                        for child in RealChildIterator::new(next_new, self.vdom) {
                            let el = child.direct_id();
                            self.edit_push_root(el);
                            self.edit_insert_before(1);
                        }
                    } else {
                        eprintln!("key is not contained {:?}", key);
                        // new node needs to be created
                        // insert it before the current milestone
                        todo!();
                        // let meta = self.create_vnode(next_new);
                        // self.edit_insert_before(meta.added_to_stack);
                    }
                }
            }

            self.edit_pop();
        }

        let final_lis_node = root.unwrap();
        let final_el_node = self.find_last_element(final_lis_node);
        let final_el = final_el_node.direct_id();
        self.edit_push_root(final_el);

        let mut last_iter = new.iter().rev().enumerate();
        let last_key = final_lis_node.key().unwrap();
        loop {
            let (last_id, last_node) = last_iter.next().unwrap();
            let key = last_node.key().unwrap();

            eprintln!("checking final nodes {:?}", key);

            if last_key == key {
                eprintln!("breaking final nodes");
                break;
            }

            if shared_keys.contains(&key) {
                eprintln!("key is contained {:?}", key);
                shared_keys.remove(key);
                // diff the two nodes
                let old_node = load_old_node_from_lsi(key);
                self.diff_node(old_node, last_node);

                // now move all the nodes into the right spot
                for child in RealChildIterator::new(last_node, self.vdom) {
                    let el = child.direct_id();
                    self.edit_push_root(el);
                    self.edit_insert_after(1);
                }
            } else {
                eprintln!("key is not contained {:?}", key);
                // new node needs to be created
                // insert it before the current milestone
                todo!();
                // let meta = self.create_vnode(last_node);
                // self.edit_insert_after(meta.added_to_stack);
            }
        }
        self.edit_pop();
    }

    // Diff the suffix of keyed children that share the same keys in the same order.
    //
    // The parent must be on the change list stack when we enter this function:
    //
    //     [... parent]
    //
    // When this function exits, the change list stack remains the same.
    fn diff_keyed_suffix(
        &mut self,
        old: &'bump [VNode<'bump>],
        new: &'bump [VNode<'bump>],
        new_shared_suffix_start: usize,
    ) {
        debug_assert_eq!(old.len(), new.len());
        debug_assert!(!old.is_empty());

        for (old_child, new_child) in old.iter().zip(new.iter()) {
            self.diff_node(old_child, new_child);
        }
    }

    // Diff children that are not keyed.
    //
    // The parent must be on the top of the change list stack when entering this
    // function:
    //
    //     [... parent]
    //
    // the change list stack is in the same state when this function returns.
    async fn diff_non_keyed_children(
        &mut self,
        old: &'bump [VNode<'bump>],
        new: &'bump [VNode<'bump>],
    ) {
        // Handled these cases in `diff_children` before calling this function.
        //
        debug_assert!(!new.is_empty());
        debug_assert!(!old.is_empty());

        match old.len().cmp(&new.len()) {
            // old.len > new.len -> removing some nodes
            Ordering::Greater => {
                // diff them together
                for (new_child, old_child) in new.iter().zip(old.iter()) {
                    self.diff_node(old_child, new_child);
                }

                // todo: we would emit fewer instructions if we just did a replace many
                // remove whatever is still dangling
                for item in &old[new.len()..] {
                    for i in RealChildIterator::new(item, self.vdom) {
                        self.edit_push_root(i.direct_id());
                        self.edit_remove();
                    }
                }
            }

            // old.len < new.len -> adding some nodes
            // this is wrong in the case where we're diffing fragments
            //
            // we need to save the last old element and then replace it with all the new ones
            Ordering::Less => {
                // Add the new elements to the last old element while it still exists
                let last = self.find_last_element(old.last().unwrap());
                self.edit_push_root(last.direct_id());

                // create the rest and insert them
                todo!();
                // let meta = self.create_children(&new[old.len()..]);
                // self.edit_insert_after(meta.added_to_stack);

                self.edit_pop();

                // diff the rest
                for (new_child, old_child) in new.iter().zip(old.iter()) {
                    self.diff_node(old_child, new_child)
                }
            }

            // old.len == new.len -> no nodes added/removed, but perhaps changed
            Ordering::Equal => {
                for (new_child, old_child) in new.iter().zip(old.iter()) {
                    self.diff_node(old_child, new_child);
                }
            }
        }
    }

    // ======================
    // Support methods
    // ======================
    // Remove all of a node's children.
    //
    // The change list stack must have this shape upon entry to this function:
    //
    //     [... parent]
    //
    // When this function returns, the change list stack is in the same state.
    fn remove_all_children(&mut self, old: &'bump [VNode<'bump>]) {
        // debug_assert!(self.traversal_is_committed());
        log::debug!("REMOVING CHILDREN");
        for _child in old {
            // registry.remove_subtree(child);
        }
        // Fast way to remove all children: set the node's textContent to an empty
        // string.
        todo!()
        // self.set_inner_text("");
    }
    // Remove the current child and all of its following siblings.
    //
    // The change list stack must have this shape upon entry to this function:
    //
    //     [... parent child]
    //
    // After the function returns, the child is no longer on the change list stack:
    //
    //     [... parent]
    fn remove_children(&mut self, old: &'bump [VNode<'bump>]) {
        self.replace_and_create_many_with_many(old, None)
    }

    fn find_last_element(&mut self, vnode: &'bump VNode<'bump>) -> &'bump VNode<'bump> {
        let mut search_node = Some(vnode);

        loop {
            let node = search_node.take().unwrap();
            match &node {
                // the ones that have a direct id
                VNode::Text(_) | VNode::Element(_) | VNode::Anchor(_) | VNode::Suspended(_) => {
                    break node
                }

                VNode::Fragment(frag) => {
                    search_node = frag.children.last();
                }
                VNode::Component(el) => {
                    let scope_id = el.ass_scope.get().unwrap();
                    let scope = self.get_scope(&scope_id).unwrap();
                    search_node = Some(scope.root());
                }
            }
        }
    }

    fn find_first_element(&mut self, vnode: &'bump VNode<'bump>) -> &'bump VNode<'bump> {
        let mut search_node = Some(vnode);

        loop {
            let node = search_node.take().unwrap();
            match &node {
                // the ones that have a direct id
                VNode::Text(_) | VNode::Element(_) | VNode::Anchor(_) | VNode::Suspended(_) => {
                    break node
                }

                VNode::Fragment(frag) => {
                    search_node = Some(&frag.children[0]);
                }
                VNode::Component(el) => {
                    let scope_id = el.ass_scope.get().unwrap();
                    let scope = self.get_scope(&scope_id).unwrap();
                    search_node = Some(scope.root());
                }
            }
        }
    }

    fn remove_child(&mut self, node: &'bump VNode<'bump>) {
        self.replace_and_create_many_with_many(Some(node), None);
    }

    /// Remove all the old nodes and replace them with newly created new nodes.
    ///
    /// The new nodes *will* be created - don't create them yourself!
    fn replace_and_create_many_with_many(
        &mut self,
        old_nodes: impl IntoIterator<Item = &'bump VNode<'bump>>,
        new_nodes: impl IntoIterator<Item = &'bump VNode<'bump>>,
    ) {
        let mut nodes_to_replace = Vec::new();
        let mut nodes_to_search = old_nodes.into_iter().collect::<Vec<_>>();
        let mut scopes_obliterated = Vec::new();
        while let Some(node) = nodes_to_search.pop() {
            match &node {
                // the ones that have a direct id return immediately
                VNode::Text(el) => nodes_to_replace.push(el.dom_id.get().unwrap()),
                VNode::Element(el) => nodes_to_replace.push(el.dom_id.get().unwrap()),
                VNode::Anchor(el) => nodes_to_replace.push(el.dom_id.get().unwrap()),
                VNode::Suspended(el) => nodes_to_replace.push(el.node.get().unwrap()),

                // Fragments will either have a single anchor or a list of children
                VNode::Fragment(frag) => {
                    for child in frag.children {
                        nodes_to_search.push(child);
                    }
                }

                // Components can be any of the nodes above
                // However, we do need to track which components need to be removed
                VNode::Component(el) => {
                    let scope_id = el.ass_scope.get().unwrap();
                    let scope = self.get_scope(&scope_id).unwrap();
                    let root = scope.root();
                    nodes_to_search.push(root);
                    scopes_obliterated.push(scope_id);
                }
            }
            // TODO: enable internal garabge collection
            // self.create_garbage(node);
        }

        let n = nodes_to_replace.len();
        for node in nodes_to_replace {
            self.edit_push_root(node);
        }

        let mut nodes_created = 0;
        for node in new_nodes {
            todo!();
            // let meta = self.create_vnode(node);
            // nodes_created += meta.added_to_stack;
        }

        // if 0 nodes are created, then it gets interperted as a deletion
        self.edit_replace_with(n as u32, nodes_created);

        // obliterate!
        for scope in scopes_obliterated {
            self.destroy_scopes(scope);
        }
    }

    fn create_garbage(&mut self, node: &'bump VNode<'bump>) {
        match self.current_scope().and_then(|id| self.get_scope(&id)) {
            Some(scope) => {
                let garbage: &'bump VNode<'static> = unsafe { std::mem::transmute(node) };
                scope.pending_garbage.borrow_mut().push(garbage);
            }
            None => {
                log::info!("No scope to collect garbage into")
            }
        }
    }

    fn immediately_dispose_garabage(&mut self, node: ElementId) {
        self.vdom.collect_garbage(node)
    }

    fn replace_node_with_node(
        &mut self,
        anchor: ElementId,
        old_node: &'bump VNode<'bump>,
        new_node: &'bump VNode<'bump>,
    ) {
        self.edit_push_root(anchor);
        todo!();
        // let meta = self.create_vnode(new_node);
        // self.edit_replace_with(1, meta.added_to_stack);
        // self.create_garbage(old_node);
        self.edit_pop();
    }

    fn remove_vnode(&mut self, node: &'bump VNode<'bump>) {
        match &node {
            VNode::Text(el) => self.immediately_dispose_garabage(node.direct_id()),
            VNode::Element(el) => {
                self.immediately_dispose_garabage(node.direct_id());
                for child in el.children {
                    self.remove_vnode(&child);
                }
            }
            VNode::Anchor(a) => {
                //
            }
            VNode::Fragment(frag) => {
                for child in frag.children {
                    self.remove_vnode(&child);
                }
            }
            VNode::Component(el) => {
                //
                // self.destroy_scopes(old_scope)
            }
            VNode::Suspended(_) => todo!(),
        }
    }

    fn current_scope(&self) -> Option<ScopeId> {
        self.scope_stack.last().map(|f| f.clone())
    }

    fn fix_listener<'a>(&mut self, listener: &'a Listener<'a>) {
        let scope_id = self.current_scope();
        if let Some(scope_id) = scope_id {
            let scope = self.get_scope(&scope_id).unwrap();
            let mut queue = scope.listeners.borrow_mut();
            let long_listener: &'a Listener<'static> = unsafe { std::mem::transmute(listener) };
            queue.push(long_listener as *const _)
        }
    }

    pub fn get_scope_mut(&mut self, id: &ScopeId) -> Option<&'bump mut Scope> {
        // ensure we haven't seen this scope before
        // if we have, then we're trying to alias it, which is not allowed
        debug_assert!(!self.seen_scopes.contains(id));

        unsafe { self.vdom.get_scope_mut(*id) }
    }
    pub fn get_scope(&mut self, id: &ScopeId) -> Option<&'bump Scope> {
        // ensure we haven't seen this scope before
        // if we have, then we're trying to alias it, which is not allowed
        unsafe { self.vdom.get_scope(*id) }
    }

    // Navigation
    pub(crate) fn edit_push_root(&mut self, root: ElementId) {
        let id = root.as_u64();
        self.mutations.edits.push(PushRoot { id });
    }

    pub(crate) fn edit_pop(&mut self) {
        self.mutations.edits.push(PopRoot {});
    }

    // Add Nodes to the dom
    // add m nodes from the stack
    pub(crate) fn edit_append_children(&mut self, many: u32) {
        self.mutations.edits.push(AppendChildren { many });
    }

    // replace the n-m node on the stack with the m nodes
    // ends with the last element of the chain on the top of the stack
    pub(crate) fn edit_replace_with(&mut self, n: u32, m: u32) {
        self.mutations.edits.push(ReplaceWith { n, m });
    }

    pub(crate) fn edit_insert_after(&mut self, n: u32) {
        self.mutations.edits.push(InsertAfter { n });
    }

    pub(crate) fn edit_insert_before(&mut self, n: u32) {
        self.mutations.edits.push(InsertBefore { n });
    }

    // Remove Nodesfrom the dom
    pub(crate) fn edit_remove(&mut self) {
        self.mutations.edits.push(Remove);
    }

    // Create
    pub(crate) fn edit_create_text_node(&mut self, text: &'bump str, id: ElementId) {
        let id = id.as_u64();
        self.mutations.edits.push(CreateTextNode { text, id });
    }

    pub(crate) fn edit_create_element(
        &mut self,
        tag: &'static str,
        ns: Option<&'static str>,
        id: ElementId,
    ) {
        let id = id.as_u64();
        match ns {
            Some(ns) => self.mutations.edits.push(CreateElementNs { id, ns, tag }),
            None => self.mutations.edits.push(CreateElement { id, tag }),
        }
    }

    // placeholders are nodes that don't get rendered but still exist as an "anchor" in the real dom
    pub(crate) fn edit_create_placeholder(&mut self, id: ElementId) {
        let id = id.as_u64();
        self.mutations.edits.push(CreatePlaceholder { id });
    }

    // events
    pub(crate) fn edit_new_event_listener(&mut self, listener: &Listener, scope: ScopeId) {
        let Listener {
            event,
            mounted_node,
            ..
        } = listener;

        let element_id = mounted_node.get().unwrap().as_u64();

        self.mutations.edits.push(NewEventListener {
            scope,
            event_name: event,
            mounted_node_id: element_id,
        });
    }

    pub(crate) fn edit_remove_event_listener(&mut self, event: &'static str) {
        self.mutations.edits.push(RemoveEventListener { event });
    }

    // modify
    pub(crate) fn edit_set_text(&mut self, text: &'bump str) {
        self.mutations.edits.push(SetText { text });
    }

    pub(crate) fn edit_set_attribute(&mut self, attribute: &'bump Attribute) {
        let Attribute {
            name,
            value,
            is_static,
            is_volatile,
            namespace,
        } = attribute;
        // field: &'static str,
        // value: &'bump str,
        // ns: Option<&'static str>,
        self.mutations.edits.push(SetAttribute {
            field: name,
            value,
            ns: *namespace,
        });
    }

    pub(crate) fn edit_set_attribute_ns(
        &mut self,
        attribute: &'bump Attribute,
        namespace: &'bump str,
    ) {
        let Attribute {
            name,
            value,
            is_static,
            is_volatile,
            // namespace,
            ..
        } = attribute;
        // field: &'static str,
        // value: &'bump str,
        // ns: Option<&'static str>,
        self.mutations.edits.push(SetAttribute {
            field: name,
            value,
            ns: Some(namespace),
        });
    }

    pub(crate) fn edit_remove_attribute(&mut self, attribute: &Attribute) {
        let name = attribute.name;
        self.mutations.edits.push(RemoveAttribute { name });
    }
}

// When we create new nodes, we need to propagate some information back up the call chain.
// This gives the caller some information on how to handle things like insertins, appending, and subtree discarding.
#[derive(Debug)]
pub struct CreateMeta {
    pub is_static: bool,
    pub added_to_stack: u32,
}

impl CreateMeta {
    fn new(is_static: bool, added_to_tack: u32) -> Self {
        Self {
            is_static,
            added_to_stack: added_to_tack,
        }
    }
}

enum KeyedPrefixResult {
    // Fast path: we finished diffing all the children just by looking at the
    // prefix of shared keys!
    Finished,
    // There is more diffing work to do. Here is a count of how many children at
    // the beginning of `new` and `old` we already processed.
    MoreWorkToDo(usize),
}

fn find_first_real_node<'a>(
    nodes: impl IntoIterator<Item = &'a VNode<'a>>,
    scopes: &'a SharedResources,
) -> Option<&'a VNode<'a>> {
    for node in nodes {
        let mut iter = RealChildIterator::new(node, scopes);
        if let Some(node) = iter.next() {
            return Some(node);
        }
    }

    None
}

/// This iterator iterates through a list of virtual children and only returns real children (Elements, Text, Anchors).
///
/// This iterator is useful when it's important to load the next real root onto the top of the stack for operations like
/// "InsertBefore".
pub struct RealChildIterator<'a> {
    scopes: &'a SharedResources,

    // Heuristcally we should never bleed into 4 completely nested fragments/components
    // Smallvec lets us stack allocate our little stack machine so the vast majority of cases are sane
    // TODO: use const generics instead of the 4 estimation
    stack: smallvec::SmallVec<[(u16, &'a VNode<'a>); 4]>,
}

impl<'a> RealChildIterator<'a> {
    pub fn new(starter: &'a VNode<'a>, scopes: &'a SharedResources) -> Self {
        Self {
            scopes,
            stack: smallvec::smallvec![(0, starter)],
        }
    }
    // keep the memory around
    pub fn reset_with(&mut self, node: &'a VNode<'a>) {
        self.stack.clear();
        self.stack.push((0, node));
    }
}

impl<'a> Iterator for RealChildIterator<'a> {
    type Item = &'a VNode<'a>;

    fn next(&mut self) -> Option<&'a VNode<'a>> {
        let mut should_pop = false;
        let mut returned_node: Option<&'a VNode<'a>> = None;
        let mut should_push = None;

        while returned_node.is_none() {
            if let Some((count, node)) = self.stack.last_mut() {
                match &node {
                    // We can only exit our looping when we get "real" nodes
                    // This includes fragments and components when they're empty (have a single root)
                    VNode::Element(_) | VNode::Text(_) => {
                        // We've recursed INTO an element/text
                        // We need to recurse *out* of it and move forward to the next
                        should_pop = true;
                        returned_node = Some(&*node);
                    }

                    // If we get a fragment we push the next child
                    VNode::Fragment(frag) => {
                        let subcount = *count as usize;

                        if frag.children.len() == 0 {
                            should_pop = true;
                            returned_node = Some(&*node);
                        }

                        if subcount >= frag.children.len() {
                            should_pop = true;
                        } else {
                            should_push = Some(&frag.children[subcount]);
                        }
                    }
                    // // If we get a fragment we push the next child
                    // VNodeKind::Fragment(frag) => {
                    //     let subcount = *count as usize;

                    //     if frag.children.len() == 0 {
                    //         should_pop = true;
                    //         returned_node = Some(&*node);
                    //     }

                    //     if subcount >= frag.children.len() {
                    //         should_pop = true;
                    //     } else {
                    //         should_push = Some(&frag.children[subcount]);
                    //     }
                    // }

                    // Immediately abort suspended nodes - can't do anything with them yet
                    VNode::Suspended(node) => {
                        // VNodeKind::Suspended => should_pop = true,
                        todo!()
                    }

                    VNode::Anchor(a) => {
                        todo!()
                    }

                    // For components, we load their root and push them onto the stack
                    VNode::Component(sc) => {
                        let scope =
                            unsafe { self.scopes.get_scope(sc.ass_scope.get().unwrap()) }.unwrap();
                        // let scope = self.scopes.get(sc.ass_scope.get().unwrap()).unwrap();

                        // Simply swap the current node on the stack with the root of the component
                        *node = scope.frames.fin_head();
                    }
                }
            } else {
                // If there's no more items on the stack, we're done!
                return None;
            }

            if should_pop {
                self.stack.pop();
                if let Some((id, _)) = self.stack.last_mut() {
                    *id += 1;
                }
                should_pop = false;
            }

            if let Some(push) = should_push {
                self.stack.push((0, push));
                should_push = None;
            }
        }

        returned_node
    }
}

fn compare_strs(a: &str, b: &str) -> bool {
    // Check by pointer, optimizing for static strs
    if !std::ptr::eq(a, b) {
        // If the pointers are different then check by value
        a == b
    } else {
        true
    }
}

struct DfsIterator<'a> {
    idx: usize,
    node: Option<(&'a VNode<'a>, &'a VNode<'a>)>,
    nodes: Option<(&'a [VNode<'a>], &'a [VNode<'a>])>,
}
impl<'a> Iterator for DfsIterator<'a> {
    type Item = (&'a VNode<'a>, &'a VNode<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}
