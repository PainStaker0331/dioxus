//! <div align="center">
//!   <h1>🌗🚀 📦 Dioxus</h1>
//!   <p>
//!     <strong>A concurrent, functional, virtual DOM for Rust</strong>
//!   </p>
//! </div>
//! Dioxus: a concurrent, functional, reactive virtual dom for any renderer in Rust.
//!
//! This crate aims to maintain a hook-based, renderer-agnostic framework for cross-platform UI development.
//!
//! ## Overview and Goals
//! Dioxus' ultimate goal is to save you from writing new code when bringing your application to new platforms. We forsee
//! a future where WebApps, Mobile Apps, Desktop Apps, and even AR apps can be written in the same language, ecosystem,
//! and leverage the same platform-agnostic libraries.
//!
//! In this aim we chose to use a variety of techniques:
//! - We use a VirtualDOM to abstract the true renderer from application logic.
//! - We use functions as components to limit the API churn for greater stability.
//! - We use hooks as state to allow reusable logic across the whole ecosystem.
//! - We support an extensible and compile-time safe DSL for building interfaces.
//!
//! Our guiding stars (in order of priority):
//! - Ergonomics
//! - Reusability
//! - Speed and memory efficiency
//! - Safety
//!
//! ## Components
//! The base unit of Dioxus is the `component`. Components can be easily created from just a function - no traits or
//! proc macros required:
//!
//! ```
//! use dioxus::prelude::*;
//!
//! fn App(cx: Context, props: &()) -> Element {
//!     cx.render(rsx!(
//!         div {"hello world"}
//!     ))
//! }
//! ```
//! Components need to take a "Context" parameter which is generic over some properties. This defines how the component can be used
//! and what properties can be used to specify it in the VNode output. Components without properties may be generic over
//! `()`, and components with properties must declare their properties as a struct:
//!
//! ```
//! #[derive(Props, PartialEq)]
//! struct AppProps {
//!     name: String
//! }
//!
//! fn App(cx: Context, props: &AppProps) -> Element {
//!     cx.render(rsx!(
//!         div { "Hello {props.name}!" }
//!     ))
//! }
//! ```
//!
//! Props that are valid for the `'pub static` lifetime automatically get memoized by Diouxs. This means the component won't
//! re-render if its Props didn't change. However, Props that borrow data from their parent cannot be safely memoized, and
//! will always re-render if their parent changes. To borrow data from a parent, your component needs to add explicit lifetimes,
//! otherwise Rust will get confused about whether data is borrowed from either Props or Context. Since Dioxus manages
//! these lifetimes internally, Context and your Props must share the same lifetime:
//!
//! ```
//! #[derive(Props)]
//! struct Props<'a> {
//!     name: &'a str
//! }
//!
//! fn Example(cx: Context, props: &AppProps) -> Element {
//!     cx.render(rsx!(
//!         div { "Hello {props.name}!" }
//!     ))
//! }
//! ```
//!
//! To use custom properties for components, you'll need to derive the `Props` trait for your properties. This trait
//! exposes a compile-time correct builder pattern (similar to typed-builder) that can be used in the `rsx!` macro to
//! build components. Component props may have default fields notated by the `Default` attribute:
//!
//! ```
//! #[derive(Props)]
//! struct Props {
//!     name: String
//!
//!     #[props(default = false)]
//!     checked: bool,
//!
//!     #[props(default, into))]
//!     title: Option<String>
//! }
//! ```
//!
//! These flags roughly follow that of typed-builder, though tweaked to support the `Props` usecase.
//!
//! ## Hooks and State
//! Dioxus uses hooks for state management. Hooks are a form of state persisted between calls of the function component.
//!
//! ```
//! pub pub static Example: FC<()> = |cx, props|{
//!     let mut val = use_state(cx, || 0);
//!     cx.render(rsx!(
//!         button { onclick: move |_| val += 1 }
//!     ))
//! }
//! ````
//!
//! As a building block for hooks, Dioxus provides the `use_hook` method on Context that stores a provided value in a
//! list of other values. Whenever `use_hook` is called, the next hook value in the list is returned.
//! ```
//! fn my_hook(cx: Context) -> &String {
//!     cx.use_hook(
//!         // Initializer stores a value
//!         |hook_idx| String::new("stored_data"),
//!
//!         // Runner returns the hook value every time the component is rendered
//!         |hook| &*hook,
//!     )
//! }
//! ```
//! Under the hood, hooks store their data in a list of `Box<dyn Any>`. The first render defines the layout of these
//! list, and on each subsequent render, each `use_hook` call accesses its corresponding list item. If a hook
//! accesses the wrong index, `use_hook` will panic when trying to downcast `Any` to your type, and your app will crash.
//! These types of errors can be easily mitigated by following the rules of hooks:
//!
//! - Don’t call Hooks inside loops, conditions, or nested functions
//! - Don't call hooks in changing order between renders
//!
//! Hooks provide a very powerful way to reuse stateful logic between components, simplify large complex components,
//! and adopt more clear context subscription patterns to make components easier to read. The mechanics of hooks in Dioxus
//! shares a great amount of similarity with React's hooks and there are many guides to hooks in React online.
//!
//! ## Supported Renderers
//! Instead of being tightly coupled to a platform, browser, or toolkit, Dioxus implements a VirtualDOM object which
//! can be consumed to draw the UI. The Dioxus VDOM is reactive and easily consumable by 3rd-party renderers via
//! the `RealDom` trait. See [Implementing a Renderer](docs/8-custom-renderer.md), the `StringRenderer`, and `WebSys` render implementations for a template
//! on how to implement your own custom renderer. We provide 1st-class support for these renderers:
//!
//! - dioxus-desktop (via WebView)
//! - dioxus-web (via WebSys)
//! - dioxus-ssr (via StringRenderer)
//! - dioxus-liveview (SSR + WebSys)
//!
//! In the main `dioxus` crate, these are all accessible through configuration flags.
//!
//! ## Rendering to the Web
//!
//! Most dioxus apps will be initialized in roughly the same way. The `launch` method in `web` will immediately start a
//! VirtualDOM and await it using `wasm_bindgen_futures`.
//!
//! An example app that starts a websys app and internally awaits works as follows:
//!
//! ```
//! use dioxus::prelude::*;
//! fn main() {
//!     dioxus::web::launch(Example);
//! }
//!
//! pub pub static Example: FC<()> = |cx, props|{
//!     cx.render(rsx! {
//!         div { "Hello World!" }
//!     })
//! };
//! ```
//!
//! In reality, you'll want to integrate analytics, logging, crash-protection and more.

// Just a heads-up, the core functionality of dioxus rests in Dioxus-Core. This crate just wraps a bunch of utilities
// together and exports their namespaces to something predicatble.
#[cfg(feature = "core")]
pub use dioxus_core as core;

#[cfg(feature = "hooks")]
pub use dioxus_hooks as hooks;

#[cfg(feature = "ssr")]
pub use dioxus_ssr as ssr;

#[cfg(feature = "web")]
pub use dioxus_web as web;

#[cfg(feature = "mobile")]
pub use dioxus_mobile as mobile;

#[cfg(feature = "desktop")]
pub use dioxus_desktop as desktop;

#[cfg(feature = "router")]
pub use dioxus_router as router;

pub mod debug {}

pub mod events {
    #[cfg(feature = "html")]
    pub use dioxus_html::{on::*, KeyCode};
}

pub mod prelude {
    //! A glob import that includes helper types like FC, rsx!, html!, and required traits
    pub use dioxus_core::prelude::*;
    pub use dioxus_core_macro::{format_args_f, rsx, Props, Routable};
    pub use dioxus_elements::{GlobalAttributes, SvgAttributes};
    pub use dioxus_hooks::*;
    pub use dioxus_html as dioxus_elements;
}
