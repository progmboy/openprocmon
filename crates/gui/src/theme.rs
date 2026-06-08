//! Theme loading. Colors live in `themes/procmon.json`, not in code.
//!
//! Two layers, both read from that file:
//! - **gpui-component `ThemeConfig`** (one per appearance) — sparse overrides
//!   applied on top of the built-in defaults via [`Theme::apply_config`], the same
//!   mechanism gpui-component uses for its own themes. These cover the neutral
//!   chrome (background, border, panels, primary/button, ring, overlay, fonts…).
//! - **[`ProcmonPalette`]** (one per appearance) — the app's *semantic* colors
//!   (per-category operation colors, result/integrity/stack-frame colors) that are
//!   not part of a generic theme. Stored as a gpui [`Global`].
//!
//! "Appearance" (light/dark, i.e. [`ThemeMode`]) selects which of the two configs
//! to apply; we ship a single theme with two appearances, so there is no theme
//! *picker* — [`set_mode`] just swaps the appearance.

use std::rc::Rc;

use gpui::{rgb, Anchor, App, Global, Hsla, Window};
use gpui_component::scroll::ScrollbarShow;
use gpui_component::{Theme, ThemeConfig, ThemeMode};
use serde::Deserialize;

/// The theme definition, embedded at build time. Edit colors here, not in code.
const THEME_JSON: &str = include_str!("../themes/procmon.json");

/// Semantic colors for event categories, results, integrity and stack frames.
/// The active appearance's instance is stored as a global and read via [`palette`].
#[derive(Clone, Copy, Debug)]
pub struct ProcmonPalette {
    pub op_registry: Hsla,
    pub op_file: Hsla,
    pub op_network: Hsla,
    pub op_process: Hsla,
    pub op_thread: Hsla,
    pub op_perf: Hsla,
    pub res_success: Hsla,
    pub res_error: Hsla,
    pub res_warn: Hsla,
    pub res_info: Hsla,
    pub pid: Hsla,
    pub path: Hsla,
    /// Accent used for the selected-row bar, active toggles, focus, etc.
    pub row_sel_bar: Hsla,
    pub integrity_low: Hsla,
    pub integrity_medium: Hsla,
    pub integrity_high: Hsla,
    pub integrity_system: Hsla,
    pub frame_kernel: Hsla,
    pub frame_user: Hsla,
}

impl Global for ProcmonPalette {}

/// Reads the active palette. Cheap (`Copy`) — callers may clone freely.
pub fn palette(cx: &App) -> ProcmonPalette {
    *cx.global::<ProcmonPalette>()
}

// ---------------------------------------------------------------------------
// JSON config (`themes/procmon.json`)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ThemeFile {
    /// gpui-component theme configs (one per appearance).
    themes: Vec<ThemeConfig>,
    /// App semantic palettes per appearance.
    palette: PaletteSet,
}

#[derive(Deserialize)]
struct PaletteSet {
    dark: PaletteCfg,
    light: PaletteCfg,
}

/// Hex-string mirror of [`ProcmonPalette`] as stored in JSON.
#[derive(Deserialize)]
struct PaletteCfg {
    op_registry: String,
    op_file: String,
    op_network: String,
    op_process: String,
    op_thread: String,
    op_perf: String,
    res_success: String,
    res_error: String,
    res_warn: String,
    res_info: String,
    pid: String,
    path: String,
    row_sel_bar: String,
    integrity_low: String,
    integrity_medium: String,
    integrity_high: String,
    integrity_system: String,
    frame_kernel: String,
    frame_user: String,
}

/// Parses a `#rrggbb` string into an `Hsla`.
fn hex(s: &str) -> Hsla {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0);
    rgb(v).into()
}

impl From<&PaletteCfg> for ProcmonPalette {
    fn from(c: &PaletteCfg) -> Self {
        Self {
            op_registry: hex(&c.op_registry),
            op_file: hex(&c.op_file),
            op_network: hex(&c.op_network),
            op_process: hex(&c.op_process),
            op_thread: hex(&c.op_thread),
            op_perf: hex(&c.op_perf),
            res_success: hex(&c.res_success),
            res_error: hex(&c.res_error),
            res_warn: hex(&c.res_warn),
            res_info: hex(&c.res_info),
            pid: hex(&c.pid),
            path: hex(&c.path),
            row_sel_bar: hex(&c.row_sel_bar),
            integrity_low: hex(&c.integrity_low),
            integrity_medium: hex(&c.integrity_medium),
            integrity_high: hex(&c.integrity_high),
            integrity_system: hex(&c.integrity_system),
            frame_kernel: hex(&c.frame_kernel),
            frame_user: hex(&c.frame_user),
        }
    }
}

/// The parsed theme file, kept as a global so [`set_mode`] can re-apply per
/// appearance without re-parsing.
#[derive(Clone)]
struct ProcmonThemes {
    dark: Rc<ThemeConfig>,
    light: Rc<ThemeConfig>,
    dark_pal: ProcmonPalette,
    light_pal: ProcmonPalette,
}

impl Global for ProcmonThemes {}

fn load() -> ProcmonThemes {
    let file: ThemeFile = serde_json::from_str(THEME_JSON).expect("parse themes/procmon.json");
    let (mut dark, mut light) = (None, None);
    for theme in file.themes {
        if theme.mode.is_dark() {
            dark = Some(theme);
        } else {
            light = Some(theme);
        }
    }
    ProcmonThemes {
        dark: Rc::new(dark.expect("procmon.json: missing dark theme")),
        light: Rc::new(light.expect("procmon.json: missing light theme")),
        dark_pal: (&file.palette.dark).into(),
        light_pal: (&file.palette.light).into(),
    }
}

/// Applies the given appearance: the gpui-component config (on top of the
/// built-in defaults) plus the matching semantic palette.
fn apply(mode: ThemeMode, window: Option<&mut Window>, cx: &mut App) {
    // Ensures the `Theme` global exists, sets the mode, and applies the built-in
    // default as the base before our overrides.
    Theme::change(mode, None, cx);

    let themes = cx.global::<ProcmonThemes>().clone();
    let (config, pal) = if mode.is_dark() {
        (themes.dark.clone(), themes.dark_pal)
    } else {
        (themes.light.clone(), themes.light_pal)
    };
    Theme::global_mut(cx).apply_config(&config);
    // Show scrollbars on hover (default is fade-while-scrolling, which hides the
    // event table's horizontal scrollbar). Re-applied here so it survives a theme
    // switch. The `DataTable` reads this global; it has no per-table show mode.
    Theme::global_mut(cx).scrollbar_show = ScrollbarShow::Hover;
    // Toasts appear at the bottom-center of the window. Re-applied here so it
    // survives a theme switch (same reason as `scrollbar_show`).
    Theme::global_mut(cx).notification.placement = Anchor::BottomCenter;
    cx.set_global(pal);

    if let Some(window) = window {
        window.refresh();
    }
}

/// Installs the default appearance (dark) + its palette. Call once during app
/// bootstrap, after `gpui_component::init`.
pub fn init(cx: &mut App) {
    cx.set_global(load());
    apply(ThemeMode::Dark, None, cx);
}

/// Switches the appearance (light/dark), keeping the gpui-component theme and our
/// palette in sync.
pub fn set_mode(mode: ThemeMode, window: &mut Window, cx: &mut App) {
    apply(mode, Some(window), cx);
}
