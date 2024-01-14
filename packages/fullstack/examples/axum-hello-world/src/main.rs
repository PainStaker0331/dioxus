//! Run with:
//!
//! ```sh
//! dx build --features web --release
//! cargo run --features ssr --release
//! ```

#![allow(non_snake_case, unused)]
use dioxus::prelude::*;
use dioxus_fullstack::{
    launch::{self, LaunchBuilder},
    prelude::*,
};
use serde::{Deserialize, Serialize};

#[derive(Props, PartialEq, Debug, Default, Serialize, Deserialize, Clone)]
struct AppProps {
    count: i32,
}

fn app(cx: Scope<AppProps>) -> Element {
    let state =
        use_server_future((), |()| async move { get_server_data().await.unwrap() })?.value();

    let mut count = use_state(|| 0);
    let text = use_state(|| "...".to_string());
    let eval = use_eval(cx);

    cx.render(rsx! {
        div { "Server state: {state}" }
        h1 { "High-Five counter: {count}" }
        button { onclick: move |_| count += 1, "Up high!" }
        button { onclick: move |_| count -= 1, "Down low!" }
        button {
            onclick: move |_| {
                to_owned![text];
                async move {
                    if let Ok(data) = get_server_data().await {
                        println!("Client received: {}", data);
                        text.set(data.clone());
                        post_server_data(data).await.unwrap();
                    }
                }
            },
            "Run a server function!"
        }
        "Server said: {text}"
    })
}

#[server]
async fn post_server_data(data: String) -> Result<(), ServerFnError> {
    let axum::extract::Host(host): axum::extract::Host = extract().await?;
    println!("Server received: {}", data);
    println!("{:?}", host);

    Ok(())
}

#[server]
async fn get_server_data() -> Result<String, ServerFnError> {
    Ok(reqwest::get("https://httpbin.org/ip").await?.text().await?)
}

fn main() {
    #[cfg(feature = "web")]
    tracing_wasm::set_as_global_default();
    #[cfg(feature = "ssr")]
    tracing_subscriber::fmt::init();

    LaunchBuilder::new_with_props(app, AppProps { count: 0 }).launch()
}
