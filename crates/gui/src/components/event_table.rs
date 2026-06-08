//! The virtualized event table.
//!
//! gpui-component's `Table` is internally virtualized (only visible rows render),
//! which is what keeps the GUI responsive under a high event rate. The delegate
//! reads rows from the shared [`AppState`] buffer's filtered view and colors each
//! cell per the design (category color for the operation, result-kind color for
//! the result, dedicated PID/path colors).

use gpui::{
    div, prelude::FluentBuilder, px, Context, Div, Entity, InteractiveElement, IntoElement,
    ParentElement, Stateful, StatefulInteractiveElement, Styled, WeakEntity,
};
use gpui_component::{
    h_flex,
    menu::{PopupMenu, PopupMenuItem},
    table::{Column, TableDelegate, TableState},
    tooltip::Tooltip,
    ActiveTheme, StyledExt,
};

use crate::app::{AppState, AppView};
use crate::model::filter::{FilterAction, FilterColumn, FilterRelation};
use crate::theme::palette;

/// Column order matches the design's event table.
pub(crate) struct EventTableDelegate {
    app: Entity<AppState>,
    app_view: WeakEntity<AppView>,
    columns: Vec<Column>,
}

impl EventTableDelegate {
    pub(crate) fn new(app: Entity<AppState>, app_view: WeakEntity<AppView>) -> Self {
        Self {
            app,
            app_view,
            columns: build_columns(),
        }
    }

    /// Rebuilds column headers in the current locale (called on language switch).
    pub(crate) fn retitle(&mut self) {
        self.columns = build_columns();
    }
}

/// The eight columns with localized headers and design widths.
fn build_columns() -> Vec<Column> {
    vec![
        Column::new("idx", rust_i18n::t!("col.idx").to_string())
            .width(px(56.))
            .text_right()
            .resizable(false),
        Column::new("time", rust_i18n::t!("col.time").to_string()).width(px(146.)),
        Column::new("proc", rust_i18n::t!("col.process").to_string()).width(px(120.)),
        Column::new("pid", rust_i18n::t!("col.pid").to_string())
            .width(px(76.))
            .text_right(),
        Column::new("op", rust_i18n::t!("col.operation").to_string()).width(px(120.)),
        Column::new("path", rust_i18n::t!("col.path").to_string()).width(px(360.)),
        Column::new("result", rust_i18n::t!("col.result").to_string()).width(px(128.)),
        Column::new("detail", rust_i18n::t!("col.detail").to_string()).width(px(224.)),
    ]
}

impl TableDelegate for EventTableDelegate {
    fn columns_count(&self, _cx: &gpui::App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, cx: &gpui::App) -> usize {
        self.app.read(cx).buffer.visible_len()
    }

    fn column(&self, col_ix: usize, _cx: &gpui::App) -> Column {
        // master's TableDelegate returns an owned Column (called only on
        // prepare/refresh, so the clone cost is negligible).
        self.columns[col_ix].clone()
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _window: &mut gpui::Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        // Design `.thead th`: muted (text-2) color, semibold — not the default
        // (near-black) header foreground.
        div()
            .size_full()
            .flex()
            .items_center()
            .text_color(cx.theme().muted_foreground)
            .font_semibold()
            .child(self.column(col_ix, cx).name.clone())
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut gpui::Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let pal = palette(cx);
        let muted = cx.theme().muted_foreground;
        let fg = cx.theme().foreground;

        let app = self.app.read(cx);
        let Some(row) = app.buffer.visible(row_ix) else {
            return div().into_any_element();
        };

        match col_ix {
            // # — monotonic index, faint, right-aligned.
            0 => div()
                .w_full()
                .text_color(muted)
                .child(row.seq().to_string())
                .into_any_element(),
            // Time.
            1 => div().text_color(muted).child(row.time()).into_any_element(),
            // Process name with its app-icon (extracted `.ico`), or a category-
            // colored letter square when no icon is available. Icon is read live
            // (not cached) so async SDK metadata appears on the next frame.
            2 => {
                let icon = row.icon();
                let name = row.process_name();
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(crate::components::app_icon(
                        icon.as_ref(),
                        &name,
                        row.category().color(&pal),
                        14.,
                    ))
                    .child(div().text_color(fg).child(name))
                    .into_any_element()
            }
            // PID (hex when enabled in Settings ▸ Display Format).
            3 => {
                let pid = if app.config.hex_thread_proc_id {
                    format!("0x{:x}", row.pid())
                } else {
                    row.pid().to_string()
                };
                div()
                    .w_full()
                    .text_color(pal.pid)
                    .child(pid)
                    .into_any_element()
            }
            // Operation, colored by category. Honors the Event ▸ "Advanced Display"
            // toggle: friendly detail names when on, raw IRP_MJ_*/FASTIO_* when off.
            4 => {
                let advance = crate::model::filter::advanced_display_on(&app.filter);
                div()
                    .text_color(row.category().color(&pal))
                    .child(row.operation_display(advance))
                    .into_any_element()
            }
            // Path — truncated, with the full path as a tooltip (shown for every
            // non-empty path: gpui-component has no "only when clipped" mode).
            5 => {
                let path = row.path();
                div()
                    .id(("path", row_ix))
                    .w_full()
                    .truncate()
                    .text_color(pal.path)
                    .child(path.clone())
                    .when(!path.is_empty(), |d| {
                        d.tooltip(move |window, cx| Tooltip::new(path.clone()).build(window, cx))
                    })
                    .into_any_element()
            }
            // Result with a colored dot.
            6 => h_flex()
                .gap_1()
                .items_center()
                .text_color(row.result_kind().color(&pal))
                .child(
                    div()
                        .size(px(6.))
                        .rounded_full()
                        .bg(row.result_kind().color(&pal)),
                )
                .child(row.result())
                .into_any_element(),
            // Detail summary.
            _ => div()
                .text_color(muted)
                .child(row.detail())
                .into_any_element(),
        }
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut gpui::Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        // Highlighted rows get an amber tint; bookmarked rows get an amber left
        // bar. The built-in selection/stripe styling handles the rest.
        let pal = palette(cx);
        let app = self.app.read(cx);
        // Highlight tint follows the configured color (Settings ▸ Appearance).
        let hl = app.config.highlight_color.hsla();
        let (highlighted, bookmarked) = app
            .buffer
            .visible(row_ix)
            .map(|r| (r.highlighted(), r.bookmarked()))
            .unwrap_or((false, false));

        div()
            .id(("row", row_ix))
            .when(highlighted, |this| this.bg(hl.opacity(0.18)))
            .when(bookmarked, |this| {
                this.border_l_2().border_color(pal.op_thread)
            })
        // master's DataTable draws the accent border on the right-clicked
        // (context-menu) row itself, so we no longer track it here.
    }

    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        _window: &mut gpui::Window,
        cx: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        // `CapturedEvent` is not Clone — project just the owned strings we need.
        let (name, path, date) = {
            let app = self.app.read(cx);
            let Some(row) = app.buffer.visible(row_ix) else {
                return menu;
            };
            (
                row.process_name().to_string(),
                row.path().to_string(),
                // The full-precision "Date & Time" string this event sorts by (cf.
                // `Column::Date`), e.g. "2026/06/05 14:42:43.9161935" — the value for
                // the before/after time filters.
                row.event().date_precise(),
            )
        };
        let view = self.app_view.clone();

        // Each item dispatches to an `AppView` quick-action via the weak handle.
        let include = {
            let (view, name) = (view.clone(), name.clone());
            move |_: &gpui::ClickEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
                let (view, name) = (view.clone(), name.clone());
                view.update(cx, |v, cx| {
                    v.quick_filter(FilterColumn::ProcessName, name, FilterAction::Include, cx)
                })
                .ok();
            }
        };
        let exclude = {
            let (view, name) = (view.clone(), name.clone());
            move |_: &gpui::ClickEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
                let (view, name) = (view.clone(), name.clone());
                view.update(cx, |v, cx| {
                    v.quick_filter(FilterColumn::ProcessName, name, FilterAction::Exclude, cx)
                })
                .ok();
            }
        };
        let highlight = {
            let (view, name) = (view.clone(), name.clone());
            move |_: &gpui::ClickEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
                let (view, name) = (view.clone(), name.clone());
                view.update(cx, |v, cx| v.add_highlight(name, cx)).ok();
            }
        };
        // Exclude every event timestamped before (Date & Time less than) this one.
        let exclude_before = {
            let (view, date) = (view.clone(), date.clone());
            move |_: &gpui::ClickEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
                let (view, date) = (view.clone(), date.clone());
                view.update(cx, |v, cx| {
                    v.quick_filter_rel(
                        FilterColumn::Date,
                        FilterRelation::LessThan,
                        date,
                        FilterAction::Exclude,
                        cx,
                    )
                })
                .ok();
            }
        };
        // Exclude every event timestamped after (Date & Time more than) this one.
        let exclude_after = {
            let (view, date) = (view.clone(), date.clone());
            move |_: &gpui::ClickEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
                let (view, date) = (view.clone(), date.clone());
                view.update(cx, |v, cx| {
                    v.quick_filter_rel(
                        FilterColumn::Date,
                        FilterRelation::MoreThan,
                        date,
                        FilterAction::Exclude,
                        cx,
                    )
                })
                .ok();
            }
        };
        let bookmark = {
            let view = view.clone();
            move |_: &gpui::ClickEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
                let view = view.clone();
                view.update(cx, |v, cx| v.toggle_bookmark(row_ix, cx)).ok();
            }
        };
        let copy_path = move |_: &gpui::ClickEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(path.clone()));
        };

        menu.item(
            PopupMenuItem::new(rust_i18n::t!("cm.include", name = name).to_string())
                .on_click(include),
        )
        .item(
            PopupMenuItem::new(rust_i18n::t!("cm.exclude", name = name).to_string())
                .on_click(exclude),
        )
        .item(
            PopupMenuItem::new(rust_i18n::t!("cm.highlight", name = name).to_string())
                .on_click(highlight),
        )
        .separator()
        .item(
            PopupMenuItem::new(rust_i18n::t!("cm.exclude_before").to_string())
                .on_click(exclude_before),
        )
        .item(
            PopupMenuItem::new(rust_i18n::t!("cm.exclude_after").to_string())
                .on_click(exclude_after),
        )
        .separator()
        .item(PopupMenuItem::new(rust_i18n::t!("cm.bookmark").to_string()).on_click(bookmark))
        .item(PopupMenuItem::new(rust_i18n::t!("cm.copy_path").to_string()).on_click(copy_path))
    }
}
