use std::ops::Deref;

use crate::{
    any_props::AnyProps,
    arena::ElementId,
    innerlude::{
        DirtyScope, ElementPath, ElementRef, VComponent, VPlaceholder, VText, WriteMutations,
    },
    nodes::RenderReturn,
    nodes::{DynamicNode, VNode},
    scopes::ScopeId,
    virtual_dom::VirtualDom,
    Attribute, TemplateNode,
};

use rustc_hash::{FxHashMap, FxHashSet};
use DynamicNode::*;

impl VirtualDom {
    pub(super) fn diff_scope(
        &mut self,
        scope: ScopeId,
        new_nodes: RenderReturn,
        to: &mut impl WriteMutations,
    ) {
        self.runtime.scope_stack.borrow_mut().push(scope);
        let scope_state = &mut self.scopes[scope.0];
        // Load the old and new bump arenas
        let new = &new_nodes;
        let old = scope_state.last_rendered_node.take().unwrap();

        use RenderReturn::{Aborted, Ready};

        match (&old, new) {
            // Normal pathway
            (Ready(l), Ready(r)) => self.diff_node(l, r, to),

            // Unwind the mutations if need be
            (Ready(l), Aborted(p)) => self.diff_ok_to_err(l, p, to),

            // Just move over the placeholder
            (Aborted(l), Aborted(r)) => {
                r.id.set(l.id.get());
                *r.parent.borrow_mut() = l.parent.borrow().clone();
            }

            // Placeholder becomes something
            // We should also clear the error now
            (Aborted(l), Ready(r)) => {
                let parent = l.parent.take();
                self.replace_placeholder(
                    l,
                    [r],
                    parent.as_ref().expect("root node should not be none"),
                    to,
                )
            }
        };

        let scope_state = &mut self.scopes[scope.0];
        scope_state.last_rendered_node = Some(new_nodes);

        self.runtime.scope_stack.borrow_mut().pop();
    }

    fn diff_ok_to_err(&mut self, l: &VNode, p: &VPlaceholder, to: &mut impl WriteMutations) {
        let id = self.next_element();
        p.id.set(Some(id));
        *p.parent.borrow_mut() = l.parent.borrow().clone();
        to.create_placeholder(id);

        to.insert_nodes_before(id, 1);

        // TODO: Instead of *just* removing it, we can use the replace mutation
        self.remove_node(l, true, to);
    }

    fn diff_node(
        &mut self,
        left_template: &VNode,
        right_template: &VNode,
        to: &mut impl WriteMutations,
    ) {
        // If hot reloading is enabled, we need to make sure we're using the latest template
        #[cfg(debug_assertions)]
        {
            let (path, byte_index) = right_template.template.get().name.rsplit_once(':').unwrap();
            if let Some(map) = self.templates.get(path) {
                let byte_index = byte_index.parse::<usize>().unwrap();
                if let Some(&template) = map.get(&byte_index) {
                    right_template.template.set(template);
                    if template != left_template.template.get() {
                        let parent = left_template.parent.take();
                        let parent = parent.as_ref();
                        return self.replace(left_template, [right_template], parent, to);
                    }
                }
            }
        }

        // Copy over the parent
        {
            *right_template.parent.borrow_mut() = left_template.parent.borrow().clone();
        }

        // If the templates are the same, we don't need to do anything, nor do we want to
        if templates_are_the_same(left_template, right_template) {
            return;
        }

        // If the templates are different by name, we need to replace the entire template
        if templates_are_different(left_template, right_template) {
            return self.light_diff_templates(left_template, right_template, to);
        }

        // If the templates are the same, we can diff the attributes and children
        // Start with the attributes
        left_template
            .dynamic_attrs
            .iter()
            .zip(right_template.dynamic_attrs.iter())
            .for_each(|(left_attr, right_attr)| {
                // Move over the ID from the old to the new
                let mounted_element = left_attr.mounted_element.get();
                right_attr.mounted_element.set(mounted_element);

                // If the attributes are different (or volatile), we need to update them
                if left_attr.value != right_attr.value || left_attr.volatile {
                    self.update_attribute(right_attr, left_attr, to);
                }
            });

        // Now diff the dynamic nodes
        left_template
            .dynamic_nodes
            .iter()
            .zip(right_template.dynamic_nodes.iter())
            .enumerate()
            .for_each(|(dyn_node_idx, (left_node, right_node))| {
                let current_ref = ElementRef {
                    element: right_template.clone(),
                    path: ElementPath {
                        path: left_template.template.get().node_paths[dyn_node_idx],
                    },
                };
                self.diff_dynamic_node(left_node, right_node, &current_ref, to);
            });

        // Make sure the roots get transferred over while we're here
        {
            let mut right = right_template.root_ids.borrow_mut();
            let left = left_template.root_ids.borrow();
            for (from, into) in left.iter().zip(right.iter_mut()) {
                *into = *from;
            }
        }
    }

    fn diff_dynamic_node(
        &mut self,
        left_node: &DynamicNode,
        right_node: &DynamicNode,
        parent: &ElementRef,
        to: &mut impl WriteMutations,
    ) {
        match (left_node, right_node) {
            (Text(left), Text(right)) => self.diff_vtext(left, right, to),
            (Fragment(left), Fragment(right)) => self.diff_non_empty_fragment(left, right, parent, to),
            (Placeholder(left), Placeholder(right)) => {
                right.id.set(left.id.get());
                *right.parent.borrow_mut() = left.parent.borrow().clone();
            },
            (Component(left), Component(right)) => self.diff_vcomponent(left, right, Some(parent), to),
            (Placeholder(left), Fragment(right)) => self.replace_placeholder(left, right, parent, to),
            (Fragment(left), Placeholder(right)) => self.node_to_placeholder(left, right, parent, to),
            _ => todo!("This is an usual custom case for dynamic nodes. We don't know how to handle it yet."),
        };
    }

    fn update_attribute(
        &mut self,
        right_attr: &Attribute,
        left_attr: &Attribute,
        to: &mut impl WriteMutations,
    ) {
        let name = &left_attr.name;
        let value = &right_attr.value;
        to.set_attribute(
            name,
            right_attr.namespace,
            value,
            left_attr.mounted_element.get(),
        );
    }

    fn diff_vcomponent(
        &mut self,
        left: &VComponent,
        right: &VComponent,
        parent: Option<&ElementRef>,
        to: &mut impl WriteMutations,
    ) {
        if std::ptr::eq(left, right) {
            return;
        }

        // Replace components that have different render fns
        if left.render_fn != right.render_fn {
            return self.replace_vcomponent(right, left, parent, to);
        }

        // Make sure the new vcomponent has the right scopeid associated to it
        let scope_id = left.scope.get().unwrap();

        right.scope.set(Some(scope_id));

        // copy out the box for both
        let old_scope = &self.scopes[scope_id.0];
        let old = old_scope.props.deref();
        let new: &dyn AnyProps = right.props.deref();

        // If the props are static, then we try to memoize by setting the new with the old
        // The target scopestate still has the reference to the old props, so there's no need to update anything
        // This also implicitly drops the new props since they're not used
        if old.memoize(new.props()) {
            tracing::trace!(
                "Memoized props for component {:#?} ({})",
                scope_id,
                old_scope.context().name
            );
            return;
        }

        // First, move over the props from the old to the new, dropping old props in the process
        self.scopes[scope_id.0].props = right.props.clone();

        // Now run the component and diff it
        let new = self.run_scope(scope_id);
        self.diff_scope(scope_id, new, to);

        self.dirty_scopes.remove(&DirtyScope {
            height: self.runtime.get_context(scope_id).unwrap().height,
            id: scope_id,
        });
    }

    fn replace_vcomponent(
        &mut self,
        right: &VComponent,
        left: &VComponent,
        parent: Option<&ElementRef>,
        to: &mut impl WriteMutations,
    ) {
        let _m = self.create_component_node(parent, right, to);

        // TODO: Instead of *just* removing it, we can use the replace mutation
        self.remove_component_node(left, true, to);

        todo!()
    }

    /// Lightly diff the two templates, checking only their roots.
    ///
    /// The goal here is to preserve any existing component state that might exist. This is to preserve some React-like
    /// behavior where the component state is preserved when the component is re-rendered.
    ///
    /// This is implemented by iterating each root, checking if the component is the same, if it is, then diff it.
    ///
    /// We then pass the new template through "create" which should be smart enough to skip roots.
    ///
    /// Currently, we only handle the case where the roots are the same component list. If there's any sort of deviation,
    /// IE more nodes, less nodes, different nodes, or expressions, then we just replace the whole thing.
    ///
    /// This is mostly implemented to help solve the issue where the same component is rendered under two different
    /// conditions:
    ///
    /// ```rust, ignore
    /// if enabled {
    ///     rsx!{ Component { enabled_sign: "abc" } }
    /// } else {
    ///     rsx!{ Component { enabled_sign: "xyz" } }
    /// }
    /// ```
    ///
    /// However, we should not that it's explicit in the docs that this is not a guarantee. If you need to preserve state,
    /// then you should be passing in separate props instead.
    ///
    /// ```rust, ignore
    /// let props = if enabled {
    ///     ComponentProps { enabled_sign: "abc" }
    /// } else {
    ///     ComponentProps { enabled_sign: "xyz" }
    /// };
    ///
    /// rsx! {
    ///     Component { ..props }
    /// }
    /// ```
    fn light_diff_templates(&mut self, left: &VNode, right: &VNode, to: &mut impl WriteMutations) {
        let parent = left.parent.take();
        let parent = parent.as_ref();
        match matching_components(left, right) {
            None => self.replace(left, [right], parent, to),
            Some(components) => components
                .into_iter()
                .for_each(|(l, r)| self.diff_vcomponent(l, r, parent, to)),
        }
    }

    /// Diff the two text nodes
    ///
    /// This just moves the ID of the old node over to the new node, and then sets the text of the new node if it's
    /// different.
    fn diff_vtext(&mut self, left: &VText, right: &VText, to: &mut impl WriteMutations) {
        let id = left.id.get().unwrap_or_else(|| self.next_element());

        right.id.set(Some(id));
        if left.value != right.value {
            to.set_node_text(&right.value, id);
        }
    }

    fn diff_non_empty_fragment(
        &mut self,
        old: &[VNode],
        new: &[VNode],
        parent: &ElementRef,
        to: &mut impl WriteMutations,
    ) {
        let new_is_keyed = new[0].key.is_some();
        let old_is_keyed = old[0].key.is_some();
        debug_assert!(
            new.iter().all(|n| n.key.is_some() == new_is_keyed),
            "all siblings must be keyed or all siblings must be non-keyed"
        );
        debug_assert!(
            old.iter().all(|o| o.key.is_some() == old_is_keyed),
            "all siblings must be keyed or all siblings must be non-keyed"
        );

        if new_is_keyed && old_is_keyed {
            self.diff_keyed_children(old, new, parent, to);
        } else {
            self.diff_non_keyed_children(old, new, parent, to);
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
    fn diff_non_keyed_children(
        &mut self,
        old: &[VNode],
        new: &[VNode],
        parent: &ElementRef,
        to: &mut impl WriteMutations,
    ) {
        use std::cmp::Ordering;

        // Handled these cases in `diff_children` before calling this function.
        debug_assert!(!new.is_empty());
        debug_assert!(!old.is_empty());

        match old.len().cmp(&new.len()) {
            Ordering::Greater => self.remove_nodes(&old[new.len()..], to),
            Ordering::Less => {
                self.create_and_insert_after(&new[old.len()..], old.last().unwrap(), parent, to)
            }
            Ordering::Equal => {}
        }

        for (new, old) in new.iter().zip(old.iter()) {
            self.diff_node(old, new, to);
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
    fn diff_keyed_children(
        &mut self,
        old: &[VNode],
        new: &[VNode],
        parent: &ElementRef,
        to: &mut impl WriteMutations,
    ) {
        if cfg!(debug_assertions) {
            let mut keys = rustc_hash::FxHashSet::default();
            let mut assert_unique_keys = |children: &[VNode]| {
                keys.clear();
                for child in children {
                    let key = child.key.clone();
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
        let (left_offset, right_offset) = match self.diff_keyed_ends(old, new, parent, to) {
            Some(count) => count,
            None => return,
        };

        // Ok, we now hopefully have a smaller range of children in the middle
        // within which to re-order nodes with the same keys, remove old nodes with
        // now-unused keys, and create new nodes with fresh keys.

        let old_middle = &old[left_offset..(old.len() - right_offset)];
        let new_middle = &new[left_offset..(new.len() - right_offset)];

        debug_assert!(
            !((old_middle.len() == new_middle.len()) && old_middle.is_empty()),
            "keyed children must have the same number of children"
        );

        if new_middle.is_empty() {
            // remove the old elements
            self.remove_nodes(old_middle, to);
        } else if old_middle.is_empty() {
            // there were no old elements, so just create the new elements
            // we need to find the right "foothold" though - we shouldn't use the "append" at all
            if left_offset == 0 {
                // insert at the beginning of the old list
                let foothold = &old[old.len() - right_offset];
                self.create_and_insert_before(new_middle, foothold, parent, to);
            } else if right_offset == 0 {
                // insert at the end  the old list
                let foothold = old.last().unwrap();
                self.create_and_insert_after(new_middle, foothold, parent, to);
            } else {
                // inserting in the middle
                let foothold = &old[left_offset - 1];
                self.create_and_insert_after(new_middle, foothold, parent, to);
            }
        } else {
            self.diff_keyed_middle(old_middle, new_middle, parent, to);
        }
    }

    /// Diff both ends of the children that share keys.
    ///
    /// Returns a left offset and right offset of that indicates a smaller section to pass onto the middle diffing.
    ///
    /// If there is no offset, then this function returns None and the diffing is complete.
    fn diff_keyed_ends(
        &mut self,
        old: &[VNode],
        new: &[VNode],
        parent: &ElementRef,
        to: &mut impl WriteMutations,
    ) -> Option<(usize, usize)> {
        let mut left_offset = 0;

        for (old, new) in old.iter().zip(new.iter()) {
            // abort early if we finally run into nodes with different keys
            if old.key != new.key {
                break;
            }
            self.diff_node(old, new, to);
            left_offset += 1;
        }

        // If that was all of the old children, then create and append the remaining
        // new children and we're finished.
        if left_offset == old.len() {
            self.create_and_insert_after(&new[left_offset..], old.last().unwrap(), parent, to);
            return None;
        }

        // And if that was all of the new children, then remove all of the remaining
        // old children and we're finished.
        if left_offset == new.len() {
            self.remove_nodes(&old[left_offset..], to);
            return None;
        }

        // if the shared prefix is less than either length, then we need to walk backwards
        let mut right_offset = 0;
        for (old, new) in old.iter().rev().zip(new.iter().rev()) {
            // abort early if we finally run into nodes with different keys
            if old.key != new.key {
                break;
            }
            self.diff_node(old, new, to);
            right_offset += 1;
        }

        Some((left_offset, right_offset))
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
    // Upon exit from this function, it will be restored to that same self.
    #[allow(clippy::too_many_lines)]
    fn diff_keyed_middle(
        &mut self,
        old: &[VNode],
        new: &[VNode],
        parent: &ElementRef,
        to: &mut impl WriteMutations,
    ) {
        /*
        1. Map the old keys into a numerical ordering based on indices.
        2. Create a map of old key to its index
        3. Map each new key to the old key, carrying over the old index.
            - IE if we have ABCD becomes BACD, our sequence would be 1,0,2,3
            - if we have ABCD to ABDE, our sequence would be 0,1,3,MAX because E doesn't exist

        now, we should have a list of integers that indicates where in the old list the new items map to.

        4. Compute the LIS of this list
            - this indicates the longest list of new children that won't need to be moved.

        5. Identify which nodes need to be removed
        6. Identify which nodes will need to be diffed

        7. Going along each item in the new list, create it and insert it before the next closest item in the LIS.
            - if the item already existed, just move it to the right place.

        8. Finally, generate instructions to remove any old children.
        9. Generate instructions to finally diff children that are the same between both
        */
        // 0. Debug sanity checks
        // Should have already diffed the shared-key prefixes and suffixes.
        debug_assert_ne!(new.first().map(|i| &i.key), old.first().map(|i| &i.key));
        debug_assert_ne!(new.last().map(|i| &i.key), old.last().map(|i| &i.key));

        // 1. Map the old keys into a numerical ordering based on indices.
        // 2. Create a map of old key to its index
        // IE if the keys were A B C, then we would have (A, 1) (B, 2) (C, 3).
        let old_key_to_old_index = old
            .iter()
            .enumerate()
            .map(|(i, o)| (o.key.as_ref().unwrap(), i))
            .collect::<FxHashMap<_, _>>();

        let mut shared_keys = FxHashSet::default();

        // 3. Map each new key to the old key, carrying over the old index.
        let new_index_to_old_index = new
            .iter()
            .map(|node| {
                let key = node.key.as_ref().unwrap();
                if let Some(&index) = old_key_to_old_index.get(&key) {
                    shared_keys.insert(key);
                    index
                } else {
                    u32::MAX as usize
                }
            })
            .collect::<Vec<_>>();

        // If none of the old keys are reused by the new children, then we remove all the remaining old children and
        // create the new children afresh.
        if shared_keys.is_empty() {
            if !old.is_empty() {
                self.remove_nodes(&old[1..], to);
                self.replace(&old[0], new, Some(parent), to);
            } else {
                // I think this is wrong - why are we appending?
                // only valid of the if there are no trailing elements
                // self.create_and_append_children(new);

                todo!("we should never be appending - just creating N");
            }
            return;
        }

        // remove any old children that are not shared
        // todo: make this an iterator
        for child in old {
            let key = child.key.as_ref().unwrap();
            if !shared_keys.contains(&key) {
                self.remove_node(child, true, to);
            }
        }

        // 4. Compute the LIS of this list
        let mut lis_sequence = Vec::with_capacity(new_index_to_old_index.len());

        let mut predecessors = vec![0; new_index_to_old_index.len()];
        let mut starts = vec![0; new_index_to_old_index.len()];

        longest_increasing_subsequence::lis_with(
            &new_index_to_old_index,
            &mut lis_sequence,
            |a, b| a < b,
            &mut predecessors,
            &mut starts,
        );

        // the lis comes out backwards, I think. can't quite tell.
        lis_sequence.sort_unstable();

        // if a new node gets u32 max and is at the end, then it might be part of our LIS (because u32 max is a valid LIS)
        if lis_sequence.last().map(|f| new_index_to_old_index[*f]) == Some(u32::MAX as usize) {
            lis_sequence.pop();
        }

        for idx in &lis_sequence {
            self.diff_node(&old[new_index_to_old_index[*idx]], &new[*idx], to);
        }

        let mut nodes_created = 0;

        // add mount instruction for the first items not covered by the lis
        let last = *lis_sequence.last().unwrap();
        if last < (new.len() - 1) {
            for (idx, new_node) in new[(last + 1)..].iter().enumerate() {
                let new_idx = idx + last + 1;
                let old_index = new_index_to_old_index[new_idx];
                if old_index == u32::MAX as usize {
                    nodes_created += self.create(new_node, to);
                } else {
                    self.diff_node(&old[old_index], new_node, to);
                    nodes_created += self.push_all_real_nodes(new_node, to);
                }
            }

            let id = self.find_last_element(&new[last]);
            if nodes_created > 0 {
                to.insert_nodes_after(id, nodes_created)
            }
            nodes_created = 0;
        }

        // for each spacing, generate a mount instruction
        let mut lis_iter = lis_sequence.iter().rev();
        let mut last = *lis_iter.next().unwrap();
        for next in lis_iter {
            if last - next > 1 {
                for (idx, new_node) in new[(next + 1)..last].iter().enumerate() {
                    let new_idx = idx + next + 1;
                    let old_index = new_index_to_old_index[new_idx];
                    if old_index == u32::MAX as usize {
                        nodes_created += self.create(new_node, to);
                    } else {
                        self.diff_node(&old[old_index], new_node, to);
                        nodes_created += self.push_all_real_nodes(new_node, to);
                    }
                }

                let id = self.find_first_element(&new[last]);
                if nodes_created > 0 {
                    to.insert_nodes_before(id, nodes_created);
                }

                nodes_created = 0;
            }
            last = *next;
        }

        // add mount instruction for the last items not covered by the lis
        let first_lis = *lis_sequence.first().unwrap();
        if first_lis > 0 {
            for (idx, new_node) in new[..first_lis].iter().enumerate() {
                let old_index = new_index_to_old_index[idx];
                if old_index == u32::MAX as usize {
                    nodes_created += self.create(new_node, to);
                } else {
                    self.diff_node(&old[old_index], new_node, to);
                    nodes_created += self.push_all_real_nodes(new_node, to);
                }
            }

            let id = self.find_first_element(&new[first_lis]);
            if nodes_created > 0 {
                to.insert_nodes_before(id, nodes_created);
            }
        }
    }

    /// Push all the real nodes on the stack
    fn push_all_real_nodes(&self, node: &VNode, to: &mut impl WriteMutations) -> usize {
        node.template
            .get()
            .roots
            .iter()
            .enumerate()
            .map(|(idx, _)| {
                let node = match node.dynamic_root(idx) {
                    Some(node) => node,
                    None => {
                        to.push_root(node.root_ids.borrow()[idx]);
                        return 1;
                    }
                };

                match node {
                    Text(t) => {
                        to.push_root(t.id.get().unwrap());
                        1
                    }
                    Placeholder(t) => {
                        to.push_root(t.id.get().unwrap());
                        1
                    }
                    Fragment(nodes) => nodes
                        .iter()
                        .map(|node| self.push_all_real_nodes(node, to))
                        .sum(),

                    Component(comp) => {
                        let scope = comp.scope.get().unwrap();
                        match self.get_scope(scope).unwrap().root_node() {
                            RenderReturn::Ready(node) => self.push_all_real_nodes(node, to),
                            RenderReturn::Aborted(_node) => todo!(),
                        }
                    }
                }
            })
            .sum()
    }

    pub(crate) fn create_children<'a>(
        &mut self,
        nodes: impl IntoIterator<Item = &'a VNode>,
        parent: Option<&ElementRef>,
        to: &mut impl WriteMutations,
    ) -> usize {
        nodes
            .into_iter()
            .map(|child| {
                self.assign_boundary_ref(parent, child);
                self.create(child, to)
            })
            .sum()
    }

    fn create_and_insert_before(
        &mut self,
        new: &[VNode],
        before: &VNode,
        parent: &ElementRef,
        to: &mut impl WriteMutations,
    ) {
        let m = self.create_children(new, Some(parent), to);
        let id = self.find_first_element(before);
        to.insert_nodes_before(id, m);
    }

    fn create_and_insert_after(
        &mut self,
        new: &[VNode],
        after: &VNode,
        parent: &ElementRef,
        to: &mut impl WriteMutations,
    ) {
        let m = self.create_children(new, Some(parent), to);
        let id = self.find_last_element(after);
        to.insert_nodes_after(id, m);
    }

    /// Simply replace a placeholder with a list of nodes
    fn replace_placeholder<'a>(
        &mut self,
        l: &VPlaceholder,
        r: impl IntoIterator<Item = &'a VNode>,
        parent: &ElementRef,
        to: &mut impl WriteMutations,
    ) {
        let m = self.create_children(r, Some(parent), to);
        let id = l.id.get().unwrap();
        to.replace_node_with(id, m);
        self.reclaim(id);
    }

    fn replace<'a>(
        &mut self,
        left: &VNode,
        right: impl IntoIterator<Item = &'a VNode>,
        parent: Option<&ElementRef>,
        to: &mut impl WriteMutations,
    ) {
        let m = self.create_children(right, parent, to);

        // TODO: Instead of *just* removing it, we can use the replace mutation
        to.insert_nodes_before(self.find_first_element(left), m);

        self.remove_node(left, true, to);
    }

    fn node_to_placeholder(
        &mut self,
        l: &[VNode],
        r: &VPlaceholder,
        parent: &ElementRef,
        to: &mut impl WriteMutations,
    ) {
        // Create the placeholder first, ensuring we get a dedicated ID for the placeholder
        let placeholder = self.next_element();

        r.id.set(Some(placeholder));
        r.parent.borrow_mut().replace(parent.clone());

        to.create_placeholder(placeholder);

        self.replace_nodes(l, 1, to);
    }

    /// Replace many nodes with a number of nodes on the stack
    fn replace_nodes(&mut self, nodes: &[VNode], m: usize, to: &mut impl WriteMutations) {
        // We want to optimize the replace case to use one less mutation if possible
        // Since mutations are done in reverse, the last node removed will be the first in the stack
        // TODO: Instead of *just* removing it, we can use the replace mutation
        to.insert_nodes_before(self.find_first_element(&nodes[0]), m);

        debug_assert!(
            !nodes.is_empty(),
            "replace_nodes must have at least one node"
        );

        self.remove_nodes(nodes, to);
    }

    /// Remove these nodes from the dom
    /// Wont generate mutations for the inner nodes
    fn remove_nodes(&mut self, nodes: &[VNode], to: &mut impl WriteMutations) {
        nodes
            .iter()
            .rev()
            .for_each(|node| self.remove_node(node, true, to));
    }

    fn remove_node(&mut self, node: &VNode, gen_muts: bool, to: &mut impl WriteMutations) {
        // Clean up any attributes that have claimed a static node as dynamic for mount/unmounta
        // Will not generate mutations!
        self.reclaim_attributes(node);

        // Remove the nested dynamic nodes
        // We don't generate mutations for these, as they will be removed by the parent (in the next line)
        // But we still need to make sure to reclaim them from the arena and drop their hooks, etc
        self.remove_nested_dyn_nodes(node, to);

        // Clean up the roots, assuming we need to generate mutations for these
        // This is done last in order to preserve Node ID reclaim order (reclaim in reverse order of claim)
        self.reclaim_roots(node, gen_muts, to);
    }

    fn reclaim_roots(&mut self, node: &VNode, gen_muts: bool, to: &mut impl WriteMutations) {
        for (idx, _) in node.template.get().roots.iter().enumerate() {
            if let Some(dy) = node.dynamic_root(idx) {
                self.remove_dynamic_node(dy, gen_muts, to);
            } else {
                let id = node.root_ids.borrow()[idx];
                if gen_muts {
                    to.remove_node(id);
                }
                self.reclaim(id);
            }
        }
    }

    fn reclaim_attributes(&mut self, node: &VNode) {
        let mut id = None;
        for (idx, attr) in node.dynamic_attrs.iter().enumerate() {
            // We'll clean up the root nodes either way, so don't worry
            let path_len = node
                .template
                .get()
                .attr_paths
                .get(idx)
                .map(|path| path.len());
            // if the path is 1 the attribute is in the root, so we don't need to clean it up
            // if the path is 0, the attribute is a not attached at all, so we don't need to clean it up

            if let Some(len) = path_len {
                if (..=1).contains(&len) {
                    continue;
                }
            }

            let next_id = attr.mounted_element.get();

            if id == Some(next_id) {
                continue;
            }

            id = Some(next_id);

            self.reclaim(next_id);
        }
    }

    fn remove_nested_dyn_nodes(&mut self, node: &VNode, to: &mut impl WriteMutations) {
        for (idx, dyn_node) in node.dynamic_nodes.iter().enumerate() {
            let path_len = node
                .template
                .get()
                .node_paths
                .get(idx)
                .map(|path| path.len());
            // Roots are cleaned up automatically above and nodes with a empty path are placeholders
            if let Some(2..) = path_len {
                self.remove_dynamic_node(dyn_node, false, to)
            }
        }
    }

    fn remove_dynamic_node(
        &mut self,
        node: &DynamicNode,
        gen_muts: bool,
        to: &mut impl WriteMutations,
    ) {
        match node {
            Component(comp) => self.remove_component_node(comp, gen_muts, to),
            Text(t) => self.remove_text_node(t, gen_muts, to),
            Placeholder(t) => self.remove_placeholder(t, gen_muts, to),
            Fragment(nodes) => nodes
                .iter()
                .for_each(|node| self.remove_node(node, gen_muts, to)),
        };
    }

    fn remove_placeholder(
        &mut self,
        t: &VPlaceholder,
        gen_muts: bool,
        to: &mut impl WriteMutations,
    ) {
        if let Some(id) = t.id.take() {
            if gen_muts {
                to.remove_node(id);
            }
            self.reclaim(id)
        }
    }

    fn remove_text_node(&mut self, t: &VText, gen_muts: bool, to: &mut impl WriteMutations) {
        if let Some(id) = t.id.take() {
            if gen_muts {
                to.remove_node(id);
            }
            self.reclaim(id)
        }
    }

    fn remove_component_node(
        &mut self,
        comp: &VComponent,
        gen_muts: bool,
        to: &mut impl WriteMutations,
    ) {
        // Remove the component reference from the vcomponent so they're not tied together
        let scope = comp
            .scope
            .take()
            .expect("VComponents to always have a scope");

        // Remove the component from the dom
        match self.scopes[scope.0].last_rendered_node.take().unwrap() {
            RenderReturn::Ready(t) => self.remove_node(&t, gen_muts, to),
            RenderReturn::Aborted(placeholder) => {
                self.remove_placeholder(&placeholder, gen_muts, to)
            }
        };

        // Now drop all the resources
        self.drop_scope(scope);
    }

    fn find_first_element(&self, node: &VNode) -> ElementId {
        match node.dynamic_root(0) {
            None => node.root_ids.borrow()[0],
            Some(Text(t)) => t.id.get().unwrap(),
            Some(Fragment(t)) => self.find_first_element(&t[0]),
            Some(Placeholder(t)) => t.id.get().unwrap(),
            Some(Component(comp)) => {
                let scope = comp.scope.get().unwrap();
                match self.get_scope(scope).unwrap().root_node() {
                    RenderReturn::Ready(t) => self.find_first_element(t),
                    _ => todo!("cannot handle nonstandard nodes"),
                }
            }
        }
    }

    fn find_last_element(&self, node: &VNode) -> ElementId {
        match node.dynamic_root(node.template.get().roots.len() - 1) {
            None => *node.root_ids.borrow().last().unwrap(),
            Some(Text(t)) => t.id.get().unwrap(),
            Some(Fragment(t)) => self.find_last_element(t.last().unwrap()),
            Some(Placeholder(t)) => t.id.get().unwrap(),
            Some(Component(comp)) => {
                let scope = comp.scope.get().unwrap();
                match self.get_scope(scope).unwrap().root_node() {
                    RenderReturn::Ready(t) => self.find_last_element(t),
                    _ => todo!("cannot handle nonstandard nodes"),
                }
            }
        }
    }

    pub(crate) fn assign_boundary_ref(&mut self, parent: Option<&ElementRef>, child: &VNode) {
        if let Some(parent) = parent {
            // assign the parent of the child
            child.parent.borrow_mut().replace(parent.clone());
        }
    }
}

/// Are the templates the same?
///
/// We need to check for the obvious case, and the non-obvious case where the template as cloned
///
/// We use the pointer of the dynamic_node list in this case
fn templates_are_the_same(left_template: &VNode, right_template: &VNode) -> bool {
    std::ptr::eq(left_template, right_template)
}

fn templates_are_different(left_template: &VNode, right_template: &VNode) -> bool {
    let left_template_name = left_template.template.get().name;
    let right_template_name = right_template.template.get().name;
    // we want to re-create the node if the template name is different by pointer even if the value is the same so that we can detect when hot reloading changes the template
    !std::ptr::eq(left_template_name, right_template_name)
}

fn matching_components<'a>(
    left: &'a VNode,
    right: &'a VNode,
) -> Option<Vec<(&'a VComponent, &'a VComponent)>> {
    let left_template = left.template.get();
    let right_template = right.template.get();
    if left_template.roots.len() != right_template.roots.len() {
        return None;
    }

    // run through the components, ensuring they're the same
    left_template
        .roots
        .iter()
        .zip(right_template.roots.iter())
        .map(|(l, r)| {
            let (l, r) = match (l, r) {
                (TemplateNode::Dynamic { id: l }, TemplateNode::Dynamic { id: r }) => (l, r),
                _ => return None,
            };

            let (l, r) = match (&left.dynamic_nodes[*l], &right.dynamic_nodes[*r]) {
                (Component(l), Component(r)) => (l, r),
                _ => return None,
            };

            Some((l, r))
        })
        .collect()
}

/// We can apply various optimizations to dynamic nodes that are the single child of their parent.
///
/// IE
///  - for text - we can use SetTextContent
///  - for clearning children we can use RemoveChildren
///  - for appending children we can use AppendChildren
#[allow(dead_code)]
fn is_dyn_node_only_child(node: &VNode, idx: usize) -> bool {
    let template = node.template.get();
    let path = template.node_paths[idx];

    // use a loop to index every static node's children until the path has run out
    // only break if the last path index is a dynamic node
    let mut static_node = &template.roots[path[0] as usize];

    for i in 1..path.len() - 1 {
        match static_node {
            TemplateNode::Element { children, .. } => static_node = &children[path[i] as usize],
            _ => return false,
        }
    }

    match static_node {
        TemplateNode::Element { children, .. } => children.len() == 1,
        _ => false,
    }
}
