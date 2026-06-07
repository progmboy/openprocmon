//! Global actions + key bindings, dispatched to [`AppView`]'s root (handled in
//! `app.rs`) or globally. The menu bar (`menubar.rs`) maps menu items to these.

use gpui::{App, Action, KeyBinding, SharedString, actions};
use gpui_component::ThemeMode;
use serde::Deserialize;

actions!(
    procmon,
    [
        // Toolbar / shortcuts.
        ToggleCapture,
        ToggleAutoscroll,
        ClearDisplay,
        FocusSearch,
        // Event menu.
        OpenFilter,
        ClearFilter,
        ToggleAdvancedDisplay,
        OpenHighlight,
        ClearHighlight,
        Bookmark,
        WebSearch,
        // Tools menu.
        OpenTree,
        OpenSummary,
        OpenProcessSummary,
        OpenFileSummary,
        OpenRegSummary,
        OpenNetSummary,
        OpenXrefSummary,
        // File menu.
        Open,
        Save,
        SaveAs,
        ImportSettings,
        ExportSettings,
        Quit,
        // Edit menu.
        Copy,
        // Options / Help.
        OpenSettings,
        AlwaysOnTop,
        ConfigureSymbols,
        HistoryDepth,
        HelpTopics,
        CheckUpdates,
        About,
    ]
);

/// Switch to a specific appearance (Options ▸ Theme submenu).
#[derive(Action, Clone, PartialEq)]
#[action(namespace = procmon, no_json)]
pub struct SwitchThemeMode(pub ThemeMode);

/// Switch the UI locale (Options ▸ Language submenu).
#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = procmon, no_json)]
pub struct SelectLocale(pub SharedString);

/// Registers the default key bindings. `None` context = active everywhere. These
/// also surface as the shortcut column in the menus.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-e", ToggleCapture, None),
        KeyBinding::new("ctrl-a", ToggleAutoscroll, None),
        KeyBinding::new("ctrl-x", ClearDisplay, None),
        KeyBinding::new("ctrl-f", FocusSearch, None),
        KeyBinding::new("ctrl-l", OpenFilter, None),
        KeyBinding::new("ctrl-b", Bookmark, None),
        KeyBinding::new("ctrl-o", Open, None),
        KeyBinding::new("ctrl-s", Save, None),
        KeyBinding::new("ctrl-c", Copy, None),
        KeyBinding::new("ctrl-,", OpenSettings, None),
        KeyBinding::new("f1", HelpTopics, None),
    ]);
}
