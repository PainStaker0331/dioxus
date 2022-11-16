use dioxus::prelude::*;

fn main() {
    dioxus_desktop::launch(app);
}

fn app(cx: Scope) -> Element {
    cx.render(rsx!(
        div { id: "123123123",
            // Use Map directly to lazily pull elements
            (0..10).map(|f| rsx! { "{f}" }),

            // Collect into an intermediate collection if necessary, and call into_iter
            ["a", "b", "c", "d", "e", "f"]
                .into_iter()
                .map(|f| rsx! { "{f}" })
                .collect::<Vec<_>>()
                .into_iter(),

            // Use optionals
            Some(rsx! { "Some" }),
        }
    ))
}
