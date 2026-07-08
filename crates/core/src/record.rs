//! Serde projections of SDK events/processes for JSON output.
//!
//! The SDK's `Event`/`PmlProcess` are not serde-derivable (and we keep the SDK
//! serde-free), so this module mirrors the fields we expose as plain, owned,
//! `Serialize` types. Everything here is built on demand from a live or PML
//! `procmon_sdk::Event` / `PmlProcess` — there is no stored projection.

use procmon_sdk::{Event, EventClass, PmlProcess};
use serde::Serialize;

/// Event category, a serde mirror of [`procmon_sdk::EventClass`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Process,
    File,
    Registry,
    Profiling,
    Network,
    Other,
}

impl From<EventClass> for Category {
    fn from(c: EventClass) -> Self {
        match c {
            EventClass::Process => Category::Process,
            EventClass::File => Category::File,
            EventClass::Registry => Category::Registry,
            EventClass::Profiling => Category::Profiling,
            EventClass::Network => Category::Network,
            EventClass::Other => Category::Other,
        }
    }
}

/// A single event projected for JSON output. `seq` is the event's index in its
/// source (PML event index), the stable id [`crate::analyze::get_event`] takes.
/// `detail` is populated only when explicitly requested (it forces a parse).
#[derive(Clone, Debug, Serialize)]
pub struct EventRecord {
    pub seq: u64,
    pub pid: u32,
    pub parent_pid: u32,
    pub tid: u32,
    pub category: Category,
    pub operation: String,
    pub process_name: String,
    pub path: String,
    pub result: String,
    pub time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl EventRecord {
    /// Projects `ev` (index `seq`); `with_detail` forces the (expensive) detail
    /// string. Other columns are cheap accessors.
    pub fn from_event(ev: &Event, seq: u64, with_detail: bool) -> Self {
        EventRecord {
            seq,
            pid: ev.pid(),
            parent_pid: ev.parent_pid().unwrap_or(0),
            tid: ev.thread_id(),
            category: ev.class().into(),
            operation: ev.operation_name().to_string(),
            process_name: ev.process_name().unwrap_or("").to_string(),
            path: ev.path().unwrap_or_default(),
            result: ev.result().into_owned(),
            time: ev.time_of_day(),
            duration: ev.duration(),
            detail: with_detail.then(|| ev.detail()),
        }
    }
}

/// A loaded module, for process detail.
#[derive(Clone, Debug, Serialize)]
pub struct ModuleRow {
    pub name: String,
    pub path: String,
    pub base: u64,
    pub size: u64,
}

/// One call-stack frame, resolved to its module + offset where possible.
#[derive(Clone, Debug, Serialize)]
pub struct StackFrameRow {
    pub frame: u32,
    /// `"K"` kernel / `"U"` user, inferred from the high address bits.
    pub kind: &'static str,
    pub module: String,
    /// `"module+0x1234"` or the raw address when outside any known module.
    pub location: String,
    pub address: String,
    pub path: String,
}

/// Full process detail (the "Process" view): identity, token info, and modules.
#[derive(Clone, Debug, Serialize)]
pub struct ProcessDetail {
    pub pid: u32,
    pub parent_pid: u32,
    pub name: String,
    pub image_path: String,
    pub command_line: String,
    pub user: String,
    pub integrity: String,
    pub session: u32,
    pub arch: &'static str,
    pub virtualized: bool,
    pub company: String,
    pub version: String,
    pub description: String,
    pub running: bool,
    pub modules: Vec<ModuleRow>,
}

impl ProcessDetail {
    /// Builds full detail from a PML process record.
    pub fn from_pml(p: &PmlProcess) -> Self {
        ProcessDetail {
            pid: p.pid,
            parent_pid: p.parent_pid,
            name: p.process_name.to_string(),
            image_path: p.image_path.to_string(),
            command_line: p.command_line.to_string(),
            user: p.user.to_string(),
            integrity: p.integrity.to_string(),
            session: p.session,
            arch: if p.is_64bit { "64-bit" } else { "32-bit" },
            virtualized: p.virtualized,
            company: p.company.to_string(),
            version: p.version.to_string(),
            description: p.description.to_string(),
            running: p.end_time == 0,
            modules: p
                .modules
                .iter()
                .map(|m| ModuleRow {
                    name: procmon_sdk::basename(&m.image_path).to_string(),
                    path: m.image_path.to_string(),
                    base: m.base_address,
                    size: m.size as u64,
                })
                .collect(),
        }
    }
}

/// A process node for the flat list / tree (lighter than [`ProcessDetail`]).
#[derive(Clone, Debug, Serialize)]
pub struct ProcessNode {
    pub pid: u32,
    pub parent_pid: u32,
    pub name: String,
    pub image_path: String,
    pub command_line: String,
    pub user: String,
    pub integrity: String,
    pub running: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ProcessNode>,
}

impl ProcessNode {
    pub fn from_pml(p: &PmlProcess) -> Self {
        ProcessNode {
            pid: p.pid,
            parent_pid: p.parent_pid,
            name: p.process_name.to_string(),
            image_path: p.image_path.to_string(),
            command_line: p.command_line.to_string(),
            user: p.user.to_string(),
            integrity: p.integrity.to_string(),
            running: p.end_time == 0,
            children: Vec::new(),
        }
    }
}
