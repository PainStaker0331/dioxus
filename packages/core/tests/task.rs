use std::time::Duration;

use dioxus_core::*;

#[tokio::test]
async fn it_works() {
    let mut dom = VirtualDom::new(app);

    let _ = dom.rebuild();

    tokio::select! {
        _ = dom.wait_for_work() => {}
        _ = tokio::time::sleep(Duration::from_millis(1000)) => {}
    };
}

fn app(cx: Scope) -> Element {
    cx.use_hook(|| {
        cx.spawn(async {
            for x in 0..10 {
                tokio::time::sleep(Duration::from_millis(50)).await;
                println!("Hello, world! {x}");
            }
        });

        cx.spawn(async {
            for x in 0..10 {
                tokio::time::sleep(Duration::from_millis(25)).await;
                println!("Hello, world from second thread! {x}");
            }
        });
    });

    None
}
