//! Basic example that renders a simple VNode to the browser.

use dioxus_core::prelude::*;
use dioxus_web::*;

fn main() {
    // Setup logging
    wasm_logger::init(wasm_logger::Config::new(log::Level::Debug));
    console_error_panic_hook::set_once();

    // Run the app
    wasm_bindgen_futures::spawn_local(WebsysRenderer::start(App));
}

static App: FC<()> = |ctx| {
    ctx.render(rsx! {
        div {
            h1 {"hello"}
            C1 {}
            C2 {}
        }
    })
};

static C1: FC<()> = |ctx| {
    ctx.render(rsx! {
        button {
            "numba 1"
        }
    })
};

static C2: FC<()> = |ctx| {
    ctx.render(rsx! {
        button {
            "numba 2"
        }
    })
};

static DocExamples: FC<()> = |ctx| {
    //

    let is_ready = false;

    let items = (0..10).map(|i| rsx! { li {"{i}"} });
    let _ = rsx! {
        ul {
            {items}
        }
    };

    rsx! {
        div {}
        h1 {}
        {""}
        "asbasd"
        dioxus::Fragment {
            //
        }
    }

    ctx.render(rsx! {
        div {
            { is_ready.then(|| rsx!{ h1 {"We are ready!"} }) }
        }
    })
};
