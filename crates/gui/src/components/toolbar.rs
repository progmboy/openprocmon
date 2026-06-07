//! The toolbar: capture controls, view actions, search, language and theme.
//!
//! The icon buttons are custom interactive elements (not gpui-component `Button`s)
//! because the design needs per-button control the Button widget can't express:
//! a dim icon that brightens on hover, the clear button turning red on hover, an
//! accent fill for active toggles, and an exact 34px button with a 17px icon.

use gpui::{
    div, prelude::FluentBuilder, px, svg, Context, Div, Entity, Hsla, InteractiveElement,
    IntoElement, ParentElement, SharedString, Stateful, StatefulInteractiveElement, Styled,
};
use gpui_component::{
    h_flex,
    input::{Input, InputState},
    tooltip::Tooltip,
    ActiveTheme, Icon, IconNamed, Sizable, StyledExt,
};
use rust_i18n::t;

use crate::app::{AppState, AppView};
use crate::components::separator;
use crate::icons::PmIcon;
use crate::theme::palette;

/// Toolbar colors derived once from the active theme + palette.
#[derive(Clone, Copy)]
struct TbColors {
    dim: Hsla,
    bright: Hsla,
    hover_bg: Hsla,
    accent: Hsla,
    accent_fg: Hsla,
    red: Hsla,
}

const BTN: f32 = 35.0;
const ICON: f32 = 16.0;

/// A ghost icon button matching the design `.tbtn`: dim by default, brighter (or
/// red, for `danger`) on hover, or an accent fill when `active`.
fn icon_btn(
    id: &'static str,
    icon: PmIcon,
    tip: &str,
    active: bool,
    danger: bool,
    c: TbColors,
    cx: &mut Context<AppView>,
    on_click: impl Fn(&mut AppView, &mut gpui::Window, &mut Context<AppView>) + 'static,
) -> Stateful<Div> {
    let tip = SharedString::from(tip.to_string());
    let group = SharedString::from(id);
    let hover_fg = if danger { c.red } else { c.bright };
    let icon_color = if active { c.accent_fg } else { c.dim };

    // The glyph is a raw `svg` (not `Icon`) so its color can change on hover: an
    // `Icon` resolves its color at build time, before hover applies, but `svg`'s
    // color is part of its computed (paint-time) style, so `group_hover` works.
    let glyph = svg()
        .flex_none()
        .size(px(ICON))
        .path(icon.path())
        .text_color(icon_color)
        .when(!active, |s| {
            s.group_hover(group.clone(), move |s| s.text_color(hover_fg))
        });

    let base = div()
        .id(id)
        .group(group)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(BTN))
        .rounded(px(5.))
        .cursor_pointer()
        .child(glyph)
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_click(cx.listener(move |view, _, window, cx| on_click(view, window, cx)));

    if active {
        base.bg(c.accent)
    } else {
        base.hover(move |s| s.bg(c.hover_bg))
    }
}

/// One segment of the language toggle (design `.lang-toggle button`): muted +
/// transparent when inactive (text/`--panel-2` bg on hover), accent-blue fill +
/// white text when active.
fn lang_seg(
    code: &'static str,
    label: &'static str,
    active: bool,
    c: TbColors,
    cx: &mut Context<AppView>,
) -> Stateful<Div> {
    let base = div()
        .id(code)
        .flex()
        .items_center()
        .justify_center()
        .h_full()
        .px(px(11.))
        .cursor_pointer()
        .font_semibold()
        .text_sm()
        .text_color(if active { c.accent_fg } else { c.dim })
        .child(label)
        .on_click(cx.listener(move |view, _, window, cx| view.set_locale(code, window, cx)));

    if active {
        base.bg(c.accent)
    } else {
        base.hover(move |s| s.bg(c.hover_bg).text_color(c.bright))
    }
}

pub(crate) fn render(
    state: &Entity<AppState>,
    search_input: &Entity<InputState>,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let (capturing, autoscroll, always_on_top, is_dark) = {
        let s = state.read(cx);
        (s.capturing, s.autoscroll, s.always_on_top, s.theme_mode.is_dark())
    };
    let has_filter = !state.read(cx).filter.rules.is_empty();
    let is_zh = rust_i18n::locale().starts_with("zh");

    let pal = palette(cx);
    let c = TbColors {
        dim: cx.theme().foreground.opacity(0.72),
        bright: cx.theme().foreground,
        hover_bg: cx.theme().secondary_hover,
        // Active-toggle fill = the design's accent blue (same in both themes); the
        // theme's `primary` is near-white in dark mode, so use the palette accent.
        accent: pal.row_sel_bar,
        accent_fg: gpui::white(),
        red: pal.res_error,
    };
    let green = pal.res_success;

    // Capture: a fixed-width labeled button, green while capturing. The eye glyph
    // is an `svg` so it brightens on hover (see `icon_btn`); the label text is the
    // div's own text and changes via the div hover.
    let cap_fg = if capturing { green } else { c.dim };
    let cap_hover_fg = if capturing { green } else { c.bright };
    let cap_group = SharedString::from("capture");
    let eye = svg()
        .flex_none()
        .size(px(16.))
        .path(if capturing { PmIcon::Pause } else { PmIcon::Play }.path())
        .text_color(cap_fg)
        .when(!capturing, |s| {
            s.group_hover(cap_group.clone(), move |s| s.text_color(c.bright))
        });
    let capture = div()
        .id("capture")
        .group(cap_group)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap_2()
        .h(px(BTN))
        .w(px(104.))
        .rounded(px(5.))
        .cursor_pointer()
        .text_color(cap_fg)
        .hover(move |s| s.text_color(cap_hover_fg).bg(c.hover_bg))
        .child(eye)
        .child(
            if capturing {
                t!("tb.pause")
            } else {
                t!("tb.capture")
            }
            .to_string(),
        )
        .tooltip({
            let tip = SharedString::from(t!("tb.capture_tip").to_string());
            move |window, cx| Tooltip::new(tip.clone()).build(window, cx)
        })
        .on_click(cx.listener(|view, _, _, cx| view.toggle_capture(cx)));

    h_flex()
        .w_full()
        .h(px(48.))
        // Design `.toolbar { flex-shrink: 0 }`: without this the bar is a v_flex
        // child with the default flex-shrink:1 and gets squeezed below 48px.
        .flex_shrink_0()
        .items_center()
        .gap(px(4.))
        .px(px(10.))
        .bg(cx.theme().secondary)
        .border_b_1()
        .border_color(cx.theme().border)
        .child(icon_btn(
            "open",
            PmIcon::Open,
            &t!("tb.open"),
            false,
            false,
            c,
            cx,
            |v, w, cx| v.open_pml_dialog(w, cx),
        ))
        .child(icon_btn(
            "save",
            PmIcon::Save,
            &t!("tb.save"),
            false,
            false,
            c,
            cx,
            |v, w, cx| v.open_save_dialog(w, cx),
        ))
        .child(separator(cx))
        .child(capture)
        .child(icon_btn(
            "autoscroll",
            PmIcon::Scroll,
            &t!("tb.autoscroll"),
            autoscroll,
            false,
            c,
            cx,
            |v, _, cx| v.toggle_autoscroll(cx),
        ))
        .child(icon_btn(
            "clear",
            PmIcon::Trash,
            &t!("tb.clear"),
            false,
            true,
            c,
            cx,
            |v, _, cx| v.clear(cx),
        ))
        .child(separator(cx))
        .child(icon_btn(
            "filter",
            PmIcon::FilterFill,
            &t!("tb.filter"),
            has_filter,
            false,
            c,
            cx,
            |v, w, cx| v.open_filter_dialog(w, cx),
        ))
        .child(icon_btn(
            "highlight",
            PmIcon::Highlight,
            &t!("tb.highlight"),
            false,
            false,
            c,
            cx,
            |v, w, cx| v.open_highlight_dialog(w, cx),
        ))
        .child(icon_btn(
            "from-window",
            PmIcon::Crosshair,
            &t!("tb.from_window"),
            false,
            false,
            c,
            cx,
            |_, _, _| {},
        ))
        .child(icon_btn(
            "tree",
            PmIcon::Tree,
            &t!("tb.tree"),
            false,
            false,
            c,
            cx,
            |v, w, cx| v.open_tree_dialog(w, cx),
        ))
        .child(icon_btn(
            "always-on-top",
            PmIcon::Pin,
            &t!("menu.always_on_top"),
            always_on_top,
            false,
            c,
            cx,
            |v, w, cx| v.toggle_always_on_top(w, cx),
        ))
        .child(icon_btn(
            "jump",
            PmIcon::Jump,
            &t!("tb.jump"),
            false,
            false,
            c,
            cx,
            |_, _, _| {},
        ))
        // Search box: forced to exactly the button height; `large` keeps the
        // design's 8px icon-text gap. Input renders its own border/bg/prefix.
        .child(
            gpui::div().flex_1().min_w(px(160.)).child(
                Input::new(search_input)
                    .large()
                    .h(px(BTN))
                    .prefix(Icon::new(PmIcon::Search).with_size(px(15.)))
                    .cleanable(true),
            ),
        )
        .child(separator(cx))
        // Language toggle: a custom segmented control (design `.lang-toggle`) so
        // the active segment gets the design's accent-blue fill + white text — the
        // built-in ButtonGroup applies a single variant to both buttons and can't
        // express the transparent/muted vs accent/white two-tone.
        .child(
            h_flex()
                .h(px(30.))
                .flex_shrink_0()
                .rounded(px(5.))
                .overflow_hidden()
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().secondary)
                .child(lang_seg("zh", "中", is_zh, c, cx))
                .child(lang_seg("en", "EN", !is_zh, c, cx)),
        )
        .child(icon_btn(
            "theme",
            if is_dark { PmIcon::Sun } else { PmIcon::Moon },
            &t!("tb.theme"),
            false,
            false,
            c,
            cx,
            |v, w, cx| v.toggle_theme(w, cx),
        ))
}
