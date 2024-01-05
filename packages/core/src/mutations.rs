use rustc_hash::FxHashSet;

use crate::{arena::ElementId, AttributeValue, ScopeId, Template};

/// Something that can handle the mutations that are generated by the diffing process and apply them to the Real DOM
///
/// This object provides a bunch of important information for a renderer to use patch the Real Dom with the state of the
/// VirtualDom. This includes the scopes that were modified, the templates that were discovered, and a list of changes
/// in the form of a [`Mutation`].
///
/// These changes are specific to one subtree, so to patch multiple subtrees, you'd need to handle each set separately.
///
/// Templates, however, apply to all subtrees, not just target subtree.
///
/// Mutations are the only link between the RealDOM and the VirtualDOM.
pub trait WriteMutations {
    /// Register a template with the renderer
    fn register_template(&mut self, template: Template<'static>);

    /// Add these m children to the target element
    ///
    /// Id: The ID of the element being mounted to
    /// M: The number of nodes on the stack to append to the target element
    fn append_children(&mut self, id: ElementId, m: usize);

    /// Assign the element at the given path the target ElementId.
    ///
    /// The path is in the form of a list of indices based on children. Templates cannot have more than 255 children per
    /// element, hence the use of a single byte.
    ///
    /// Path: The path of the child of the topmost node on the stack. A path of `[]` represents the topmost node. A path of `[0]` represents the first child. `[0,1,2]` represents 1st child's 2nd child's 3rd child.
    /// Id: The ID we're assigning to this element/placeholder. This will be used later to modify the element or replace it with another element.
    fn assign_node_id(&mut self, path: &'static [u8], id: ElementId);

    /// Create a placeholder in the DOM that we will use later.
    ///
    /// Dioxus currently requires the use of placeholders to maintain a re-entrance point for things like list diffing
    ///
    /// Id: The ID we're assigning to this element/placeholder. This will be used later to modify the element or replace it with another element.
    fn create_placeholder(&mut self, id: ElementId);

    /// Create a node specifically for text with the given value
    ///
    /// Value: The text content of this text node
    /// Id: The ID we're assigning to this specific text nodes. This will be used later to modify the element or replace it with another element.
    fn create_text_node(&mut self, value: &str, id: ElementId);

    /// Hydrate an existing text node at the given path with the given text.
    ///
    /// Assign this text node the given ID since we will likely need to modify this text at a later point
    ///
    /// Path: The path of the child of the topmost node on the stack. A path of `[]` represents the topmost node. A path of `[0]` represents the first child. `[0,1,2]` represents 1st child's 2nd child's 3rd child.
    /// Value: The value of the textnode that we want to set the placeholder with
    /// Id: The ID we're assigning to this specific text nodes. This will be used later to modify the element or replace it with another element.
    fn hydrate_text_node(&mut self, path: &'static [u8], value: &str, id: ElementId);

    /// Load and clone an existing node from a template saved under that specific name
    ///
    /// Dioxus guarantees that the renderer will have already been provided the template.
    /// When the template is picked up in the template list, it should be saved under its "name" - here, the name
    ///
    /// Name: The unique "name" of the template based on the template location. When paired with `rsx!`, this is autogenerated
    /// Index: The index root we loading from the template. The template is stored as a list of nodes. This index represents the position of that root
    /// Id: The ID we're assigning to this element being loaded from the template (This will be used later to move the element around in lists)
    fn load_template(&mut self, name: &'static str, index: usize, id: ElementId);

    /// Replace the target element (given by its ID) with the topmost m nodes on the stack
    ///
    /// id: The ID of the node we're going to replace with new nodes
    /// m: The number of nodes on the stack to replace the target element with
    fn replace_node_with(&mut self, id: ElementId, m: usize);

    /// Replace an existing element in the template at the given path with the m nodes on the stack
    ///
    /// Path: The path of the child of the topmost node on the stack. A path of `[]` represents the topmost node. A path of `[0]` represents the first child. `[0,1,2]` represents 1st child's 2nd child's 3rd child.
    /// M: The number of nodes on the stack to replace the target element with
    fn replace_placeholder_with_nodes(&mut self, path: &'static [u8], m: usize);

    /// Insert a number of nodes after a given node.
    ///
    /// Id: The ID of the node to insert after.
    /// M: The number of nodes on the stack to insert after the target node.
    fn insert_nodes_after(&mut self, id: ElementId, m: usize);

    /// Insert a number of nodes before a given node.
    ///
    /// Id: The ID of the node to insert before.
    /// M: The number of nodes on the stack to insert before the target node.
    fn insert_nodes_before(&mut self, id: ElementId, m: usize);

    /// Set the value of a node's attribute.
    ///
    /// Name: The name of the attribute to set.
    /// NS: The (optional) namespace of the attribute. For instance, "style" is in the "style" namespace.
    /// Value: The value of the attribute.
    /// Id: The ID of the node to set the attribute of.
    fn set_attribute(
        &mut self,
        name: &'static str,
        ns: Option<&'static str>,
        value: &AttributeValue,
        id: ElementId,
    );

    /// Set the text content of a node.
    ///
    /// Value: The textcontent of the node
    /// Id: The ID of the node to set the textcontent of.
    fn set_node_text(&mut self, value: &str, id: ElementId);

    /// Create a new Event Listener.
    ///
    /// Name: The name of the event to listen for.
    /// Id: The ID of the node to attach the listener to.
    fn create_event_listener(&mut self, name: &'static str, id: ElementId);

    /// Remove an existing Event Listener.
    ///
    /// Name: The name of the event to remove.
    /// Id: The ID of the node to remove.
    fn remove_event_listener(&mut self, name: &'static str, id: ElementId);

    /// Remove a particular node from the DOM
    ///
    /// Id: The ID of the node to remove.
    fn remove_node(&mut self, id: ElementId);

    /// Push the given root node onto our stack.
    ///
    /// Id: The ID of the root node to push.
    fn push_root(&mut self, id: ElementId) {}

    /// Swap to a new subtree
    fn swap_subtree(&mut self, subtree_index: usize) {}

    /// Mark a scope as dirty
    fn mark_scope_dirty(&mut self, scope_id: ScopeId) {}
}

/// A `Mutation` represents a single instruction for the renderer to use to modify the UI tree to match the state
/// of the Dioxus VirtualDom.
///
/// These edits can be serialized and sent over the network or through any interface
#[derive(Debug, PartialEq)]
pub enum Mutation {
    /// Add these m children to the target element
    AppendChildren {
        /// The ID of the element being mounted to
        id: ElementId,

        /// The number of nodes on the stack to append to the target element
        m: usize,
    },

    /// Assign the element at the given path the target ElementId.
    ///
    /// The path is in the form of a list of indices based on children. Templates cannot have more than 255 children per
    /// element, hence the use of a single byte.
    ///
    ///
    AssignId {
        /// The path of the child of the topmost node on the stack
        ///
        /// A path of `[]` represents the topmost node. A path of `[0]` represents the first child.
        /// `[0,1,2]` represents 1st child's 2nd child's 3rd child.
        path: &'static [u8],

        /// The ID we're assigning to this element/placeholder.
        ///
        /// This will be used later to modify the element or replace it with another element.
        id: ElementId,
    },

    /// Create a placeholder in the DOM that we will use later.
    ///
    /// Dioxus currently requires the use of placeholders to maintain a re-entrance point for things like list diffing
    CreatePlaceholder {
        /// The ID we're assigning to this element/placeholder.
        ///
        /// This will be used later to modify the element or replace it with another element.
        id: ElementId,
    },

    /// Create a node specifically for text with the given value
    CreateTextNode {
        /// The text content of this text node
        value: String,

        /// The ID we're assigning to this specific text nodes
        ///
        /// This will be used later to modify the element or replace it with another element.
        id: ElementId,
    },

    /// Hydrate an existing text node at the given path with the given text.
    ///
    /// Assign this text node the given ID since we will likely need to modify this text at a later point
    HydrateText {
        /// The path of the child of the topmost node on the stack
        ///
        /// A path of `[]` represents the topmost node. A path of `[0]` represents the first child.
        /// `[0,1,2]` represents 1st child's 2nd child's 3rd child.
        path: &'static [u8],

        /// The value of the textnode that we want to set the placeholder with
        value: String,

        /// The ID we're assigning to this specific text nodes
        ///
        /// This will be used later to modify the element or replace it with another element.
        id: ElementId,
    },

    /// Load and clone an existing node from a template saved under that specific name
    ///
    /// Dioxus guarantees that the renderer will have already been provided the template.
    /// When the template is picked up in the template list, it should be saved under its "name" - here, the name
    LoadTemplate {
        /// The "name" of the template. When paired with `rsx!`, this is autogenerated
        name: &'static str,

        /// Which root are we loading from the template?
        ///
        /// The template is stored as a list of nodes. This index represents the position of that root
        index: usize,

        /// The ID we're assigning to this element being loaded from the template
        ///
        /// This will be used later to move the element around in lists
        id: ElementId,
    },

    /// Replace the target element (given by its ID) with the topmost m nodes on the stack
    ReplaceWith {
        /// The ID of the node we're going to replace with
        id: ElementId,

        /// The number of nodes on the stack to replace the target element with
        m: usize,
    },

    /// Replace an existing element in the template at the given path with the m nodes on the stack
    ReplacePlaceholder {
        /// The path of the child of the topmost node on the stack
        ///
        /// A path of `[]` represents the topmost node. A path of `[0]` represents the first child.
        /// `[0,1,2]` represents 1st child's 2nd child's 3rd child.
        path: &'static [u8],

        /// The number of nodes on the stack to replace the target element with
        m: usize,
    },

    /// Insert a number of nodes after a given node.
    InsertAfter {
        /// The ID of the node to insert after.
        id: ElementId,

        /// The number of nodes on the stack to insert after the target node.
        m: usize,
    },

    /// Insert a number of nodes before a given node.
    InsertBefore {
        /// The ID of the node to insert before.
        id: ElementId,

        /// The number of nodes on the stack to insert before the target node.
        m: usize,
    },

    /// Set the value of a node's attribute.
    SetAttribute {
        /// The name of the attribute to set.
        name: &'static str,

        /// The (optional) namespace of the attribute.
        /// For instance, "style" is in the "style" namespace.
        ns: Option<&'static str>,

        /// The value of the attribute.
        value: AttributeValue,

        /// The ID of the node to set the attribute of.
        id: ElementId,
    },

    /// Set the textcontent of a node.
    SetText {
        /// The textcontent of the node
        value: String,

        /// The ID of the node to set the textcontent of.
        id: ElementId,
    },

    /// Create a new Event Listener.
    NewEventListener {
        /// The name of the event to listen for.
        name: String,

        /// The ID of the node to attach the listener to.
        id: ElementId,
    },

    /// Remove an existing Event Listener.
    RemoveEventListener {
        /// The name of the event to remove.
        name: String,

        /// The ID of the node to remove.
        id: ElementId,
    },

    /// Remove a particular node from the DOM
    Remove {
        /// The ID of the node to remove.
        id: ElementId,
    },

    /// Push the given root node onto our stack.
    PushRoot {
        /// The ID of the root node to push.
        id: ElementId,
    },
}

/// A static list of mutations that can be applied to the DOM. Note: this list does not contain any `Any` attribute values
pub struct MutationsVec {
    /// The list of Scopes that were diffed, created, and removed during the Diff process.
    pub dirty_scopes: FxHashSet<ScopeId>,

    /// Any templates encountered while diffing the DOM.
    ///
    /// These must be loaded into a cache before applying the edits
    pub templates: Vec<Template<'static>>,

    /// Any mutations required to patch the renderer to match the layout of the VirtualDom
    pub edits: Vec<Mutation>,
}

impl MutationsVec {
    /// Rewrites IDs to just be "template", so you can compare the mutations
    ///
    /// Used really only for testing
    pub fn santize(mut self) -> Self {
        for edit in self.edits.iter_mut() {
            if let Mutation::LoadTemplate { name, .. } = edit {
                *name = "template"
            }
        }

        self
    }
}

impl WriteMutations for MutationsVec {
    fn register_template(&mut self, template: Template<'static>) {
        self.templates.push(template)
    }

    fn append_children(&mut self, id: ElementId, m: usize) {
        self.edits.push(Mutation::AppendChildren { id, m })
    }

    fn assign_node_id(&mut self, path: &'static [u8], id: ElementId) {
        self.edits.push(Mutation::AssignId { path, id })
    }

    fn create_placeholder(&mut self, id: ElementId) {
        self.edits.push(Mutation::CreatePlaceholder { id })
    }

    fn create_text_node(&mut self, value: &str, id: ElementId) {
        self.edits.push(Mutation::CreateTextNode {
            value: value.into(),
            id,
        })
    }

    fn hydrate_text_node(&mut self, path: &'static [u8], value: &str, id: ElementId) {
        self.edits.push(Mutation::HydrateText {
            path,
            value: value.into(),
            id,
        })
    }

    fn load_template(&mut self, name: &'static str, index: usize, id: ElementId) {
        self.edits.push(Mutation::LoadTemplate { name, index, id })
    }

    fn replace_node_with(&mut self, id: ElementId, m: usize) {
        self.edits.push(Mutation::ReplaceWith { id, m })
    }

    fn replace_placeholder_with_nodes(&mut self, path: &'static [u8], m: usize) {
        self.edits.push(Mutation::ReplacePlaceholder { path, m })
    }

    fn insert_nodes_after(&mut self, id: ElementId, m: usize) {
        self.edits.push(Mutation::InsertAfter { id, m })
    }

    fn insert_nodes_before(&mut self, id: ElementId, m: usize) {
        self.edits.push(Mutation::InsertBefore { id, m })
    }

    fn set_attribute(
        &mut self,
        name: &'static str,
        ns: Option<&'static str>,
        value: &AttributeValue,
        id: ElementId,
    ) {
        self.edits.push(Mutation::SetAttribute {
            name,
            ns,
            value: match value {
                AttributeValue::Text(s) => AttributeValue::Text(s.clone()),
                AttributeValue::Bool(b) => AttributeValue::Bool(*b),
                AttributeValue::Float(n) => AttributeValue::Float(*n),
                AttributeValue::Int(n) => AttributeValue::Int(*n),
                AttributeValue::None => AttributeValue::None,
                _ => panic!("Cannot serialize attribute value"),
            },
            id,
        })
    }

    fn set_node_text(&mut self, value: &str, id: ElementId) {
        self.edits.push(Mutation::SetText {
            value: value.into(),
            id,
        })
    }

    fn create_event_listener(&mut self, name: &'static str, id: ElementId) {
        self.edits.push(Mutation::NewEventListener {
            name: name.into(),
            id,
        })
    }

    fn remove_event_listener(&mut self, name: &'static str, id: ElementId) {
        self.edits.push(Mutation::RemoveEventListener {
            name: name.into(),
            id,
        })
    }

    fn remove_node(&mut self, id: ElementId) {
        self.edits.push(Mutation::Remove { id })
    }

    fn push_root(&mut self, id: ElementId) {
        self.edits.push(Mutation::PushRoot { id })
    }

    fn swap_subtree(&mut self, _subtree_index: usize) {}

    fn mark_scope_dirty(&mut self, scope_id: ScopeId) {
        self.dirty_scopes.insert(scope_id);
    }
}

/// A struct that ignores all mutations
pub struct NoOpMutations;

impl WriteMutations for NoOpMutations {
    fn register_template(&mut self, template: Template<'static>) {}

    fn append_children(&mut self, id: ElementId, m: usize) {}

    fn assign_node_id(&mut self, path: &'static [u8], id: ElementId) {}

    fn create_placeholder(&mut self, id: ElementId) {}

    fn create_text_node(&mut self, value: &str, id: ElementId) {}

    fn hydrate_text_node(&mut self, path: &'static [u8], value: &str, id: ElementId) {}

    fn load_template(&mut self, name: &'static str, index: usize, id: ElementId) {}

    fn replace_node_with(&mut self, id: ElementId, m: usize) {}

    fn replace_placeholder_with_nodes(&mut self, path: &'static [u8], m: usize) {}

    fn insert_nodes_after(&mut self, id: ElementId, m: usize) {}

    fn insert_nodes_before(&mut self, id: ElementId, m: usize) {}

    fn set_attribute(
        &mut self,
        name: &'static str,
        ns: Option<&'static str>,
        value: &AttributeValue,
        id: ElementId,
    ) {
    }

    fn set_node_text(&mut self, value: &str, id: ElementId) {}

    fn create_event_listener(&mut self, name: &'static str, id: ElementId) {}

    fn remove_event_listener(&mut self, name: &'static str, id: ElementId) {}

    fn remove_node(&mut self, id: ElementId) {}
}
