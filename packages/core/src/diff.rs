use crate::{
    arena::ElementId,
    factory::RenderReturn,
    innerlude::{DirtyScope, VComponent, VFragment, VText},
    mutations::Mutation,
    nodes::{DynamicNode, VNode},
    scopes::ScopeId,
    virtual_dom::VirtualDom,
    AttributeValue, TemplateNode,
};

use fxhash::{FxHashMap, FxHashSet};
use std::cell::Cell;
use DynamicNode::*;

impl<'b> VirtualDom {
    pub(super) fn diff_scope(&mut self, scope: ScopeId) {
        let scope_state = &mut self.scopes[scope.0];

        self.scope_stack.push(scope);
        unsafe {
            // Load the old and new bump arenas
            let old = scope_state
                .previous_frame()
                .try_load_node()
                .expect("Call rebuild before diffing");
            let new = scope_state
                .current_frame()
                .try_load_node()
                .expect("Call rebuild before diffing");
            self.diff_maybe_node(old, new);
        }
        self.scope_stack.pop();
    }

    fn diff_maybe_node(&mut self, old: &'b RenderReturn<'b>, new: &'b RenderReturn<'b>) {
        use RenderReturn::{Async, Sync};
        match (old, new) {
            (Sync(Ok(l)), Sync(Ok(r))) => self.diff_node(l, r),

            // Err cases
            (Sync(Ok(l)), Sync(Err(e))) => self.diff_ok_to_err(l, e),
            (Sync(Err(e)), Sync(Ok(r))) => self.diff_err_to_ok(e, r),
            (Sync(Err(_eo)), Sync(Err(_en))) => { /* nothing */ }

            // Async
            (Sync(Ok(_l)), Async(_)) => todo!(),
            (Sync(Err(_e)), Async(_)) => todo!(),
            (Async(_), Sync(Ok(_r))) => todo!(),
            (Async(_), Sync(Err(_e))) => { /* nothing */ }
            (Async(_), Async(_)) => { /* nothing */ }
        }
    }

    fn diff_ok_to_err(&mut self, _l: &'b VNode<'b>, _e: &anyhow::Error) {
        todo!("Not yet handling error rollover")
    }
    fn diff_err_to_ok(&mut self, _e: &anyhow::Error, _l: &'b VNode<'b>) {
        todo!("Not yet handling error rollover")
    }

    fn diff_node(&mut self, left_template: &'b VNode<'b>, right_template: &'b VNode<'b>) {
        if left_template.template.id != right_template.template.id {
            return self.light_diff_templates(left_template, right_template);
        }

        for (left_attr, right_attr) in left_template
            .dynamic_attrs
            .iter()
            .zip(right_template.dynamic_attrs.iter())
        {
            // Move over the ID from the old to the new
            right_attr
                .mounted_element
                .set(left_attr.mounted_element.get());

            if left_attr.value != right_attr.value || left_attr.volatile {
                // todo: add more types of attribute values
                match right_attr.value {
                    AttributeValue::Text(text) => {
                        let name = unsafe { std::mem::transmute(left_attr.name) };
                        let value = unsafe { std::mem::transmute(text) };
                        self.mutations.push(Mutation::SetAttribute {
                            id: left_attr.mounted_element.get(),
                            ns: right_attr.namespace,
                            name,
                            value,
                        });
                    }
                    // todo: more types of attribute values
                    _ => todo!("other attribute types"),
                }
            }
        }

        for (idx, (left_node, right_node)) in left_template
            .dynamic_nodes
            .iter()
            .zip(right_template.dynamic_nodes.iter())
            .enumerate()
        {
            match (left_node, right_node) {
                (Text(left), Text(right)) => self.diff_vtext(left, right),
                (Fragment(left), Fragment(right)) => self.diff_vfragment(left, right),
                (Component(left), Component(right)) => {
                    self.diff_vcomponent(left, right, right_template, idx)
                }
                _ => self.replace(left_template, right_template),
            };
        }

        // Make sure the roots get transferred over
        for (left, right) in left_template
            .root_ids
            .iter()
            .zip(right_template.root_ids.iter())
        {
            right.set(left.get());
        }
    }

    fn diff_vcomponent(
        &mut self,
        left: &'b VComponent<'b>,
        right: &'b VComponent<'b>,
        right_template: &'b VNode<'b>,
        idx: usize,
    ) {
        // Replace components that have different render fns
        if left.render_fn != right.render_fn {
            dbg!(&left, &right);
            let created = self.create_component_node(right_template, right, idx);
            let head = unsafe {
                self.scopes[left.scope.get().unwrap().0]
                    .root_node()
                    .extend_lifetime_ref()
            };
            let id = match head {
                RenderReturn::Sync(Ok(node)) => self.replace_inner(node),
                _ => todo!(),
            };
            self.mutations
                .push(Mutation::ReplaceWith { id, m: created });
            return;
        }

        // Make sure the new vcomponent has the right scopeid associated to it
        let scope_id = left.scope.get().unwrap();
        right.scope.set(Some(scope_id));

        // copy out the box for both
        let old = self.scopes[scope_id.0].props.as_ref();
        let new = right.props.replace(None).unwrap();

        // If the props are static, then we try to memoize by setting the new with the old
        // The target scopestate still has the reference to the old props, so there's no need to update anything
        // This also implicitly drops the new props since they're not used
        if left.static_props && unsafe { old.memoize(new.as_ref()) } {
            return;
        }

        // If the props are dynamic *or* the memoization failed, then we need to diff the props

        // First, move over the props from the old to the new, dropping old props in the process
        self.scopes[scope_id.0].props = unsafe { std::mem::transmute(new.as_ref()) };
        right.props.set(Some(new));

        // Now run the component and diff it
        self.run_scope(scope_id);
        self.diff_scope(scope_id);
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
    fn light_diff_templates(&mut self, left: &'b VNode<'b>, right: &'b VNode<'b>) {
        match matching_components(left, right) {
            None => self.replace(left, right),
            Some(components) => components
                .into_iter()
                .enumerate()
                .for_each(|(idx, (l, r))| self.diff_vcomponent(l, r, right, idx)),
        }
    }

    /// Diff the two text nodes
    ///
    /// This just moves the ID of the old node over to the new node, and then sets the text of the new node if it's
    /// different.
    fn diff_vtext(&mut self, left: &'b VText<'b>, right: &'b VText<'b>) {
        let id = left.id.get();

        right.id.set(id);
        if left.value != right.value {
            let value = unsafe { std::mem::transmute(right.value) };
            self.mutations.push(Mutation::SetText { id, value });
        }
    }

    fn diff_vfragment(&mut self, left: &'b VFragment<'b>, right: &'b VFragment<'b>) {
        use VFragment::*;
        match (left, right) {
            (Empty(l), Empty(r)) => r.set(l.get()),
            (Empty(l), NonEmpty(r)) => self.replace_placeholder_with_nodes(l, r),
            (NonEmpty(l), Empty(r)) => self.replace_nodes_with_placeholder(l, r),
            (NonEmpty(old), NonEmpty(new)) => self.diff_non_empty_fragment(new, old),
        }
    }

    fn replace_placeholder_with_nodes(&mut self, l: &'b Cell<ElementId>, r: &'b [VNode<'b>]) {
        let m = self.create_children(r);
        let id = l.get();
        self.mutations.push(Mutation::ReplaceWith { id, m });
        self.reclaim(id);
    }

    fn replace_nodes_with_placeholder(&mut self, l: &'b [VNode<'b>], r: &'b Cell<ElementId>) {
        // Create the placeholder first, ensuring we get a dedicated ID for the placeholder
        let placeholder = self.next_element(&l[0], &[]);
        r.set(placeholder);
        self.mutations
            .push(Mutation::CreatePlaceholder { id: placeholder });

        // Remove the old nodes, except for onea
        let first = self.replace_inner(&l[0]);
        self.remove_nodes(&l[1..]);

        self.mutations
            .push(Mutation::ReplaceWith { id: first, m: 1 });

        self.try_reclaim(first);
    }

    /// Remove all the top-level nodes, returning the firstmost root ElementId
    ///
    /// All IDs will be garbage collected
    fn replace_inner(&mut self, node: &'b VNode<'b>) -> ElementId {
        let id = match node.dynamic_root(0) {
            None => node.root_ids[0].get(),
            Some(Text(t)) => t.id.get(),
            Some(Fragment(VFragment::Empty(e))) => e.get(),
            Some(Fragment(VFragment::NonEmpty(nodes))) => {
                let id = self.replace_inner(&nodes[0]);
                self.remove_nodes(&nodes[1..]);
                id
            }
            Some(Component(comp)) => {
                let scope = comp.scope.get().unwrap();
                match unsafe { self.scopes[scope.0].root_node().extend_lifetime_ref() } {
                    RenderReturn::Sync(Ok(t)) => self.replace_inner(t),
                    _ => todo!("cannot handle nonstandard nodes"),
                }
            }
        };

        // Just remove the rest from the dom
        for (idx, _) in node.template.roots.iter().enumerate().skip(1) {
            self.remove_root_node(node, idx);
        }

        // Garabge collect all of the nodes since this gets used in replace
        self.clean_up_node(node);

        id
    }

    /// Clean up the node, not generating mutations
    ///
    /// Simply walks through the dynamic nodes
    fn clean_up_node(&mut self, node: &'b VNode<'b>) {
        for (idx, dyn_node) in node.dynamic_nodes.iter().enumerate() {
            // Roots are cleaned up automatically?
            if node.template.node_paths[idx].len() == 1 {
                continue;
            }

            match dyn_node {
                Component(comp) => {
                    let scope = comp.scope.get().unwrap();
                    match unsafe { self.scopes[scope.0].root_node().extend_lifetime_ref() } {
                        RenderReturn::Sync(Ok(t)) => self.clean_up_node(t),
                        _ => todo!("cannot handle nonstandard nodes"),
                    };
                }
                Text(t) => self.reclaim(t.id.get()),
                Fragment(VFragment::Empty(t)) => self.reclaim(t.get()),
                Fragment(VFragment::NonEmpty(nodes)) => {
                    nodes.iter().for_each(|node| self.clean_up_node(node))
                }
            };
        }

        // we clean up nodes with dynamic attributes, provided the node is unique and not a root node
        let mut id = None;
        for (idx, attr) in node.dynamic_attrs.iter().enumerate() {
            // We'll clean up the root nodes either way, so don't worry
            if node.template.attr_paths[idx].len() == 1 {
                continue;
            }

            let next_id = attr.mounted_element.get();

            if id == Some(next_id) {
                continue;
            }

            id = Some(next_id);

            self.reclaim(next_id);
        }
    }

    fn remove_root_node(&mut self, node: &'b VNode<'b>, idx: usize) {
        match node.dynamic_root(idx) {
            Some(Text(i)) => {
                let id = i.id.get();
                self.mutations.push(Mutation::Remove { id });
                self.reclaim(id);
            }
            Some(Fragment(VFragment::Empty(e))) => {
                let id = e.get();
                self.mutations.push(Mutation::Remove { id });
                self.reclaim(id);
            }
            Some(Fragment(VFragment::NonEmpty(nodes))) => self.remove_nodes(nodes),
            Some(Component(comp)) => {
                let scope = comp.scope.get().unwrap();
                match unsafe { self.scopes[scope.0].root_node().extend_lifetime_ref() } {
                    RenderReturn::Sync(Ok(t)) => self.remove_node(t),
                    _ => todo!("cannot handle nonstandard nodes"),
                };
            }
            None => {
                let id = node.root_ids[idx].get();
                self.mutations.push(Mutation::Remove { id });
                self.reclaim(id);
            }
        };
    }

    fn diff_non_empty_fragment(&mut self, new: &'b [VNode<'b>], old: &'b [VNode<'b>]) {
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
            self.diff_keyed_children(old, new);
        } else {
            self.diff_non_keyed_children(old, new);
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
    fn diff_non_keyed_children(&mut self, old: &'b [VNode<'b>], new: &'b [VNode<'b>]) {
        use std::cmp::Ordering;

        // Handled these cases in `diff_children` before calling this function.
        debug_assert!(!new.is_empty());
        debug_assert!(!old.is_empty());

        match old.len().cmp(&new.len()) {
            Ordering::Greater => self.remove_nodes(&old[new.len()..]),
            Ordering::Less => self.create_and_insert_after(&new[old.len()..], old.last().unwrap()),
            Ordering::Equal => {}
        }

        for (new, old) in new.iter().zip(old.iter()) {
            self.diff_node(old, new);
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
    fn diff_keyed_children(&mut self, old: &'b [VNode<'b>], new: &'b [VNode<'b>]) {
        if cfg!(debug_assertions) {
            let mut keys = fxhash::FxHashSet::default();
            let mut assert_unique_keys = |children: &'b [VNode<'b>]| {
                keys.clear();
                for child in children {
                    let key = child.key;
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
        let (left_offset, right_offset) = match self.diff_keyed_ends(old, new) {
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
            self.remove_nodes(old_middle);
        } else if old_middle.is_empty() {
            // there were no old elements, so just create the new elements
            // we need to find the right "foothold" though - we shouldn't use the "append" at all
            if left_offset == 0 {
                // insert at the beginning of the old list
                let foothold = &old[old.len() - right_offset];
                self.create_and_insert_before(new_middle, foothold);
            } else if right_offset == 0 {
                // insert at the end  the old list
                let foothold = old.last().unwrap();
                self.create_and_insert_after(new_middle, foothold);
            } else {
                // inserting in the middle
                let foothold = &old[left_offset - 1];
                self.create_and_insert_after(new_middle, foothold);
            }
        } else {
            self.diff_keyed_middle(old_middle, new_middle);
        }
    }

    /// Diff both ends of the children that share keys.
    ///
    /// Returns a left offset and right offset of that indicates a smaller section to pass onto the middle diffing.
    ///
    /// If there is no offset, then this function returns None and the diffing is complete.
    fn diff_keyed_ends(
        &mut self,
        old: &'b [VNode<'b>],
        new: &'b [VNode<'b>],
    ) -> Option<(usize, usize)> {
        let mut left_offset = 0;

        for (old, new) in old.iter().zip(new.iter()) {
            // abort early if we finally run into nodes with different keys
            if old.key != new.key {
                break;
            }
            self.diff_node(old, new);
            left_offset += 1;
        }

        // If that was all of the old children, then create and append the remaining
        // new children and we're finished.
        if left_offset == old.len() {
            self.create_and_insert_after(&new[left_offset..], old.last().unwrap());
            return None;
        }

        // And if that was all of the new children, then remove all of the remaining
        // old children and we're finished.
        if left_offset == new.len() {
            self.remove_nodes(&old[left_offset..]);
            return None;
        }

        // if the shared prefix is less than either length, then we need to walk backwards
        let mut right_offset = 0;
        for (old, new) in old.iter().rev().zip(new.iter().rev()) {
            // abort early if we finally run into nodes with different keys
            if old.key != new.key {
                break;
            }
            self.diff_node(old, new);
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
    fn diff_keyed_middle(&mut self, old: &'b [VNode<'b>], new: &'b [VNode<'b>]) {
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
        debug_assert_ne!(new.first().map(|i| i.key), old.first().map(|i| i.key));
        debug_assert_ne!(new.last().map(|i| i.key), old.last().map(|i| i.key));

        // 1. Map the old keys into a numerical ordering based on indices.
        // 2. Create a map of old key to its index
        // IE if the keys were A B C, then we would have (A, 1) (B, 2) (C, 3).
        let old_key_to_old_index = old
            .iter()
            .enumerate()
            .map(|(i, o)| (o.key.unwrap(), i))
            .collect::<FxHashMap<_, _>>();

        let mut shared_keys = FxHashSet::default();

        // 3. Map each new key to the old key, carrying over the old index.
        let new_index_to_old_index = new
            .iter()
            .map(|node| {
                let key = node.key.unwrap();
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
            if old.get(0).is_some() {
                self.remove_nodes(&old[1..]);
                self.replace_many(&old[0], new);
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
            let key = child.key.unwrap();
            if !shared_keys.contains(&key) {
                self.remove_node(child);
            }
        }

        // 4. Compute the LIS of this list
        let mut lis_sequence = Vec::default();
        lis_sequence.reserve(new_index_to_old_index.len());

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
            self.diff_node(&old[new_index_to_old_index[*idx]], &new[*idx]);
        }

        let mut nodes_created = 0;

        // add mount instruction for the first items not covered by the lis
        let last = *lis_sequence.last().unwrap();
        if last < (new.len() - 1) {
            for (idx, new_node) in new[(last + 1)..].iter().enumerate() {
                let new_idx = idx + last + 1;
                let old_index = new_index_to_old_index[new_idx];
                if old_index == u32::MAX as usize {
                    nodes_created += self.create(new_node);
                } else {
                    self.diff_node(&old[old_index], new_node);
                    nodes_created += self.push_all_real_nodes(new_node);
                }
            }

            let id = self.find_last_element(&new[last]);
            self.mutations.push(Mutation::InsertAfter {
                id,
                m: nodes_created,
            });
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
                        nodes_created += self.create(new_node);
                    } else {
                        self.diff_node(&old[old_index], new_node);
                        nodes_created += self.push_all_real_nodes(new_node);
                    }
                }

                let id = self.find_first_element(&new[last]);
                self.mutations.push(Mutation::InsertBefore {
                    id,
                    m: nodes_created,
                });

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
                    nodes_created += self.create(new_node);
                } else {
                    self.diff_node(&old[old_index], new_node);
                    nodes_created += self.push_all_real_nodes(new_node);
                }
            }

            let id = self.find_first_element(&new[first_lis]);
            self.mutations.push(Mutation::InsertBefore {
                id,
                m: nodes_created,
            });
        }
    }

    /// Remove these nodes from the dom
    /// Wont generate mutations for the inner nodes
    fn remove_nodes(&mut self, nodes: &'b [VNode<'b>]) {
        nodes.iter().for_each(|node| self.remove_node(node));
    }

    fn remove_node(&mut self, node: &'b VNode<'b>) {
        for (idx, _) in node.template.roots.iter().enumerate() {
            let id = match node.dynamic_root(idx) {
                Some(Text(t)) => t.id.get(),
                Some(Fragment(VFragment::Empty(t))) => t.get(),
                Some(Fragment(VFragment::NonEmpty(t))) => return self.remove_nodes(t),
                Some(Component(comp)) => return self.remove_component(comp.scope.get().unwrap()),
                None => node.root_ids[idx].get(),
            };

            self.mutations.push(Mutation::Remove { id })
        }

        self.clean_up_node(node);

        for root in node.root_ids {
            let id = root.get();
            if id.0 != 0 {
                self.reclaim(id);
            }
        }
    }

    fn remove_component(&mut self, scope_id: ScopeId) {
        let height = self.scopes[scope_id.0].height;
        self.dirty_scopes.remove(&DirtyScope {
            height,
            id: scope_id,
        });

        // I promise, since we're descending down the tree, this is safe
        match unsafe { self.scopes[scope_id.0].root_node().extend_lifetime_ref() } {
            RenderReturn::Sync(Ok(t)) => self.remove_node(t),
            _ => todo!("cannot handle nonstandard nodes"),
        }
    }

    /// Push all the real nodes on the stack
    fn push_all_real_nodes(&mut self, node: &'b VNode<'b>) -> usize {
        let mut onstack = 0;

        for (idx, _) in node.template.roots.iter().enumerate() {
            match node.dynamic_root(idx) {
                Some(Text(t)) => {
                    self.mutations.push(Mutation::PushRoot { id: t.id.get() });
                    onstack += 1;
                }
                Some(Fragment(VFragment::Empty(t))) => {
                    self.mutations.push(Mutation::PushRoot { id: t.get() });
                    onstack += 1;
                }
                Some(Fragment(VFragment::NonEmpty(t))) => {
                    for node in *t {
                        onstack += self.push_all_real_nodes(node);
                    }
                }
                Some(Component(comp)) => {
                    let scope = comp.scope.get().unwrap();
                    onstack +=
                        match unsafe { self.scopes[scope.0].root_node().extend_lifetime_ref() } {
                            RenderReturn::Sync(Ok(node)) => self.push_all_real_nodes(node),
                            _ => todo!(),
                        }
                }
                None => {
                    self.mutations.push(Mutation::PushRoot {
                        id: node.root_ids[idx].get(),
                    });
                    onstack += 1;
                }
            };
        }

        onstack
    }

    fn create_children(&mut self, nodes: &'b [VNode<'b>]) -> usize {
        nodes.iter().fold(0, |acc, child| acc + self.create(child))
    }

    fn create_and_insert_before(&mut self, new: &'b [VNode<'b>], before: &'b VNode<'b>) {
        let m = self.create_children(new);
        let id = self.find_first_element(before);
        self.mutations.push(Mutation::InsertBefore { id, m })
    }

    fn create_and_insert_after(&mut self, new: &'b [VNode<'b>], after: &'b VNode<'b>) {
        let m = self.create_children(new);
        let id = self.find_last_element(after);
        self.mutations.push(Mutation::InsertAfter { id, m })
    }

    fn find_first_element(&self, node: &'b VNode<'b>) -> ElementId {
        match node.dynamic_root(0) {
            None => node.root_ids[0].get(),
            Some(Text(t)) => t.id.get(),
            Some(Fragment(VFragment::NonEmpty(t))) => self.find_first_element(&t[0]),
            Some(Fragment(VFragment::Empty(t))) => t.get(),
            Some(Component(comp)) => {
                let scope = comp.scope.get().unwrap();
                match unsafe { self.scopes[scope.0].root_node().extend_lifetime_ref() } {
                    RenderReturn::Sync(Ok(t)) => self.find_first_element(t),
                    _ => todo!("cannot handle nonstandard nodes"),
                }
            }
        }
    }

    fn find_last_element(&self, node: &'b VNode<'b>) -> ElementId {
        match node.dynamic_root(node.template.roots.len() - 1) {
            None => node.root_ids.last().unwrap().get(),
            Some(Text(t)) => t.id.get(),
            Some(Fragment(VFragment::NonEmpty(t))) => self.find_last_element(t.last().unwrap()),
            Some(Fragment(VFragment::Empty(t))) => t.get(),
            Some(Component(comp)) => {
                let scope = comp.scope.get().unwrap();
                match unsafe { self.scopes[scope.0].root_node().extend_lifetime_ref() } {
                    RenderReturn::Sync(Ok(t)) => self.find_last_element(t),
                    _ => todo!("cannot handle nonstandard nodes"),
                }
            }
        }
    }

    fn replace(&mut self, left: &'b VNode<'b>, right: &'b VNode<'b>) {
        let first = self.find_first_element(left);
        let id = self.replace_inner(left);
        let created = self.create(right);
        self.mutations.push(Mutation::ReplaceWith {
            id: first,
            m: created,
        });
        self.try_reclaim(id);
    }

    fn replace_many(&mut self, left: &'b VNode<'b>, right: &'b [VNode<'b>]) {
        let first = self.find_first_element(left);
        let id = self.replace_inner(left);
        let created = self.create_children(right);
        self.mutations.push(Mutation::ReplaceWith {
            id: first,
            m: created,
        });
        self.try_reclaim(id);
    }
}

fn matching_components<'a>(
    left: &'a VNode<'a>,
    right: &'a VNode<'a>,
) -> Option<Vec<(&'a VComponent<'a>, &'a VComponent<'a>)>> {
    if left.template.roots.len() != right.template.roots.len() {
        return None;
    }

    // run through the components, ensuring they're the same
    left.template
        .roots
        .iter()
        .zip(right.template.roots.iter())
        .map(|(l, r)| {
            let (l, r) = match (l, r) {
                (TemplateNode::Dynamic(l), TemplateNode::Dynamic(r)) => (l, r),
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
    let path = node.template.node_paths[idx];

    // use a loop to index every static node's children until the path has run out
    // only break if the last path index is a dynamic node
    let mut static_node = &node.template.roots[path[0] as usize];

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
