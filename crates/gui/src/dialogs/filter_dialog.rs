//! The Filter dialog: build include/exclude rules and apply them to the view.
//!
//! It is a real view entity (not an inline closure) because gpui-component renders
//! the dialog layer from `Root`; an entity re-renders itself on edits via
//! `cx.notify`. The column/relation/action selectors are click-to-cycle buttons
//! styled like dropdowns — functional and state-free across rebuilds.

use gpui::{
    div, prelude::FluentBuilder, px, AppContext, Context, Entity, Hsla, InteractiveElement,
    IntoElement, ParentElement, SharedString, StatefulInteractiveElement, Styled, WeakEntity,
    Window,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    select::{Select, SelectState},
    v_flex, ActiveTheme, Icon, IndexPath, StyledExt, WindowExt,
};

use rust_i18n::t;

use crate::app::AppView;
use crate::icons::PmIcon;
use crate::model::filter::{FilterAction, FilterColumn, FilterModel, FilterRelation, FilterRule};
use crate::theme::{palette, ProcmonPalette};

const ACTIONS: [FilterAction; 2] = [FilterAction::Include, FilterAction::Exclude];

// Fixed-width edge columns (px). The middle four columns use the design's
// proportional `fr` ratios (see `FR_*`) so the builder fields and list columns
// line up and adapt to the dialog width — matching the design's CSS grids.
const W_CHK: f32 = 28.0;
const W_DEL: f32 = 40.0;
// design `grid-template-columns: … 1.1fr 0.9fr 1.4fr 0.9fr …`
const FR_COL: f32 = 1.1;
const FR_REL: f32 = 0.9;
const FR_VAL: f32 = 1.4;
const FR_ACT: f32 = 0.9;

/// A flexible grid cell that grows by the given `fr` ratio (design `Nfr`).
fn fr(grow: f32) -> gpui::Div {
    div().flex_grow(grow).flex_basis(px(0.)).min_w(px(0.))
}

/// Which rule set this dialog edits — they share the same UI, differing only in
/// where Apply writes (the view filter vs the highlight rule set).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum RuleKind {
    Filter,
    Highlight,
}

pub(crate) struct FilterDialog {
    app: WeakEntity<AppView>,
    kind: RuleKind,
    value: Entity<InputState>,
    col_select: Entity<SelectState<Vec<SharedString>>>,
    rel_select: Entity<SelectState<Vec<SharedString>>>,
    act_select: Entity<SelectState<Vec<SharedString>>>,
    rules: Vec<FilterRule>,
}

impl FilterDialog {
    pub(crate) fn new(
        app: WeakEntity<AppView>,
        kind: RuleKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let value = cx.new(|cx| InputState::new(window, cx).placeholder("Value"));
        let col_items: Vec<SharedString> =
            FilterColumn::ALL.iter().map(|c| c.label().into()).collect();
        let rel_items: Vec<SharedString> = FilterRelation::ALL
            .iter()
            .map(|c| c.label().into())
            .collect();
        let act_items: Vec<SharedString> = ACTIONS.iter().map(|a| a.label().into()).collect();
        let col_select =
            cx.new(|cx| SelectState::new(col_items, Some(IndexPath::default()), window, cx));
        let rel_select =
            cx.new(|cx| SelectState::new(rel_items, Some(IndexPath::default()), window, cx));
        let act_select =
            cx.new(|cx| SelectState::new(act_items, Some(IndexPath::default()), window, cx));
        Self {
            app,
            kind,
            value,
            col_select,
            rel_select,
            act_select,
            rules: Vec::new(),
        }
    }

    /// Loads the current filter as the editable draft (call when opening).
    pub(crate) fn load(&mut self, model: &FilterModel) {
        self.rules = model.rules.clone();
    }

    /// Rough rendered height (px) used to vertically center the (content-sized)
    /// dialog. gpui-component can't auto-center a content-sized dialog, so we
    /// estimate from the rule count and set `margin_top` accordingly. Capped so a
    /// long list scrolls within the dialog instead of overflowing the screen.
    pub(crate) fn estimated_height(&self) -> f32 {
        const CHROME: f32 = 57.0 + 58.0 + 32.0 + 40.0 + 12.0 + 35.0; // head+foot+body+builder+gap+list head
        let list_body = if self.rules.is_empty() {
            64.0
        } else {
            self.rules.len() as f32 * 41.0
        };
        (CHROME + list_body).min(620.0)
    }

    /// The dialog footer button row (design `.dialog-foot`): Reset on the left,
    /// Cancel + Apply on the right. The [`FormDialog`](super::form_dialog::FormDialog)
    /// shell wraps this with the standard padding + top divider and pins it; the
    /// buttons are wired back to the entity via `dialog.update`.
    pub(crate) fn footer(dialog: &Entity<FilterDialog>) -> impl IntoElement {
        let (d_reset, d_cancel, d_apply) = (dialog.clone(), dialog.clone(), dialog.clone());
        h_flex()
            .w_full()
            .items_center()
            .gap_2()
            // All footer buttons are the design `.btn` height (34px), matching the
            // builder's Add button.
            .child(
                Button::new("flt-reset")
                    .ghost()
                    .h(px(34.))
                    .label(t!("dlg.reset").to_string())
                    .on_click(move |_, _, cx| {
                        d_reset.update(cx, |this, cx| this.reset(cx));
                    }),
            )
            .child(div().flex_1())
            .child(
                Button::new("flt-cancel")
                    .h(px(34.))
                    .label(t!("dlg.cancel").to_string())
                    .on_click(move |_, window, cx| {
                        d_cancel.update(cx, |this, cx| this.cancel(window, cx));
                    }),
            )
            .child(
                Button::new("flt-apply")
                    .primary()
                    .h(px(34.))
                    .icon(PmIcon::Check)
                    .label(t!("dlg.apply").to_string())
                    .on_click(move |_, window, cx| {
                        d_apply.update(cx, |this, cx| this.apply(window, cx));
                    }),
            )
    }

    fn add_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.value.read(cx).value().to_string();
        if value.trim().is_empty() {
            return;
        }
        let col_ix = self
            .col_select
            .read(cx)
            .selected_index(cx)
            .map(|p| p.row)
            .unwrap_or(0);
        let rel_ix = self
            .rel_select
            .read(cx)
            .selected_index(cx)
            .map(|p| p.row)
            .unwrap_or(0);
        let act_ix = self
            .act_select
            .read(cx)
            .selected_index(cx)
            .map(|p| p.row)
            .unwrap_or(0);
        self.rules.push(FilterRule::new(
            FilterColumn::ALL[col_ix],
            FilterRelation::ALL[rel_ix],
            value,
            ACTIONS[act_ix],
        ));
        self.value.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    fn apply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let model = FilterModel {
            rules: self.rules.clone(),
        };
        let kind = self.kind;
        self.app
            .update(cx, |view, cx| match kind {
                RuleKind::Filter => view.set_filter(model, cx),
                RuleKind::Highlight => view.set_highlight(model, cx),
            })
            .ok();
        window.close_dialog(cx);
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        self.rules.clear();
        cx.notify();
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.close_dialog(cx);
    }
}

impl gpui::Render for FilterDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let bg2 = cx.theme().title_bar; // design list-head / field color (`--bg-2`)
                                        // Empty-state text differs by kind (filter vs highlight).
        let empty_text = match self.kind {
            RuleKind::Filter => t!("dlg.no_rules"),
            RuleKind::Highlight => t!("dlg.no_highlights"),
        }
        .to_string();

        // Body only (the header/footer are pinned via the dialog's title/footer
        // slots). Content-sized so the dialog shrinks for an empty list and grows
        // with rules. Padded to the design `.dialog-body` (16px 18px).
        v_flex()
            .w_full()
            .gap_3()
            .px(px(18.))
            .py(px(16.))
            // Builder row (design `.filter-row-add`): column / relation / value /
            // action dropdowns + an Add button.
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    // Fields are the design's `.fld` height (34px); the default
                    // component size (md = 32px) is overridden via `.h()`.
                    .child(fr(FR_COL).child(Select::new(&self.col_select).w_full().h(px(34.))))
                    .child(fr(FR_REL).child(Select::new(&self.rel_select).w_full().h(px(34.))))
                    .child(
                        fr(FR_VAL).child(Input::new(&self.value).w_full().map(|mut i| {
                            // `Input::h()` is multi-line-only; set the Styled height so
                            // the single-line value field matches the 34px Selects.
                            i.style().size.height = Some(px(34.).into());
                            i
                        })),
                    )
                    .child(fr(FR_ACT).child(Select::new(&self.act_select).w_full().h(px(34.))))
                    .child(
                        Button::new("flt-add")
                            .primary()
                            .h(px(34.))
                            .label(t!("dlg.add").to_string())
                            .on_click(cx.listener(|this, _, window, cx| this.add_rule(window, cx))),
                    ),
            )
            // Rule list (design `.filter-list`): a bordered box with a header row
            // and one grid row per rule. Content-sized so it shrinks to fit an
            // empty list (the dialog as a whole scrolls when there are many rules).
            .child(
                v_flex()
                    .w_full()
                    .rounded(px(8.))
                    .overflow_hidden()
                    .border_1()
                    .border_color(border)
                    // Header row.
                    .child(
                        h_flex()
                            .w_full()
                            .px(px(12.))
                            .py(px(8.))
                            .gap_2()
                            .bg(bg2)
                            .border_b_1()
                            .border_color(border)
                            .child(div().w(px(W_CHK)).flex_shrink_0())
                            .child(fr(FR_COL).child(head_cell(&t!("dlg.col_column"), muted)))
                            .child(fr(FR_REL).child(head_cell(&t!("dlg.col_relation"), muted)))
                            .child(fr(FR_VAL).child(head_cell(&t!("dlg.value"), muted)))
                            .child(fr(FR_ACT).child(head_cell(&t!("dlg.col_action"), muted)))
                            .child(div().w(px(W_DEL)).flex_shrink_0()),
                    )
                    .when(self.rules.is_empty(), |this| {
                        this.child(
                            div()
                                .w_full()
                                .py(px(22.))
                                .text_center()
                                .text_color(muted)
                                .text_sm()
                                .child(empty_text),
                        )
                    })
                    .children(
                        self.rules
                            .iter()
                            .enumerate()
                            .map(|(i, rule)| rule_row(i, rule, cx)),
                    ),
            )
    }
}

/// A header label (design `.filter-list-head` cell): 11px, semibold, muted.
fn head_cell(label: &str, muted: Hsla) -> impl IntoElement {
    div()
        .text_size(px(11.))
        .font_semibold()
        .text_color(muted)
        .child(label.to_string())
}

/// The include/exclude action pill (design `.filter-item .act`): a rounded chip
/// tinted green for Include, red for Exclude.
fn action_pill(action: FilterAction, pal: &ProcmonPalette) -> impl IntoElement {
    let (color, key) = match action {
        FilterAction::Include => (pal.res_success, "dlg.include"),
        FilterAction::Exclude => (pal.res_error, "dlg.exclude"),
    };
    div()
        .px(px(8.))
        .py(px(2.))
        .rounded_full()
        .bg(color.opacity(0.16))
        .text_color(color)
        .text_xs()
        .font_semibold()
        .child(rust_i18n::t!(key).to_string())
}

/// One rule row (design `.filter-item`): checkbox · column · relation · value ·
/// action pill · delete, on the same column grid as the header. Disabled rules
/// dim their text.
fn rule_row(i: usize, rule: &FilterRule, cx: &mut Context<FilterDialog>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let border = cx.theme().border;
    let row_hover = cx.theme().list_hover;
    let panel2 = cx.theme().secondary_hover;
    let pal = palette(cx);
    let enabled = rule.enabled;
    let text = if enabled {
        cx.theme().foreground
    } else {
        muted
    };

    h_flex()
        .id(("flt-row", i))
        .w_full()
        .px(px(12.))
        .py(px(9.))
        .gap_2()
        .items_center()
        .when(i > 0, |this| {
            this.border_t_1().border_color(border.opacity(0.55))
        })
        .hover(move |s| s.bg(row_hover))
        .child(
            div().w(px(W_CHK)).flex_shrink_0().child(
                Checkbox::new(("flt-rule", i))
                    .checked(enabled)
                    .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                        if let Some(r) = this.rules.get_mut(i) {
                            r.enabled = *checked;
                            cx.notify();
                        }
                    })),
            ),
        )
        .child(
            fr(FR_COL).child(
                div()
                    .truncate()
                    .text_color(text)
                    .text_size(px(11.5))
                    .font_family("Consolas")
                    .child(rule.column.label().to_string()),
            ),
        )
        .child(
            fr(FR_REL).child(
                div()
                    .text_color(muted)
                    .text_size(px(11.5))
                    .child(rule.relation.label().to_string()),
            ),
        )
        .child(
            fr(FR_VAL).child(
                div()
                    .truncate()
                    .text_color(text)
                    .text_size(px(11.5))
                    .font_family("Consolas")
                    .child(rule.value.clone()),
            ),
        )
        .child(fr(FR_ACT).child(action_pill(rule.action, &pal)))
        .child(
            div().w(px(W_DEL)).flex_shrink_0().child(
                div()
                    .id(("flt-del", i))
                    .size(px(24.))
                    .rounded(px(5.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(muted)
                    .hover(move |s| s.bg(panel2))
                    .child(Icon::new(PmIcon::Trash).size(px(14.)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if i < this.rules.len() {
                            this.rules.remove(i);
                            cx.notify();
                        }
                    })),
            ),
        )
}
