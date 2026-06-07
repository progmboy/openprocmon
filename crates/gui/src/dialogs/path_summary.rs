//! The File / Registry / Network summary dialogs (Tools menu) — design
//! `SummaryDialog` (`kind: "file" | "registry" | "network"`). All three share the
//! same layout (group captured rows by path, three sub-counts + total), differing
//! only in title/icon and the column headers, so one parameterized dialog covers
//! them via [`PathKind`].
//!
//! Also hosts the Cross Reference summary ([`XrefSummaryDialog`]) — paths touched
//! by more than one process — since it's the same path-grouped table shell, just
//! with different columns (process count + process list).
//!
//! Like the other summaries these are view entities (the filter box edits state);
//! the shared `FormDialog` provides the chrome and they render toolbar+table+foot.

use std::collections::{HashMap, HashSet};

use gpui::{
    div, prelude::FluentBuilder, px, AppContext, Context, Entity, Hsla, InteractiveElement,
    IntoElement, ParentElement, Pixels, Render, SharedString, Styled, Window,
};
use gpui_component::{
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
    v_flex, ActiveTheme, Icon, Sizable, StyledExt, WindowExt,
    button::{Button, ButtonVariants},
};
use rust_i18n::t;

use crate::icons::PmIcon;
use crate::model::domain::{EventCategory, EventSummaryRow};
use crate::theme::{palette, ProcmonPalette};

const MAX_ROWS: usize = 200;
const W_NUM: f32 = 92.;
const W_TOTAL: f32 = 84.;

/// Which path-grouped summary to show.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathKind {
    File,
    Registry,
    Network,
}

impl PathKind {
    fn category(self) -> EventCategory {
        match self {
            PathKind::File => EventCategory::File,
            PathKind::Registry => EventCategory::Registry,
            PathKind::Network => EventCategory::Network,
        }
    }

    pub(crate) fn icon(self) -> PmIcon {
        match self {
            PathKind::File => PmIcon::Filesys,
            PathKind::Registry => PmIcon::Registry,
            PathKind::Network => PmIcon::Network,
        }
    }

    pub(crate) fn title(self) -> String {
        match self {
            PathKind::File => t!("dlg.file_summary").to_string(),
            PathKind::Registry => t!("dlg.reg_summary").to_string(),
            PathKind::Network => t!("dlg.net_summary").to_string(),
        }
    }

    /// Column header labels: (path/connection, a, b, c).
    fn headers(self) -> (String, String, String, String) {
        match self {
            PathKind::File => (
                t!("sumc.path").to_string(),
                t!("sumc.reads").to_string(),
                t!("sumc.writes").to_string(),
                t!("sumc.procs").to_string(),
            ),
            PathKind::Registry => (
                t!("sumc.path").to_string(),
                t!("sumc.opens").to_string(),
                t!("sumc.queries").to_string(),
                t!("sumc.sets").to_string(),
            ),
            PathKind::Network => (
                t!("sumc.connection").to_string(),
                t!("sumc.sends").to_string(),
                t!("sumc.receives").to_string(),
                t!("sumc.procs").to_string(),
            ),
        }
    }
}

/// One aggregated path row. `a`/`b`/`c` meaning depends on the kind (see headers).
struct PathRow {
    path: SharedString,
    a: usize,
    b: usize,
    c: usize,
    total: usize,
}

pub(crate) struct PathSummaryDialog {
    search: Entity<InputState>,
    query: String,
    kind: PathKind,
    rows: Vec<PathRow>,
    total_events: usize,
}

impl PathSummaryDialog {
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
            kind: PathKind::File,
            rows: Vec::new(),
            total_events: 0,
        }
    }

    /// Sets the kind + aggregates a fresh snapshot (call when opening).
    pub(crate) fn load(&mut self, kind: PathKind, rows: &[EventSummaryRow], cx: &mut Context<Self>) {
        self.kind = kind;
        self.total_events = rows.len();
        self.rows = aggregate(kind, rows);
        cx.notify();
    }

    /// Header sub-text ("N items · M events") — total, not filtered (design).
    pub(crate) fn summary_text(&self) -> String {
        t!("dlg.sum_items", n = self.rows.len(), m = self.total_events).to_string()
    }

    fn filtered(&self) -> Vec<&PathRow> {
        let q = self.query.to_lowercase();
        self.rows
            .iter()
            .filter(|r| q.is_empty() || r.path.to_lowercase().contains(&q))
            .take(MAX_ROWS)
            .collect()
    }
}

impl Render for PathSummaryDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let co = Co::new(cx);
        // Send/Receive counts are network-colored; other sub-counts are muted.
        let ab_color = if self.kind == PathKind::Network { co.pal.op_network } else { co.muted };
        let (h_path, h_a, h_b, h_c) = self.kind.headers();
        let filtered = self.filtered();
        let shown = filtered.len();

        v_flex()
            .w_full()
            // Toolbar (design `.sum-toolbar`).
            .child(
                h_flex().flex_shrink_0().px(px(16.)).py(px(12.)).border_b_1().border_color(co.border).child(
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
            // Sticky header row.
            .child(
                h_flex()
                    .flex_shrink_0()
                    .bg(co.panel)
                    .border_b_1()
                    .border_color(co.border)
                    .child(th(h_path, None, &co))
                    .child(th(h_a, Some(px(W_NUM)), &co))
                    .child(th(h_b, Some(px(W_NUM)), &co))
                    .child(th(h_c, Some(px(W_NUM)), &co))
                    .child(th(t!("sumc.total_n").to_string(), Some(px(W_TOTAL)), &co)),
            )
            // Scrollable body.
            .child(
                div().h(px(420.)).min_h(px(0.)).child(
                    div()
                        .id("path-sum-body")
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
                        .children(filtered.into_iter().map(|r| data_row(r, ab_color, &co)))
                        .overflow_y_scrollbar(),
                ),
            )
            // Footer (design `.dialog-foot`).
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
                        Button::new("path-sum-close")
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

/// A header cell (design `.sum-table th`). Numeric cols (fixed width) right-align.
fn th(label: String, width: Option<Pixels>, co: &Co) -> impl IntoElement {
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
        .child(label)
}

/// One data row: path (mono, truncated) + three sub-counts + total (bold).
fn data_row(r: &PathRow, ab_color: Hsla, co: &Co) -> impl IntoElement {
    h_flex()
        .items_center()
        .border_b_1()
        .border_color(co.border.opacity(0.5))
        .text_size(px(11.5))
        .hover(|s| s.bg(co.hover))
        // Path / Connection (design `.sum-path`): mono, path color, ellipsized.
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .px(px(14.))
                .py(px(7.))
                .truncate()
                .font_family("Consolas")
                .text_color(co.pal.path)
                .child(r.path.clone()),
        )
        .child(num_cell(count(r.a), W_NUM, color_for(r.a, ab_color, co)))
        .child(num_cell(count(r.b), W_NUM, color_for(r.b, ab_color, co)))
        .child(num_cell(count(r.c), W_NUM, color_for(r.c, co.muted, co)))
        .child(num_cell(r.total.to_string(), W_TOTAL, co.fg).font_bold())
}

/// A right-aligned numeric cell (design `.sum-table td.num`, mono).
fn num_cell(text: String, width: f32, color: Hsla) -> gpui::Div {
    div()
        .w(px(width))
        .px(px(14.))
        .py(px(7.))
        .text_right()
        .font_family("Consolas")
        .text_color(color)
        .child(text)
}

fn count(n: usize) -> String {
    if n == 0 { "—".to_string() } else { n.to_string() }
}

/// `color` when non-zero, faint otherwise.
fn color_for(n: usize, color: Hsla, co: &Co) -> Hsla {
    if n > 0 { color } else { co.muted.opacity(0.45) }
}

// ============================ Cross Reference ============================

const W_XREF: f32 = 96.;

/// One cross-reference row: a path touched by >1 process.
struct XrefRow {
    path: SharedString,
    proc_count: usize,
    total: usize,
    procs: SharedString,
}

/// Cross Reference summary (Tools menu): paths accessed by more than one process,
/// with the access count and the list of processes. Same shell as the path
/// summaries, different columns.
pub(crate) struct XrefSummaryDialog {
    search: Entity<InputState>,
    query: String,
    rows: Vec<XrefRow>,
    total_events: usize,
}

impl XrefSummaryDialog {
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
        Self { search, query: String::new(), rows: Vec::new(), total_events: 0 }
    }

    pub(crate) fn load(&mut self, rows: &[EventSummaryRow], cx: &mut Context<Self>) {
        self.total_events = rows.len();
        self.rows = aggregate_xref(rows);
        cx.notify();
    }

    pub(crate) fn summary_text(&self) -> String {
        t!("dlg.sum_items", n = self.rows.len(), m = self.total_events).to_string()
    }

    fn filtered(&self) -> Vec<&XrefRow> {
        let q = self.query.to_lowercase();
        self.rows
            .iter()
            .filter(|r| q.is_empty() || r.path.to_lowercase().contains(&q))
            .take(MAX_ROWS)
            .collect()
    }
}

impl Render for XrefSummaryDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let co = Co::new(cx);
        let filtered = self.filtered();
        let shown = filtered.len();

        v_flex()
            .w_full()
            .child(
                h_flex().flex_shrink_0().px(px(16.)).py(px(12.)).border_b_1().border_color(co.border).child(
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
            .child(
                h_flex()
                    .flex_shrink_0()
                    .bg(co.panel)
                    .border_b_1()
                    .border_color(co.border)
                    .child(th(t!("sumc.path").to_string(), None, &co))
                    .child(th(t!("sumc.processes").to_string(), Some(px(W_XREF)), &co))
                    .child(th(t!("sumc.accesses").to_string(), Some(px(W_XREF)), &co))
                    .child(th(t!("sumc.proclist").to_string(), None, &co)),
            )
            .child(
                div().h(px(420.)).min_h(px(0.)).child(
                    div()
                        .id("xref-sum-body")
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
                        .children(filtered.into_iter().map(|r| xref_row(r, &co)))
                        .overflow_y_scrollbar(),
                ),
            )
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
                        Button::new("xref-sum-close")
                            .primary()
                            .h(px(34.))
                            .label(t!("dlg.close").to_string())
                            .on_click(cx.listener(|_, _, window, cx| window.close_dialog(cx))),
                    ),
            )
    }
}

/// One cross-reference data row: path + process count (bold) + accesses + list.
fn xref_row(r: &XrefRow, co: &Co) -> impl IntoElement {
    h_flex()
        .items_center()
        .border_b_1()
        .border_color(co.border.opacity(0.5))
        .text_size(px(11.5))
        .hover(|s| s.bg(co.hover))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .px(px(14.))
                .py(px(7.))
                .truncate()
                .font_family("Consolas")
                .text_color(co.pal.path)
                .child(r.path.clone()),
        )
        .child(num_cell(r.proc_count.to_string(), W_XREF, co.fg).font_bold())
        .child(num_cell(r.total.to_string(), W_XREF, co.muted))
        // Process list (design `.sum-proclist`): faint, truncated.
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .px(px(14.))
                .py(px(7.))
                .truncate()
                .text_size(px(11.))
                .text_color(co.muted)
                .child(r.procs.clone()),
        )
}

/// Groups all rows with a path by path, keeping those touched by >1 process
/// (sorted by process count desc, then total desc).
fn aggregate_xref(rows: &[EventSummaryRow]) -> Vec<XrefRow> {
    let mut map: HashMap<SharedString, (usize, HashSet<SharedString>)> = HashMap::new();
    for row in rows {
        if row.path.is_empty() {
            continue;
        }
        let entry = map.entry(row.path.clone()).or_default();
        entry.0 += 1;
        entry.1.insert(row.process_name.clone());
    }
    let mut out: Vec<XrefRow> = map
        .into_iter()
        .filter(|(_, (_, procs))| procs.len() > 1)
        .map(|(path, (total, procs))| {
            let mut names: Vec<String> = procs.into_iter().map(|p| p.to_string()).collect();
            names.sort();
            XrefRow {
                path,
                proc_count: names.len(),
                total,
                procs: names.join(", ").into(),
            }
        })
        .collect();
    out.sort_by(|a, b| b.proc_count.cmp(&a.proc_count).then(b.total.cmp(&a.total)));
    out
}

/// Per-path accumulator while aggregating.
#[derive(Default)]
struct Agg {
    a: usize,
    b: usize,
    c: usize,
    total: usize,
    procs: HashSet<SharedString>,
}

/// True if `op` contains `needle` (case-insensitive).
fn op_has(op: &str, needle: &str) -> bool {
    op.to_lowercase().contains(needle)
}

/// Groups rows of the kind's category by path, computing the three sub-counts +
/// total (sorted by total desc). For File/Network the 3rd count is the distinct
/// process count; for Registry it's "Sets".
fn aggregate(kind: PathKind, rows: &[EventSummaryRow]) -> Vec<PathRow> {
    let cat = kind.category();
    let mut map: HashMap<SharedString, Agg> = HashMap::new();
    for row in rows {
        if row.category != cat || row.path.is_empty() {
            continue;
        }
        let agg = map.entry(row.path.clone()).or_default();
        agg.total += 1;
        let op = row.operation.as_ref();
        match kind {
            PathKind::File => {
                if op_has(op, "read") {
                    agg.a += 1;
                }
                if op_has(op, "write") {
                    agg.b += 1;
                }
                agg.procs.insert(row.process_name.clone());
            }
            PathKind::Registry => {
                if op_has(op, "open") || op_has(op, "create") {
                    agg.a += 1;
                } else if op_has(op, "query") || op_has(op, "enum") {
                    agg.b += 1;
                } else if op_has(op, "set") || op_has(op, "delete") {
                    agg.c += 1;
                }
            }
            PathKind::Network => {
                if op_has(op, "send") {
                    agg.a += 1;
                } else if op_has(op, "receive") {
                    agg.b += 1;
                }
                agg.procs.insert(row.process_name.clone());
            }
        }
    }

    let proc_count_kind = matches!(kind, PathKind::File | PathKind::Network);
    let mut out: Vec<PathRow> = map
        .into_iter()
        .map(|(path, agg)| PathRow {
            path,
            a: agg.a,
            b: agg.b,
            c: if proc_count_kind { agg.procs.len() } else { agg.c },
            total: agg.total,
        })
        .collect();
    out.sort_by(|a, b| b.total.cmp(&a.total));
    out
}
