#![allow(unused, non_upper_case_globals)]
#![allow(non_snake_case)]

//! Tests for the lifecycle of components.
use dioxus::dioxus_core::{ElementId, Mutation::*};
use dioxus::prelude::*;
use dioxus_html::SerializedHtmlEventConverter;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

type Shared<T> = Arc<Mutex<T>>;

#[test]
fn manual_diffing() {
    #[derive(Clone)]
    struct AppProps {
        value: Shared<&'static str>,
    }

    fn app(cx: AppProps) -> Element {
        let val = cx.value.lock().unwrap();
        rsx! { div { "{val}" } }
    };

    let value = Arc::new(Mutex::new("Hello"));
    let mut dom = VirtualDom::new_with_props(app, AppProps { value: value.clone() });

    dom.rebuild(&mut dioxus_core::NoOpMutations);

    *value.lock().unwrap() = "goodbye";

    assert_eq!(
        dom.rebuild_to_vec().santize().edits,
        [
            LoadTemplate { name: "template", index: 0, id: ElementId(3) },
            HydrateText { path: &[0], value: "goodbye".to_string(), id: ElementId(4) },
            AppendChildren { m: 1, id: ElementId(0) }
        ]
    );
}

#[test]
fn events_generate() {
    set_event_converter(Box::new(SerializedHtmlEventConverter));
    fn app() -> Element {
        let mut count = use_signal(|| 0);

        match count() {
            0 => rsx! {
                div { onclick: move |_| count += 1,
                    div { "nested" }
                    "Click me!"
                }
            },
            _ => None,
        }
    };

    let mut dom = VirtualDom::new(app);
    dom.rebuild(&mut dioxus_core::NoOpMutations);

    dom.handle_event(
        "click",
        Rc::new(PlatformEventData::new(Box::<SerializedMouseData>::default())),
        ElementId(1),
        true,
    );

    dom.mark_dirty(ScopeId::ROOT);
    let edits = dom.render_immediate_to_vec();

    assert_eq!(
        edits.edits,
        [
            CreatePlaceholder { id: ElementId(2) },
            ReplaceWith { id: ElementId(1), m: 1 }
        ]
    )
}

// #[test]
// fn components_generate() {
//     fn app() -> Element {
//         let render_phase = use_hook(|| 0);
//         *render_phase += 1;

//         match *render_phase {
//             1 => rsx_without_templates!("Text0"),
//             2 => rsx_without_templates!(div {}),
//             3 => rsx_without_templates!("Text2"),
//             4 => rsx_without_templates!(Child {}),
//             5 => rsx_without_templates!({ None as Option<()> }),
//             6 => rsx_without_templates!("text 3"),
//             7 => rsx_without_templates!({ (0..2).map(|f| rsx_without_templates!("text {f}")) }),
//             8 => rsx_without_templates!(Child {}),
//             _ => todo!(),
//         })
//     };

//     fn Child() -> Element {
//         println!("Running child");
//         render_without_templates! {
//             h1 {}
//         })
//     }

//     let mut dom = VirtualDom::new(app);
//     let edits = dom.rebuild_to_vec();
//     assert_eq!(
//         edits.edits,
//         [
//             CreateTextNode { root: Some(1), text: "Text0" },
//             AppendChildren { root: Some(0), children: vec![1] }
//         ]
//     );

//     assert_eq!(
//         dom.hard_diff(ScopeId::ROOT).edits,
//         [
//             CreateElement { root: Some(2), tag: "div", children: 0 },
//             ReplaceWith { root: Some(1), nodes: vec![2] }
//         ]
//     );

//     assert_eq!(
//         dom.hard_diff(ScopeId::ROOT).edits,
//         [
//             CreateTextNode { root: Some(1), text: "Text2" },
//             ReplaceWith { root: Some(2), nodes: vec![1] }
//         ]
//     );

//     // child {}
//     assert_eq!(
//         dom.hard_diff(ScopeId::ROOT).edits,
//         [
//             CreateElement { root: Some(2), tag: "h1", children: 0 },
//             ReplaceWith { root: Some(1), nodes: vec![2] }
//         ]
//     );

//     // placeholder
//     assert_eq!(
//         dom.hard_diff(ScopeId::ROOT).edits,
//         [
//             CreatePlaceholder { root: Some(1) },
//             ReplaceWith { root: Some(2), nodes: vec![1] }
//         ]
//     );

//     assert_eq!(
//         dom.hard_diff(ScopeId::ROOT).edits,
//         [
//             CreateTextNode { root: Some(2), text: "text 3" },
//             ReplaceWith { root: Some(1), nodes: vec![2] }
//         ]
//     );

//     assert_eq!(
//         dom.hard_diff(ScopeId::ROOT).edits,
//         [
//             CreateTextNode { text: "text 0", root: Some(1) },
//             CreateTextNode { text: "text 1", root: Some(3) },
//             ReplaceWith { root: Some(2), nodes: vec![1, 3] },
//         ]
//     );

//     assert_eq!(
//         dom.hard_diff(ScopeId::ROOT).edits,
//         [
//             CreateElement { tag: "h1", root: Some(2), children: 0 },
//             ReplaceWith { root: Some(1), nodes: vec![2] },
//             Remove { root: Some(3) },
//         ]
//     );
// }
