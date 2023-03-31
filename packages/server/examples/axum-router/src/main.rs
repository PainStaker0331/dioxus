//! Run with:
//!
//! ```sh
//! dioxus build --features web
//! cargo run --features ssr
//! ```

#![allow(non_snake_case)]
use dioxus::prelude::*;
use dioxus_router::*;
use dioxus_server::prelude::*;

fn main() {
    #[cfg(feature = "web")]
    dioxus_web::launch_with_props(
        App,
        AppProps { route: None },
        dioxus_web::Config::new().hydrate(true),
    );
    #[cfg(feature = "ssr")]
    {
        use axum::extract::State;
        PostServerData::register().unwrap();
        GetServerData::register().unwrap();
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async move {
                let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8080));
                use tower_http::services::ServeDir;

                // Serve the dist/assets folder with the javascript and WASM files created by the CLI
                let serve_dir = ServeDir::new("dist/assets");

                axum::Server::bind(&addr)
                    .serve(
                        axum::Router::new()
                            // Register server functions
                            .register_server_fns("")
                            // Serve the static assets folder
                            .nest_service("/assets", serve_dir)
                            // If the path is unknown, render the application
                            .fallback(
                                move |uri: http::uri::Uri, State(ssr_state): State<SSRState>| {
                                    let rendered = ssr_state.render(
                                        &ServeConfig::new(
                                            App,
                                            AppProps {
                                                route: Some(format!("http://{addr}{uri}")),
                                            },
                                        )
                                        .head(r#"<title>Hello World!</title>"#),
                                    );
                                    async move { axum::body::Full::from(rendered) }
                                },
                            )
                            .with_state(SSRState::default())
                            .into_make_service(),
                    )
                    .await
                    .unwrap();
            });
    }
}

#[derive(Clone, Debug, Props, PartialEq)]
struct AppProps {
    route: Option<String>,
}

fn App(cx: Scope<AppProps>) -> Element {
    cx.render(rsx! {
        Router {
            initial_url: cx.props.route.clone(),

            Route { to: "/blog",
                Link {
                    to: "/",
                    "Go to counter"
                }
                table {
                    tbody {
                        for _ in 0..100 {
                            tr {
                                for _ in 0..100 {
                                    td { "hello world??" }
                                }
                            }
                        }
                    }
                }
            },
            // Fallback
            Route { to: "",
                Counter {}
            },
        }
    })
}

fn Counter(cx: Scope) -> Element {
    let mut count = use_state(cx, || 0);
    let text = use_state(cx, || "...".to_string());

    cx.render(rsx! {
        Link {
            to: "/blog",
            "Go to blog"
        }
        div{
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
                "Run a server function"
            }
            "Server said: {text}"
        }
    })
}

#[server(PostServerData)]
async fn post_server_data(data: String) -> Result<(), ServerFnError> {
    println!("Server received: {}", data);

    Ok(())
}

#[server(GetServerData)]
async fn get_server_data() -> Result<String, ServerFnError> {
    Ok("Hello from the server!".to_string())
}
