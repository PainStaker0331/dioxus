use dioxus_core as dioxus;
use dioxus_core::{events::on::MouseEvent, prelude::*};
use dioxus_web::WebsysRenderer;
use dioxus_html as dioxus_elements;


fn main() {
    // Setup logging
    wasm_logger::init(wasm_logger::Config::new(log::Level::Debug));
    console_error_panic_hook::set_once();

    log::info!("hello world");
    wasm_bindgen_futures::spawn_local(WebsysRenderer::start(Example));
}

// this is a component
pub static Example: FC<()> = |cx| {
    let (event, set_event) = use_state_classic(cx, || None);

    let handler = move |evt| {
        set_event(Some(evt));
    };

    log::info!("hello world");

    cx.render(rsx! {
        div {  
            
            class: "py-12 px-4 w-full max-w-2xl mx-auto bg-red-100"
            span { 
                class: "text-sm font-semibold"
                "Dioxus Example: Synthetic Events"
            }            
            button {
                class: "inline-block py-4 px-8 mr-6 leading-none text-white bg-indigo-600 hover:bg-indigo-900 font-semibold rounded shadow"
                "press me"
            }
            pre {
                onmousemove: {handler}
                id: "json"
                "Hello world"
            }
            Example2 { name: "Blah".into() }
        }
    })
};

#[derive(Debug, PartialEq, Props)]
struct ExampleProps {
    name: String,
}

pub static Example2: FC<ExampleProps> = |cx| {
    cx.render(rsx! {
        div {
            h1 {"hello {cx.name}"}
        }
    })
};
