#![doc = include_str!("readme.md")]
#![doc(html_logo_url = "https://avatars.githubusercontent.com/u/79236386")]
#![doc(html_favicon_url = "https://avatars.githubusercontent.com/u/79236386")]
#![deny(missing_docs)]

mod app;
mod assets;
mod cfg;
mod desktop_context;
mod edits;
mod element;
mod escape;
mod eval;
mod events;
mod file_upload;
mod hooks;
mod ipc;
mod menubar;
mod protocol;
mod query;
mod shortcut;
mod waker;
mod webview;

#[cfg(feature = "collect-assets")]
mod collect_assets;

#[cfg(any(target_os = "ios", target_os = "android"))]
mod mobile_shortcut;

// The main entrypoint for this crate
pub use launch::*;
mod launch;

// Reexport tao and wry, might want to re-export other important things
pub use tao;
pub use tao::dpi::{LogicalPosition, LogicalSize};
pub use tao::event::WindowEvent;
pub use tao::window::WindowBuilder;
pub use wry;

// Public exports
pub use assets::AssetRequest;
pub use cfg::{Config, WindowCloseBehaviour};
pub use desktop_context::{
    window, DesktopContext, DesktopService, WryEventHandler, WryEventHandlerId,
};
pub use hooks::{use_asset_handler, use_global_shortcut, use_window, use_wry_event_handler};
pub use menubar::build_default_menu_bar;
pub use shortcut::{ShortcutHandle, ShortcutId, ShortcutRegistryError};
pub use wry::RequestAsyncResponder;
