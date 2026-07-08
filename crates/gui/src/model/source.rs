//! The event-source boundary that decouples the UI from where events come from.
//!
//! The whole GUI depends only on [`EventSource`] + the owned domain types, so the
//! live (SDK) and offline (PML) backends are interchangeable. The source delivers
//! events over a crossbeam channel that the app drains on a frame timer.

use crossbeam_channel::Receiver;
use gpui::SharedString;

use crate::app::MonitorToggles;
use crate::model::domain::{CapturedEvent, CategoryCounts, EventDetail, ModuleRow, ProcessNode};
use crate::model::filter::FilterModel;

/// A message from the source to the UI.
// In practice nearly every message is a `Row`, so the size skew vs the rare
// counts/error variants wastes nothing; boxing `Row` (clippy's suggestion)
// would instead cost a heap allocation per captured event.
#[allow(clippy::large_enum_variant)]
pub enum SourceEvent {
    /// A newly captured event row.
    Row(CapturedEvent),
    /// Updated per-category totals. Reserved for backends that push aggregate
    /// counts (the buffer derives counts from rows today).
    #[allow(dead_code)]
    CountsChanged(CategoryCounts),
    /// A source-level error to surface to the user (e.g. driver connect failed,
    /// `.PML` open failed). The app shows it as a notification.
    Error(SharedString),
}

/// A backend producing events. Implementations: `SdkSource` (live capture) and
/// `PmlSource` (offline `.PML` viewing).
pub trait EventSource: Send + 'static {
    /// Begins producing and returns the channel the UI drains.
    fn start(&mut self) -> Receiver<SourceEvent>;
    /// Stops production and releases resources.
    fn stop(&mut self);
    /// Pauses/resumes capture without tearing the source down.
    fn set_capturing(&mut self, on: bool);
    /// Selects which categories to capture (driver-level for the SDK backend).
    fn set_monitor(&mut self, flags: MonitorToggles);
    /// Pushes the active filter (controller-level for the SDK backend). The UI
    /// still re-evaluates its buffer view, so this is an optimization hint.
    fn set_filter(&mut self, filter: FilterModel);
    /// Builds the rich detail for a selected row. The Event-tab fields come from
    /// the row's columns; the source adds process info, modules and the call stack.
    fn detail_for(&self, row: &CapturedEvent) -> EventDetail;
    /// Snapshot of the process tree for the Process Tree dialog.
    fn process_tree(&self) -> Vec<ProcessNode>;
    /// System (PID 4) driver modules, for symbolizing kernel call-stack frames when
    /// exporting to XML.
    fn kernel_modules(&self) -> Vec<ModuleRow> {
        Vec::new()
    }
    /// The backing `PmlReader` when this source replays a `.PML` (else `None`).
    /// Save-as-PML uses it for a byte-faithful subset copy (`write_subset`) that
    /// keeps the capture's host header and full process table, instead of
    /// re-encoding rows through a `PmlWriter` stamped with *this* machine.
    fn as_pml_reader(&self) -> Option<std::sync::Arc<procmon_sdk::PmlReader>> {
        None
    }
}
