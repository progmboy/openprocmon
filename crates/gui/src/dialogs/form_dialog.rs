//! A reusable shell for the app's form-style dialogs (Filter, Highlight, …).
//!
//! In the design every such dialog shares the same chrome: an accent icon +
//! title + description header with a full-width divider, a padded body, and a
//! footer with a full-width top divider and a variable set of buttons — the whole
//! dialog vertically centered. `FormDialog` wraps gpui-component's [`Dialog`] and
//! applies that chrome consistently, so each dialog only supplies its body and
//! footer buttons.
//!
//! Usage (inside a `window.open_dialog` closure):
//! ```ignore
//! FormDialog::new(d)
//!     .icon(PmIcon::Filter)
//!     .title(t!("dlg.filter"))
//!     .description(t!("dlg.filter_hint"))
//!     .width(px(760.))
//!     .estimated_height(px(est))
//!     .footer(my_buttons_row)
//!     .body(my_entity.clone())
//!     .build(window, cx)
//! ```

use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, ParentElement, Pixels, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use gpui_component::{ActiveTheme, Icon, StyledExt, WindowExt, dialog::Dialog, h_flex};

use crate::icons::PmIcon;
use crate::theme::palette;

/// Builder that applies the shared form-dialog chrome to a gpui-component [`Dialog`].
pub(crate) struct FormDialog {
    dialog: Dialog,
    icon: PmIcon,
    title: SharedString,
    description: Option<SharedString>,
    width: Pixels,
    /// Approximate rendered height, used to vertically center the (content-sized)
    /// dialog via `margin_top` — gpui-component can't auto-center one.
    est_height: Pixels,
    body: Option<AnyElement>,
    footer: Option<AnyElement>,
}

impl FormDialog {
    pub(crate) fn new(dialog: Dialog) -> Self {
        Self {
            dialog,
            icon: PmIcon::Info,
            title: SharedString::default(),
            description: None,
            width: px(560.),
            est_height: px(360.),
            body: None,
            footer: None,
        }
    }

    pub(crate) fn icon(mut self, icon: PmIcon) -> Self {
        self.icon = icon;
        self
    }

    pub(crate) fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = title.into();
        self
    }

    pub(crate) fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub(crate) fn width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    pub(crate) fn estimated_height(mut self, height: Pixels) -> Self {
        self.est_height = height;
        self
    }

    /// The dialog body (typically the dialog's view entity). Rendered in a
    /// scroll area between the header and footer; supply its own inner padding.
    pub(crate) fn body(mut self, body: impl IntoElement) -> Self {
        self.body = Some(body.into_any_element());
        self
    }

    /// The footer's button row (e.g. `h_flex` with Reset + spacer + Cancel/Apply).
    /// The shell wraps it with the standard padding + top divider.
    pub(crate) fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    pub(crate) fn build(self, window: &mut Window, cx: &App) -> Dialog {
        let accent = palette(cx).row_sel_bar;
        let muted = cx.theme().muted_foreground;
        let fg = cx.theme().foreground;
        let border = cx.theme().border;

        // Header (design `.dialog-head`): icon + title + description + bottom rule.
        let mut header = h_flex()
            .w_full()
            .items_center()
            .gap(px(10.))
            .px(px(18.))
            .py(px(14.))
            .border_b_1()
            .border_color(border)
            .child(Icon::new(self.icon).size(px(18.)).text_color(accent))
            .child(div().text_color(fg).text_size(px(14.)).font_semibold().child(self.title));
        if let Some(desc) = self.description {
            header = header.child(div().text_color(muted).text_sm().child(desc));
        }
        // Our own close button (design `.dialog-head .x`: 28×28, rounded, centered)
        // — we disable gpui's built-in one below so it can sit inside the header row,
        // which also makes the header the design height.
        let hover_bg = cx.theme().secondary_hover;
        header = header.child(div().flex_1()).child(
            div()
                .id("dialog-close")
                .flex_shrink_0()
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .text_color(muted)
                .hover(move |s| s.bg(hover_bg).text_color(fg))
                .child(Icon::new(crate::icons::PmIcon::X).size(px(16.)))
                .on_click(|_, window, cx| window.close_dialog(cx)),
        );

        let vh = window.viewport_size().height;
        let mt = ((vh - self.est_height) / 2.0).max(px(24.));

        // `p_0` clears gpui's default section padding so our header/body/footer own
        // all padding and the dividers reach the dialog's edges. `close_button(false)`
        // disables gpui's built-in (top-right, off-center) X in favor of ours.
        let mut d = self
            .dialog
            .p_0()
            .w(self.width)
            .max_h(vh * 0.86)
            .margin_top(mt)
            .close_button(false)
            .title(header);
        if let Some(footer) = self.footer {
            d = d.footer(
                div()
                    .w_full()
                    .px(px(18.))
                    .py(px(13.))
                    .border_t_1()
                    .border_color(border)
                    .child(footer),
            );
        }
        if let Some(body) = self.body {
            d = d.child(body);
        }
        d
    }
}
