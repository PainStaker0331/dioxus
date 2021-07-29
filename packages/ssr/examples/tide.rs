//!
//!
//!
use dioxus::virtual_dom::VirtualDom;
use dioxus_core as dioxus;
use dioxus_core::prelude::*;
use dioxus_hooks::use_state;
use dioxus_html as dioxus_elements;

use tide::{Request, Response};

#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
    let mut app = tide::new();

    app.at("/:name").get(|req: Request<()>| async move {
        let initial_name: String = req
            .param("name")
            .map(|f| f.parse().unwrap_or("...?".to_string()))
            .unwrap_or("...?".to_string());

        let dom = VirtualDom::launch_with_props_in_place(Example, ExampleProps { initial_name });

        Ok(Response::builder(200)
            .body(format!("{}", dioxus_ssr::render_vdom(&dom, |c| c)))
            .content_type(tide::http::mime::HTML)
            .build())
    });

    println!("Server available at [http://127.0.0.1:8080/bill]");
    app.listen("127.0.0.1:8080").await?;

    Ok(())
}

#[derive(PartialEq, Props)]
struct ExampleProps {
    initial_name: String,
}

static Example: FC<ExampleProps> = |cx| {
    let dispaly_name = use_state(cx, move || cx.initial_name.clone());

    cx.render(rsx! {
        div { class: "py-12 px-4 text-center w-full max-w-2xl mx-auto",
            span { class: "text-sm font-semibold"
                "Dioxus Example: Jack and Jill"
            }
            h2 { class: "text-5xl mt-2 mb-6 leading-tight font-semibold font-heading"
                "Hello, {dispaly_name}"
            }
            ul {
                {(0..10).map(|f| rsx!( li {"Element {f}"} ))}
            }
        }
    })
};
