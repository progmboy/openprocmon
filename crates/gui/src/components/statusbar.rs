//! The bottom status bar (design `.statusbar`): a row of segments separated by
//! vertical dividers — capture state, shown/total counts, active filter/highlight/
//! bookmark indicators, and (right-aligned) the autoscroll state.

use gpui::{div, px, App, Div, Entity, Hsla, IntoElement, ParentElement, SharedString, Styled};
use gpui_component::{h_flex, ActiveTheme, Icon, StyledExt};
use rust_i18n::t;

use crate::app::AppState;
use crate::icons::PmIcon;
use crate::theme::palette;

pub(crate) fn render(state: &Entity<AppState>, cx: &App) -> impl IntoElement {
    let pal = palette(cx);
    let border = cx.theme().border;
    let muted = cx.theme().muted_foreground;
    let text2 = cx.theme().foreground.opacity(0.72); // design `--text-2`

    let s = state.read(cx);
    let capturing = s.capturing;
    let autoscroll = s.autoscroll;
    let visible = s.buffer.visible_len();
    let total = s.buffer.total();
    let filters = s.filter.rules.iter().filter(|r| r.enabled).count();
    let highlights = s.highlight.rules.len();
    let bookmarks = s.buffer.bookmark_count();

    // design `.status-dot`: run = success, pause = warn.
    let dot_color = if capturing {
        pal.res_success
    } else {
        pal.res_warn
    };

    let mut bar = h_flex()
        .w_full()
        .h(px(26.))
        // Chrome bars are `flex-shrink: 0` in the design; keep the fixed height.
        .flex_shrink_0()
        .items_center()
        .px(px(12.))
        .bg(cx.theme().title_bar) // design `--bg-2`
        .border_t_1()
        .border_color(border)
        .text_color(muted)
        .text_size(px(11.5));

    // Capture state (first segment — no left padding).
    bar = bar.child(
        seg(border)
            .pl(px(0.))
            .child(div().size(px(7.)).rounded_full().bg(dot_color))
            .child(
                if capturing {
                    t!("st.capturing")
                } else {
                    t!("st.paused")
                }
                .to_string(),
            ),
    );
    // Shown / total counts.
    bar = bar.child(
        seg(border)
            .child(t!("st.showing_label").to_string())
            .child(val(text2, visible)),
    );
    bar = bar.child(
        seg(border)
            .child(t!("st.total_label").to_string())
            .child(val(text2, total)),
    );
    // Active filter / highlight / bookmark indicators (only when present).
    if filters > 0 {
        bar = bar.child(
            seg(border)
                .child(Icon::new(PmIcon::Filter).size(px(12.)).text_color(muted))
                .child(t!("st.filters", n = filters).to_string()),
        );
    }
    if highlights > 0 {
        bar = bar.child(
            seg(border)
                .child(Icon::new(PmIcon::Highlight).size(px(12.)).text_color(muted))
                .child(t!("st.highlights", n = highlights).to_string()),
        );
    }
    if bookmarks > 0 {
        bar = bar.child(
            seg(border)
                .child(div().size(px(7.)).rounded_full().bg(pal.op_thread))
                .child(t!("st.bookmarks", n = bookmarks).to_string()),
        );
    }
    // Spacer + right-aligned autoscroll state (last segment — no divider).
    bar = bar.child(div().flex_1()).child(
        h_flex()
            .items_center()
            .gap(px(7.))
            .px(px(14.))
            .h_full()
            .child(
                if autoscroll {
                    t!("st.autoscroll_on")
                } else {
                    t!("st.autoscroll_off")
                }
                .to_string(),
            ),
    );
    bar
}

/// A `.seg`: gap 7, horizontal padding 14, full height, with a right divider.
fn seg(border: Hsla) -> Div {
    h_flex()
        .items_center()
        .gap(px(7.))
        .px(px(14.))
        .h_full()
        .border_r_1()
        .border_color(border)
}

/// A bold mono value (design `.statusbar b`).
fn val(text2: Hsla, n: usize) -> impl IntoElement {
    div()
        .text_color(text2)
        .font_semibold()
        .font_family("Consolas")
        .child(SharedString::from(n.to_string()))
}
