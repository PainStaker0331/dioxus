use dioxus::prelude::*;

fn main() {
    dioxus_desktop::launch(app);
}

fn app() -> Element {
    cx.render(rsx! {
        div {
            button {
                onclick: move |_| {
                    let dom = VirtualDom::new(popup);
                    dioxus_desktop::window().new_window(dom, Default::default());
                },
                "New Window"
            }
        }
    })
}

fn popup() -> Element {
    cx.render(rsx! {
        div { "This is a popup!" }
    })
}
