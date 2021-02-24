//! basic example that renders a simple domtree to the page :)
//!
//!
//!
use dioxus_core::prelude::*;
use dioxus_web::*;

fn main() {
    wasm_logger::init(wasm_logger::Config::new(log::Level::Debug));
    console_error_panic_hook::set_once();
    // todo: wire up the component macro macro
    // div! { class = "asd" onclick = move |_| {}
    //     div! { class = "asd" onclick = move |_| {} }
    //     h1! { "123456" "123456" "123456" }
    //     h2! { "123456" "123456" "123456" }
    //     h3! { "123456" "123456" "123456" }
    // };

    WebsysRenderer::simple_render(html! {
        <div>
            <div class="flex items-center justify-center flex-col">
                <div class="flex items-center justify-center">
                    <div class="flex flex-col bg-white rounded p-4 w-full max-w-xs">
                        // Title
                        <div class="font-bold text-xl">
                            "Jon's awesome site!!11"
                        </div>

                        // Subtext / description
                        <div class="text-sm text-gray-500">
                            "He worked so hard on it :)"
                        </div>

                        <div class="flex flex-row items-center justify-center mt-6">
                            // Main number
                            <div class="font-medium text-6xl">
                                "1337"
                            </div>
                        </div>

                        // Try another
                        <div class="flex flex-row justify-between mt-6">
                            // <a href=format!("http://localhost:8080/fib/{}", other_fib_to_try) class="underline">
                                "Legit made my own React"
                            // </a>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    });
}
