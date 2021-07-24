use dioxus::prelude::*;

fn main() {
    let g = dioxus::prelude::LazyNodes::new(move |__cx: NodeFactory| {
        use dioxus_elements::{GlobalAttributes, SvgAttributes};
        __cx.element(
            dioxus_elements::button,
            __cx.bump()
                .alloc([dioxus::events::on::onclick(__cx, move |_| {})]),
            __cx.bump().alloc([]),
            __cx.bump().alloc([]),
            None,
        )
    });
}
