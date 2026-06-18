//! GUI filter glue around the SDK's single filter engine.
//!
//! The filter model and evaluation live in `procmon_sdk::filter` (one engine for
//! both the driver pipeline and the UI). This module only adds the GUI-side
//! pieces: it re-exports the SDK types under the names the GUI uses, implements
//! [`procmon_sdk::FilterFields`] for the GUI's [`CapturedEvent`] (delegating to the
//! unified `procmon_sdk::Event` it carries), and holds the GUI-specific helpers
//! (monitor-toggle gating, the Advanced Display rule set).

use procmon_sdk::filter::{Column, FilterFields};
use procmon_sdk::Relation::Contains;

use crate::app::MonitorToggles;
use crate::model::domain::{CapturedEvent, EventCategory};

// The GUI refers to the SDK's filter types under these names.
pub use procmon_sdk::filter::{
    Action as FilterAction, Column as FilterColumn, FilterSet as FilterModel,
    Relation as FilterRelation, Rule as FilterRule,
};

/// Lets the SDK's `FilterSet` evaluate rules against a GUI row. The row carries the
/// unified `procmon_sdk::Event` (live or PML), which supports every column. The
/// expensive derived columns (Path, Detail) are served from the row's lazy render
/// caches, so a rule evaluation and the later table render derive them once
/// between them; everything else delegates straight to the event.
impl FilterFields for CapturedEvent {
    fn filter_field(&self, column: Column) -> Option<std::borrow::Cow<'_, str>> {
        use std::borrow::Cow;
        match column {
            Column::Path => {
                let p = self.path_str();
                if p.is_empty() {
                    // The cell stores "" for a missing path; preserve the SDK's
                    // `None` semantics by re-asking the event in that case.
                    self.event().filter_field(column)
                } else {
                    Some(Cow::Borrowed(p))
                }
            }
            Column::Detail => Some(Cow::Borrowed(self.detail_str())),
            _ => self.event().filter_field(column),
        }
    }

    fn filter_number(&self, column: Column) -> Option<i64> {
        self.event().filter_number(column)
    }
}

/// Whether a category is enabled by the monitor toggles (used to gate the view).
pub fn category_enabled(category: EventCategory, monitor: &MonitorToggles) -> bool {
    match category {
        EventCategory::Registry => monitor.registry,
        EventCategory::File => monitor.file,
        EventCategory::Network => monitor.network,
        EventCategory::Process => monitor.process,
        EventCategory::Profiling => monitor.profiling,
        EventCategory::Other => true,
    }
}

/// Procmon's default display filter — the normal (non-advanced) view's exclude set:
/// excludes the monitoring tools themselves and the System process, the IRP/FastIO
/// bookkeeping operations, and NTFS metadata files. Always appended at the end of
/// the set so they evaluate after any user rules. Enabling Advanced Output (the
/// Event menu) strips this filter; disabling it re-applies it.
pub fn default_display_filter() -> Vec<FilterRule> {
    use FilterAction::Exclude;
    use FilterColumn::{Operation, Path, ProcessName, Result};
    use FilterRelation::{BeginsWith, EndsWith, Is};

    let proc = |name: &str| FilterRule::new(ProcessName, Is, name, Exclude);
    let ends = |name: &str| FilterRule::new(Path, EndsWith, name, Exclude);
    let contains = |name: &str| FilterRule::new(Path, Contains, name, Exclude);
    vec![
        proc("OpenProcmon.exe"),
        proc("Procmon.exe"),
        proc("Procexp.exe"),
        proc("Autoruns.exe"),
        proc("Procmon64.exe"),
        proc("Procexp64.exe"),
        proc("System"),
        FilterRule::new(Operation, BeginsWith, "IRP_MJ_", Exclude),
        FilterRule::new(Operation, BeginsWith, "FASTIO_", Exclude),
        FilterRule::new(Operation, BeginsWith, "FAST IO", Exclude),
        FilterRule::new(Result, BeginsWith, "FAST IO", Exclude),
        ends("pagefile.sys"),
        ends("$Mft"),
        ends("$MftMirr"),
        ends("$LogFile"),
        ends("$Volume"),
        ends("$AttrDef"),
        ends("$Root"),
        ends("$Bitmap"),
        ends("$Boot"),
        ends("$BadClus"),
        ends("$Secure"),
        ends("$Upcase"),
        contains("$Extend"),
    ]
}

/// Whether Advanced Output is on (drives the Event menu's check state): true when
/// the default display filter is *not* fully present — i.e. the low-level view that
/// shows every event with raw `IRP_MJ_*`/`FASTIO_*` operation names. The normal,
/// friendly+filtered view (all default rules present) reads as off.
pub fn advanced_display_on(set: &FilterModel) -> bool {
    !default_display_filter().iter().all(|d| set.contains(d))
}

/// Enables (`on`) or disables Advanced Output. Advanced output removes the default
/// display filter (show every event with low-level names); disabling re-appends the
/// full default filter at the very end (after the user's own rules). Existing copies
/// are always stripped first.
pub fn set_advanced_display(set: &mut FilterModel, on: bool) {
    let defaults = default_display_filter();
    set.rules
        .retain(|r| !defaults.iter().any(|d| d.same_rule(r)));
    if !on {
        set.rules.extend(defaults);
    }
}
