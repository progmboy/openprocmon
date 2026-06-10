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
/// unified `procmon_sdk::Event` (live or PML), which supports every column, so we
/// delegate straight to it.
impl FilterFields for CapturedEvent {
    fn filter_field(&self, column: Column) -> Option<std::borrow::Cow<'_, str>> {
        self.event().filter_field(column)
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

/// The default noise-suppression rules toggled by the Event menu's "Advanced
/// Display" item: excludes the monitoring tools themselves and the System process,
/// the IRP/FastIO bookkeeping operations, and NTFS metadata files. Always appended
/// at the end of the set so they evaluate after any user rules.
pub fn advanced_display_rules() -> Vec<FilterRule> {
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

/// Whether the advanced-display default rules are all present (drives the Event
/// menu's check state — clearing the filter or editing it away auto-unchecks).
pub fn advanced_display_on(set: &FilterModel) -> bool {
    advanced_display_rules().iter().all(|d| set.contains(d))
}

/// Adds (`on`) or removes (`!on`) the advanced-display default rules. Any existing
/// copies are stripped first, so enabling always re-appends the full set at the
/// very end (after the user's own rules).
pub fn set_advanced_display(set: &mut FilterModel, on: bool) {
    let defaults = advanced_display_rules();
    set.rules
        .retain(|r| !defaults.iter().any(|d| d.same_rule(r)));
    if on {
        set.rules.extend(defaults);
    }
}
