use crate::dioxus_elements::SerializedMouseData;
use dioxus::prelude::*;
use dioxus_core::ElementId;
use dioxus_elements::SerializedHtmlEventConverter;
use std::rc::Rc;

#[test]
fn miri_rollover() {
    set_event_converter(Box::new(SerializedHtmlEventConverter));
    let mut dom = VirtualDom::new(App);

    _ = dom.rebuild_to_vec(&mut dioxus_core::NoOpMutations);

    for _ in 0..3 {
        dom.handle_event(
            "click",
            Rc::new(PlatformEventData::new(Box::<SerializedMouseData>::default())),
            ElementId(2),
            true,
        );
        dom.process_events();
        _ = dom.render_immediate_to_vec();
    }
}

#[component]
fn App() -> Element {
    let mut idx = use_signal(|| 0);
    let onhover = |_| println!("go!");

    render! {
        div {
            button {
                onclick: move |_| {
                    idx += 1;
                    println!("Clicked");
                },
                "+"
            }
            button { onclick: move |_| idx -= 1, "-" }
            ul {
                (0..**idx).map(|i| render! {
                    ChildExample { i: i, onhover: onhover }
                })
            }
        }
    }
}

#[component]
fn ChildExample<'a>(i: i32, onhover: EventHandler<'a, MouseEvent>) -> Element {
    render! { li { onmouseover: move |e| onhover.call(e), "{i}" } }
}
