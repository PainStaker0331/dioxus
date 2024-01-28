use dioxus::prelude::*;

fn main() {
    launch_desktop(app);
}

fn app() -> Element {
    let mut state = use_signal(|| 0);
    let mut depth = use_signal(|| 0 as usize);
    let mut items = use_memo(move || (0..depth()).map(|f| f as _).collect::<Vec<isize>>());

    let a = use_memo(move || state() + 1);

    println!("rendering app");

    rsx! {
        button { onclick: move |_| state += 1, "Increment" }
        button { onclick: move |_| depth += 1, "Add depth" }
        button { onclick: move |_| depth -= 1, "Remove depth" }
        Child {
            depth: depth.into(),
            items: items,
            state: a,
        }
    }
}

#[component]
fn Child(
    state: ReadOnlySignal<isize>,
    items: ReadOnlySignal<Vec<isize>>,
    depth: ReadOnlySignal<usize>,
) -> Element {
    if depth() == 0 {
        return None;
    }

    // These memos don't get re-computed when early returns happen
    // In dioxus futures spawned with use_future won't progress if they don't get hit during rendering
    let state = use_memo(move || state() + 1);
    let item = use_memo(move || items()[depth()]);
    let depth = use_memo(move || depth() - 1);

    println!("rendering child: {}", depth());

    rsx! {
        h3 { "Depth({depth})-Item({item}): {state}"}
        Child {
            depth,
            state,
            items
        }
    }
}
