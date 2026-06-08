//! The About dialog (Help menu) — design `AboutDialog`: a small centered card with
//! NO header bar (unlike the other dialogs, so it deliberately does not use
//! `FormDialog`). Just a gradient logo, the product name, a tagline, a description
//! and a copyright line, over an OK footer. Stateless, so it's built inline.

use gpui::{div, px, App, IntoElement, ParentElement, Styled};
use gpui_component::{
    button::{Button, ButtonVariants},
    h_flex, v_flex, ActiveTheme, StyledExt, WindowExt,
};
use rust_i18n::t;

/// The card body (design `.about` body: centered, `padding: 30px 26px`).
pub(crate) fn body(cx: &App) -> impl IntoElement {
    let fg = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let text2 = fg.opacity(0.72);
    let faint = muted.opacity(0.6);

    v_flex()
        .w_full()
        .items_center()
        .text_center()
        .px(px(26.))
        .py(px(30.))
        // Brand logo (design `.brand-logo` enlarged to 56×56, radius 14) — the app icon.
        .child(div().mb(px(16.)).child(crate::components::brand_icon(56.)))
        .child(
            div()
                .text_color(fg)
                .text_size(px(19.))
                .font_bold()
                .child("OpenProcmon"),
        )
        .child(
            div()
                .mt(px(4.))
                .text_color(muted)
                .text_size(px(12.))
                .child(t!("dlg.about_tagline").to_string()),
        )
        .child(
            div()
                .mt(px(16.))
                .text_color(text2)
                .text_size(px(12.))
                .line_height(px(20.))
                .child(t!("dlg.about_desc").to_string()),
        )
        .child(
            div()
                .mt(px(18.))
                .text_color(faint)
                .text_size(px(11.))
                .child(t!("dlg.about_copyright").to_string()),
        )
}

/// The footer's OK button row (design `.dialog-foot`, with a top divider).
pub(crate) fn footer(cx: &App) -> impl IntoElement {
    div()
        .w_full()
        .px(px(18.))
        .py(px(13.))
        .border_t_1()
        .border_color(cx.theme().border)
        .child(
            h_flex().w_full().justify_end().child(
                Button::new("about-ok")
                    .primary()
                    .h(px(34.))
                    .label(t!("dlg.ok").to_string())
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            ),
        )
}
