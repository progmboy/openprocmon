//! The monitor bar: a "Monitor:" label + category toggle pills (Registry / File
//! / Network / Process / Profiling). Per the design (`.mtoggle`) the pills are
//! uniform — gray when off, accent-blue (border + soft fill + dot + icon + label)
//! when on — not per-category colored. No per-category counts.

use gpui::{
    div, px, Context, Entity, Hsla, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use gpui_component::{h_flex, ActiveTheme, Icon, Sizable};

use crate::app::{AppState, AppView, MonitorKind};
use crate::icons::PmIcon;
use crate::theme::palette;

pub(crate) fn render(state: &Entity<AppState>, cx: &mut Context<AppView>) -> impl IntoElement {
    let pal = palette(cx);
    let m = state.read(cx).monitor;

    // (kind, label, icon, dot color, active). The dot is category-colored; the
    // icon/label are white/gray; active adds the accent border + soft fill.
    let pills: [(MonitorKind, String, PmIcon, Hsla, bool); 5] = [
        (
            MonitorKind::Registry,
            rust_i18n::t!("mon.registry").to_string(),
            PmIcon::Registry,
            pal.op_registry,
            m.registry,
        ),
        (
            MonitorKind::File,
            rust_i18n::t!("mon.file").to_string(),
            PmIcon::Filesys,
            pal.op_file,
            m.file,
        ),
        (
            MonitorKind::Network,
            rust_i18n::t!("mon.network").to_string(),
            PmIcon::Network,
            pal.op_network,
            m.network,
        ),
        (
            MonitorKind::Process,
            rust_i18n::t!("mon.process").to_string(),
            PmIcon::ProcThread,
            pal.op_process,
            m.process,
        ),
        (
            MonitorKind::Profiling,
            rust_i18n::t!("mon.profiling").to_string(),
            PmIcon::Perf,
            pal.op_perf,
            m.profiling,
        ),
    ];

    let accent = pal.row_sel_bar;
    let accent_soft = accent.opacity(0.16);
    let border = cx.theme().border;
    let muted = cx.theme().muted_foreground;
    let pill_bg = cx.theme().secondary;

    h_flex()
        .w_full()
        .h(px(44.))
        // Design `.monbar { flex-shrink: 0 }`: keep the bar at 44px instead of
        // being compressed as a v_flex child (default flex-shrink:1).
        .flex_shrink_0()
        .items_center()
        .gap_2()
        .px(px(12.))
        .bg(cx.theme().background)
        .border_b_1()
        .border_color(border)
        .child(
            div()
                .mr_1()
                .text_color(muted)
                .text_sm()
                .child(rust_i18n::t!("mon.label").to_string()),
        )
        .children(
            pills
                .into_iter()
                .map(move |(kind, label, icon, color, active)| {
                    // Icon and label use the category color when on, gray when off; the
                    // dot is the accent blue when on (design `.mtoggle .dot`), faint off.
                    let text = if active { color } else { muted };
                    let dot = div().size(px(7.)).rounded_full().bg(if active {
                        accent
                    } else {
                        muted.opacity(0.5)
                    });

                    let id = match kind {
                        MonitorKind::Registry => "mon-registry",
                        MonitorKind::File => "mon-file",
                        MonitorKind::Network => "mon-network",
                        MonitorKind::Process => "mon-process",
                        MonitorKind::Profiling => "mon-profiling",
                    };

                    div()
                        .id(id)
                        .flex()
                        .items_center()
                        .gap_2()
                        .h(px(30.))
                        .px(px(13.))
                        .rounded_full()
                        .border_1()
                        .border_color(if active { accent } else { border })
                        .bg(if active { accent_soft } else { pill_bg })
                        .text_color(text)
                        .text_sm()
                        .cursor_pointer()
                        .child(dot)
                        .child(Icon::new(icon).small().text_color(text))
                        .child(label)
                        .on_click(cx.listener(move |view, _, _, cx| view.toggle_monitor(kind, cx)))
                }),
        )
}
