//! The read-only Process Tree dialog — a process hierarchy rendered with
//! gpui-component's `Tree` (built-in expand/collapse, selection, scrollbar).
//!
//! Self-contained: the entity owns the `TreeState`, the per-row description map,
//! and the footer (live "Selected …" text + Close + View-in-Events). `AppView`
//! only seeds it with a snapshot and wraps it in the shared `FormDialog` chrome.
//!
//! Each row mirrors the design `.tree-node-row`: caret + app-icon (first letter)
//! + name + PID (mono, muted) + description (company/user, faint, right-aligned).

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    div, px, AppContext, Context, Entity, Hsla, IntoElement, ParentElement, Render, SharedString,
    Styled, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    h_flex,
    list::ListItem,
    tree::{tree, TreeItem, TreeState},
    v_flex, ActiveTheme, Icon, StyledExt, WindowExt,
};
use rust_i18n::t;

use crate::app::AppView;
use crate::icons::PmIcon;
use crate::model::domain::ProcessNode;
use crate::model::filter::{FilterAction, FilterColumn};
use crate::theme::{palette, ProcmonPalette};

pub(crate) struct ProcessTreeDialog {
    app: WeakEntity<AppView>,
    tree: Entity<TreeState>,
    /// PID → description (company, falling back to user) for the right column.
    meta: HashMap<SharedString, SharedString>,
    /// PID → app-icon bytes (only for processes that have one).
    icons: HashMap<SharedString, Arc<[u8]>>,
}

impl ProcessTreeDialog {
    pub(crate) fn new(
        app: WeakEntity<AppView>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let tree = cx.new(|cx| TreeState::new(cx));
        // TreeState emits no selection event; it only `cx.notify()`s on click. We
        // observe that so the footer's "Selected …" text re-renders on selection.
        cx.observe(&tree, |_, _, cx| cx.notify()).detach();
        Self {
            app,
            tree,
            meta: HashMap::new(),
            icons: HashMap::new(),
        }
    }

    /// Seeds the tree from a process snapshot (call when opening).
    pub(crate) fn load(&mut self, nodes: &[ProcessNode], cx: &mut Context<Self>) {
        let items = build_items(nodes);
        self.tree.update(cx, |s, cx| s.set_items(items, cx));
        self.meta = meta_map(nodes);
        self.icons = icon_map(nodes);
        cx.notify();
    }

    /// "View in Events": filter the table to the selected process, then close.
    fn view_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(name) = self
            .tree
            .read(cx)
            .selected_item()
            .map(|i| i.label.to_string())
        {
            self.app
                .update(cx, |view, cx| {
                    view.quick_filter(FilterColumn::ProcessName, name, FilterAction::Include, cx)
                })
                .ok();
        }
        window.close_dialog(cx);
    }
}

impl Render for ProcessTreeDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let meta = self.meta.clone();
        let icons = self.icons.clone();
        let body = tree(&self.tree, move |ix, entry, _selected, _window, cx| {
            let pal = palette(cx);
            let muted = cx.theme().muted_foreground;
            let fg = cx.theme().foreground;
            let faint = muted.opacity(0.7);

            let item = entry.item();
            let name = item.label.clone();
            let pid = item.id.clone();
            let desc = meta.get(&item.id).cloned().unwrap_or_default();

            // Caret (design `.tree-caret`, 13px): chevron, down when expanded; a
            // hidden 16px spacer for leaves. (Clicking the row toggles expansion.)
            let caret = if entry.is_folder() {
                let icon = if entry.is_expanded() {
                    PmIcon::ChevronDown
                } else {
                    PmIcon::Chevron
                };
                Icon::new(icon)
                    .size(px(13.))
                    .text_color(muted)
                    .into_any_element()
            } else {
                div().size(px(16.)).flex_shrink_0().into_any_element()
            };

            ListItem::new(ix)
                .w_full()
                .rounded(px(6.))
                .pl(px(14. + entry.depth() as f32 * 20.))
                .pr(px(14.))
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .w_full()
                        .text_size(px(12.))
                        .child(caret)
                        // App-icon (extracted `.ico`) or a colored letter square.
                        .child(crate::components::app_icon(
                            icons.get(&item.id),
                            &name,
                            appicon_color(&name, &pal),
                            16.,
                        ))
                        .child(div().text_color(fg).font_medium().child(name)) // .tname
                        .child(div().text_color(muted).text_size(px(10.5)).child(pid)) // .tpid
                        .child(div().flex_1())
                        .child(div().text_color(faint).text_size(px(11.)).child(desc)), // .tdesc
                )
        });

        let muted = cx.theme().muted_foreground;
        let fg = cx.theme().foreground;
        // Footer (design `.tree-dialog .dialog-foot`): live "Selected name (pid)"
        // (muted label + mono-bold value) on the left, Close + View on the right.
        let selected = self
            .tree
            .read(cx)
            .selected_item()
            .map(|i| (i.label.clone(), i.id.clone()));
        let left = match selected {
            Some((name, pid)) => h_flex()
                .flex_1()
                .items_center()
                .gap_1()
                .text_size(px(12.))
                .child(
                    div()
                        .text_color(muted)
                        .child(t!("dlg.selected").to_string()),
                )
                .child(
                    div()
                        .text_color(fg)
                        .font_family("Consolas")
                        .font_bold()
                        .child(format!("{name} ({pid})")),
                )
                .into_any_element(),
            None => div().flex_1().into_any_element(),
        };

        v_flex()
            .w_full()
            // Tree body — fixed height keeps the list scrollable (built-in
            // scrollbar); design `.tree-dialog` body is `padding: 8px 0`.
            .child(div().w_full().h(px(446.)).py(px(8.)).child(body))
            // Footer with the design's top divider + padding.
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .px(px(18.))
                    .py(px(13.))
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(left)
                    .child(
                        Button::new("tree-close")
                            .h(px(34.))
                            .label(t!("dlg.close").to_string())
                            .on_click(cx.listener(|_, _, window, cx| window.close_dialog(cx))),
                    )
                    .child(
                        Button::new("tree-view")
                            .primary()
                            .h(px(34.))
                            .label(t!("dlg.tree_view").to_string())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.view_selected(window, cx)),
                            ),
                    ),
            )
    }
}

/// Builds the `TreeItem` hierarchy (id = PID, label = name; expanded by default).
fn build_items(nodes: &[ProcessNode]) -> Vec<TreeItem> {
    nodes.iter().map(item_of).collect()
}

fn item_of(node: &ProcessNode) -> TreeItem {
    let item = TreeItem::new(node.pid.to_string(), node.name.clone()).expanded(true);
    if node.children.is_empty() {
        item
    } else {
        item.children(node.children.iter().map(item_of).collect::<Vec<_>>())
    }
}

fn meta_map(nodes: &[ProcessNode]) -> HashMap<SharedString, SharedString> {
    let mut map = HashMap::new();
    fn walk(node: &ProcessNode, map: &mut HashMap<SharedString, SharedString>) {
        let desc = if node.company.is_empty() {
            node.user.clone()
        } else {
            node.company.clone()
        };
        map.insert(SharedString::from(node.pid.to_string()), desc);
        for child in &node.children {
            walk(child, map);
        }
    }
    for node in nodes {
        walk(node, &mut map);
    }
    map
}

fn icon_map(nodes: &[ProcessNode]) -> HashMap<SharedString, Arc<[u8]>> {
    let mut map = HashMap::new();
    fn walk(node: &ProcessNode, map: &mut HashMap<SharedString, Arc<[u8]>>) {
        if let Some(icon) = &node.icon {
            map.insert(SharedString::from(node.pid.to_string()), icon.clone());
        }
        for child in &node.children {
            walk(child, map);
        }
    }
    for node in nodes {
        walk(node, &mut map);
    }
    map
}

/// A deterministic accent for a process's app-icon, varied by name.
fn appicon_color(name: &str, pal: &ProcmonPalette) -> Hsla {
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
