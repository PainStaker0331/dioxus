use dioxus::dioxus_core::Mutation::*;
use dioxus::prelude::*;
use dioxus_core::{ElementId, NoOpMutations};

#[test]
fn text_diff() {
    fn app() -> Element {
        let gen = generation();
        rsx!( h1 { "hello {gen}" } )
    }

    let mut vdom = VirtualDom::new(app);
    vdom.rebuild(&mut NoOpMutations);

    vdom.mark_dirty(ScopeId::ROOT);
    assert_eq!(
        vdom.render_immediate_to_vec().edits,
        [SetText { value: "hello 1".to_string(), id: ElementId(2) }]
    );

    vdom.mark_dirty(ScopeId::ROOT);
    assert_eq!(
        vdom.render_immediate_to_vec().edits,
        [SetText { value: "hello 2".to_string(), id: ElementId(2) }]
    );

    vdom.mark_dirty(ScopeId::ROOT);
    assert_eq!(
        vdom.render_immediate_to_vec().edits,
        [SetText { value: "hello 3".to_string(), id: ElementId(2) }]
    );
}

#[test]
fn element_swap() {
    fn app() -> Element {
        let gen = generation();

        match gen % 2 {
            0 => rsx!( h1 { "hello 1" } ),
            1 => rsx!( h2 { "hello 2" } ),
            _ => unreachable!(),
        }
    }

    let mut vdom = VirtualDom::new(app);
    vdom.rebuild(&mut NoOpMutations);

    vdom.mark_dirty(ScopeId::ROOT);
    assert_eq!(
        vdom.render_immediate_to_vec().santize().edits,
        [
            LoadTemplate { name: "template", index: 0, id: ElementId(2,) },
            ReplaceWith { id: ElementId(1,), m: 1 },
        ]
    );

    vdom.mark_dirty(ScopeId::ROOT);
    assert_eq!(
        vdom.render_immediate_to_vec().santize().edits,
        [
            LoadTemplate { name: "template", index: 0, id: ElementId(1,) },
            ReplaceWith { id: ElementId(2,), m: 1 },
        ]
    );

    vdom.mark_dirty(ScopeId::ROOT);
    assert_eq!(
        vdom.render_immediate_to_vec().santize().edits,
        [
            LoadTemplate { name: "template", index: 0, id: ElementId(2,) },
            ReplaceWith { id: ElementId(1,), m: 1 },
        ]
    );

    vdom.mark_dirty(ScopeId::ROOT);
    assert_eq!(
        vdom.render_immediate_to_vec().santize().edits,
        [
            LoadTemplate { name: "template", index: 0, id: ElementId(1,) },
            ReplaceWith { id: ElementId(2,), m: 1 },
        ]
    );
}
