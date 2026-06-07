//! The custom title bar: brand mark + a design-styled menu bar.
//!
//! gpui-component's `AppMenuBar` hardcodes its trigger buttons (`ghost().small()
//! .compact()`), which can't match the design's `.menu-item` (—`--text-2` text,
//! `4px 11px` padding, `--panel-2` hover, 12.5px). So this is a small bespoke menu
//! bar that mirrors `AppMenuBar`'s *behavior* — a shared `selected` index gives
//! proper menu-bar semantics (hovering switches the open menu) and the dropdowns
//! are gpui-component `PopupMenu`s (so clicking an item, including the Theme /
//! Language submenus, dismisses correctly) — while owning the trigger styling.
//!
//! The dropdown is rebuilt every time it opens, so the Theme/Language checks and
//! all localized labels are always current (no global observers needed). Items map
//! to the actions in `actions.rs`, dispatched to `AppView`'s focus context so its
//! `on_action` handlers run.

use gpui::{
    anchored, deferred, div, prelude::FluentBuilder, px, Anchor, Context, DismissEvent, Entity,
    FocusHandle, Focusable, InteractiveElement, IntoElement, MouseButton, ParentElement, Render,
    StatefulInteractiveElement, Styled, Subscription, WeakEntity, Window,
};
use gpui_component::{h_flex, menu::PopupMenu, ActiveTheme, Icon, StyledExt, TitleBar};
use rust_i18n::t;

use crate::app::AppView;

use crate::actions::{
    About, AlwaysOnTop, Bookmark, CheckUpdates, ClearDisplay, ClearFilter, ClearHighlight, Copy,
    ExportSettings, FocusSearch, HelpTopics, ImportSettings, Open, OpenFileSummary, OpenFilter,
    OpenHighlight, OpenNetSummary, OpenProcessSummary, OpenRegSummary, OpenSettings, OpenSummary,
    OpenTree, OpenXrefSummary, Quit, Save, SaveAs, ToggleAdvancedDisplay, ToggleAutoscroll,
    WebSearch,
};
use crate::icons::PmIcon;

/// The top-level menu names (i18n keys), in order.
const MENUS: [&str; 6] =
    ["menu.file", "menu.edit", "menu.event", "menu.tools", "menu.options", "menu.help"];

/// A dropdown row body (design `.menu-row`): a leading 14px icon (muted), the
/// 12.5px label, an optional mono shortcut, and an optional trailing tick. Used as
/// a PopupMenu *element* item so the font/layout match the design (built-in rows
/// are locked to `text_sm` and a tiny `.xsmall()` check).
fn row(
    icon: PmIcon,
    label: String,
    shortcut: Option<String>,
    check: Option<bool>,
    cx: &gpui::App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let accent = cx.theme().primary;
    // `py(7)` lifts the row past PopupMenu's 26px `min_h` floor to the design's
    // ~30px (`.menu-row` is `6px 9px` padding + 12.5px line-height). 13px label.
    h_flex()
        .w_full()
        .items_center()
        .gap(px(10.))
        .py(px(7.))
        .text_size(px(13.))
        // Leading icon column (design `.mrow-icon`, 16px box / 14px glyph, muted).
        .child(
            div()
                .size(px(16.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .text_color(muted)
                .child(Icon::new(icon).size(px(14.))),
        )
        .child(label)
        .child(div().flex_1())
        .when_some(shortcut, move |this, sc| {
            this.child(
                div().text_size(px(11.5)).font_family("Consolas").text_color(muted).child(sc),
            )
        })
        // Trailing tick (reserved so toggling on/off doesn't shift the row).
        .when_some(check, |this, checked| {
            this.child(
                div()
                    .w(px(16.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(checked, |d| {
                        d.child(Icon::new(PmIcon::Check).size(px(13.)).text_color(accent))
                    }),
            )
        })
}

/// Adds a localized (i18n key) element row with a leading icon, no check.
fn it(
    menu: PopupMenu,
    icon: PmIcon,
    key: &str,
    action: Box<dyn gpui::Action>,
    shortcut: Option<&'static str>,
) -> PopupMenu {
    let label = t!(key).to_string();
    let sc = shortcut.map(|s| s.to_string());
    menu.menu_element(action, move |_w, cx| row(icon, label.clone(), sc.clone(), None, cx))
}

/// Adds a localized (i18n key) element row with a leading icon + a trailing check.
fn itc(
    menu: PopupMenu,
    icon: PmIcon,
    key: &str,
    checked: bool,
    action: Box<dyn gpui::Action>,
    shortcut: Option<&'static str>,
) -> PopupMenu {
    let label = t!(key).to_string();
    let sc = shortcut.map(|s| s.to_string());
    menu.menu_element(action, move |_w, cx| row(icon, label.clone(), sc.clone(), Some(checked), cx))
}

/// A bespoke, design-styled menu bar (one entity drives all top-level menus).
pub(crate) struct MenuBar {
    /// Read for the checkable items' state (auto-scroll / bookmark / on-top).
    app: WeakEntity<AppView>,
    /// Focus context the menu items dispatch their actions to (the `AppView` root,
    /// so its `on_action` handlers run).
    action_context: FocusHandle,
    /// The currently open menu, if any.
    selected: Option<usize>,
    /// The live dropdown for the open menu.
    popup: Option<Entity<PopupMenu>>,
    _sub: Option<Subscription>,
}

impl MenuBar {
    pub(crate) fn new(app: WeakEntity<AppView>, action_context: FocusHandle) -> Self {
        Self { app, action_context, selected: None, popup: None, _sub: None }
    }

    /// Current state of the checkable menu items: (auto-scroll, selected row is
    /// bookmarked, always-on-top, advanced-display). Read from `AppState` when
    /// building a dropdown.
    fn checks(&self, cx: &Context<Self>) -> (bool, bool, bool, bool) {
        self.app
            .upgrade()
            .map(|app| {
                let s = app.read(cx).state.read(cx);
                let bookmarked = s
                    .selected
                    .and_then(|ix| s.buffer.visible(ix))
                    .map(|row| row.bookmarked())
                    .unwrap_or(false);
                (
                    s.autoscroll,
                    bookmarked,
                    s.always_on_top,
                    crate::model::filter::advanced_display_on(&s.filter),
                )
            })
            .unwrap_or((false, false, false, false))
    }

    /// Toggles the menu at `ix` (click): close if it's already open, else open it.
    fn toggle(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected == Some(ix) {
            self.close(cx);
        } else {
            self.open(ix, window, cx);
        }
    }

    /// Hovering a different trigger while a menu is open switches to it — the
    /// hallmark menu-bar behavior. Does nothing when no menu is open.
    fn hover(&mut self, ix: usize, hovered: bool, window: &mut Window, cx: &mut Context<Self>) {
        if hovered && self.selected.is_some() && self.selected != Some(ix) {
            self.open(ix, window, cx);
        }
    }

    fn open(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = Some(ix);
        let popup = self.build_popup(ix, window, cx);
        // Focus the popup so an outside click dismisses it (PopupMenu closes on
        // blur), and so it owns keyboard nav.
        let focus = popup.read(cx).focus_handle(cx);
        if !focus.contains_focused(window, cx) {
            focus.focus(window, cx);
        }
        self._sub = Some(cx.subscribe_in(&popup, window, |this, _, _: &DismissEvent, _, cx| {
            this.close(cx);
        }));
        self.popup = Some(popup);
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self._sub.take();
        self.popup.take();
        self.selected = None;
        cx.notify();
    }

    /// Builds the dropdown for menu `ix`. Read fresh each open so the Theme /
    /// Language checks and localized labels reflect the current state.
    fn build_popup(&self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> Entity<PopupMenu> {
        use PmIcon as I;
        let handle = self.action_context.clone();
        let (autoscroll, bookmarked, always_on_top, advanced_display) = self.checks(cx);

        PopupMenu::build(window, cx, move |menu, _window, _cx| {
            // Rows are custom *element* items (`it`/`itc`): leading icon + 12.5px
            // label + shortcut + trailing tick (PopupMenu's built-in rows are locked
            // to `text_sm` and a tiny check). `min_w(220)` = design `.menu-dropdown`.
            let menu = menu.action_context(handle.clone()).min_w(px(220.));
            match ix {
                0 => {
                    // File
                    let menu = it(menu, I::Open, "menu.open", Box::new(Open), Some("Ctrl+O"));
                    let menu = it(menu, I::Save, "menu.save", Box::new(Save), Some("Ctrl+S"));
                    let menu = it(menu, I::SaveAs, "menu.save_as", Box::new(SaveAs), None).separator();
                    let menu = it(menu, I::Download, "menu.import", Box::new(ImportSettings), None);
                    let menu =
                        it(menu, I::Upload, "menu.export", Box::new(ExportSettings), None).separator();
                    it(menu, I::Logout, "menu.exit", Box::new(Quit), None)
                }
                1 => {
                    // Edit
                    let menu = it(menu, I::Copy, "menu.copy", Box::new(Copy), Some("Ctrl+C"));
                    let menu =
                        it(menu, I::Search, "menu.find", Box::new(FocusSearch), Some("Ctrl+F"))
                            .separator();
                    it(menu, I::Trash, "menu.clear_display", Box::new(ClearDisplay), Some("Ctrl+X"))
                }
                2 => {
                    // Event
                    let menu = it(menu, I::Filter, "menu.filter", Box::new(OpenFilter), Some("Ctrl+L"));
                    let menu =
                        it(menu, I::Ban, "menu.clear_filter", Box::new(ClearFilter), None);
                    let menu = itc(
                        menu,
                        I::FilterFill,
                        "menu.advanced_display",
                        advanced_display,
                        Box::new(ToggleAdvancedDisplay),
                        None,
                    )
                    .separator();
                    let menu = it(menu, I::Highlight, "menu.highlight", Box::new(OpenHighlight), None);
                    let menu =
                        it(menu, I::Ban, "menu.clear_highlight", Box::new(ClearHighlight), None)
                            .separator();
                    let menu = itc(
                        menu,
                        I::Scroll,
                        "menu.auto_scroll",
                        autoscroll,
                        Box::new(ToggleAutoscroll),
                        Some("Ctrl+A"),
                    )
                    .separator();
                    let menu = itc(
                        menu,
                        I::Bookmark,
                        "menu.bookmark",
                        bookmarked,
                        Box::new(Bookmark),
                        Some("Ctrl+B"),
                    )
                    .separator();
                    it(menu, I::Globe, "menu.web_search", Box::new(WebSearch), None)
                }
                3 => {
                    // Tools
                    let menu = it(menu, I::Tree, "menu.tree", Box::new(OpenTree), None);
                    let menu = it(menu, I::Perf, "menu.summary", Box::new(OpenSummary), None).separator();
                    let menu = it(menu, I::Cpu, "menu.sum_process", Box::new(OpenProcessSummary), None);
                    let menu = it(menu, I::Filesys, "menu.sum_file", Box::new(OpenFileSummary), None);
                    let menu =
                        it(menu, I::Registry, "menu.sum_registry", Box::new(OpenRegSummary), None);
                    let menu = it(menu, I::Network, "menu.sum_network", Box::new(OpenNetSummary), None);
                    it(menu, I::Crosshair, "menu.sum_xref", Box::new(OpenXrefSummary), None)
                }
                4 => {
                    // Options (theme/language/symbols/history now live in Settings)
                    let menu =
                        it(menu, I::Settings, "menu.settings", Box::new(OpenSettings), Some("Ctrl+,"))
                            .separator();
                    itc(menu, I::Pin, "menu.always_on_top", always_on_top, Box::new(AlwaysOnTop), None)
                }
                _ => {
                    // Help
                    let menu = it(menu, I::Help, "menu.help_topics", Box::new(HelpTopics), Some("F1"));
                    let menu =
                        it(menu, I::Refresh, "menu.check_updates", Box::new(CheckUpdates), None)
                            .separator();
                    it(menu, I::Info, "menu.about", Box::new(About), None)
                }
            }
        })
    }

    /// One trigger (design `.menu-item`): `--text-2` normally, `--panel-2` bg +
    /// full `--text` when open or hovered.
    fn trigger(&self, ix: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.selected == Some(ix);
        let fg = cx.theme().foreground;
        let text2 = fg.opacity(0.72);
        let panel2 = cx.theme().secondary_hover;
        let popup = self.popup.clone();

        div()
            .id(ix)
            .relative()
            .px(px(11.))
            .py(px(4.))
            .rounded(px(5.))
            .text_size(px(12.5))
            .text_color(if open { fg } else { text2 })
            .when(open, |s| s.bg(panel2))
            .hover(|s| s.bg(panel2).text_color(fg))
            .child(t!(MENUS[ix]).to_string())
            // Don't let the press start a window drag.
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_click(cx.listener(move |this, _, window, cx| this.toggle(ix, window, cx)))
            .on_hover(cx.listener(move |this, hovered, window, cx| {
                this.hover(ix, *hovered, window, cx)
            }))
            // Anchor the open dropdown under this trigger (design `.menu-dropdown`
            // sits at `top: 100%; margin-top: 3px`).
            .when(open, |this| {
                this.children(popup.map(|popup| {
                    deferred(
                        anchored().anchor(Anchor::TopLeft).snap_to_window_with_margin(px(8.)).child(
                            div().occlude().mt(px(3.)).child(popup),
                        ),
                    )
                }))
            })
    }
}

impl Render for MenuBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("menubar")
            .items_center()
            .gap(px(2.))
            .children((0..MENUS.len()).map(|ix| self.trigger(ix, cx)))
    }
}

/// Renders the full title bar: brand mark + the menu bar (left-aligned), inside a
/// `TitleBar` (which adds the window controls + drag, and hides the OS bar).
pub(crate) fn render(menu_bar: &Entity<MenuBar>, cx: &gpui::App) -> impl IntoElement {
    let fg = cx.theme().foreground;

    // Brand (design `.brand`): a 16px gradient logo square + the product name.
    let brand = h_flex()
        .items_center()
        .gap(px(7.))
        .pl(px(6.))
        .pr(px(12.))
        .child(crate::components::brand_icon(16.))
        .child(div().text_color(fg).text_size(px(12.5)).font_semibold().child("OpenProcmon"));

    TitleBar::new()
        .h(px(30.))
        .child(h_flex().items_center().gap(px(2.)).child(brand).child(menu_bar.clone()))
}
