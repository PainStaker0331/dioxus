use std::rc::Rc;

use dioxus_core::ScopeState;
use wry::application::event_loop::EventLoopProxy;

use crate::UserWindowEvent;

type ProxyType = EventLoopProxy<UserWindowEvent>;

/// Desktop-Window handle api context
///
/// you can use this context control some window event
///
/// you can use `cx.consume_context::<DesktopContext>` to get this context
///
/// ```rust
///     let desktop = cx.consume_context::<DesktopContext>().unwrap();
/// ```
#[derive(Clone)]
pub struct DesktopContext {
    proxy: ProxyType,
}

impl DesktopContext {
    pub(crate) fn new(proxy: ProxyType) -> Self {
        Self { proxy }
    }

    /// trigger the drag-window event
    ///
    /// Moves the window with the left mouse button until the button is released.
    ///
    /// you need use it in `onmousedown` event:
    /// ```rust
    /// onmousedown: move |_| { desktop.drag_window(); }
    /// ```
    pub fn drag(&self) {
        let _ = self.proxy.send_event(UserWindowEvent::DragWindow);
    }

    /// set window minimize state
    pub fn minimize(&self, minimized: bool) {
        let _ = self.proxy.send_event(UserWindowEvent::Minimize(minimized));
    }

    /// set window maximize state
    pub fn maximize(&self, maximized: bool) {
        let _ = self.proxy.send_event(UserWindowEvent::Maximize(maximized));
    }

    pub fn visible(&self, visible: bool) {
        let _ = self.proxy.send_event(UserWindowEvent::Visible(visible));
    }

    /// close window
    pub fn close(&self) {
        let _ = self.proxy.send_event(UserWindowEvent::CloseWindow);
    }

    /// set window to focus
    pub fn focus(&self) {
        let _ = self.proxy.send_event(UserWindowEvent::FocusWindow);
    }

    /// set resizable state
    pub fn resizable(&self, resizable: bool) {
        let _ = self.proxy.send_event(UserWindowEvent::Resizable(resizable));
    }

    pub fn always_on_top(&self, top: bool) {
        let _ = self.proxy.send_event(UserWindowEvent::AlwaysOnTop(top));
    }

    pub fn cursor_visible(&self, visible: bool) {
        let _ = self
            .proxy
            .send_event(UserWindowEvent::CursorVisible(visible));
    }

    /// set window title
    pub fn set_title(&self, title: &str) {
        let _ = self
            .proxy
            .send_event(UserWindowEvent::SetTitle(String::from(title)));
    }

    /// hide the menu
    pub fn set_decorations(&self, decoration: bool) {
        let _ = self
            .proxy
            .send_event(UserWindowEvent::SetDecorations(decoration));
    }
}

/// use this function can get the `DesktopContext` context.
pub fn use_window(cx: &ScopeState) -> &Rc<DesktopContext> {
    cx.use_hook(|_| cx.consume_context::<DesktopContext>())
        .as_ref()
        .unwrap()
}
