//! Root application state and the top-level window view.
//!
//! [`AppState`] is the shared entity holding the event source, the retained event
//! buffer, and the UI toggles that the regions and the table delegate read from.
//! [`AppView`] is the window's root view: it owns the table state and the
//! background drain task, lays out the five stacked regions, and renders
//! gpui-component's overlay layers (which `Root` itself does not render).

use std::time::Duration;

use gpui::{
    div, prelude::FluentBuilder, px, AppContext, Context, Entity, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Render, Styled, Task, Window,
};
use gpui_component::{
    h_flex,
    input::{InputEvent, InputState},
    notification::NotificationType,
    table::{DataTable, TableEvent, TableState},
    v_flex, ActiveTheme, Root, ThemeMode, WindowExt,
};

use crate::actions::{
    About, AlwaysOnTop, Bookmark, CheckUpdates, ClearDisplay, ClearFilter, ClearHighlight, Copy,
    ExportSettings, FocusSearch, HelpTopics, ImportSettings, Open, OpenFileSummary, OpenFilter,
    OpenHighlight, OpenNetSummary, OpenProcessSummary, OpenRegSummary, OpenSettings, OpenSummary,
    OpenTree, OpenXrefSummary, Quit, Save, SelectLocale, SwitchThemeMode, ToggleAdvancedDisplay,
    ToggleAutoscroll, ToggleCapture, WebSearch,
};
use crate::components::detail_panel::DetailView;
use crate::components::event_table::EventTableDelegate;
use crate::components::menubar::MenuBar;
use crate::components::{menubar, monitorbar, statusbar, toolbar};
use crate::dialogs::about;
use crate::dialogs::filter_dialog::{FilterDialog, RuleKind};
use crate::dialogs::form_dialog::FormDialog;
use crate::dialogs::path_summary::{PathKind, PathSummaryDialog, XrefSummaryDialog};
use crate::dialogs::process_summary::ProcessSummaryDialog;
use crate::dialogs::process_tree::ProcessTreeDialog;
use crate::dialogs::save_dialog::{SaveCounts, SaveDialog, SaveOptions};
use crate::dialogs::settings_dialog::SettingsDialog;
use crate::dialogs::summary;
use crate::icons::PmIcon;
use crate::model::buffer::EventBuffer;
use crate::model::config::AppConfig;
use crate::model::domain::EventDetail;
use crate::model::filter::{
    advanced_display_on, set_advanced_display, FilterAction, FilterColumn, FilterModel,
    FilterRelation, FilterRule,
};
use crate::model::source::{EventSource, SourceEvent};
use crate::theme;

/// Owned inputs for the async call-stack symbol resolver: `(frame index, address)`
/// pairs plus the originating process's module ranges `(base, size, path)`.
type SymbolInputs = (Vec<(usize, u64)>, Vec<(u64, u64, String)>);

/// The five monitor categories shown as toggle pills. PROCESS/FILE/REGISTRY map
/// to the SDK's `MonitorFlags`; PROFILING/NETWORK are surfaced here too so the
/// bar matches the design (NETWORK is ETW-backed, PROFILING is GUI-only for now).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonitorToggles {
    pub registry: bool,
    pub file: bool,
    pub network: bool,
    pub process: bool,
    pub profiling: bool,
}

impl Default for MonitorToggles {
    fn default() -> Self {
        // Profiling defaults off, like Process Monitor.
        Self {
            registry: true,
            file: true,
            network: true,
            process: true,
            profiling: false,
        }
    }
}

/// Identifies which monitor pill was toggled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonitorKind {
    Registry,
    File,
    Network,
    Process,
    Profiling,
}

impl MonitorKind {
    fn apply(self, t: &mut MonitorToggles) {
        match self {
            MonitorKind::Registry => t.registry = !t.registry,
            MonitorKind::File => t.file = !t.file,
            MonitorKind::Network => t.network = !t.network,
            MonitorKind::Process => t.process = !t.process,
            MonitorKind::Profiling => t.profiling = !t.profiling,
        }
    }
}

/// Builds the active event source: the real SDK live-capture backend.
fn make_source() -> Box<dyn EventSource> {
    Box::new(crate::model::sdk_source::SdkSource::new())
}

/// Shared, mutable application state. Held as an `Entity` so child views and the
/// table delegate can read it during render and update it through listeners.
pub struct AppState {
    pub theme_mode: ThemeMode,
    pub capturing: bool,
    pub autoscroll: bool,
    /// "Always on Top" toggle (Options menu). gpui exposes no window-level API on
    /// this platform, so it only drives the design's pinned accent border + check.
    pub always_on_top: bool,
    pub monitor: MonitorToggles,
    pub buffer: EventBuffer,
    pub search: String,
    pub selected: Option<usize>,
    /// Current applied filter (kept so the Filter dialog can edit it).
    pub filter: FilterModel,
    /// Whether Advanced Output is on (the default display filter is *not* fully
    /// present) — cached on every filter change because the table's Operation cell
    /// reads it per row per frame (recomputing it there rebuilt the 23-rule default
    /// set each time). Drives both the raw vs friendly operation name and the menu.
    pub advanced_display: bool,
    /// Highlight rule set — same shape as `filter`; matching rows are tinted.
    pub highlight: FilterModel,
    /// Settings-dialog configuration (highlight color, hex display, symbols, …).
    pub config: AppConfig,

    source: Box<dyn EventSource>,
    rx: Option<crossbeam_channel::Receiver<SourceEvent>>,
    /// A pending notice (error or save result) to surface as a toast notification
    /// on the next UI tick, tagged with the level it should be shown at.
    pending_notice: Option<(NotificationType, gpui::SharedString)>,
    /// True while viewing a loaded `.PML` (offline): history limits don't apply, so
    /// the whole capture stays visible.
    offline: bool,
    /// Session-long call-stack symbol resolver, lazily built from the config's
    /// dbghelp/symbols paths and shared by the detail panel's async resolve and the
    /// XML export. `None` until first use, or when no dbghelp is configured/present.
    symbols: Option<std::sync::Arc<procmon_sdk::SymbolResolver>>,
}

impl AppState {
    pub fn new() -> Self {
        // Load the persisted config (%USERPROFILE%\openprocmon\config.json), falling
        // back to defaults if it is missing or unreadable.
        let config = AppConfig::load();
        // Advanced Output (Event menu) defaults to OFF: seed the default display
        // filter so the monitor's own tools / NTFS metadata are excluded and
        // operations show their friendly names out of the box.
        let mut filter = FilterModel::default();
        set_advanced_display(&mut filter, false);
        let mut state = Self {
            theme_mode: ThemeMode::Dark,
            // Launch paused by default; the Settings "Start capture at boot"
            // (`boot_capture`) toggle opts into capturing on launch.
            capturing: config.boot_capture,
            autoscroll: true,
            always_on_top: false,
            monitor: MonitorToggles::default(),
            buffer: EventBuffer::new(),
            search: String::new(),
            selected: None,
            filter: FilterModel::default(),
            advanced_display: false,
            highlight: FilterModel::default(),
            config,
            source: make_source(),
            rx: None,
            pending_notice: None,
            offline: false,
            symbols: None,
        };
        // Push the seeded filter through the normal path so the buffer's visible
        // view actually applies it (setting the field alone leaves the buffer on
        // an empty filter — the menu would read as checked but nothing filtered).
        state.set_filter(filter);
        state.apply_retention();
        state
    }

    /// Applies the history ring-buffer limits from the config (live only; offline
    /// PML viewing is never trimmed).
    fn apply_retention(&mut self) {
        let retention =
            (!self.offline && self.config.history_ring).then(|| crate::model::buffer::Retention {
                max_bytes: self.config.history_mb.saturating_mul(1024 * 1024),
                max_age_ticks: (self.config.history_min as i64).saturating_mul(60 * 10_000_000),
            });
        self.buffer.set_retention(retention);
    }

    /// Returns the shared symbol resolver, building it from the configured dbghelp /
    /// symbols paths on first use. `None` when the dbghelp path is empty or missing,
    /// so callers skip symbol resolution and keep the `module+offset` fallback.
    fn symbol_resolver(&mut self) -> Option<std::sync::Arc<procmon_sdk::SymbolResolver>> {
        if self.symbols.is_none() {
            self.symbols = procmon_sdk::SymbolResolver::new(
                &self.config.dbghelp_path,
                &self.config.symbols_path,
            )
            .map(std::sync::Arc::new);
        }
        self.symbols.clone()
    }

    /// Takes the pending notice, if any (drained by the UI into a notification).
    fn take_notice(&mut self) -> Option<(NotificationType, gpui::SharedString)> {
        self.pending_notice.take()
    }

    /// Writes the events selected by `opts` to its path, returning a success/error
    /// notice for the caller to surface as a toast.
    fn save_to_file(&mut self, opts: &SaveOptions) -> (NotificationType, gpui::SharedString) {
        match self.do_save(opts) {
            Ok(n) => (
                NotificationType::Success,
                rust_i18n::t!("notify.save_ok", n = n, path = opts.path.clone())
                    .to_string()
                    .into(),
            ),
            Err(e) => (
                NotificationType::Error,
                rust_i18n::t!("notify.save_err", detail = e)
                    .to_string()
                    .into(),
            ),
        }
    }

    fn do_save(&mut self, opts: &SaveOptions) -> Result<usize, String> {
        use crate::dialogs::save_dialog::{SaveFormat, SaveScope};
        use crate::model::domain::{CapturedEvent, EventCategory};

        // Build the (owned) symbol resolver up front — only XML-with-stacks needs it —
        // so the long immutable `selected` borrow below doesn't clash with `&mut self`.
        let resolver = if matches!(opts.format, SaveFormat::Xml) && opts.stacks {
            self.symbol_resolver()
        } else {
            None
        };

        let rows = self.buffer.rows();
        let selected: Vec<&CapturedEvent> = match opts.scope {
            SaveScope::All => rows.iter().collect(),
            SaveScope::Filtered => self
                .buffer
                .visible_indices()
                .iter()
                .map(|&i| &rows[i])
                .filter(|r| opts.profiling || r.category() != EventCategory::Profiling)
                .collect(),
            SaveScope::Highlighted => rows.iter().filter(|r| r.highlighted()).collect(),
        };
        let mut n = selected.len();

        match opts.format {
            SaveFormat::Pml => {
                if let Some(reader) = self.source.as_pml_reader() {
                    // PML-sourced view: byte-faithful subset copy, keeping the
                    // capture's host header and full process table. Row `seq` is
                    // 1-based over the reader's event order (see `PmlSource::start`).
                    let keep: std::collections::HashSet<usize> = selected
                        .iter()
                        .map(|r| (r.seq().saturating_sub(1)) as usize)
                        .collect();
                    reader
                        .write_subset(&opts.path, |i| keep.contains(&i))
                        .map_err(|e| e.to_string())?;
                } else {
                    // Live capture: stamp this machine's host metadata and finish
                    // with the System (PID 4) process so kernel frames resolve.
                    // Process INIT ("Process Defined") seed rows are written
                    // whatever the scope: pushing them interns every pre-existing
                    // process into the saved process table (they are hidden from
                    // every view, so a Filtered scope never selects them).
                    let mut writer =
                        procmon_sdk::PmlWriter::new(cfg!(target_pointer_width = "64"));
                    writer.stamp_host();
                    let chosen: std::collections::HashSet<u64> =
                        selected.iter().map(|r| r.seq()).collect();
                    n = 0;
                    for row in rows.iter().filter(|r| {
                        r.event().is_process_defined() || chosen.contains(&r.seq())
                    }) {
                        writer.push_event(row.event());
                        n += 1;
                    }
                    writer
                        .finish_live_to_path(&opts.path)
                        .map_err(|e| e.to_string())?;
                }
            }
            SaveFormat::Csv => {
                // Textual exports never contain the hidden seed rows.
                let events: Vec<&procmon_sdk::Event> = selected
                    .iter()
                    .map(|r| r.event())
                    .filter(|e| !e.is_process_defined())
                    .collect();
                n = procmon_core::export_csv(&events, &opts.path)?;
            }
            SaveFormat::Xml => {
                let events: Vec<&procmon_sdk::Event> = selected
                    .iter()
                    .map(|r| r.event())
                    .filter(|e| !e.is_process_defined())
                    .collect();
                // The core encoder takes owned-string module rows; the kernel
                // module list is small and fetched once per export.
                let kernel_mods: Vec<procmon_core::ModuleRow> = self
                    .source
                    .kernel_modules()
                    .iter()
                    .map(|m| procmon_core::ModuleRow {
                        name: m.name.to_string(),
                        path: m.path.to_string(),
                        base: m.base,
                        size: m.size,
                    })
                    .collect();
                let sym = procmon_core::StackSymbolizer {
                    kernel_mods: &kernel_mods,
                    symbols: resolver.as_deref(),
                };
                n = procmon_core::export_xml(&events, opts.stacks, &sym, &opts.path)?;
            }
        }
        Ok(n)
    }

    fn set_highlight(&mut self, highlight: FilterModel) {
        self.highlight = highlight.clone();
        self.buffer.set_highlight(highlight);
    }

    fn process_tree(&self) -> Vec<crate::model::domain::ProcessNode> {
        self.source.process_tree()
    }

    /// Starts the source and remembers its channel for draining. The source is
    /// synced to the current capture state first, so a paused launch never starts
    /// the generator / connects the driver until the user hits play.
    fn start_source(&mut self) {
        self.source.set_capturing(self.capturing);
        let rx = self.source.start();
        self.rx = Some(rx);
    }

    fn set_capturing(&mut self, on: bool) {
        self.capturing = on;
        self.source.set_capturing(on);
    }

    fn set_monitor(&mut self, monitor: MonitorToggles) {
        self.monitor = monitor;
        self.source.set_monitor(monitor);
        self.buffer.set_monitor(monitor);
    }

    fn set_filter(&mut self, filter: FilterModel) {
        self.advanced_display = advanced_display_on(&filter);
        self.filter = filter.clone();
        self.source.set_filter(filter.clone());
        self.buffer.set_filter(filter);
        self.selected = None;
    }

    fn set_search(&mut self, search: String) {
        self.search = search.clone();
        self.buffer.set_search(search);
        self.selected = None;
    }

    fn clear(&mut self) {
        self.buffer.clear();
        self.selected = None;
    }

    /// Switches the source to an offline `.PML` reader (File ▸ Open), tearing down
    /// the current source and clearing the buffer first.
    fn open_pml(&mut self, path: std::path::PathBuf) {
        self.source.stop();
        self.buffer.clear();
        self.selected = None;
        // Offline viewing: show the whole capture, never trim by history limits.
        self.offline = true;
        self.apply_retention();
        self.source = Box::new(crate::model::sdk_source::PmlSource::new(path));
        self.start_source();
    }

    /// Stops the event source (joins its thread). Called on teardown.
    fn shutdown(&mut self) {
        self.source.stop();
    }

    /// Drains pending events into the buffer. Returns whether anything changed,
    /// so the caller can avoid refreshing the table when idle.
    fn drain(&mut self) -> bool {
        // Clone the receiver (an `Arc` handle) so rows stream straight into the
        // buffer — no intermediate `Vec<SourceEvent>` per tick (at ~250 bytes an
        // event, a full batch collected to ~1 MB of scratch).
        let Some(rx) = self.rx.clone() else {
            return false;
        };
        let mut added = false;
        // Bounded per-tick to keep a fast producer from starving the UI.
        for event in rx.try_iter().take(4096) {
            match event {
                SourceEvent::Row(row) => {
                    self.buffer.push(row);
                    added = true;
                }
                SourceEvent::CountsChanged(_) => {}
                // Surfaced to the user as an error toast by `on_tick`.
                SourceEvent::Error(msg) => {
                    self.pending_notice = Some((NotificationType::Error, msg))
                }
            }
        }
        added
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The window's root view. Owns the shared [`AppState`], the table state and the
/// background drain task.
pub struct AppView {
    pub(crate) state: Entity<AppState>,
    table_state: Entity<TableState<EventTableDelegate>>,
    detail_view: Entity<DetailView>,
    /// Whether the docked detail panel is shown (toggled by selection / close).
    show_detail: bool,
    pub(crate) search_input: Entity<InputState>,
    filter_dialog: Entity<FilterDialog>,
    highlight_dialog: Entity<FilterDialog>,
    save_dialog: Entity<SaveDialog>,
    tree_dialog: Entity<ProcessTreeDialog>,
    process_summary: Entity<ProcessSummaryDialog>,
    path_summary: Entity<PathSummaryDialog>,
    xref_summary: Entity<XrefSummaryDialog>,
    settings_dialog: Entity<SettingsDialog>,
    menu_bar: Entity<MenuBar>,
    focus_handle: FocusHandle,
    _drain: Task<()>,
}

impl AppView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|_| AppState::new());
        state.update(cx, |s, _| s.start_source());

        let app_weak = cx.entity().downgrade();
        let delegate = EventTableDelegate::new(state.clone(), app_weak.clone());
        let table_state = cx.new(|cx| {
            TableState::new(delegate, window, cx)
                .row_selectable(true)
                .col_resizable(true)
        });

        // Single click only selects (highlights) the row; double click opens the
        // docked detail panel for it.
        cx.subscribe(
            &table_state,
            |view, _table, event: &TableEvent, cx| match event {
                TableEvent::SelectRow(ix) => view.highlight_row(*ix, cx),
                TableEvent::DoubleClickedRow(ix) => view.select_row(*ix, cx),
                _ => {}
            },
        )
        .detach();

        let detail_view = cx.new(|cx| DetailView::new(app_weak.clone(), window, cx));

        // Toolbar search box: filter the buffer view on every change.
        let search_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(rust_i18n::t!("tb.search").to_string())
        });
        cx.subscribe(&search_input, |view, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let value = input.read(cx).value().to_string();
                view.set_search(value, cx);
            }
        })
        .detach();

        let filter_dialog =
            cx.new(|cx| FilterDialog::new(app_weak.clone(), RuleKind::Filter, window, cx));
        let highlight_dialog =
            cx.new(|cx| FilterDialog::new(app_weak.clone(), RuleKind::Highlight, window, cx));
        let save_dialog = cx.new(|cx| SaveDialog::new(app_weak.clone(), window, cx));
        let tree_dialog = cx.new(|cx| ProcessTreeDialog::new(app_weak.clone(), window, cx));
        let process_summary = cx.new(|cx| ProcessSummaryDialog::new(window, cx));
        let path_summary = cx.new(|cx| PathSummaryDialog::new(window, cx));
        let xref_summary = cx.new(|cx| XrefSummaryDialog::new(window, cx));
        let settings_dialog = cx.new(|cx| SettingsDialog::new(app_weak.clone(), window, cx));
        let focus_handle = cx.focus_handle();
        // Menu items dispatch their actions to the AppView root's focus context, so
        // its `on_action` handlers fire even while the popup holds focus. The weak
        // handle lets the menu read toggle state (auto-scroll/bookmark/on-top) for
        // its checks.
        let menu_bar = cx.new(|_| MenuBar::new(app_weak.clone(), focus_handle.clone()));

        // Drain the source channel on a frame timer and refresh the table only
        // when new rows arrived.
        let drain = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                // `update_in` gives the `&mut Window` `push_notification` needs.
                if this
                    .update_in(cx, |view, window, cx| view.on_tick(window, cx))
                    .is_err()
                {
                    break; // the view was dropped; stop draining.
                }
            }
        });

        Self {
            state,
            table_state,
            detail_view,
            show_detail: false,
            search_input,
            filter_dialog,
            highlight_dialog,
            save_dialog,
            tree_dialog,
            process_summary,
            path_summary,
            xref_summary,
            settings_dialog,
            menu_bar,
            focus_handle,
            _drain: drain,
        }
    }

    /// Opens the Filter dialog, seeding it with the current filter.
    pub(crate) fn open_filter_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.state.read(cx).filter.clone();
        self.filter_dialog.update(cx, |d, _| d.load(&current));
        let dialog = self.filter_dialog.clone();
        window.open_dialog(cx, move |d, window, cx| {
            // Shared form-dialog chrome (centered, icon+title+desc header, footer);
            // the body is content-sized so the dialog shrinks for an empty list.
            let est = px(dialog.read(cx).estimated_height());
            FormDialog::new(d)
                .icon(PmIcon::Filter)
                .title(rust_i18n::t!("dlg.filter").to_string())
                .description(rust_i18n::t!("dlg.filter_hint").to_string())
                .width(px(760.))
                .estimated_height(est)
                .footer(FilterDialog::footer(&dialog))
                .body(dialog.clone())
                .build(window, cx)
        });
    }

    /// Opens the Save To File dialog, seeded with live event counts.
    pub(crate) fn open_save_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let counts = {
            let s = self.state.read(cx);
            SaveCounts {
                total: s.buffer.total() as u64,
                filtered: s.buffer.visible_len() as u64,
                highlighted: s.buffer.highlighted_count() as u64,
            }
        };
        self.save_dialog.update(cx, |d, _| d.load(counts));
        let dialog = self.save_dialog.clone();
        window.open_dialog(cx, move |d, window, cx| {
            FormDialog::new(d)
                .icon(PmIcon::Save)
                .title(rust_i18n::t!("dlg.save").to_string())
                .width(px(600.))
                .estimated_height(px(560.))
                .footer(SaveDialog::footer(&dialog, cx))
                .body(dialog.clone())
                .build(window, cx)
        });
    }

    /// Writes the selected events to the chosen file, then toasts the result
    /// (green "saved N events" / red "save failed").
    pub(crate) fn save_to_file(
        &mut self,
        opts: SaveOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let notice = self.state.update(cx, |s, _| s.save_to_file(&opts));
        window.push_notification(notice, cx);
    }

    pub(crate) fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.search_input.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    }

    /// Opens the Highlight dialog, seeding it with the current highlight set.
    pub(crate) fn open_highlight_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.state.read(cx).highlight.clone();
        self.highlight_dialog.update(cx, |d, _| d.load(&current));
        let dialog = self.highlight_dialog.clone();
        window.open_dialog(cx, move |d, window, cx| {
            let est = px(dialog.read(cx).estimated_height());
            FormDialog::new(d)
                .icon(PmIcon::Highlight)
                .title(rust_i18n::t!("dlg.highlight").to_string())
                .description(rust_i18n::t!("dlg.highlight_hint").to_string())
                .width(px(760.))
                .estimated_height(est)
                .footer(FilterDialog::footer(&dialog))
                .body(dialog.clone())
                .build(window, cx)
        });
    }

    /// Opens the read-only Process Tree dialog. The dialog entity owns all of its
    /// logic (tree, footer, View-in-Events); here we only seed it + apply the shell.
    pub(crate) fn open_tree_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let snapshot = self.state.read(cx).process_tree();
        self.tree_dialog.update(cx, |d, cx| d.load(&snapshot, cx));
        let dialog = self.tree_dialog.clone();
        window.open_dialog(cx, move |d, window, cx| {
            FormDialog::new(d)
                .icon(PmIcon::Tree)
                .title(rust_i18n::t!("dlg.tree").to_string())
                .description(rust_i18n::t!("dlg.tree_hint").to_string())
                .width(px(720.))
                .estimated_height(px(560.))
                .body(dialog.clone())
                .build(window, cx)
        });
    }

    pub(crate) fn set_highlight(&mut self, highlight: FilterModel, cx: &mut Context<Self>) {
        self.state.update(cx, |s, _| s.set_highlight(highlight));
        self.notify_table(cx);
        cx.notify();
    }

    /// File ▸ Open .PML — pick a file via the native dialog and load it offline.
    ///
    /// The native dialog runs a blocking modal message loop, so it must NOT run on
    /// the gpui main thread (gpui owns that loop — a reentrant modal there crashes).
    /// We spawn the picker on its own thread and apply the result back on the UI
    /// loop once it returns.
    pub(crate) fn open_pml_dialog(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let (tx, rx) = crossbeam_channel::bounded::<Option<std::path::PathBuf>>(1);
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .add_filter("Process Monitor Log", &["pml", "PML"])
                .pick_file();
            let _ = tx.send(picked);
        });
        cx.spawn(async move |this, cx| {
            // Poll the picker thread without blocking the UI loop.
            let path = loop {
                match rx.try_recv() {
                    Ok(picked) => break picked,
                    Err(crossbeam_channel::TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(30))
                            .await;
                    }
                    Err(crossbeam_channel::TryRecvError::Disconnected) => break None,
                }
            };
            if let Some(path) = path {
                let _ = this.update(cx, |view, cx| {
                    view.state.update(cx, |s, _| s.open_pml(path));
                    view.notify_table(cx);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Opens the read-only System Activity Summary dialog. Stats are aggregated
    /// once here (owned), then the summary module renders the charts/bars.
    pub(crate) fn open_summary_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.state.read(cx).buffer.summary_rows();
        let stats = summary::SummaryStats::from_rows(&rows);
        window.open_dialog(cx, move |d, window, cx| {
            FormDialog::new(d)
                .icon(PmIcon::Perf)
                .title(rust_i18n::t!("dlg.summary").to_string())
                .description(rust_i18n::t!("dlg.summary_hint").to_string())
                .width(px(800.))
                .estimated_height(px(560.))
                .footer(summary::footer())
                .body(summary::render(&stats, cx))
                .build(window, cx)
        });
    }

    /// Opens the Process Activity Summary dialog (Tools menu). The dialog entity
    /// owns the table + filter; here we seed it with a snapshot and apply the shell.
    pub(crate) fn open_process_summary_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = self.state.read(cx).buffer.summary_rows();
        self.process_summary.update(cx, |d, cx| d.load(&rows, cx));
        let dialog = self.process_summary.clone();
        window.open_dialog(cx, move |d, window, cx| {
            let desc = dialog.read(cx).summary_text();
            FormDialog::new(d)
                .icon(PmIcon::Cpu)
                .title(rust_i18n::t!("dlg.proc_summary").to_string())
                .description(desc)
                .width(px(860.))
                .estimated_height(px(600.))
                .body(dialog.clone())
                .build(window, cx)
        });
    }

    /// Opens a File/Registry/Network summary (one parameterized dialog). Seeds the
    /// shared entity with the kind + a snapshot, then applies the FormDialog shell.
    pub(crate) fn open_path_summary_dialog(
        &mut self,
        kind: PathKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = self.state.read(cx).buffer.summary_rows();
        self.path_summary
            .update(cx, |d, cx| d.load(kind, &rows, cx));
        let dialog = self.path_summary.clone();
        window.open_dialog(cx, move |d, window, cx| {
            let (icon, title, desc) = {
                let d = dialog.read(cx);
                (kind.icon(), kind.title(), d.summary_text())
            };
            FormDialog::new(d)
                .icon(icon)
                .title(title)
                .description(desc)
                .width(px(860.))
                .estimated_height(px(600.))
                .body(dialog.clone())
                .build(window, cx)
        });
    }

    /// Opens the Cross Reference summary (paths touched by >1 process).
    pub(crate) fn open_xref_summary_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.state.read(cx).buffer.summary_rows();
        self.xref_summary.update(cx, |d, cx| d.load(&rows, cx));
        let dialog = self.xref_summary.clone();
        window.open_dialog(cx, move |d, window, cx| {
            let desc = dialog.read(cx).summary_text();
            FormDialog::new(d)
                .icon(PmIcon::Crosshair)
                .title(rust_i18n::t!("dlg.xref_summary").to_string())
                .description(desc)
                .width(px(860.))
                .estimated_height(px(600.))
                .body(dialog.clone())
                .build(window, cx)
        });
    }

    /// Opens the unified Settings dialog (Options menu). The dialog entity owns the
    /// draft; here we seed it from live state and apply the FormDialog shell.
    pub(crate) fn open_settings_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Read the config/theme here (not inside the dialog, which would re-borrow
        // AppView through its weak handle while this method holds the borrow).
        let (config, theme) = {
            let s = self.state.read(cx);
            (s.config.clone(), s.theme_mode)
        };
        self.settings_dialog
            .update(cx, |d, cx| d.load(config, theme, window, cx));
        let dialog = self.settings_dialog.clone();
        window.open_dialog(cx, move |d, window, cx| {
            FormDialog::new(d)
                .icon(PmIcon::Settings)
                .title(rust_i18n::t!("set.title").to_string())
                .width(px(760.))
                .estimated_height(px(560.))
                .footer(SettingsDialog::footer(&dialog))
                .body(dialog.clone())
                .build(window, cx)
        });
    }

    /// Commits the Settings dialog: stores the config and applies theme/locale +
    /// re-renders the table (highlight color / hex display take effect immediately).
    pub(crate) fn apply_settings(
        &mut self,
        config: AppConfig,
        theme: ThemeMode,
        zh: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Persist to %USERPROFILE%\openprocmon\config.json (best-effort; a write
        // failure is surfaced but doesn't block applying the in-memory settings).
        if let Err(e) = config.save() {
            window.push_notification(
                (
                    NotificationType::Error,
                    rust_i18n::t!("set.save_failed", detail = e.to_string()).to_string(),
                ),
                cx,
            );
        }
        self.state.update(cx, |s, _| {
            s.config = config;
            s.apply_retention();
            // Drop the cached resolver so it rebuilds with the new dbghelp/symbols
            // paths the next time a stack is symbolized.
            s.symbols = None;
        });
        self.set_theme_mode(theme, window, cx);
        self.set_locale(if zh { "zh" } else { "en" }, window, cx);
        self.notify_table(cx);
        cx.notify();
    }

    /// Opens the About dialog (Help menu). Design `AboutDialog` is a headerless
    /// centered card, so it uses the raw `Dialog` (not the `FormDialog` shell).
    pub(crate) fn open_about_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.open_dialog(cx, move |d, window, cx| {
            // Center the (content-sized) card; `p_0` so our body owns padding and no
            // header/close button is rendered (design has neither).
            let vh = window.viewport_size().height;
            let mt = ((vh - px(360.)) / 2.0).max(px(24.));
            d.p_0()
                .w(px(400.))
                .margin_top(mt)
                .close_button(false)
                .child(about::body(cx))
                .footer(about::footer(cx))
        });
    }

    /// Context-menu quick action: append an Include/Exclude rule on a column.
    pub(crate) fn quick_filter(
        &mut self,
        column: FilterColumn,
        value: String,
        action: FilterAction,
        cx: &mut Context<Self>,
    ) {
        self.quick_filter_rel(column, FilterRelation::Is, value, action, cx);
    }

    /// Like [`quick_filter`](Self::quick_filter) but with an explicit relation —
    /// used by the "Exclude Events Before/After" actions (Date & Time less/more than).
    pub(crate) fn quick_filter_rel(
        &mut self,
        column: FilterColumn,
        relation: FilterRelation,
        value: String,
        action: FilterAction,
        cx: &mut Context<Self>,
    ) {
        let mut model = self.state.read(cx).filter.clone();
        // The filter list is a set: skip duplicates, newest at the front.
        if model.add_front(FilterRule::new(column, relation, value, action)) {
            self.set_filter(model, cx);
        }
    }

    /// Context-menu quick action: highlight a process name (adds an Include rule
    /// on ProcessName to the highlight rule set).
    pub(crate) fn add_highlight(&mut self, name: String, cx: &mut Context<Self>) {
        let mut model = self.state.read(cx).highlight.clone();
        let dup = model
            .rules
            .iter()
            .any(|r| r.column == FilterColumn::ProcessName && r.value.eq_ignore_ascii_case(&name));
        if !dup {
            model.rules.push(FilterRule::new(
                FilterColumn::ProcessName,
                FilterRelation::Is,
                name,
                FilterAction::Include,
            ));
        }
        self.set_highlight(model, cx);
    }

    /// Menu/shortcut action: toggle the bookmark on the currently selected row.
    pub(crate) fn bookmark_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(ix) = self.state.read(cx).selected {
            self.toggle_bookmark(ix, cx);
        }
    }

    /// Context-menu quick action: toggle a row's bookmark.
    pub(crate) fn toggle_bookmark(&mut self, visible_ix: usize, cx: &mut Context<Self>) {
        self.state
            .update(cx, |s, _| s.buffer.toggle_bookmark(visible_ix));
        self.notify_table(cx);
        cx.notify();
    }

    /// Single-click selection. With the panel closed it only records the
    /// highlighted row (the table draws its own selection style) — the panel
    /// opens on double click. With the panel already open, single click switches
    /// the displayed row, so the user can browse rows once it's up.
    fn highlight_row(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.state.update(cx, |s, _| s.selected = Some(ix));
        if self.show_detail {
            let detail: Option<EventDetail> = {
                let s = self.state.read(cx);
                s.buffer.visible(ix).map(|row| s.source.detail_for(row))
            };
            if let Some(detail) = detail {
                let sym_inputs = Self::symbol_inputs(&detail);
                self.detail_view
                    .update(cx, |d, cx| d.set_detail(detail, cx));
                self.spawn_symbol_resolution(sym_inputs, cx);
            }
        }
        cx.notify();
    }

    /// Builds the rich detail for the selected visible row and shows it in the
    /// docked detail panel (beside the table; the table stays interactive).
    fn select_row(&mut self, ix: usize, cx: &mut Context<Self>) {
        let detail: Option<EventDetail> = {
            let s = self.state.read(cx);
            s.buffer.visible(ix).map(|row| s.source.detail_for(row))
        };
        if let Some(detail) = detail {
            let sym_inputs = Self::symbol_inputs(&detail);
            self.state.update(cx, |s, _| s.selected = Some(ix));
            self.detail_view
                .update(cx, |d, cx| d.set_detail(detail, cx));
            self.show_detail = true;
            self.spawn_symbol_resolution(sym_inputs, cx);
        }
        cx.notify();
    }

    /// Snapshots the inputs the async symbol resolver needs (frame addresses + this
    /// process's module ranges), as owned data safe to move onto a worker thread.
    fn symbol_inputs(detail: &EventDetail) -> SymbolInputs {
        let frames = detail
            .stack
            .iter()
            .enumerate()
            .map(|(i, f)| (i, f.address))
            .collect();
        let mods = detail
            .modules
            .iter()
            .map(|m| (m.base, m.size, m.path.to_string()))
            .collect();
        (frames, mods)
    }

    /// Resolves the selected event's call-stack symbols off the UI thread (cf. the
    /// C++ `CResolveSymbolThread`), then applies them back to the detail panel. A
    /// no-op when no dbghelp is configured/present or the event has no stack.
    fn spawn_symbol_resolution(&self, inputs: SymbolInputs, cx: &mut Context<Self>) {
        let (frames, mut mods) = inputs;
        if frames.is_empty() {
            return;
        }
        // Fetch the shared resolver (lazily built) and the System/PID-4 driver
        // modules used to symbolize kernel-mode frames.
        let (resolver, kernel) = self.state.update(cx, |s, _| {
            let kernel: Vec<(u64, u64, String)> = s
                .source
                .kernel_modules()
                .into_iter()
                .map(|m| (m.base, m.size, m.path.to_string()))
                .collect();
            (s.symbol_resolver(), kernel)
        });
        let Some(resolver) = resolver else {
            return;
        };
        mods.extend(kernel);

        // Capture the generation *after* set_detail bumped it, so a later selection
        // invalidates this result.
        let generation = self.detail_view.read(cx).symbol_gen();
        let detail_view = self.detail_view.clone();
        cx.spawn(async move |_this, cx| {
            let resolved = cx
                .background_executor()
                .spawn(async move {
                    let symmods: Vec<procmon_sdk::SymModule> = mods
                        .iter()
                        .map(|(b, sz, p)| procmon_sdk::SymModule {
                            base: *b,
                            size: *sz,
                            path: p.as_str(),
                        })
                        .collect();
                    frames
                        .iter()
                        .filter_map(|&(i, addr)| {
                            resolver.resolve(addr, &symmods).map(|s| (i, s.to_string()))
                        })
                        .collect::<Vec<(usize, String)>>()
                })
                .await;
            if resolved.is_empty() {
                return;
            }
            detail_view.update(cx, |d, cx| d.apply_symbols(generation, resolved, cx));
        })
        .detach();
    }

    /// Hides the docked detail panel (close button only — never on focus loss).
    pub(crate) fn close_detail(&mut self, cx: &mut Context<Self>) {
        self.show_detail = false;
        cx.notify();
    }

    /// Re-renders the table rows after a data/view change *without* rebuilding the
    /// columns. `TableState::refresh` rebuilds the column groups from the
    /// delegate's fixed widths, which would discard any user column resize — so
    /// only `set_locale` (which actually changes the column titles) calls
    /// `refresh`; everything else just notifies the table to re-render its rows.
    fn notify_table(&self, cx: &mut Context<Self>) {
        self.table_state.update(cx, |_, cx| cx.notify());
    }

    fn on_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let changed = self.state.update(cx, |s, _| s.drain());
        if let Some((level, message)) = self.state.update(cx, |s, _| s.take_notice()) {
            // Toast at top-right: red for errors (driver/connect/save failures),
            // green for save success. Auto-hides; the message carries the detail.
            window.push_notification((level, message), cx);
        }
        if !changed {
            return;
        }
        self.notify_table(cx);
        let (autoscroll, visible) = {
            let s = self.state.read(cx);
            (s.autoscroll, s.buffer.visible_len())
        };
        if autoscroll && visible > 0 {
            self.table_state
                .update(cx, |t, cx| t.scroll_to_row(visible - 1, cx));
        }
        cx.notify();
    }

    /// Edit ▸ Copy (Ctrl+C): copies the selected row's columns to the clipboard as
    /// tab-separated values (`#`, Time, Process, PID, Operation, Path, Result,
    /// Detail), so it pastes cleanly into a spreadsheet. No-op when nothing is
    /// selected. The Operation honors the Advanced Display toggle, matching the table.
    fn copy_selected_row(&self, cx: &mut Context<Self>) {
        let text = {
            let s = self.state.read(cx);
            let Some(row) = s.selected.and_then(|ix| s.buffer.visible(ix)) else {
                return;
            };
            let advance = s.advanced_display;
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                row.seq(),
                row.time(),
                row.process_name(),
                row.pid(),
                row.operation_display(advance),
                row.path(),
                row.result(),
                row.detail(),
            )
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
    }

    /// Toasts a "not implemented yet" warning for menu items that aren't wired up.
    fn notify_unimplemented(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.push_notification(
            (
                NotificationType::Warning,
                rust_i18n::t!("notify.unimplemented").to_string(),
            ),
            cx,
        );
    }

    /// File ▸ Export Settings: writes the current config to a user-chosen `.json`.
    fn export_settings(&self, window: &mut Window, cx: &mut Context<Self>) {
        let config = self.state.read(cx).config.clone();
        let (tx, rx) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send(
                rfd::FileDialog::new()
                    .set_file_name("openprocmon-config.json")
                    .add_filter("JSON", &["json"])
                    .save_file(),
            );
        });
        cx.spawn_in(window, async move |this, cx| {
            let picked = loop {
                match rx.try_recv() {
                    Ok(p) => break p,
                    Err(crossbeam_channel::TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(30))
                            .await;
                    }
                    Err(_) => break None,
                }
            };
            let Some(path) = picked else { return };
            let result = config.save_to(&path);
            let _ = this.update_in(cx, |_, window, cx| match result {
                Ok(()) => {
                    window.push_notification(
                        (
                            NotificationType::Success,
                            rust_i18n::t!("set.export_ok").to_string(),
                        ),
                        cx,
                    );
                }
                Err(e) => {
                    window.push_notification(
                        (
                            NotificationType::Error,
                            rust_i18n::t!("set.export_failed", detail = e.to_string()).to_string(),
                        ),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    /// File ▸ Import Settings: loads a config from a user-chosen `.json`, overwriting
    /// the live config and persisting it to the canonical location.
    fn import_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (tx, rx) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send(
                rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .pick_file(),
            );
        });
        cx.spawn_in(window, async move |this, cx| {
            let picked = loop {
                match rx.try_recv() {
                    Ok(p) => break p,
                    Err(crossbeam_channel::TryRecvError::Empty) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(30))
                            .await;
                    }
                    Err(_) => break None,
                }
            };
            let Some(path) = picked else { return };
            let loaded = AppConfig::load_from(&path);
            let _ = this.update_in(cx, |view, window, cx| match loaded {
                Ok(cfg) => view.apply_imported_config(cfg, window, cx),
                Err(e) => {
                    window.push_notification(
                        (
                            NotificationType::Error,
                            rust_i18n::t!("set.import_failed", detail = e).to_string(),
                        ),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    /// Overwrites the live config with an imported one, persists it, and re-renders.
    fn apply_imported_config(
        &mut self,
        cfg: AppConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(e) = cfg.save() {
            window.push_notification(
                (
                    NotificationType::Error,
                    rust_i18n::t!("set.save_failed", detail = e.to_string()).to_string(),
                ),
                cx,
            );
        }
        self.state.update(cx, |s, _| {
            s.config = cfg;
            s.apply_retention();
            s.symbols = None;
        });
        self.notify_table(cx);
        cx.notify();
        window.push_notification(
            (
                NotificationType::Success,
                rust_i18n::t!("set.import_ok").to_string(),
            ),
            cx,
        );
    }

    /// Flips light/dark mode, keeping the gpui-component theme and our custom
    /// palette in sync (see [`theme::set_mode`]).
    pub(crate) fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = if self.state.read(cx).theme_mode.is_dark() {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };
        self.state.update(cx, |s, _| s.theme_mode = next);
        theme::set_mode(next, window, cx);
        cx.notify();
    }

    /// Switches to a specific appearance (Options ▸ Theme submenu).
    pub(crate) fn set_theme_mode(
        &mut self,
        mode: ThemeMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |s, _| s.theme_mode = mode);
        theme::set_mode(mode, window, cx);
        cx.notify();
    }

    /// Switches the UI locale and re-localizes (table headers + full redraw).
    pub(crate) fn set_locale(
        &mut self,
        code: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        rust_i18n::set_locale(code);
        self.table_state.update(cx, |t, cx| {
            t.delegate_mut().retitle();
            t.refresh(cx);
        });
        // The menu triggers re-localize on render and the dropdown is rebuilt on
        // each open, so no explicit menu rebuild is needed.
        window.refresh();
    }

    pub(crate) fn toggle_capture(&mut self, cx: &mut Context<Self>) {
        let on = !self.state.read(cx).capturing;
        self.state.update(cx, |s, _| s.set_capturing(on));
        cx.notify();
    }

    pub(crate) fn toggle_autoscroll(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |s, _| s.autoscroll = !s.autoscroll);
        cx.notify();
    }

    pub(crate) fn toggle_always_on_top(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let on = !self.state.read(cx).always_on_top;
        self.state.update(cx, |s, _| s.always_on_top = on);
        set_window_topmost(window, on);
        cx.notify();
    }

    pub(crate) fn toggle_monitor(&mut self, which: MonitorKind, cx: &mut Context<Self>) {
        self.state.update(cx, |s, _| {
            let mut m = s.monitor;
            which.apply(&mut m);
            s.set_monitor(m);
        });
        self.notify_table(cx);
        cx.notify();
    }

    pub(crate) fn clear(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |s, _| s.clear());
        self.notify_table(cx);
        cx.notify();
    }

    #[allow(dead_code)]
    pub(crate) fn set_filter(&mut self, filter: FilterModel, cx: &mut Context<Self>) {
        self.state.update(cx, |s, _| s.set_filter(filter));
        self.notify_table(cx);
        cx.notify();
    }

    /// Event ▸ "Advanced Display": toggle Advanced Output. Enabling it strips the
    /// default display filter (showing every event with low-level operation names);
    /// disabling re-applies the filter. State is derived from the filter contents.
    pub(crate) fn toggle_advanced_display(&mut self, cx: &mut Context<Self>) {
        let mut model = self.state.read(cx).filter.clone();
        let on = !advanced_display_on(&model);
        set_advanced_display(&mut model, on);
        self.set_filter(model, cx);
    }

    pub(crate) fn set_search(&mut self, search: String, cx: &mut Context<Self>) {
        self.state.update(cx, |s, _| s.set_search(search));
        self.notify_table(cx);
        cx.notify();
    }
}

impl AppView {
    /// The central workspace: the event table, plus a resizable detail panel
    /// docked on the right when a row is selected.
    fn render_workspace(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        // Table fills the row; data cells use a monospace face (~11.4px ≈ design).
        let table = div()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .font_family("Consolas")
            .text_sm()
            .child(
                DataTable::new(&self.table_state)
                    .stripe(true)
                    .bordered(false),
            );

        // Workspace = table + (when shown) a docked detail panel beside it. The
        // panel is in-layout (no overlay), so the table stays interactive and the
        // panel's height matches the table area; it closes only via its button.
        let mut row = h_flex().size_full().min_h(px(0.)).child(table);
        if self.show_detail {
            row = row.child(
                div()
                    .w(px(460.))
                    .h_full()
                    .flex_shrink_0()
                    .border_l_1()
                    .border_color(cx.theme().border)
                    .child(self.detail_view.clone()),
            );
        }
        row
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Design `.app.pinned`: an accent strip across the top when "Always on Top".
        let pinned = self.state.read(cx).always_on_top;
        let accent = cx.theme().primary;
        v_flex()
            .size_full()
            .track_focus(&self.focus_handle)
            .when(pinned, |s| s.border_t_2().border_color(accent))
            .on_action(cx.listener(|view, _: &ToggleCapture, _, cx| view.toggle_capture(cx)))
            .on_action(cx.listener(|view, _: &ToggleAutoscroll, _, cx| view.toggle_autoscroll(cx)))
            .on_action(cx.listener(|view, _: &ClearDisplay, _, cx| view.clear(cx)))
            .on_action(
                cx.listener(|view, _: &FocusSearch, window, cx| view.focus_search(window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &OpenFilter, window, cx| view.open_filter_dialog(window, cx)),
            )
            .on_action(cx.listener(|view, _: &ClearFilter, _, cx| {
                view.set_filter(FilterModel::default(), cx)
            }))
            .on_action(cx.listener(|view, _: &ToggleAdvancedDisplay, _, cx| {
                view.toggle_advanced_display(cx)
            }))
            .on_action(cx.listener(|view, _: &OpenHighlight, window, cx| {
                view.open_highlight_dialog(window, cx)
            }))
            .on_action(cx.listener(|view, _: &ClearHighlight, _, cx| {
                view.set_highlight(FilterModel::default(), cx)
            }))
            .on_action(cx.listener(|view, _: &Bookmark, _, cx| view.bookmark_selected(cx)))
            .on_action(cx.listener(|view, _: &AlwaysOnTop, window, cx| {
                view.toggle_always_on_top(window, cx)
            }))
            .on_action(
                cx.listener(|view, _: &OpenTree, window, cx| view.open_tree_dialog(window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &OpenSummary, window, cx| {
                    view.open_summary_dialog(window, cx)
                }),
            )
            .on_action(cx.listener(|view, _: &OpenProcessSummary, window, cx| {
                view.open_process_summary_dialog(window, cx)
            }))
            .on_action(cx.listener(|view, _: &OpenFileSummary, window, cx| {
                view.open_path_summary_dialog(PathKind::File, window, cx)
            }))
            .on_action(cx.listener(|view, _: &OpenRegSummary, window, cx| {
                view.open_path_summary_dialog(PathKind::Registry, window, cx)
            }))
            .on_action(cx.listener(|view, _: &OpenNetSummary, window, cx| {
                view.open_path_summary_dialog(PathKind::Network, window, cx)
            }))
            .on_action(cx.listener(|view, _: &OpenXrefSummary, window, cx| {
                view.open_xref_summary_dialog(window, cx)
            }))
            .on_action(cx.listener(|view, _: &OpenSettings, window, cx| {
                view.open_settings_dialog(window, cx)
            }))
            .on_action(cx.listener(|view, _: &Open, window, cx| view.open_pml_dialog(window, cx)))
            .on_action(cx.listener(|view, _: &Save, window, cx| view.open_save_dialog(window, cx)))
            .on_action(
                cx.listener(|view, _: &About, window, cx| view.open_about_dialog(window, cx)),
            )
            .on_action(cx.listener(|view, action: &SwitchThemeMode, window, cx| {
                view.set_theme_mode(action.0, window, cx)
            }))
            .on_action(cx.listener(|view, action: &SelectLocale, window, cx| {
                let code = if action.0.starts_with("zh") {
                    "zh"
                } else {
                    "en"
                };
                view.set_locale(code, window, cx);
            }))
            // Menu items without a backing implementation yet: each shows a "not
            // implemented" toast so the menu is fully clickable.
            .on_action(cx.listener(|v, _: &Copy, _, cx| v.copy_selected_row(cx)))
            .on_action(cx.listener(|v, _: &WebSearch, w, cx| v.notify_unimplemented(w, cx)))
            .on_action(cx.listener(|v, _: &ImportSettings, w, cx| v.import_settings(w, cx)))
            .on_action(cx.listener(|v, _: &ExportSettings, w, cx| v.export_settings(w, cx)))
            .on_action(cx.listener(|v, _: &HelpTopics, w, cx| v.notify_unimplemented(w, cx)))
            .on_action(cx.listener(|v, _: &CheckUpdates, w, cx| v.notify_unimplemented(w, cx)))
            .on_action(|_: &Quit, _, cx| cx.quit())
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(menubar::render(&self.menu_bar, cx))
            .child(toolbar::render(&self.state, &self.search_input, cx))
            .child(monitorbar::render(&self.state, cx))
            .child(self.render_workspace(cx))
            .child(statusbar::render(&self.state, cx))
            // Overlay layers (Root renders only the view, not these).
            .children(Root::render_notification_layer(window, cx))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
    }
}

/// Toggles the OS "always on top" window flag. gpui exposes no window-level API, so
/// on Windows we go straight to Win32 `SetWindowPos` via the window's native HWND
/// (from gpui's `HasWindowHandle`); other platforms are a no-op.
#[cfg(windows)]
fn set_window_topmost(window: &Window, on: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };

    // gpui's `Window` also has an inherent `window_handle()` returning its own type,
    // so call the trait method explicitly to get the raw handle.
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    if let RawWindowHandle::Win32(h) = handle.as_raw() {
        let hwnd = h.hwnd.get() as HWND;
        let insert_after = if on { HWND_TOPMOST } else { HWND_NOTOPMOST };
        // SAFETY: a valid live HWND from the platform; flags keep position/size/focus.
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                insert_after,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

#[cfg(not(windows))]
fn set_window_topmost(_window: &Window, _on: bool) {}
