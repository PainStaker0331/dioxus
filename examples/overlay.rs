use dioxus::prelude::*;
use dioxus_desktop::{use_window, WindowBuilder};

fn main() {
    dioxus_desktop::launch(app);
}

fn app(cx: Scope) -> Element {
    let window = use_window(cx);

    cx.render(rsx! {
        div {
            button {
                onclick: move |_| {
                    let dom = VirtualDom::new(app);
                    window.new_window(dom, Default::default());
                },
                "Open overlay"
            }
        }
    })
}

fn popup(cx: Scope) -> Element {
    cx.render(rsx! {
        div {
            width: "200px",
            height: "200px",
            background: "white",
            border: "1px solid black",
            "This is a popup!"
        }
    })
}
