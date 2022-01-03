#![allow(non_snake_case)]
#![doc = include_str!("../README.md")]

pub(crate) mod diff;
pub(crate) mod lazynodes;
pub(crate) mod mutations;
pub(crate) mod nodes;
pub(crate) mod scopes;
pub(crate) mod virtual_dom;

pub(crate) mod innerlude {
    pub(crate) use crate::diff::*;
    pub use crate::lazynodes::*;
    pub use crate::mutations::*;
    pub use crate::nodes::*;
    pub use crate::scopes::*;
    pub use crate::virtual_dom::*;

    /// An [`Element`] is a possibly-none [`VNode`] created by calling `render` on [`Scope`] or [`ScopeState`].
    ///
    /// Any [`None`] [`Element`] will automatically be coerced into a placeholder [`VNode`] with the [`VNode::Placeholder`] variant.
    pub type Element<'a> = Option<VNode<'a>>;

    /// A [`Component`] is a function that takes a [`Scope`] and returns an [`Element`].
    ///
    /// Components can be used in other components with two syntax options:
    /// - lowercase as a function call with named arguments (rust style)
    /// - uppercase as an element (react style)
    ///
    /// ## Rust-Style
    ///
    /// ```rust, ignore
    /// fn example(cx: Scope<Props>) -> Element {
    ///     // ...
    /// }
    ///
    /// rsx!(
    ///     example()
    /// )
    /// ```
    /// ## React-Style
    /// ```rust, ignore
    /// fn Example(cx: Scope<Props>) -> Element {
    ///     // ...
    /// }
    ///
    /// rsx!(
    ///     Example {}
    /// )
    /// ```
    ///
    /// ## As a closure
    /// This particular type alias lets you even use static closures for pure/static components:
    ///
    /// ```rust, ignore
    /// static Example: Component<Props> = |cx| {
    ///     // ...
    /// };
    /// ```
    pub type Component<P = ()> = fn(Scope<P>) -> Element;

    /// A list of attributes
    ///
    pub type Attributes<'a> = Option<&'a [Attribute<'a>]>;
}

pub use crate::innerlude::{
    Attribute, Component, DioxusElement, DomEdit, Element, ElementId, ElementIdIterator, Event,
    EventHandler, EventPriority, IntoVNode, LazyNodes, Listener, Mutations, NodeFactory,
    Properties, SchedulerMsg, Scope, ScopeId, ScopeState, TaskId, UserEvent, VComponent, VElement,
    VFragment, VNode, VPlaceholder, VText, VirtualDom,
};

pub mod prelude {
    pub use crate::innerlude::Scope;
    pub use crate::innerlude::{
        Attributes, Component, DioxusElement, Element, EventHandler, LazyNodes, NodeFactory,
        ScopeState,
    };
    pub use crate::nodes::VNode;
    pub use crate::virtual_dom::{fc_to_builder, Fragment, Properties};
    pub use crate::VirtualDom;
}

pub mod exports {
    //! Important dependencies that are used by the rest of the library
    //! Feel free to just add the dependencies in your own Crates.toml
    pub use bumpalo;
    pub use futures_channel;
}

/// Functions that wrap unsafe functionality to prevent us from misusing it at the callsite
pub(crate) mod unsafe_utils {
    use crate::VNode;

    pub unsafe fn extend_vnode<'a, 'b>(node: &'a VNode<'a>) -> &'b VNode<'b> {
        std::mem::transmute(node)
    }
}
