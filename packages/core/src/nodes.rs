use crate::{any_props::AnyProps, arena::ElementId, ScopeId, ScopeState, UiEvent};
use std::{
    any::{Any, TypeId},
    cell::{Cell, RefCell},
    hash::Hasher,
    rc::Rc,
};

pub type TemplateId = &'static str;

/// A reference to a template along with any context needed to hydrate it
#[derive(Debug)]
pub struct VNode<'a> {
    // The ID assigned for the root of this template
    pub node_id: Cell<ElementId>,

    pub key: Option<&'a str>,

    // When rendered, this template will be linked to its parent manually
    pub parent: Option<*const DynamicNode<'a>>,

    pub template: Template<'static>,

    pub root_ids: &'a [Cell<ElementId>],

    pub dynamic_nodes: &'a [DynamicNode<'a>],

    pub dynamic_attrs: &'a [Attribute<'a>],
}

impl<'a> VNode<'a> {
    pub fn single_component(
        cx: &'a ScopeState,
        node: DynamicNode<'a>,
        id: &'static str,
    ) -> Option<Self> {
        Some(VNode {
            node_id: Cell::new(ElementId(0)),
            key: None,
            parent: None,
            root_ids: &[],
            dynamic_nodes: cx.bump().alloc([node]),
            dynamic_attrs: &[],
            template: Template {
                id,
                roots: &[TemplateNode::Dynamic(0)],
                node_paths: &[&[0]],
                attr_paths: &[],
            },
        })
    }

    pub fn single_text(
        _cx: &'a ScopeState,
        text: &'static [TemplateNode<'static>],
        id: &'static str,
    ) -> Option<Self> {
        Some(VNode {
            node_id: Cell::new(ElementId(0)),
            key: None,
            parent: None,
            root_ids: &[],
            dynamic_nodes: &[],
            dynamic_attrs: &[],
            template: Template {
                id,
                roots: text,
                node_paths: &[&[0]],
                attr_paths: &[],
            },
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Template<'a> {
    pub id: &'a str,
    pub roots: &'a [TemplateNode<'a>],
    pub node_paths: &'a [&'a [u8]],
    pub attr_paths: &'a [&'a [u8]],
}

impl<'a> std::hash::Hash for Template<'a> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}
impl Eq for Template<'_> {}
impl PartialEq for Template<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// A weird-ish variant of VNodes with way more limited types
#[derive(Debug, Clone, Copy)]
pub enum TemplateNode<'a> {
    Element {
        tag: &'a str,
        namespace: Option<&'a str>,
        attrs: &'a [TemplateAttribute<'a>],
        children: &'a [TemplateNode<'a>],
        inner_opt: bool,
    },
    Text(&'a str),
    Dynamic(usize),
    DynamicText(usize),
}

#[derive(Debug)]
pub enum DynamicNode<'a> {
    Component {
        name: &'static str,
        static_props: bool,
        props: Cell<*mut dyn AnyProps<'a>>,
        placeholder: Cell<Option<ElementId>>,
        scope: Cell<Option<ScopeId>>,
    },
    Text {
        id: Cell<ElementId>,
        value: &'a str,
        inner: bool,
    },
    Fragment {
        nodes: &'a [VNode<'a>],
        inner: bool,
    },
    Placeholder(Cell<ElementId>),
}

#[derive(Debug)]
pub enum TemplateAttribute<'a> {
    Static {
        name: &'static str,
        value: &'a str,
        namespace: Option<&'static str>,
        volatile: bool,
    },
    Dynamic(usize),
}

#[derive(Debug)]
pub struct Attribute<'a> {
    pub name: &'a str,
    pub value: AttributeValue<'a>,
    pub namespace: Option<&'static str>,
    pub mounted_element: Cell<ElementId>,
    pub volatile: bool,
}

pub enum AttributeValue<'a> {
    Text(&'a str),
    Float(f32),
    Int(i32),
    Bool(bool),
    Listener(RefCell<&'a mut dyn FnMut(&dyn Any)>),
    Any(&'a dyn AnyValue),
    None,
}

impl<'a> AttributeValue<'a> {
    pub fn new_listener<T: 'static>(
        cx: &'a ScopeState,
        mut f: impl FnMut(UiEvent<T>) + 'a,
    ) -> AttributeValue<'a> {
        let f = cx.bump().alloc(move |a: &dyn Any| {
            a.downcast_ref::<UiEvent<T>>()
                .map(|a| f(a.clone()))
                .unwrap_or_else(|| {
                    panic!(
                        "Expected UiEvent<{}>, got {:?}",
                        std::any::type_name::<T>(),
                        a
                    )
                })
        }) as &mut dyn FnMut(&dyn Any);

        AttributeValue::Listener(RefCell::new(f))
    }
}

impl<'a> std::fmt::Debug for AttributeValue<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(arg0) => f.debug_tuple("Text").field(arg0).finish(),
            Self::Float(arg0) => f.debug_tuple("Float").field(arg0).finish(),
            Self::Int(arg0) => f.debug_tuple("Int").field(arg0).finish(),
            Self::Bool(arg0) => f.debug_tuple("Bool").field(arg0).finish(),
            Self::Listener(_) => f.debug_tuple("Listener").finish(),
            Self::Any(_) => f.debug_tuple("Any").finish(),
            Self::None => write!(f, "None"),
        }
    }
}

impl<'a> PartialEq for AttributeValue<'a> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Text(l0), Self::Text(r0)) => l0 == r0,
            (Self::Float(l0), Self::Float(r0)) => l0 == r0,
            (Self::Int(l0), Self::Int(r0)) => l0 == r0,
            (Self::Bool(l0), Self::Bool(r0)) => l0 == r0,
            (Self::Listener(_), Self::Listener(_)) => true,
            (Self::Any(l0), Self::Any(r0)) => l0.any_cmp(*r0),
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

impl<'a> AttributeValue<'a> {
    pub fn matches_type(&self, other: &'a AttributeValue<'a>) -> bool {
        match (self, other) {
            (Self::Text(_), Self::Text(_)) => true,
            (Self::Float(_), Self::Float(_)) => true,
            (Self::Int(_), Self::Int(_)) => true,
            (Self::Bool(_), Self::Bool(_)) => true,
            (Self::Listener(_), Self::Listener(_)) => true,
            (Self::Any(_), Self::Any(_)) => true,
            _ => return false,
        }
    }
}

pub trait AnyValue {
    fn any_cmp(&self, other: &dyn AnyValue) -> bool;
    fn our_typeid(&self) -> TypeId;
}

impl<T: PartialEq + Any> AnyValue for T {
    fn any_cmp(&self, other: &dyn AnyValue) -> bool {
        if self.type_id() != other.our_typeid() {
            return false;
        }

        self == unsafe { &*(other as *const _ as *const T) }
    }

    fn our_typeid(&self) -> TypeId {
        self.type_id()
    }
}

#[test]
fn what_are_the_sizes() {
    dbg!(std::mem::size_of::<VNode>());
    dbg!(std::mem::size_of::<Template>());
    dbg!(std::mem::size_of::<TemplateNode>());
}

/*


SSR includes data-id which allows O(1) hydration


we read the edit stream dn then we can just rehydare



ideas:
- IDs for lookup
- use edit stream to hydrate
- write comments to dom that specify size of children

IDs for lookups
- adds noise to generated html
- doesnt work for text nodes
- suspense could cause ordering to be weird

Names for lookups:
- label each root or something with the template name
- label each dynamic node with a path
- noisy too
- allows reverse lookups

Ideal:
- no noise in the dom
- fast, ideally O(1)
- able to pick apart text nodes that get merged during SSR


--> render vdom
--> traverse vdom and real dom simultaneously

IE

div {
    div {
        div {
            "thing"
        }
    }
}





*/
