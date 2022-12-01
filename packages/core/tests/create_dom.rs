#![allow(unused, non_upper_case_globals, non_snake_case)]

//! Prove that the dom works normally through virtualdom methods.
//!
//! This methods all use "rebuild" which completely bypasses the scheduler.
//! Hard rebuilds don't consume any events from the event queue.

use dioxus::core::Mutation::*;
use dioxus::prelude::*;
use dioxus_core::ElementId;

#[test]
fn test_original_diff() {
    let mut dom = VirtualDom::new(|cx| {
        cx.render(rsx! {
            div {
                div {
                    "Hello, world!"
                }
            }
        })
    });

    let edits = dom.rebuild().santize();
    assert_eq!(
        edits.templates,
        [
            // create template
            CreateElement { name: "div" },
            CreateElement { name: "div" },
            CreateStaticText { value: "Hello, world!" },
            AppendChildren { m: 1 },
            AppendChildren { m: 1 },
            SaveTemplate { name: "template", m: 1 },
        ]
    );

    assert_eq!(
        edits.edits,
        [
            // add to root
            LoadTemplate { name: "template", index: 0, id: ElementId(1) },
            AppendChildren { m: 1 }
        ]
    )
}

#[test]
fn create() {
    let mut dom = VirtualDom::new(|cx| {
        cx.render(rsx! {
            div {
                div {
                    "Hello, world!"
                    div {
                        div {
                            Fragment {
                                "hello"
                                "world"
                            }
                        }
                    }
                }
            }
        })
    });

    let edits = dom.rebuild().santize();
    assert_eq!(
        edits.templates,
        [
            // create template
            CreateElement { name: "div" },
            CreateElement { name: "div" },
            CreateStaticText { value: "Hello, world!" },
            CreateElement { name: "div" },
            CreateElement { name: "div" },
            CreateStaticPlaceholder {},
            AppendChildren { m: 1 },
            AppendChildren { m: 1 },
            AppendChildren { m: 2 },
            AppendChildren { m: 1 },
            SaveTemplate { name: "template", m: 1 },
            // The fragment child template
            CreateStaticText { value: "hello" },
            CreateStaticText { value: "world" },
            SaveTemplate { name: "template", m: 2 },
        ]
    );
}

#[test]
fn create_list() {
    let mut dom = VirtualDom::new(|cx| {
        cx.render(rsx! {
            (0..3).map(|f| rsx!( div { "hello" } ))
        })
    });

    let edits = dom.rebuild().santize();
    assert_eq!(
        edits.templates,
        [
            // create template
            CreateElement { name: "div" },
            CreateStaticText { value: "hello" },
            AppendChildren { m: 1 },
            SaveTemplate { name: "template", m: 1 }
        ]
    );
}

#[test]
fn create_simple() {
    let mut dom = VirtualDom::new(|cx| {
        cx.render(rsx! {
            div {}
            div {}
            div {}
            div {}
        })
    });

    let edits = dom.rebuild().santize();
    assert_eq!(
        edits.templates,
        [
            // create template
            CreateElement { name: "div" },
            CreateElement { name: "div" },
            CreateElement { name: "div" },
            CreateElement { name: "div" },
            // add to root
            SaveTemplate { name: "template", m: 4 }
        ]
    );
}
#[test]
fn create_components() {
    let mut dom = VirtualDom::new(|cx| {
        cx.render(rsx! {
            Child { "abc1" }
            Child { "abc2" }
            Child { "abc3" }
        })
    });

    #[derive(Props)]
    struct ChildProps<'a> {
        children: Element<'a>,
    }

    fn Child<'a>(cx: Scope<'a, ChildProps<'a>>) -> Element {
        cx.render(rsx! {
            h1 {}
            div { &cx.props.children }
            p {}
        })
    }

    let edits = dom.rebuild().santize();
    assert_eq!(
        edits.templates,
        [
            // The "child" template
            CreateElement { name: "h1" },
            CreateElement { name: "div" },
            CreateStaticPlaceholder {},
            AppendChildren { m: 1 },
            CreateElement { name: "p" },
            SaveTemplate { name: "template", m: 3 },
            // Sub template for component children
            CreateStaticText { value: "abc1" },
            SaveTemplate { name: "template", m: 1 },
            CreateStaticText { value: "abc2" },
            SaveTemplate { name: "template", m: 1 },
            CreateStaticText { value: "abc3" },
            SaveTemplate { name: "template", m: 1 }
        ]
    );
}

#[test]
fn anchors() {
    let mut dom = VirtualDom::new(|cx| {
        cx.render(rsx! {
            if true {
                rsx!( div { "hello" } )
            }
            if false {
                rsx!( div { "goodbye" } )
            }
        })
    });

    // note that the template under "false" doesn't show up since it's not loaded
    let edits = dom.rebuild().santize();
    assert_eq!(
        edits.templates,
        [
            // create each template
            CreateElement { name: "div" },
            CreateStaticText { value: "hello" },
            AppendChildren { m: 1 },
            SaveTemplate { m: 1, name: "template" },
        ]
    );

    assert_eq!(
        edits.edits,
        [
            LoadTemplate { name: "template", index: 0, id: ElementId(1) },
            CreatePlaceholder { id: ElementId(2) },
            AppendChildren { m: 2 }
        ]
    )
}
