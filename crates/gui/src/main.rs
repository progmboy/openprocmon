//! procmon-gui — a gpui + gpui-component rebuild of the OpenProcessMonitor
//! "Process Monitor" UI, driven by the real kernel-driver SDK (live capture) and
//! the PML reader (offline `.PML` viewing).

// Release builds are pure GUI apps: use the Windows GUI subsystem so launching the
// exe never spawns a console window. Debug builds keep the console so `tracing`
// output stays visible while developing.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app;
mod components;
mod dialogs;
mod icons;
mod model;
mod sysicon;
mod theme;

use app::AppView;
use gpui::{px, size, AppContext, Bounds, WindowBounds, WindowOptions};
use gpui_component::{Root, TitleBar};
use icons::Assets;

// Loads `locales/*.yml` for this crate's `t!` calls (English fallback).
rust_i18n::i18n!("locales", fallback = "en");

fn main() {
    // master gpui has no `Application::new()`; the platform-backed app is created
    // by `gpui_platform::application()` (this is also what the gpui-component story
    // app uses as its entry point).
    gpui_platform::application().with_assets(Assets).run(|cx| {
        gpui_component::init(cx);
        theme::init(cx);
        actions::bind_keys(cx);

        let bounds = Bounds::centered(None, size(px(1150.), px(750.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            // Custom client-side title bar (drawn by `menubar`/`TitleBar`); this
            // hides the OS title bar while keeping working window controls + drag.
            titlebar: Some(TitleBar::title_bar_options()),
            ..Default::default()
        };

        cx.open_window(options, |window, cx| {
            // The window's root entity must be `Root` so the dialog/notification/
            // sheet layers work; it wraps our actual `AppView`.
            let view = cx.new(|cx| AppView::new(window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("failed to open window");

        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    // Verifies the locale file parses and keys resolve in both languages.
    #[test]
    fn locales_load() {
        rust_i18n::set_locale("en");
        assert_eq!(rust_i18n::t!("menu.file"), "File");
        assert_eq!(rust_i18n::t!("dt.tab_stack"), "Stack");
        rust_i18n::set_locale("zh");
        assert_eq!(rust_i18n::t!("menu.file"), "文件");
        assert_eq!(rust_i18n::t!("mon.network"), "网络");
    }
}
