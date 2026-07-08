//! The Process Activity Summary dialog (Tools menu) — design `SummaryDialog`
//! (`kind: "process"`). Aggregates the captured rows per process (PID) into
//! file/registry/network/total counts, sorted by total desc, in a scrollable
//! table with a live filter box. Read-only.
//!
//! It's a view entity (not a stateless render) because the filter box edits state;
//! the shared `FormDialog` provides the centered chrome + header, and this entity
//! renders the toolbar + table + footer (so "Showing top N" tracks the filter).

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    div, prelude::FluentBuilder, px, AppContext, Context, Entity, Hsla, InteractiveElement,
    IntoElement, ParentElement, Pixels, Render, SharedString, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
    v_flex, ActiveTheme, Icon, Sizable, StyledExt, WindowExt,
};
use rust_i18n::t;

use crate::icons::PmIcon;
use crate::model::domain::{EventCategory, EventSummaryRow};
use crate::theme::{palette, ProcmonPalette};

/// Number of rows rendered (design caps the table at 200).
const MAX_ROWS: usize = 200;
// Fixed numeric column widths; the process column grows.
const W_PID: f32 = 72.;
const W_FILE: f32 = 74.;
const W_REG: f32 = 88.;
const W_NET: f32 = 78.;
const W_TOTAL: f32 = 74.;

/// One aggregated process row.
struct ProcRow {
    name: SharedString,
    pid: u32,
    icon: Option<Arc<gpui::Image>>,
    file: usize,
    registry: usize,
    network: usize,
    total: usize,
}

pub(crate) struct ProcessSummaryDialog {
    search: Entity<InputState>,
    query: String,
    rows: Vec<ProcRow>,
    total_events: usize,
}

impl ProcessSummaryDialog {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("dlg.sum_filter").to_string()));
        cx.subscribe(&search, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.query = input.read(cx).value().to_string();
                cx.notify();
            }
        })
        .detach();
        Self {
            search,
            query: String::new(),
            rows: Vec::new(),
            total_events: 0,
        }
    }

    /// Aggregates a fresh snapshot of the captured rows (call when opening).
    pub(crate) fn load(&mut self, rows: &[EventSummaryRow], cx: &mut Context<Self>) {
        self.total_events = rows.len();
        self.rows = aggregate(rows);
        cx.notify();
    }

    /// Header sub-text ("N items · M events"), over the currently visible (filtered) rows.
    pub(crate) fn summary_text(&self) -> String {
        t!("dlg.sum_items", n = self.rows.len(), m = self.total_events).to_string()
    }

    fn filtered(&self) -> Vec<&ProcRow> {
        let q = self.query.to_lowercase();
        self.rows
            .iter()
            .filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q))
            .take(MAX_ROWS)
            .collect()
    }
}

impl Render for ProcessSummaryDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let co = Co::new(cx);
        let filtered = self.filtered();
        let shown = filtered.len();

        v_flex()
            .w_full()
            // Toolbar (design `.sum-toolbar`): a single filter box.
            .child(
                h_flex()
                    .flex_shrink_0()
                    .px(px(16.))
                    .py(px(12.))
                    .border_b_1()
                    .border_color(co.border)
                    .child(
                        Input::new(&self.search)
                            .small()
                            .w_full()
                            .prefix(Icon::new(PmIcon::Search).size(px(13.)).text_color(co.muted))
                            .cleanable(true)
                            .map(|mut i| {
                                i.style().size.height = Some(px(30.).into());
                                i
                            }),
                    ),
            )
            // Sticky header row (design `.sum-table th`).
            .child(
                h_flex()
                    .flex_shrink_0()
                    .bg(co.panel)
                    .border_b_1()
                    .border_color(co.border)
                    .child(th(t!("sumc.process").to_string(), None, false, &co))
                    .child(th("PID".to_string(), Some(px(W_PID)), true, &co))
                    .child(th(t!("sumc.file").to_string(), Some(px(W_FILE)), true, &co))
                    .child(th(
                        t!("sumc.registry").to_string(),
                        Some(px(W_REG)),
                        true,
                        &co,
                    ))
                    .child(th(
                        t!("sumc.network").to_string(),
                        Some(px(W_NET)),
                        true,
                        &co,
                    ))
                    .child(th(
                        t!("sumc.total").to_string(),
                        Some(px(W_TOTAL)),
                        true,
                        &co,
                    )),
            )
            // Scrollable body (fixed height bounds the scroll region).
            .child(
                div().h(px(420.)).min_h(px(0.)).child(
                    div()
                        .id("proc-sum-body")
                        .size_full()
                        .when(shown == 0, |this| {
                            this.child(
                                div()
                                    .py(px(30.))
                                    .w_full()
                                    .text_center()
                                    .text_color(co.muted)
                                    .text_size(px(12.))
                                    .child(t!("dlg.sum_no_data").to_string()),
                            )
                        })
                        .children(filtered.into_iter().map(|r| data_row(r, &co)))
                        .overflow_y_scrollbar(),
                ),
            )
            // Footer (design `.dialog-foot`): "Showing top N items" + Close.
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .px(px(18.))
                    .py(px(13.))
                    .border_t_1()
                    .border_color(co.border)
                    .child(
                        div()
                            .flex_1()
                            .text_color(co.muted)
                            .text_size(px(11.5))
                            .child(t!("dlg.sum_showing", n = shown).to_string()),
                    )
                    .child(
                        Button::new("proc-sum-close")
                            .primary()
                            .h(px(34.))
                            .label(t!("dlg.close").to_string())
                            .on_click(cx.listener(|_, _, window, cx| window.close_dialog(cx))),
                    ),
            )
    }
}

/// Resolved colors for the dialog.
struct Co {
    fg: Hsla,
    muted: Hsla,
    border: Hsla,
    panel: Hsla,
    hover: Hsla,
    pal: ProcmonPalette,
}

impl Co {
    fn new(cx: &gpui::App) -> Self {
        Self {
            fg: cx.theme().foreground,
            muted: cx.theme().muted_foreground,
            border: cx.theme().border,
            panel: cx.theme().secondary,
            hover: cx.theme().table_hover,
            pal: palette(cx),
        }
    }
}

/// A header cell (design `.sum-table th`): muted 11px; `num` cols right-aligned.
fn th(label: String, width: Option<Pixels>, num: bool, co: &Co) -> impl IntoElement {
    div()
        .px(px(14.))
        .py(px(9.))
        .text_size(px(11.))
        .font_semibold()
        .text_color(co.muted)
        .map(|d| match width {
            Some(w) => d.w(w).text_right(),
            None => d.flex_1().min_w(px(0.)),
        })
        .when(num && width.is_none(), |d| d.text_right())
        .child(label)
}

/// One data row (design `.sum-table td` + `tr:hover`).
fn data_row(r: &ProcRow, co: &Co) -> impl IntoElement {
    let icon_color = co.pal.proc_color(&r.name);
    h_flex()
        .items_center()
        .border_b_1()
        .border_color(co.border.opacity(0.5))
        .text_size(px(11.5))
        .hover(|s| s.bg(co.hover))
        // Process (design `.sum-name`): icon + name.
        .child(
            h_flex()
                .flex_1()
                .min_w(px(0.))
                .items_center()
                .gap(px(8.))
                .px(px(14.))
                .py(px(7.))
                .child(crate::components::app_icon(
                    r.icon.as_ref(),
                    &r.name,
                    icon_color,
                    16.,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .truncate()
                        .text_color(co.fg)
                        .font_medium()
                        .child(r.name.clone()),
                ),
        )
        .child(num_cell(r.pid.to_string(), W_PID, co.muted, false, co))
        .child(num_cell(
            count(r.file),
            W_FILE,
            color_for(r.file, co.pal.op_file, co),
            false,
            co,
        ))
        .child(num_cell(
            count(r.registry),
            W_REG,
            color_for(r.registry, co.pal.op_registry, co),
            false,
            co,
        ))
        .child(num_cell(
            count(r.network),
            W_NET,
            color_for(r.network, co.pal.op_network, co),
            false,
            co,
        ))
        .child(num_cell(r.total.to_string(), W_TOTAL, co.fg, true, co))
}

/// A right-aligned numeric cell (design `.sum-table td.num`, mono).
fn num_cell(text: String, width: f32, color: Hsla, bold: bool, _co: &Co) -> impl IntoElement {
    div()
        .w(px(width))
        .px(px(14.))
        .py(px(7.))
        .text_right()
        .font_family("Consolas")
        .text_color(color)
        .when(bold, |d| d.font_bold())
        .child(text)
}

fn count(n: usize) -> String {
    if n == 0 {
        "—".to_string()
    } else {
        n.to_string()
    }
}

/// Category count color: the op color when non-zero, faint otherwise.
fn color_for(n: usize, op: Hsla, co: &Co) -> Hsla {
    if n > 0 {
        op
    } else {
        co.muted.opacity(0.45)
    }
}

/// Aggregates rows per PID into file/registry/network/total counts (total desc).
fn aggregate(rows: &[EventSummaryRow]) -> Vec<ProcRow> {
    let mut map: HashMap<u32, ProcRow> = HashMap::new();
    for row in rows {
        let entry = map.entry(row.pid).or_insert_with(|| ProcRow {
            name: row.process_name.clone(),
            pid: row.pid,
            icon: row.icon.clone(),
            file: 0,
            registry: 0,
            network: 0,
            total: 0,
        });
        match row.category {
            EventCategory::File => entry.file += 1,
            EventCategory::Registry => entry.registry += 1,
            EventCategory::Network => entry.network += 1,
            _ => {}
        }
        entry.total += 1;
    }
    let mut out: Vec<ProcRow> = map.into_values().collect();
    out.sort_by_key(|r| std::cmp::Reverse(r.total));
    out
}


