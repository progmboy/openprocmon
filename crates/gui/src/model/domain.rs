//! GUI-owned, display-ready domain model.
//!
//! The SDK's `Event` is lazy, borrowed and not `Clone`, so it cannot back a
//! retained, virtualized table. Instead every incoming event is wrapped once into a
//! [`CapturedEvent`] that owns the `Event` and exposes cheap, lazily-cached display
//! columns. The heavier [`EventDetail`] (process info, modules, call stack) is built
//! on demand only when a row is selected. These types are the single shape the whole
//! UI renders, for both the live and PML sources.
//!
//! Some fields (process/module icons, module base/size, the `Other` category)
//! are populated by the real SDK backend and richer detail views; they are part
//! of the stable model shape, so dead-code is allowed at the module level.
#![allow(dead_code)]

use std::sync::Arc;

use gpui::{Hsla, SharedString};

use crate::theme::ProcmonPalette;

/// Event category, mirroring the SDK's `EventClass`. Drives row/operation color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventCategory {
    Registry,
    File,
    Network,
    Process,
    Profiling,
    Other,
}

impl EventCategory {
    pub fn label(self) -> &'static str {
        match self {
            EventCategory::Registry => "Registry",
            EventCategory::File => "File System",
            EventCategory::Network => "Network",
            EventCategory::Process => "Process",
            EventCategory::Profiling => "Profiling",
            EventCategory::Other => "Other",
        }
    }

    /// The category's operation color from the active palette.
    pub fn color(self, pal: &ProcmonPalette) -> Hsla {
        match self {
            EventCategory::Registry => pal.op_registry,
            EventCategory::File => pal.op_file,
            EventCategory::Network => pal.op_network,
            EventCategory::Process => pal.op_process,
            EventCategory::Profiling => pal.op_perf,
            EventCategory::Other => pal.op_perf,
        }
    }
}

/// Result classification for coloring the Result column / badge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultKind {
    Success,
    Error,
    Warn,
    Info,
}

impl ResultKind {
    pub fn color(self, pal: &ProcmonPalette) -> Hsla {
        match self {
            ResultKind::Success => pal.res_success,
            ResultKind::Error => pal.res_error,
            ResultKind::Warn => pal.res_warn,
            ResultKind::Info => pal.res_info,
        }
    }

    /// Classify a Procmon-style result string (e.g. "SUCCESS", "NAME NOT FOUND",
    /// "0xC0000034") into a color bucket.
    pub fn classify(result: &str) -> ResultKind {
        let r = result.trim();
        if r.eq_ignore_ascii_case("SUCCESS") {
            ResultKind::Success
        } else if r.is_empty() || r.eq_ignore_ascii_case("REPARSE") {
            ResultKind::Info
        } else if r.contains("PENDING") {
            ResultKind::Warn
        } else {
            // Any named status or NTSTATUS hex code is treated as an error.
            ResultKind::Error
        }
    }
}

/// Stable handle used to (lazily) fetch the rich detail for a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DetailKey {
    pub pid: u32,
    pub seq: u64,
}

/// Event backend (live or PML): owns the non-`Clone` unified `procmon_sdk::Event`
/// and computes display columns lazily (cached by [`CapturedEvent`]). PML events
/// resolve their process columns/icon via the reader the `Event` holds internally.
pub struct EventBackend {
    ev: procmon_sdk::Event,
}

impl EventBackend {
    /// The underlying SDK event (for the source to build the rich detail).
    pub(crate) fn event(&self) -> &procmon_sdk::Event {
        &self.ev
    }

    fn render_cells(&self) -> RenderCells {
        let ev = &self.ev;
        RenderCells {
            time: ev.time_of_day().into(),
            process_name: ev.process_name().unwrap_or("").into(),
            // operation_name() is &'static — no allocation.
            operation: SharedString::new_static(ev.operation_name()),
            path: ev.path().unwrap_or_default().into(),
            result: ev.result().into_owned().into(),
        }
    }

    /// Operation name honoring the Advanced Display toggle (`&'static`, uncached).
    fn operation_display(&self, advance: bool) -> SharedString {
        SharedString::new_static(self.ev.operation_name_advanced(advance))
    }

    fn detail(&self) -> SharedString {
        self.ev.detail().into()
    }

    /// Read live (not cached) so async SDK metadata appears on the next frame.
    fn icon(&self) -> Option<Arc<[u8]>> {
        self.ev
            .icon_small()
            .or_else(|| self.ev.icon_large())
            .map(Arc::<[u8]>::from)
    }
}

/// Maps the SDK's `EventClass` to the GUI category.
fn event_class_to_category(c: procmon_sdk::EventClass) -> EventCategory {
    use procmon_sdk::EventClass as E;
    match c {
        E::Process => EventCategory::Process,
        E::File => EventCategory::File,
        E::Registry => EventCategory::Registry,
        E::Network => EventCategory::Network,
        E::Profiling => EventCategory::Profiling,
        E::Other => EventCategory::Other,
    }
}

/// Lazily-computed display columns (filter/render-relevant; not `detail`, which is
/// cached separately so filtering by path doesn't force a detail parse).
#[derive(Clone)]
pub(crate) struct RenderCells {
    pub time: SharedString,
    pub process_name: SharedString,
    pub operation: SharedString,
    pub path: SharedString,
    pub result: SharedString,
}

/// One captured event in the buffer. Stored BY VALUE and addressed via the view
/// index — intentionally NOT `Clone` (it holds a non-`Clone` `procmon_sdk::Event`).
/// Code needing an owned, cloneable snapshot uses [`EventSummaryRow`]. Display
/// columns are produced via methods (lazy + cached); metadata-dependent fields
/// (icon) are read live each render so async metadata appears on the next frame.
pub struct CapturedEvent {
    /// Monotonic display index (the `#` column).
    seq: u64,
    pid: u32,
    category: EventCategory,
    result_kind: ResultKind,
    bookmarked: bool,
    highlighted: bool,
    backend: EventBackend,
    /// Lazy display columns.
    cells: std::cell::OnceCell<RenderCells>,
    /// Lazy detail column (cached separately from `cells`).
    detail_cell: std::cell::OnceCell<SharedString>,
}

impl CapturedEvent {
    /// Builds a row from a unified SDK event (live or PML). The buffer takes
    /// ownership; display columns are computed lazily + cached on first access.
    /// Scalars are taken eagerly here — they're zero-cost and the filter/counts hot
    /// paths use them.
    pub fn from_event(ev: procmon_sdk::Event, seq: u64) -> Self {
        Self {
            seq,
            pid: ev.pid(),
            category: event_class_to_category(ev.class()),
            result_kind: ResultKind::classify(&ev.result()),
            bookmarked: false,
            highlighted: false,
            backend: EventBackend { ev },
            cells: std::cell::OnceCell::new(),
            detail_cell: std::cell::OnceCell::new(),
        }
    }

    /// Lazily computes + caches the display columns.
    fn render_cells(&self) -> &RenderCells {
        self.cells.get_or_init(|| self.backend.render_cells())
    }

    /// Lazily computes + caches the detail column.
    fn detail_column(&self) -> SharedString {
        self.detail_cell
            .get_or_init(|| self.backend.detail())
            .clone()
    }

    // --- scalars (zero-copy) ---
    pub fn seq(&self) -> u64 {
        self.seq
    }
    pub fn pid(&self) -> u32 {
        self.pid
    }
    pub fn category(&self) -> EventCategory {
        self.category
    }
    pub fn result_kind(&self) -> ResultKind {
        self.result_kind
    }
    pub fn bookmarked(&self) -> bool {
        self.bookmarked
    }
    pub fn set_bookmarked(&mut self, on: bool) {
        self.bookmarked = on;
    }
    pub fn highlighted(&self) -> bool {
        self.highlighted
    }
    pub fn set_highlighted(&mut self, on: bool) {
        self.highlighted = on;
    }
    pub fn detail_key(&self) -> DetailKey {
        DetailKey {
            pid: self.pid,
            seq: self.seq,
        }
    }
    /// The underlying SDK event (for the source's `detail_for`, export, filtering).
    pub fn event(&self) -> &procmon_sdk::Event {
        self.backend.event()
    }

    /// Approximate retained size in bytes (PRE+POST record bytes), for the history
    /// ring buffer's memory accounting.
    pub fn byte_size(&self) -> usize {
        self.backend.event().byte_size()
    }

    /// The event's raw timestamp (100-ns ticks) for the history age window.
    pub fn time_raw(&self) -> i64 {
        self.backend.event().time_raw()
    }

    // --- display columns (lazy + cached) ---
    pub fn time(&self) -> SharedString {
        self.render_cells().time.clone()
    }
    pub fn process_name(&self) -> SharedString {
        self.render_cells().process_name.clone()
    }
    pub fn operation(&self) -> SharedString {
        self.render_cells().operation.clone()
    }
    /// Operation name honoring the "Advanced Display" toggle (C++ `bAdvance`): the
    /// table column uses this so toggling switches friendly ⇄ raw `IRP_MJ_*`/`FASTIO_*`
    /// names. Computed live (cheap `&'static`) — no cached cell to invalidate. The
    /// plain [`operation`](Self::operation) stays canonical for search/export/detail.
    pub fn operation_display(&self, advance: bool) -> SharedString {
        self.backend.operation_display(advance)
    }
    pub fn path(&self) -> SharedString {
        self.render_cells().path.clone()
    }
    pub fn result(&self) -> SharedString {
        self.render_cells().result.clone()
    }
    pub fn detail(&self) -> SharedString {
        self.detail_column()
    }
    /// Process icon — read live (not cached) so async SDK metadata appears next frame.
    pub fn icon(&self) -> Option<Arc<[u8]>> {
        self.backend.icon()
    }

    /// An owned, cloneable projection for analytics dialogs (forces the lazy cells).
    pub fn summary_row(&self) -> EventSummaryRow {
        EventSummaryRow {
            pid: self.pid,
            process_name: self.process_name(),
            category: self.category,
            operation: self.operation(),
            path: self.path(),
            result: self.result(),
            icon: self.icon(),
        }
    }
}

/// Owned, cloneable projection of a row for analytics dialogs (Summary / Process /
/// Path / Xref) which snapshot the whole buffer.
#[derive(Clone)]
pub struct EventSummaryRow {
    pub pid: u32,
    pub process_name: SharedString,
    pub category: EventCategory,
    pub operation: SharedString,
    pub path: SharedString,
    pub result: SharedString,
    pub icon: Option<Arc<[u8]>>,
}

/// Kernel vs user frame, for call-stack coloring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Kernel,
    User,
}

impl FrameKind {
    pub fn color(self, pal: &ProcmonPalette) -> Hsla {
        match self {
            FrameKind::Kernel => pal.frame_kernel,
            FrameKind::User => pal.frame_user,
        }
    }

    pub fn tag(self) -> &'static str {
        match self {
            FrameKind::Kernel => "K",
            FrameKind::User => "U",
        }
    }
}

/// A loaded module row (Process tab modules list).
#[derive(Clone, Debug)]
pub struct ModuleRow {
    pub name: SharedString,
    pub path: SharedString,
    pub base: u64,
    pub size: u64,
}

/// A call-stack frame row (Stack tab).
#[derive(Clone, Debug)]
pub struct StackRow {
    pub frame: u32,
    pub kind: FrameKind,
    pub module: SharedString,
    pub location: SharedString,
    pub address: u64,
    pub path: SharedString,
}

/// A process node for the Process tab and the Process Tree dialog.
#[derive(Clone, Debug)]
pub struct ProcessNode {
    pub pid: u32,
    pub name: SharedString,
    pub company: SharedString,
    pub version: SharedString,
    pub running: bool,
    pub integrity: SharedString,
    pub arch: SharedString,
    pub parent_pid: u32,
    pub session_id: u32,
    pub virtualized: bool,
    pub user: SharedString,
    pub start_time: SharedString,
    pub image_path: SharedString,
    pub command_line: SharedString,
    pub icon: Option<Arc<[u8]>>,
    pub children: Vec<ProcessNode>,
}

/// The rich, per-event detail backing the three detail-panel tabs.
#[derive(Clone, Debug)]
pub struct EventDetail {
    pub category: EventCategory,
    pub operation: SharedString,
    pub time: SharedString,
    pub date: SharedString,
    pub duration: Option<SharedString>,
    pub pid: u32,
    pub tid: u32,
    pub path: SharedString,
    pub result: SharedString,
    pub result_kind: ResultKind,
    pub other_details: SharedString,
    pub target_version: Option<SharedString>,
    pub target_company: Option<SharedString>,
    pub signed: Option<bool>,
    pub process: ProcessNode,
    pub modules: Vec<ModuleRow>,
    pub stack: Vec<StackRow>,
}

/// Live per-category event counts for the monitor bar badges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CategoryCounts {
    pub registry: u64,
    pub file: u64,
    pub network: u64,
    pub process: u64,
    pub profiling: u64,
}

impl CategoryCounts {
    pub fn bump(&mut self, category: EventCategory) {
        match category {
            EventCategory::Registry => self.registry += 1,
            EventCategory::File => self.file += 1,
            EventCategory::Network => self.network += 1,
            EventCategory::Process => self.process += 1,
            EventCategory::Profiling => self.profiling += 1,
            EventCategory::Other => {}
        }
    }
}
