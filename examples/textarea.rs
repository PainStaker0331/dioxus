// How to use textareas

use dioxus::prelude::*;

fn main() {
    dioxus::desktop::launch(app);
}

fn app(cx: Scope) -> Element {
    let (model, set_model) = use_state(&cx, || String::from("asd"));

    println!("{}", model);

    cx.render(rsx! {
        textarea {
            class: "border",
            rows: "10",
            cols: "80",
            value: "{model}",
            oninput: move |e| set_model(e.value.clone()),
        }
    })
}
