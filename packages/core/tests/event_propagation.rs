use dioxus::prelude::*;
use dioxus_core::ElementId;
use std::{rc::Rc, sync::Mutex};

static CLICKS: Mutex<usize> = Mutex::new(0);

#[test]
fn events_propagate() {
    set_event_converter(Box::new(dioxus_html::SerializedHtmlEventConverter));

    let mut dom = VirtualDom::new(app);
    _ = dom.rebuild_to_vec(&mut dioxus_core::NoOpMutations);

    // Top-level click is registered
    dom.handle_event(
        "click",
        Rc::new(PlatformEventData::new(Box::<SerializedMouseData>::default())),
        ElementId(1),
        true,
    );
    assert_eq!(*CLICKS.lock().unwrap(), 1);

    // break reference....
    for _ in 0..5 {
        dom.mark_dirty(ScopeId(0));
        _ = dom.render_immediate_to_vec();
    }

    // Lower click is registered
    dom.handle_event(
        "click",
        Rc::new(PlatformEventData::new(Box::<SerializedMouseData>::default())),
        ElementId(2),
        true,
    );
    assert_eq!(*CLICKS.lock().unwrap(), 3);

    // break reference....
    for _ in 0..5 {
        dom.mark_dirty(ScopeId(0));
        _ = dom.render_immediate_to_vec();
    }

    // Stop propagation occurs
    dom.handle_event(
        "click",
        Rc::new(PlatformEventData::new(Box::<SerializedMouseData>::default())),
        ElementId(2),
        true,
    );
    assert_eq!(*CLICKS.lock().unwrap(), 3);
}

fn app(cx: Scope) -> Element {
    render! {
        div { onclick: move |_| {
                println!("top clicked");
                *CLICKS.lock().unwrap() += 1;
            },

            vec![
                render! {
                    problematic_child {}
                }
            ].into_iter()
        }
    }
}

fn problematic_child(cx: Scope) -> Element {
    render! {
        button { onclick: move |evt| {
                println!("bottom clicked");
                let mut clicks = CLICKS.lock().unwrap();
                if *clicks == 3 {
                    evt.stop_propagation();
                } else {
                    *clicks += 1;
                }
            } }
    }
}
