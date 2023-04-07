//! A tree of nodes intigated with shipyard

use crate::NodeId;
use shipyard::{Component, EntitiesViewMut, Get, View, ViewMut};
use std::fmt::Debug;

/// A subtree of a tree.
#[derive(PartialEq, Eq, Clone, Debug, Component)]
pub struct Subtree {
    /// The root of the subtree
    shadow_roots: Vec<NodeId>,
    /// The node that children of the super tree should be inserted under.
    slot: Option<NodeId>,
    /// The node in the super tree that the subtree is attached to.
    super_tree_root: NodeId,
}

/// A node in a tree.
#[derive(PartialEq, Eq, Clone, Debug, Component)]
pub struct Node {
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    child_subtree: Option<Subtree>,
    /// If this node is a slot in a subtree, this is node whose child_subtree is that subtree.
    slot_for_supertree: Option<NodeId>,
    height: u16,
}

/// A view of a tree.
pub type TreeRefView<'a> = View<'a, Node>;
/// A mutable view of a tree.
pub type TreeMutView<'a> = (EntitiesViewMut<'a>, ViewMut<'a, Node>);

/// A immutable view of a tree.
pub trait TreeRef {
    /// The parent id of the node.
    fn parent_id(&self, id: NodeId) -> Option<NodeId>;
    /// The children ids of the node.
    fn children_ids(&self, id: NodeId) -> Vec<NodeId>;
    /// The subtree tree under the node.
    fn subtree(&self, id: NodeId) -> Option<&Subtree>;
    /// The height of the node.
    fn height(&self, id: NodeId) -> Option<u16>;
    /// Returns true if the node exists.
    fn contains(&self, id: NodeId) -> bool;
}

/// A mutable view of a tree.
pub trait TreeMut: TreeRef {
    /// Removes the node and its children from the tree but do not delete the entities.
    fn remove(&mut self, id: NodeId);
    /// Adds a new node to the tree.
    fn create_node(&mut self, id: NodeId);
    /// Adds a child to the node.
    fn add_child(&mut self, parent: NodeId, new: NodeId);
    /// Replaces the node with a new node.
    fn replace(&mut self, old_id: NodeId, new_id: NodeId);
    /// Inserts a node before another node.
    fn insert_before(&mut self, old_id: NodeId, new_id: NodeId);
    /// Inserts a node after another node.
    fn insert_after(&mut self, old_id: NodeId, new_id: NodeId);
    /// Creates a new subtree.
    fn create_subtree(&mut self, id: NodeId, shadow_roots: Vec<NodeId>, slot: Option<NodeId>);
}

impl<'a> TreeRef for TreeRefView<'a> {
    fn parent_id(&self, id: NodeId) -> Option<NodeId> {
        self.get(id).ok()?.parent
    }

    fn children_ids(&self, id: NodeId) -> Vec<NodeId> {
        self.get(id)
            .map(|node| node.children.clone())
            .unwrap_or_default()
    }

    fn height(&self, id: NodeId) -> Option<u16> {
        Some(self.get(id).ok()?.height)
    }

    fn contains(&self, id: NodeId) -> bool {
        self.get(id).is_ok()
    }

    fn subtree(&self, id: NodeId) -> Option<&Subtree> {
        self.get(id).ok()?.child_subtree.as_ref()
    }
}

impl<'a> TreeMut for TreeMutView<'a> {
    fn remove(&mut self, id: NodeId) {
        fn recurse(tree: &mut TreeMutView<'_>, id: NodeId) {
            let (supertree, children) = {
                let node = (&mut tree.1).get(id).unwrap();
                (node.slot_for_supertree, std::mem::take(&mut node.children))
            };

            for child in children {
                recurse(tree, child);
            }

            // If this node is a slot in a subtree, remove it from the subtree.
            if let Some(supertree) = supertree {
                let supertree_root = (&mut tree.1).get(supertree).unwrap();

                if let Some(subtree) = &mut supertree_root.child_subtree {
                    subtree.slot = None;
                }

                debug_assert!(
                    supertree_root.children.is_empty(),
                    "Subtree root should have no children when slot is removed."
                );
            }
        }

        {
            let mut node_data_mut = &mut self.1;
            if let Some(parent) = node_data_mut.get(id).unwrap().parent {
                let parent = (&mut node_data_mut).get(parent).unwrap();
                parent.children.retain(|&child| child != id);
            }
        }

        recurse(self, id);
    }

    fn create_node(&mut self, id: NodeId) {
        let (entities, node_data_mut) = self;
        entities.add_component(
            id,
            node_data_mut,
            Node {
                parent: None,
                children: Vec::new(),
                height: 0,
                child_subtree: None,
                slot_for_supertree: None,
            },
        );
    }

    fn add_child(&mut self, parent: NodeId, new: NodeId) {
        {
            let mut node_state = &mut self.1;
            (&mut node_state).get(new).unwrap().parent = Some(parent);
            let parent = (&mut node_state).get(parent).unwrap();
            parent.children.push(new);
        }
        let height = child_height((&self.1).get(parent).unwrap(), self);
        set_height(self, new, height);
    }

    fn replace(&mut self, old_id: NodeId, new_id: NodeId) {
        {
            let mut node_state = &mut self.1;
            // update the parent's link to the child
            if let Some(parent_id) = node_state.get(old_id).unwrap().parent {
                let parent = (&mut node_state).get(parent_id).unwrap();
                for id in &mut parent.children {
                    if *id == old_id {
                        *id = new_id;
                        break;
                    }
                }
                let height = child_height((&self.1).get(parent_id).unwrap(), self);
                set_height(self, new_id, height);
            }
        }
        self.remove(old_id);
    }

    fn insert_before(&mut self, old_id: NodeId, new_id: NodeId) {
        let parent_id = {
            let old_node = self.1.get(old_id).unwrap();
            old_node.parent.expect("tried to insert before root")
        };
        {
            (&mut self.1).get(new_id).unwrap().parent = Some(parent_id);
        }
        let parent = (&mut self.1).get(parent_id).unwrap();
        let index = parent
            .children
            .iter()
            .position(|child| *child == old_id)
            .unwrap();
        parent.children.insert(index, new_id);
        let height = child_height((&self.1).get(parent_id).unwrap(), self);
        set_height(self, new_id, height);
    }

    fn insert_after(&mut self, old_id: NodeId, new_id: NodeId) {
        let mut node_state = &mut self.1;
        let old_node = node_state.get(old_id).unwrap();
        let parent_id = old_node.parent.expect("tried to insert before root");
        (&mut node_state).get(new_id).unwrap().parent = Some(parent_id);
        let parent = (&mut node_state).get(parent_id).unwrap();
        let index = parent
            .children
            .iter()
            .position(|child| *child == old_id)
            .unwrap();
        parent.children.insert(index + 1, new_id);
        let height = child_height((&self.1).get(parent_id).unwrap(), self);
        set_height(self, new_id, height);
    }

    fn create_subtree(&mut self, id: NodeId, shadow_roots: Vec<NodeId>, slot: Option<NodeId>) {
        let (_, node_data_mut) = self;

        let light_root_height;
        {
            let subtree = Subtree {
                super_tree_root: id,
                shadow_roots: shadow_roots.clone(),
                slot,
            };

            let light_root = node_data_mut
                .get(id)
                .expect("tried to create subtree with non-existent id");

            light_root.child_subtree = Some(subtree);
            light_root_height = light_root.height;

            if let Some(slot) = slot {
                let slot = node_data_mut
                    .get(slot)
                    .expect("tried to create subtree with non-existent slot");
                slot.slot_for_supertree = Some(id);
            }
        }

        // Now that we have created the subtree, we need to update the height of the subtree roots
        for root in shadow_roots {
            set_height(self, root, light_root_height + 1);
        }
    }
}

fn child_height(parent: &Node, tree: &impl TreeRef) -> u16 {
    match &parent.child_subtree {
        Some(subtree) => {
            if let Some(slot) = subtree.slot {
                tree.height(slot)
                    .expect("Attempted to read a slot that does not exist")
                    + 1
            } else {
                panic!("Attempted to read the height of a subtree without a slot");
            }
        }
        None => parent.height + 1,
    }
}

/// Sets the height of a node and updates the height of all its children
fn set_height(tree: &mut TreeMutView<'_>, node: NodeId, height: u16) {
    let (subtree, supertree, children) = {
        let mut node_data_mut = &mut tree.1;
        let mut node = (&mut node_data_mut).get(node).unwrap();
        node.height = height;

        (
            node.child_subtree.clone(),
            node.slot_for_supertree,
            node.children.clone(),
        )
    };

    // If the children are actually part of a subtree, there height is determined by the height of the subtree
    if let Some(subtree) = subtree {
        // Set the height of the subtree roots
        for &shadow_root in &subtree.shadow_roots {
            set_height(tree, shadow_root, height);
        }
    } else {
        // Otherwise, we just set the height of the children to be one more than the height of the parent
        for child in children {
            set_height(tree, child, height + 1);
        }
    }

    // If this nodes is a slot for a subtree, we need to go to the super tree and update the height of its children
    if let Some(supertree) = supertree {
        let children = (&tree.1).get(supertree).unwrap().children.clone();
        for child in children {
            set_height(tree, child, height + 1);
        }
    }
}

impl<'a> TreeRef for TreeMutView<'a> {
    fn parent_id(&self, id: NodeId) -> Option<NodeId> {
        let node_data = &self.1;
        node_data.get(id).unwrap().parent
    }

    fn children_ids(&self, id: NodeId) -> Vec<NodeId> {
        let node_data = &self.1;
        node_data
            .get(id)
            .map(|node| node.children.clone())
            .unwrap_or_default()
    }

    fn height(&self, id: NodeId) -> Option<u16> {
        let node_data = &self.1;
        node_data.get(id).map(|node| node.height).ok()
    }

    fn contains(&self, id: NodeId) -> bool {
        self.1.get(id).is_ok()
    }

    fn subtree(&self, id: NodeId) -> Option<&Subtree> {
        let node_data = &self.1;
        node_data.get(id).unwrap().child_subtree.as_ref()
    }
}

#[test]
fn creation() {
    use shipyard::World;
    #[derive(Component)]
    struct Num(i32);

    let mut world = World::new();
    let parent_id = world.add_entity(Num(1i32));
    let child_id = world.add_entity(Num(0i32));

    let mut tree = world.borrow::<TreeMutView>().unwrap();

    tree.create_node(parent_id);
    tree.create_node(child_id);

    tree.add_child(parent_id, child_id);

    assert_eq!(tree.height(parent_id), Some(0));
    assert_eq!(tree.height(child_id), Some(1));
    assert_eq!(tree.parent_id(parent_id), None);
    assert_eq!(tree.parent_id(child_id).unwrap(), parent_id);
    assert_eq!(tree.children_ids(parent_id), &[child_id]);
}

#[test]
fn subtree_creation() {
    use shipyard::World;
    #[derive(Component)]
    struct Num(i32);

    let mut world = World::new();
    // Create main tree
    let parent_id = world.add_entity(Num(1i32));
    let child_id = world.add_entity(Num(0i32));

    // Create shadow tree
    let shadow_parent_id = world.add_entity(Num(2i32));
    let shadow_child_id = world.add_entity(Num(3i32));

    let mut tree = world.borrow::<TreeMutView>().unwrap();

    tree.create_node(parent_id);
    tree.create_node(child_id);

    tree.add_child(parent_id, child_id);

    tree.create_node(shadow_parent_id);
    tree.create_node(shadow_child_id);

    tree.add_child(shadow_parent_id, shadow_child_id);

    assert_eq!(tree.height(parent_id), Some(0));
    assert_eq!(tree.height(child_id), Some(1));
    assert_eq!(tree.parent_id(parent_id), None);
    assert_eq!(tree.parent_id(child_id).unwrap(), parent_id);
    assert_eq!(tree.children_ids(parent_id), &[child_id]);

    assert_eq!(tree.height(shadow_parent_id), Some(0));
    assert_eq!(tree.height(shadow_child_id), Some(1));
    assert_eq!(tree.parent_id(shadow_parent_id), None);
    assert_eq!(tree.parent_id(shadow_child_id).unwrap(), shadow_parent_id);
    assert_eq!(tree.children_ids(shadow_parent_id), &[shadow_child_id]);

    // Add shadow tree to main tree
    tree.create_subtree(parent_id, vec![shadow_parent_id], Some(shadow_child_id));

    assert_eq!(tree.height(parent_id), Some(0));
    assert_eq!(tree.height(shadow_parent_id), Some(1));
    assert_eq!(tree.height(shadow_child_id), Some(2));
    assert_eq!(tree.height(child_id), Some(3));
}

#[test]
fn insertion() {
    use shipyard::World;
    #[derive(Component)]
    struct Num(i32);

    let mut world = World::new();
    let parent = world.add_entity(Num(0));
    let child = world.add_entity(Num(2));
    let before = world.add_entity(Num(1));
    let after = world.add_entity(Num(3));

    let mut tree = world.borrow::<TreeMutView>().unwrap();

    tree.create_node(parent);
    tree.create_node(child);
    tree.create_node(before);
    tree.create_node(after);

    tree.add_child(parent, child);
    tree.insert_before(child, before);
    tree.insert_after(child, after);

    assert_eq!(tree.height(parent), Some(0));
    assert_eq!(tree.height(child), Some(1));
    assert_eq!(tree.height(before), Some(1));
    assert_eq!(tree.height(after), Some(1));
    assert_eq!(tree.parent_id(before).unwrap(), parent);
    assert_eq!(tree.parent_id(child).unwrap(), parent);
    assert_eq!(tree.parent_id(after).unwrap(), parent);
    assert_eq!(tree.children_ids(parent), &[before, child, after]);
}

#[test]
fn deletion() {
    use shipyard::World;
    #[derive(Component)]
    struct Num(i32);

    let mut world = World::new();
    let parent = world.add_entity(Num(0));
    let child = world.add_entity(Num(2));
    let before = world.add_entity(Num(1));
    let after = world.add_entity(Num(3));

    let mut tree = world.borrow::<TreeMutView>().unwrap();

    tree.create_node(parent);
    tree.create_node(child);
    tree.create_node(before);
    tree.create_node(after);

    tree.add_child(parent, child);
    tree.insert_before(child, before);
    tree.insert_after(child, after);

    assert_eq!(tree.height(parent), Some(0));
    assert_eq!(tree.height(child), Some(1));
    assert_eq!(tree.height(before), Some(1));
    assert_eq!(tree.height(after), Some(1));
    assert_eq!(tree.parent_id(before).unwrap(), parent);
    assert_eq!(tree.parent_id(child).unwrap(), parent);
    assert_eq!(tree.parent_id(after).unwrap(), parent);
    assert_eq!(tree.children_ids(parent), &[before, child, after]);

    tree.remove(child);

    assert_eq!(tree.height(parent), Some(0));
    assert_eq!(tree.height(before), Some(1));
    assert_eq!(tree.height(after), Some(1));
    assert_eq!(tree.parent_id(before).unwrap(), parent);
    assert_eq!(tree.parent_id(after).unwrap(), parent);
    assert_eq!(tree.children_ids(parent), &[before, after]);

    tree.remove(before);

    assert_eq!(tree.height(parent), Some(0));
    assert_eq!(tree.height(after), Some(1));
    assert_eq!(tree.parent_id(after).unwrap(), parent);
    assert_eq!(tree.children_ids(parent), &[after]);

    tree.remove(after);

    assert_eq!(tree.height(parent), Some(0));
    assert_eq!(tree.children_ids(parent), &[]);
}
