<div align="center">
  <h1>🌗🚀 Dioxus</h1>
  <p>
    <strong>A concurrent, functional, virtual DOM for Rust</strong>
  </p>
</div>

# About

Dioxus is a new approach for creating performant cross platform user experiences in Rust. In Dioxus, the UI is represented as a tree of Virtual Nodes not bound to any specific renderer. Instead, external renderers can leverage Dioxus' virtual DOM and event system as a source of truth for rendering to a medium of their choice. Developers experienced with building react-based experiences should feel comfortable with Dioxus.

Dioxus was built in a way to facilitate powerful external renderers - especially designed for the web, servers, desktop, and hybrid approaches like Dioxus Liveview.

Dioxus is supported by Dioxus Labs, a company providing end-to-end services for building, testing, deploying, and managing Dioxus apps on all supported platforms.

## Features
Dioxus' goal is to be the most advanced UI system for Rust, targeting isomorphism and hybrid approaches. Our goal is to eliminate context-switching for cross-platform development - both in UI patterns and programming language. Hooks and components should work *everywhere* without compromise.

Dioxus Core supports:
- [ ] Hooks for component state
- [ ] Concurrent rendering
- [ ] Context subscriptions
- [ ] State management integrations

On top of these, we have several projects you can find in the `packages` folder.
- [x] `dioxus-cli`: Testing, development, and packaging tools for Dioxus apps
- [ ] `dioxus-router`: A hook-based router implementation for Dioxus web apps
- [ ] `dioxus-vscode`: Syntax highlighting, code formatting, and hints for Dioxus html! blocks
- [ ] `redux-rs`: Redux-style global state management
- [ ] `recoil-rs`: Recoil-style global state management
- [ ] `dioxus-iso`: Hybrid apps (SSR + Web)
- [ ] `dioxus-live`: Live view
- [ ] `dioxus-webview`: Desktop Applications
- [ ] `dioxus-ios`: iOS apps
- [ ] `dioxus-android`: Android apps
- [ ] `dioxus-magic`: AR/VR Apps

## Components
Dioxus should look and feel just like writing functional React components. In Dioxus, there are no class components with lifecycles. All state management is done via hooks. This encourages logic reusability and lessens the burden on Dioxus to maintain a non-breaking lifecycle API.

```rust
#[derive(Properties, PartialEq)]
struct MyProps {
    name: String
}

async fn Example(ctx: &Context<MyProps>) -> VNode {
    html! { <div> "Hello {ctx.props.name}!" </div> }
}
```

Here, the `Context` object is used to access hook state, create subscriptions, and interact with the built-in context API. Props, children, and component APIs are accessible via the `Context` object. The functional component macro makes life more productive by inlining props directly as function arguments, similar to how Rocket parses URIs.

```rust
// A very terse component!
#[fc]
fn Example(ctx: &Context, name: String) -> VNode {
    html! { <div> "Hello {name}!" </div> }
}

// or

#[functional_component]
static Example: FC = |ctx, name: String| html! { <div> "Hello {name}!" </div> }; 
```

The final output of components must be a tree of VNodes. We provide an html macro for using JSX-style syntax to write these, though, you could use any macro, DSL, templating engine, or the constructors directly. 

## Concurrency
In Dioxus, VNodes are asynchronous and can their rendering can be paused at any time by awaiting a future. Hooks can combine this functionality with the Context and Subscription APIs to craft dynamic and efficient user experiences. 

```rust
fn user_data(ctx: &Context<()>) -> VNode {
    // Register this future as a task
    use_suspense(ctx, async {
        // Continue on with the component as usual, waiting for data to arrive
        let Profile { name, birthday, .. } = fetch_data().await;
        html! {
            <div>
                {"Hello, {name}!"}
                {if birthday === std::Instant::now() {html! {"Happy birthday!"}}}
            </div>
        }
    })
}
```
Asynchronous components are powerful but can also be easy to misuse as they pause rendering for the component and its children. Refer to the concurrent guide for information on how to best use async components. 

## Liveview
With the Context, Subscription, and Asynchronous APIs, we've built Dioxus Liveview: a coupling of frontend and backend to deliver user experiences that do not require dedicated API development. Instead of building and maintaining frontend-specific API endpoints, components can directly access databases, server caches, and other services directly from the component.

These set of features are still experimental. Currently, we're still working on making these components more ergonomic

```rust
async fn live_component(ctx: &Context<()>) -> VNode {
    use_live_component(
        ctx,
        // Rendered via the client
        #[cfg(target_arch = "wasm32")]
        || html! { <div> {"Loading data from server..."} </div> },

        // Renderered on the server
        #[cfg(not(target_arch = "wasm32"))]
        || html! { <div> {"Server Data Loaded!"} </div> },
    )
}
```

## Dioxus LiveHost
Dioxus LiveHost is a paid service dedicated to hosting your Dioxus Apps - whether they be server-rendered, wasm-only, or a liveview. LiveHost enables a wide set of features:

- Versioned fronted/backend with unique links
- Builtin CI/CD for all supported Dioxus platforms (mac, windows, server, wasm, etc)
- Managed and pluggable storage database backends
- Serverless support for minimal latency
- Analytics
- Lighthouse optimization
- On-premise support (see license terms)
- Cloudfare/DDoS protection integrations

For small teams, LiveHost is free. Check out the pricing page to see if Dioxus LiveHost is good your team.

## Examples
We use the dedicated `dioxus-cli` to build and test dioxus web-apps. This can run examples, tests, build web workers, launch development servers, bundle, and more. It's general purpose, but currently very tailored to Dioxus for liveview and bundling. If you've not used it before, `cargo install --path pacakages/dioxus-cli` will get it installed. This CLI tool should feel like using `cargo` but with 1st party support for assets, bundling, and other important dioxus-specific features.

Alternatively, `trunk` works but can't run examples.

- tide_ssr: Handle an HTTP request and return an html body using the html! macro. `cargo run --example tide_ssr`
- doc_generator: Use dioxus SSR to generate the website and docs. `cargo run --example doc_generator`
- fc_macro: Use the functional component macro to build terse components. `cargo run --example fc_macro`
- hello_web: Start a simple wasm app. Requires a web packer like dioxus-cli or trunk `cargo run --example hello`
- router: `cargo run --example router`
- tide_ssr: `cargo run --example tide_ssr`
- webview: Use liveview to bridge into a webview context for a simple desktop application. `cargo run --example webview`
- twitter-clone: A full-featured Twitter clone showcasing dioxus-liveview, state management patterns, and hooks. `cargo run --example twitter`

## Documentation


