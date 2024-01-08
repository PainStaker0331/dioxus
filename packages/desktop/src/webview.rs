use crate::{
    app::SharedContext,
    assets::AssetHandlerRegistry,
    edits::EditQueue,
    eval::DesktopEvalProvider,
    ipc::{EventData, UserWindowEvent},
    protocol::{self},
    waker::tao_waker,
    Config, DesktopContext, DesktopService,
};
use dioxus_core::VirtualDom;
use dioxus_html::prelude::EvalProvider;
use futures_util::{pin_mut, FutureExt};
use std::{rc::Rc, task::Waker};
use wry::{RequestAsyncResponder, WebContext, WebViewBuilder};

pub struct WebviewInstance {
    pub dom: VirtualDom,
    pub desktop_context: DesktopContext,
    pub waker: Waker,

    // Wry assumes the webcontext is alive for the lifetime of the webview.
    // We need to keep the webcontext alive, otherwise the webview will crash
    _web_context: WebContext,
}

impl WebviewInstance {
    pub fn new(mut cfg: Config, dom: VirtualDom, shared: Rc<SharedContext>) -> WebviewInstance {
        let window = cfg.window.clone().build(&shared.target).unwrap();

        // TODO: allow users to specify their own menubars, again :/
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        crate::menubar::build_menu(&window, cfg.enable_default_menu_bar);

        // We assume that if the icon is None in cfg, then the user just didnt set it
        if cfg.window.window.window_icon.is_none() {
            window.set_window_icon(Some(
                tao::window::Icon::from_rgba(
                    include_bytes!("./assets/default_icon.bin").to_vec(),
                    460,
                    460,
                )
                .expect("image parse failed"),
            ));
        }

        let mut web_context = WebContext::new(cfg.data_dir.clone());
        let edit_queue = EditQueue::default();
        let asset_handlers = AssetHandlerRegistry::new(dom.runtime());
        let headless = !cfg.window.window.visible;

        // Rust :(
        let window_id = window.id();
        let file_handler = cfg.file_drop_handler.take();
        let custom_head = cfg.custom_head.clone();
        let index_file = cfg.custom_index.clone();
        let root_name = cfg.root_name.clone();
        let asset_handlers_ = asset_handlers.clone();
        let edit_queue_ = edit_queue.clone();
        let proxy_ = shared.proxy.clone();

        let request_handler = move |request, responder: RequestAsyncResponder| {
            // Try to serve the index file first
            let index_bytes = protocol::index_request(
                &request,
                custom_head.clone(),
                index_file.clone(),
                &root_name,
                headless,
            );

            // Otherwise, try to serve an asset, either from the user or the filesystem
            match index_bytes {
                Some(body) => responder.respond(body),
                None => protocol::desktop_handler(
                    request,
                    asset_handlers_.clone(),
                    &edit_queue_,
                    responder,
                ),
            }
        };

        let ipc_handler = move |payload: String| {
            // defer the event to the main thread
            if let Ok(message) = serde_json::from_str(&payload) {
                _ = proxy_.send_event(UserWindowEvent(EventData::Ipc(message), window_id));
            }
        };

        let mut webview = WebViewBuilder::new(&window)
            .with_transparent(cfg.window.window.transparent)
            .with_url("dioxus://index.html/")
            .unwrap()
            .with_ipc_handler(ipc_handler)
            .with_asynchronous_custom_protocol(String::from("dioxus"), request_handler)
            .with_file_drop_handler(file_drop_handler)
            .with_web_context(&mut web_context);

        if let Some(handler) = file_handler {
            webview = webview.with_file_drop_handler(handler)
        }

        // This was removed from wry, I'm not sure what replaced it
        // #[cfg(windows)]
        // {
        //     // Windows has a platform specific settings to disable the browser shortcut keys
        //     use wry::WebViewBuilderExtWindows;
        //     webview = webview.with_browser_accelerator_keys(false);
        // }

        if let Some(color) = cfg.background_color {
            webview = webview.with_background_color(color);
        }

        for (name, handler) in cfg.protocols.drain(..) {
            webview = webview.with_custom_protocol(name, handler);
        }

        const INITIALIZATION_SCRIPT: &str = r#"
        if (document.addEventListener) {
        document.addEventListener('contextmenu', function(e) {
            e.preventDefault();
        }, false);
        } else {
        document.attachEvent('oncontextmenu', function() {
            window.event.returnValue = false;
        });
        }
    "#;

        if cfg.disable_context_menu {
            // in release mode, we don't want to show the dev tool or reload menus
            webview = webview.with_initialization_script(INITIALIZATION_SCRIPT)
        } else {
            // in debug, we are okay with the reload menu showing and dev tool
            webview = webview.with_devtools(true);
        }

        let desktop_context = Rc::from(DesktopService::new(
            webview.build().unwrap(),
            window,
            shared.clone(),
            edit_queue,
            asset_handlers,
        ));

        // Provide the desktop context to the virtualdom
        dom.base_scope().provide_context(desktop_context.clone());

        // Also set up its eval provider
        // It's important that we provide as dyn EvalProvider - using the concrete type has
        // a different TypeId.
        let provider: Rc<dyn EvalProvider> =
            Rc::new(DesktopEvalProvider::new(desktop_context.clone()));
        dom.base_scope().provide_context(provider);

        WebviewInstance {
            waker: tao_waker(shared.proxy.clone(), desktop_context.window.id()),
            desktop_context,
            dom,
            _web_context: web_context,
        }
    }

    pub fn poll_vdom(&mut self) {
        let mut cx = std::task::Context::from_waker(&self.waker);

        // Continously poll the virtualdom until it's pending
        // Wait for work will return Ready when it has edits to be sent to the webview
        // It will return Pending when it needs to be polled again - nothing is ready
        loop {
            {
                let fut = self.dom.wait_for_work();
                pin_mut!(fut);

                match fut.poll_unpin(&mut cx) {
                    std::task::Poll::Ready(_) => {}
                    std::task::Poll::Pending => return,
                }
            }

            self.desktop_context.send_edits(self.dom.render_immediate());
        }
    }
}
