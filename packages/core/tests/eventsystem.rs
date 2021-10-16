use bumpalo::Bump;

use anyhow::{Context, Result};
use dioxus::{prelude::*, DomEdit};
use dioxus_core as dioxus;
use dioxus_core_macro::*;
use dioxus_html as dioxus_elements;

#[async_std::test]
async fn event_queue_works() {
    static App: FC<()> = |(cx, props)| {
        cx.render(rsx! {
            div { "hello world" }
        })
    };

    let mut dom = VirtualDom::new(App);
    let edits = dom.rebuild();
}
