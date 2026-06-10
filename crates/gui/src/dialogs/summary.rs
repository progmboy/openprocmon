//! The read-only System Activity Summary dialog body (design `PerfDialog`).
//!
//! A 2×2 grid of cards: Event Rate (sparkline), Network Throughput (sparkline),
//! By Category (bars) and Most Active Processes (bars). Statistics are aggregated
//! once from the captured rows into [`SummaryStats`]; the sparklines use
//! gpui-component's `AreaChart`.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    div, linear_color_stop, linear_gradient, prelude::FluentBuilder, px, relative, App, Hsla,
    IntoElement, ParentElement, SharedString, Styled,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    chart::AreaChart,
    h_flex, v_flex, ActiveTheme, StyledExt, WindowExt,
};
use rust_i18n::t;

use crate::model::domain::{EventCategory, EventSummaryRow};
use crate::theme::{palette, ProcmonPalette};

const BINS: usize = 24;

/// A "top process" aggregate row: name, event count and (optional) app icon.
type TopProc = (SharedString, usize, Option<Arc<gpui::Image>>);
/// Per-process accumulator value: running count and first-seen icon.
type ProcStat = (usize, Option<Arc<gpui::Image>>);

/// One sparkline point (`AreaChart` needs an `Into<SharedString>` x label).
#[derive(Clone)]
struct Pt {
    x: SharedString,
    y: f64,
}

/// Aggregated statistics, computed once from the captured rows.
#[derive(Clone)]
pub(crate) struct SummaryStats {
    total: usize,
    network: usize,
    /// (category, count), sorted by count desc.
    cats: Vec<(EventCategory, usize)>,
    /// Event-rate series (counts per time bin).
    rate: Vec<f64>,
    /// Network series (network counts per time bin).
    net: Vec<f64>,
    /// (process name, count, app-icon), top 6.
    top_proc: Vec<TopProc>,
}

impl SummaryStats {
    pub(crate) fn from_rows(rows: &[EventSummaryRow]) -> Self {
        let total = rows.len();
        let mut rate = vec![0f64; BINS];
        let mut net = vec![0f64; BINS];
        let mut cat_counts = [0usize; 6];
        let mut proc: HashMap<SharedString, ProcStat> = HashMap::new();

        for (i, row) in rows.iter().enumerate() {
            let bin = (i * BINS).checked_div(total).map_or(0, |b| b.min(BINS - 1));
            rate[bin] += 1.0;
            cat_counts[cat_index(row.category)] += 1;
            if row.category == EventCategory::Network {
                net[bin] += 1.0;
            }
            let entry = proc.entry(row.process_name.clone()).or_insert((0, None));
            entry.0 += 1;
            if entry.1.is_none() {
                entry.1 = row.icon.clone();
            }
        }

        let mut cats: Vec<(EventCategory, usize)> = [
            EventCategory::Registry,
            EventCategory::File,
            EventCategory::Network,
            EventCategory::Process,
            EventCategory::Profiling,
        ]
        .into_iter()
        .map(|c| (c, cat_counts[cat_index(c)]))
        .collect();
        cats.sort_by_key(|c| std::cmp::Reverse(c.1));

        let mut top_proc: Vec<TopProc> = proc
            .into_iter()
            .map(|(name, (n, icon))| (name, n, icon))
            .collect();
        top_proc.sort_by_key(|p| std::cmp::Reverse(p.1));
        top_proc.truncate(6);

        Self {
            total,
            network: cat_counts[cat_index(EventCategory::Network)],
            cats,
            rate,
            net,
            top_proc,
        }
    }
}

fn cat_index(c: EventCategory) -> usize {
    match c {
        EventCategory::Registry => 0,
        EventCategory::File => 1,
        EventCategory::Network => 2,
        EventCategory::Process => 3,
        EventCategory::Profiling => 4,
        EventCategory::Other => 5,
    }
}

/// Resolved colors for the dialog.
struct Co {
    bg2: Hsla,
    border: Hsla,
    fg: Hsla,
    muted: Hsla,
    text2: Hsla,
    panel2: Hsla,
    bg: Hsla,
    pal: ProcmonPalette,
}

/// The footer's Close button row (the dialog is read-only).
pub(crate) fn footer() -> impl IntoElement {
    h_flex().w_full().items_center().justify_end().child(
        Button::new("sum-close")
            .primary()
            .h(px(34.))
            .label(t!("dlg.close").to_string())
            .on_click(|_, window, cx| window.close_dialog(cx)),
    )
}

pub(crate) fn render(stats: &SummaryStats, cx: &App) -> impl IntoElement {
    let co = Co {
        bg2: cx.theme().title_bar,
        border: cx.theme().border,
        fg: cx.theme().foreground,
        muted: cx.theme().muted_foreground,
        text2: cx.theme().foreground.opacity(0.72),
        panel2: cx.theme().secondary_hover,
        bg: cx.theme().background,
        pal: palette(cx),
    };

    let pts = |series: &[f64]| -> Vec<Pt> {
        series
            .iter()
            .enumerate()
            .map(|(i, v)| Pt {
                x: i.to_string().into(),
                y: *v,
            })
            .collect()
    };

    // Design `.perf-grid` — 2×2 cards.
    v_flex()
        .w_full()
        .px(px(18.))
        .py(px(16.))
        .gap(px(14.))
        .child(
            h_flex()
                .gap(px(14.))
                .child(card(
                    &co,
                    t!("dlg.sum_rate").to_string(),
                    Some((
                        t!("dlg.sum_events", n = stats.total).to_string().into(),
                        co.pal.row_sel_bar,
                    )),
                    sparkline(pts(&stats.rate), co.pal.row_sel_bar, co.bg),
                ))
                .child(card(
                    &co,
                    t!("dlg.sum_net").to_string(),
                    Some((
                        t!("dlg.sum_pkts", n = stats.network).to_string().into(),
                        co.pal.op_network,
                    )),
                    sparkline(pts(&stats.net), co.pal.op_network, co.bg),
                )),
        )
        .child(
            h_flex()
                .gap(px(14.))
                .child(card(
                    &co,
                    t!("dlg.sum_by_cat").to_string(),
                    None,
                    cat_bars(&co, stats),
                ))
                .child(card(
                    &co,
                    t!("dlg.sum_top").to_string(),
                    None,
                    top_procs(&co, stats),
                )),
        )
}

/// A `.perf-card`: title + optional value header, then the body.
fn card(
    co: &Co,
    title: String,
    value: Option<(SharedString, Hsla)>,
    body: impl IntoElement,
) -> impl IntoElement {
    v_flex()
        .flex_1()
        .min_w(px(0.))
        .bg(co.bg2)
        .border_1()
        .border_color(co.border)
        .rounded(px(10.))
        .px(px(16.))
        .py(px(14.))
        .gap(px(10.))
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_color(co.fg)
                        .text_size(px(12.5))
                        .font_semibold()
                        .child(title),
                )
                .when_some(value, |this, (v, color)| {
                    this.child(
                        div()
                            .text_color(color)
                            .font_family("Consolas")
                            .text_size(px(13.))
                            .font_bold()
                            .child(v),
                    )
                }),
        )
        .child(body)
}

/// A 60px area sparkline (axes/grid off), design `.spark`.
fn sparkline(points: Vec<Pt>, color: Hsla, bg: Hsla) -> impl IntoElement {
    div().w_full().h(px(60.)).child(
        AreaChart::new(points)
            .x(|p| p.x.clone())
            .y(|p| p.y)
            .stroke(color)
            .fill(linear_gradient(
                0.,
                linear_color_stop(color.opacity(0.35), 1.),
                linear_color_stop(bg.opacity(0.0), 0.),
            ))
            .x_axis(false)
            .grid(false),
    )
}

/// Design `.cat-bars`: swatch + label, a track bar, the count.
fn cat_bars(co: &Co, stats: &SummaryStats) -> impl IntoElement {
    let max = stats.cats.iter().map(|(_, n)| *n).max().unwrap_or(0).max(1) as f32;
    v_flex()
        .gap(px(9.))
        .children(stats.cats.iter().map(|(cat, n)| {
            let color = cat.color(&co.pal);
            let ratio = *n as f32 / max;
            h_flex()
                .items_center()
                .gap(px(10.))
                .text_size(px(11.5))
                .child(
                    h_flex()
                        .w(px(92.))
                        .items_center()
                        .gap(px(7.))
                        .text_color(co.text2)
                        .child(div().size(px(9.)).rounded(px(2.)).bg(color))
                        .child(div().child(cat.label())),
                )
                .child(track(co, ratio, color))
                .child(
                    div()
                        .w(px(52.))
                        .text_right()
                        .text_color(co.muted)
                        .font_family("Consolas")
                        .child(n.to_string()),
                )
        }))
}

/// Design `.top-proc-row`: app-icon + name + bar + count.
fn top_procs(co: &Co, stats: &SummaryStats) -> impl IntoElement {
    let max = stats
        .top_proc
        .iter()
        .map(|(_, n, _)| *n)
        .max()
        .unwrap_or(0)
        .max(1) as f32;
    v_flex().children(stats.top_proc.iter().map(|(name, n, icon)| {
        let ratio = *n as f32 / max;
        let color = proc_color(name, &co.pal);
        h_flex()
            .items_center()
            .gap(px(10.))
            .py(px(7.))
            .text_size(px(11.5))
            .border_b_1()
            .border_color(co.border.opacity(0.5))
            .child(crate::components::app_icon(icon.as_ref(), name, color, 16.))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .text_color(co.fg)
                    .child(name.clone()),
            )
            .child(div().w(px(60.)).child(track(co, ratio, co.pal.row_sel_bar)))
            .child(
                div()
                    .w(px(50.))
                    .text_right()
                    .text_color(co.muted)
                    .font_family("Consolas")
                    .child(n.to_string()),
            )
    }))
}

/// A `.cat-bar-track`: recessed 8px track with a colored fill.
fn track(co: &Co, ratio: f32, color: Hsla) -> impl IntoElement {
    div()
        .flex_1()
        .h(px(8.))
        .rounded(px(4.))
        .bg(co.panel2)
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .w(relative(ratio.clamp(0., 1.)))
                .rounded(px(4.))
                .bg(color),
        )
}

fn proc_color(name: &str, pal: &ProcmonPalette) -> Hsla {
    let h = name.bytes().fold(0u32, |a, b| a.wrapping_add(b as u32));
    match h % 6 {
        0 => pal.op_registry,
        1 => pal.op_file,
        2 => pal.op_network,
        3 => pal.op_process,
        4 => pal.op_thread,
        _ => pal.op_perf,
    }
}
