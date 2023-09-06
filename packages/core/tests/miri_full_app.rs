use crate::dioxus_elements::SerializedMouseData;
use dioxus::prelude::*;
use dioxus_core::ElementId;
use dioxus_elements::SerializedHtmlEventConverter;
use std::rc::Rc;

#[test]
fn miri_rollover() {
    set_event_converter(Box::new(SerializedHtmlEventConverter));
    let mut dom = VirtualDom::new(app);

    _ = dom.rebuild();

    for _ in 0..3 {
        dom.handle_event(
            "click",
            Rc::new(PlatformEventData::new(Box::<SerializedMouseData>::default())),
            ElementId(2),
            true,
        );
        dom.process_events();
        _ = dom.render_immediate();
    }
}

fn app(cx: Scope) -> Element {
    let mut idx = use_state(cx, || 0);
    let onhover = |_| println!("go!");

    cx.render(rsx! {
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
                (0..**idx).map(|i| rsx! {
                    child_example { i: i, onhover: onhover }
                })
            }
        }
    })
}

#[inline_props]
fn child_example<'a>(cx: Scope<'a>, i: i32, onhover: EventHandler<'a, MouseEvent>) -> Element {
    cx.render(rsx! {
        li {
            onmouseover: move |e| onhover.call(e),
            "{i}"
        }
    })
}
