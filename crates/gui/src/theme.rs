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

use gpui_kit::component::scroll::ScrollbarMode;
use gpui_kit::component::{Theme, ThemeConfig, ThemeMode, ThemeToken};
use gpui_kit::{rgb, Anchor, App, Global, Hsla, Window};
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

impl ProcmonPalette {
    /// A stable per-process accent color: hash the name into one of the six
    /// operation colors (used for process avatars in the summaries and tree).
    pub fn proc_color(&self, name: &str) -> Hsla {
        let h = name.bytes().fold(0u32, |a, b| a.wrapping_add(b as u32));
        match h % 6 {
            0 => self.op_registry,
            1 => self.op_file,
            2 => self.op_network,
            3 => self.op_process,
            4 => self.op_thread,
            _ => self.op_perf,
        }
    }
}

impl Global for ProcmonPalette {}

/// Reads the active palette. Cheap (`Copy`) — callers may clone freely.
pub fn palette(cx: &App) -> ProcmonPalette {
    *cx.global::<ProcmonPalette>()
}

/// The row tint for a highlighted event, for the active appearance.
///
/// The six choices in Settings ▸ Appearance are one fixed set (design
/// `HL_COLORS`), all pale — they are picked to sit on a dark row. Washed over the
/// light theme's white rows at the same strength they leave almost no contrast
/// and the highlight is invisible, so here the colour is darkened and laid on
/// more strongly. That mirrors what `procmon.json` itself does between its two
/// palettes: the same hue at a much lower lightness (its amber goes `#f0c36b`
/// dark → `#b97e12` light), which is also how the design's own light theme
/// redefines the `--op-*` vars this tint falls back to.
pub fn highlight_tint(color: Hsla, dark: bool) -> Hsla {
    if dark {
        color.opacity(0.18)
    } else {
        Hsla {
            l: color.l.min(0.45),
            ..color
        }
        .opacity(0.30)
    }
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
    if cx.global::<BackgroundImage>().0 {
        thin_chrome(cx);
    }
    // `apply_config` writes the theme's public fields, which does not reach the
    // gpui-base layer's own copy — and base paints on its own: scrollbars, resize
    // handles, and the text-selection highlight in the detail panel. Without this
    // they would keep the built-in default colors instead of `procmon.json`'s.
    Theme::sync_base(cx);
    // Show scrollbars on hover (default is fade-while-scrolling, which hides the
    // event table's horizontal scrollbar). Re-applied here so it survives a theme
    // switch. The `DataTable` reads this global; it has no per-table show mode.
    // Must go through the setter (it mirrors the mode into the base theme too)
    // and after `sync_base`, which rebuilds that theme from scratch.
    Theme::set_scrollbar_mode(ScrollbarMode::Hover, cx);
    // Toasts appear at the bottom-center of the window. Re-applied here so it
    // survives a theme switch (same reason as `scrollbar_mode`).
    Theme::global_mut(cx).notification.placement = Anchor::BottomCenter;
    cx.set_global(pal);

    if let Some(window) = window {
        window.refresh();
    }
}

/// Whether the main window is painting a background image, so [`apply`] knows to
/// thin out the chrome. A global because the appearance can be re-applied at any
/// time (light/dark switch) and has to come back translucent.
#[derive(Clone, Copy, Default)]
struct BackgroundImage(bool);

impl Global for BackgroundImage {}

/// Makes the surfaces that sit over the window translucent, so a background image
/// reads through all of them — title bar to status bar — instead of only showing
/// in the gaps.
///
/// This is deliberately done on the *theme*, not on each region: every surface
/// already paints with one of these tokens, so mixing alpha in here reaches the
/// whole window (including `DataTable`, which we do not draw ourselves) without a
/// single component changing how it renders. With no image set, nothing below
/// runs and the theme is exactly what `procmon.json` describes.
///
/// The base layer stays opaque: `tokens.background` is what the window itself is
/// cleared to, and the image is painted over it, so it must not be see-through.
/// Percentages come from the design (`gui-design-v2/styles.css`, `.app.has-bg`).
fn thin_chrome(cx: &mut App) {
    let theme = Theme::global_mut(cx);
    // Menu bar, tool bar, monitor bar, status bar, detail panel (design: --panel).
    theme.title_bar = theme.title_bar.opacity(0.80);
    theme.secondary = theme.secondary.opacity(0.82);
    theme.secondary_hover = theme.secondary_hover.opacity(0.82);
    // The monitor bar and the detail panel's field boxes (design: --bg / --bg-2).
    theme.background = theme.background.opacity(0.75);
    theme.table_head = theme.table_head.opacity(0.88);
    // The event table paints from the semantic tokens rather than the colors above.
    // Only its container is tinted. A striped row fills `table_even` *over* that
    // tint, and two translucent layers stack, so the stripe would come out far more
    // opaque than its neighbours and band the picture. `AppView::render` turns the
    // stripe off while an image is set; zeroing the token here makes that fill a
    // no-op as well, so the banding cannot come back through this path alone.
    theme.tokens.table = fade(theme.tokens.table, 0.72);
    theme.tokens.table_even = fade(theme.tokens.table_even, 0.);
    theme.tokens.table_hover = fade(theme.tokens.table_hover, 0.88);
    theme.tokens.table_head = fade(theme.tokens.table_head, 0.88);
    theme.tokens.table_foot = fade(theme.tokens.table_foot, 0.88);
}

/// A semantic token at `alpha` of its opacity. Both halves are faded: a token
/// carries a flat color *and* a `Background` that may be a gradient, and tables
/// paint from the latter.
fn fade(token: ThemeToken, alpha: f32) -> ThemeToken {
    ThemeToken::new(token.color.opacity(alpha), token.background.opacity(alpha))
}

/// Installs the default appearance (dark) + its palette. Call once during app
/// bootstrap, after `gpui_kit::component::init`.
pub fn init(cx: &mut App) {
    cx.set_global(load());
    cx.set_global(BackgroundImage::default());
    apply(ThemeMode::Dark, None, cx);
}

/// Switches the appearance (light/dark), keeping the gpui-component theme and our
/// palette in sync.
pub fn set_mode(mode: ThemeMode, window: &mut Window, cx: &mut App) {
    apply(mode, Some(window), cx);
}

/// Records whether the main window paints a background image. Takes effect on the
/// next [`apply`], so callers pair it with [`set_mode`] (which always re-applies).
pub fn set_background_image(on: bool, cx: &mut App) {
    cx.set_global(BackgroundImage(on));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The six highlight choices are pale, tuned to sit on a dark row. Reusing
    /// that wash on the light theme's white rows leaves no contrast — which is
    /// the whole reason [`highlight_tint`] branches on the appearance.
    #[test]
    fn the_light_highlight_tint_is_darker_and_stronger() {
        // Design `HL_COLORS` amber — the default highlight color.
        let amber: Hsla = rgb(0xf0c36b).into();
        let on_dark = highlight_tint(amber, true);
        let on_light = highlight_tint(amber, false);

        assert_eq!(on_dark.l, amber.l, "a dark row takes the color as picked");
        assert!(on_light.l < on_dark.l, "a white row needs a darker tint");
        assert!(on_light.a > on_dark.a, "laid on more strongly, too");
        assert_eq!(on_light.h, amber.h, "the hue the user picked is kept");
    }

    /// Only pale colors are pulled down; a choice that already reads on white is
    /// left where it is rather than being driven towards black.
    #[test]
    fn an_already_dark_highlight_color_keeps_its_lightness() {
        // `procmon.json`'s light-palette green, well below the clamp.
        let deep: Hsla = rgb(0x1f9d57).into();
        assert_eq!(highlight_tint(deep, false).l, deep.l);
    }
}
