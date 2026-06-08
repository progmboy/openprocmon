//! A parsed event: a lightweight, owned handle to one operation.
//!
//! Per Design B, a minifilter [`Event`] owns the few bytes of its PRE record (and
//! POST record, once correlated) in compacted `Box<[u8]>` buffers, so reads are
//! over a stable contiguous slice and the event has no lifetime tie to the receive
//! batch. Network events (from ETW) instead hold a small owned [`NetworkEvent`]
//! behind an `Arc`. Either way an `Event` is cheap to move across channels and is
//! `Send`. Path/detail for minifilter events are produced lazily through the
//! [`OperationView`] trait and dispatched statically by class.

use crate::kernel_types::{LogEntry, MonitorType, StackFrame};
use crate::network::NetworkEvent;
use crate::parse::OperationView;
use crate::process::{ProcessInfo, ProcessRecord};
use std::sync::Arc;

/// High-level category of an event. The single category type for both live
/// events and PML (its `event_class` u32 maps here via [`from_u32`](Self::from_u32)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EventClass {
    Process,
    File,
    Registry,
    Profiling,
    Network,
    #[default]
    Other,
}

impl EventClass {
    /// From the PML `event_class` u32 (1=Process, 2=Registry, 3=File,
    /// 4=Profiling, 5=Network; anything else `Other`).
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::Process,
            2 => Self::Registry,
            3 => Self::File,
            4 => Self::Profiling,
            5 => Self::Network,
            _ => Self::Other,
        }
    }

    /// The PML `event_class` u32 for this category.
    pub fn to_u32(self) -> u32 {
        match self {
            Self::Process => 1,
            Self::Registry => 2,
            Self::File => 3,
            Self::Profiling => 4,
            Self::Network => 5,
            Self::Other => 0,
        }
    }

    /// Display name of operation `code` within this class, reusing the canonical
    /// maps in [`crate::strings`] / [`crate::network::NetOp`] (one source of truth).
    /// File passes minor function 0 — a fallback; precise file names come from the
    /// `FileView`, which reads the minor function from the record.
    pub fn operation_name(self, code: u16) -> &'static str {
        match self {
            Self::Process => crate::strings::process_operation(code),
            Self::Registry => crate::strings::reg_operation(code),
            // Canonical (advanced) name with a minor-function fallback of 0.
            Self::File => crate::strings::file_operation(code, 0, false, true),
            Self::Network => crate::network::NetOp::from_pml(code).name(),
            Self::Profiling => crate::strings::profiling_operation(code),
            Self::Other => "<Unknown>",
        }
    }
}

/// Where an event's bytes come from. miniFilter events (process/file/registry,
/// live or PML) own their compacted kernel-record bytes; network events own a
/// small decoded ETW/PML structure. `mode` selects the detail-string encoding
/// (live driver vs PML re-serialization).
pub(crate) enum Backing {
    KernelRecord {
        pre: Box<[u8]>,
        post: Option<Box<[u8]>>,
        mode: crate::parse::DetailMode,
    },
    Network(Arc<NetworkEvent>),
}

/// Where an event's process information comes from: the live process table
/// (`Arc<ProcessRecord>`) or a PML capture's process table (reader + index).
pub(crate) enum ProcessSource {
    Live(Option<Arc<ProcessRecord>>),
    Pml(Arc<crate::pml::PmlReader>, u32 /* process_index */),
}

/// A single monitored operation.
pub struct Event {
    pub(crate) backing: Backing,
    pub(crate) proc: ProcessSource,
    /// Operation duration in 100-ns ticks when known directly (PML stores it as a
    /// field). Live events leave this `None` and derive duration from the
    /// correlated POST record instead.
    duration: Option<i64>,
}

// SAFETY: `Event` owns its bytes (`Box<[u8]>`) / `Arc<NetworkEvent>` and its
// process source (`Arc<ProcessRecord>` / `Arc<PmlReader>`); all fields are `Send`
// and there is no interior mutability.
unsafe impl Send for Event {}

impl Event {
    /// Builds a minifilter event from compacted PRE (and optional POST) bytes.
    /// Returns `None` if either buffer lacks a full record header.
    pub(crate) fn from_filter(
        pre: Box<[u8]>,
        post: Option<Box<[u8]>>,
        proc: Option<Arc<ProcessRecord>>,
    ) -> Option<Self> {
        LogEntry::view(&pre, 0)?;
        if let Some(p) = &post {
            LogEntry::view(p, 0)?;
        }
        Some(Event {
            backing: Backing::KernelRecord {
                pre,
                post,
                mode: crate::parse::DetailMode::Live,
            },
            proc: ProcessSource::Live(proc),
            duration: None,
        })
    }

    /// Builds an event whose detail bytes are in PML form (see [`crate::parse::DetailMode`]).
    /// `proc` carries the PML process source so process columns resolve via the
    /// reader; `duration` is the event's recorded duration in 100-ns ticks (`None`
    /// when absent), used directly instead of a synthetic POST.
    pub(crate) fn from_pml_with(
        pre: Box<[u8]>,
        post: Option<Box<[u8]>>,
        proc: ProcessSource,
        duration: Option<i64>,
    ) -> Option<Self> {
        LogEntry::view(&pre, 0)?;
        if let Some(p) = &post {
            LogEntry::view(p, 0)?;
        }
        Some(Event {
            backing: Backing::KernelRecord {
                pre,
                post,
                mode: crate::parse::DetailMode::Pml,
            },
            proc,
            duration,
        })
    }

    /// Builds a PML-form event with no attached process (detail-only parsing).
    /// Used by the PML detail decode path (round-trip / comparison tests).
    #[allow(dead_code)]
    pub(crate) fn from_pml(pre: Box<[u8]>, post: Option<Box<[u8]>>) -> Option<Self> {
        Self::from_pml_with(pre, post, ProcessSource::Live(None), None)
    }

    /// The detail-byte encoding of this event.
    pub(crate) fn mode(&self) -> crate::parse::DetailMode {
        match &self.backing {
            Backing::KernelRecord { mode, .. } => *mode,
            Backing::Network(_) => crate::parse::DetailMode::Live,
        }
    }

    /// Builds a network event from a decoded ETW/PML record.
    pub(crate) fn from_network(net: Arc<NetworkEvent>, proc: ProcessSource) -> Self {
        Event {
            backing: Backing::Network(net),
            proc,
            duration: None,
        }
    }

    /// The PRE record header for a minifilter event, else `None`.
    fn pre_entry(&self) -> Option<&LogEntry> {
        match &self.backing {
            // Validated at construction; `expect` documents the invariant.
            Backing::KernelRecord { pre, .. } => {
                Some(LogEntry::view(pre, 0).expect("PRE header validated at construction"))
            }
            Backing::Network(_) => None,
        }
    }

    /// The POST record header, if a completion has been correlated.
    fn post_entry(&self) -> Option<&LogEntry> {
        match &self.backing {
            Backing::KernelRecord { post: Some(p), .. } => {
                Some(LogEntry::view(p, 0).expect("POST header validated at construction"))
            }
            _ => None,
        }
    }

    /// The live process record, if this event's process source is live.
    fn live_record(&self) -> Option<&Arc<ProcessRecord>> {
        match &self.proc {
            ProcessSource::Live(p) => p.as_ref(),
            ProcessSource::Pml(..) => None,
        }
    }

    /// The PML process row, if this event's process source is a PML capture.
    fn pml_proc(&self) -> Option<&crate::pml::PmlProcess> {
        match &self.proc {
            ProcessSource::Pml(reader, idx) => reader.process(*idx),
            ProcessSource::Live(_) => None,
        }
    }

    /// The decoded network event, for network-backed events.
    pub fn network(&self) -> Option<&Arc<NetworkEvent>> {
        match &self.backing {
            Backing::Network(n) => Some(n),
            _ => None,
        }
    }

    /// Approximate retained size in bytes: the PRE + POST record buffers for a
    /// minifilter event, or the decoded struct size for a network event. Used by
    /// the GUI's history ring buffer to bound memory.
    pub fn byte_size(&self) -> usize {
        match &self.backing {
            Backing::KernelRecord { pre, post, .. } => {
                pre.len() + post.as_ref().map_or(0, |p| p.len())
            }
            Backing::Network(_) => std::mem::size_of::<NetworkEvent>(),
        }
    }

    /// Sequence id of the originating process (0 for network events).
    pub fn process_seq(&self) -> i32 {
        self.pre_entry().map(|e| e.process_seq).unwrap_or(0)
    }

    /// Originating thread id (0 for network events).
    pub fn thread_id(&self) -> u32 {
        self.pre_entry().map(|e| e.thread_id).unwrap_or(0)
    }

    /// Process id of the operation, from the attached process record, or — for a
    /// network event — from the ETW record.
    pub fn pid(&self) -> u32 {
        if let Backing::Network(n) = &self.backing {
            return n.pid;
        }
        match &self.proc {
            ProcessSource::Live(_) => self.live_record().map(|p| p.info.pid).unwrap_or(0),
            ProcessSource::Pml(..) => self.pml_proc().map(|p| p.pid).unwrap_or(0),
        }
    }

    /// Operation discriminant within the category (0 for network events).
    pub fn notify_type(&self) -> u16 {
        self.pre_entry().map(|e| e.notify()).unwrap_or(0)
    }

    /// PRE/POST correlation sequence number (0 for network events).
    pub fn sequence(&self) -> i32 {
        self.pre_entry().map(|e| e.sequence).unwrap_or(0)
    }

    /// High-level category of the event.
    pub fn class(&self) -> EventClass {
        match &self.backing {
            Backing::Network(_) => EventClass::Network,
            Backing::KernelRecord { .. } => match self.monitor_type() {
                MonitorType::Process => EventClass::Process,
                MonitorType::File => EventClass::File,
                MonitorType::Reg => EventClass::Registry,
                MonitorType::Profiling => EventClass::Profiling,
                _ => EventClass::Other,
            },
        }
    }

    /// Monitor type of a minifilter event; `Unknown` for network events.
    pub fn monitor_type(&self) -> MonitorType {
        self.pre_entry()
            .map(|e| e.monitor())
            .unwrap_or(MonitorType::Unknown(0))
    }

    /// Event time as a raw 100-nanosecond timestamp.
    pub fn time_raw(&self) -> i64 {
        match &self.backing {
            Backing::Network(n) => n.time,
            Backing::KernelRecord { .. } => self.pre_entry().map(|e| e.time).unwrap_or(0),
        }
    }

    /// Operation result as a raw `NTSTATUS` (0/SUCCESS for network events).
    pub fn status_raw(&self) -> i32 {
        match self.post_entry() {
            Some(post) => post.status,
            None => self.pre_entry().map(|e| e.status).unwrap_or(0),
        }
    }

    /// Whether a correlated POST record is attached.
    pub fn has_post(&self) -> bool {
        self.post_entry().is_some()
    }

    /// Completion time as a raw 100-nanosecond timestamp. For PML the duration is a
    /// stored field, so completion = start + duration; for live it is the POST time.
    pub fn end_time_raw(&self) -> Option<i64> {
        if let Some(d) = self.duration {
            return Some(self.time_raw().saturating_add(d));
        }
        self.post_entry().map(|e| e.time)
    }

    /// Operation duration in 100-nanosecond ticks (completion minus start), if a
    /// completion is attached.
    pub fn duration_ticks(&self) -> Option<i64> {
        self.end_time_raw().map(|end| end - self.time_raw())
    }

    /// Start time as local `HH:MM:SS.fffffff` (cf. `emTimeOfDay`).
    pub fn time_of_day(&self) -> String {
        crate::time::time_of_day(self.time_raw())
    }

    /// Start date/time as local `YYYY/MM/DD HH:MM:SS` (cf. `emDataTime`).
    pub fn date(&self) -> String {
        crate::time::date(self.time_raw())
    }

    /// Start date/time at full 100-ns precision (`YYYY/MM/DD HH:MM:SS.fffffff`),
    /// used as the comparison value for Date & Time filtering so "before/after this
    /// event" is exact rather than truncated to the second.
    pub fn date_precise(&self) -> String {
        crate::time::date_precise(self.time_raw())
    }

    /// Completion time as local `HH:MM:SS.fffffff`, if a completion is attached.
    pub fn completion_time(&self) -> Option<String> {
        self.end_time_raw().map(crate::time::time_of_day)
    }

    /// Operation duration as `S.fffffff` seconds, if a completion is attached.
    pub fn duration(&self) -> Option<String> {
        self.end_time_raw()
            .map(|end| crate::time::duration(self.time_raw(), end))
    }

    // --- Process-derived accessors (cf. C++ `CEventView`) -------------------
    // All return `None` when no process is attached (network or untracked).

    /// The originating process's info, if attached (live source only; PML process
    /// columns are resolved separately via the reader — see the PML accessors).
    fn info(&self) -> Option<&ProcessInfo> {
        self.live_record().map(|r| &r.info)
    }

    /// Parent process id (`emParentPid`).
    pub fn parent_pid(&self) -> Option<u32> {
        match &self.proc {
            ProcessSource::Live(_) => self.info().map(|i| i.parent_pid),
            ProcessSource::Pml(..) => self.pml_proc().map(|p| p.parent_pid),
        }
    }

    /// Session id (`emSession`).
    pub fn session_id(&self) -> Option<u32> {
        match &self.proc {
            ProcessSource::Live(_) => self.info().map(|i| i.session_id),
            ProcessSource::Pml(..) => self.pml_proc().map(|p| p.session),
        }
    }

    /// Whether the process is WoW64 (32-bit on 64-bit) (`emArchiteture`).
    pub fn is_wow64(&self) -> Option<bool> {
        match &self.proc {
            ProcessSource::Live(_) => self.info().map(|i| i.is_wow64),
            ProcessSource::Pml(..) => self.pml_proc().map(|p| !p.is_64bit),
        }
    }

    /// Whether token virtualization is enabled (`emVirtualize`).
    pub fn is_virtualized(&self) -> Option<bool> {
        match &self.proc {
            ProcessSource::Live(_) => self.info().map(|i| i.is_virtualized),
            ProcessSource::Pml(..) => self.pml_proc().map(|p| p.virtualized),
        }
    }

    /// Integrity level name, e.g. `Medium` (`emIntegrity`).
    pub fn integrity(&self) -> Option<&str> {
        match &self.proc {
            ProcessSource::Live(_) => self
                .info()
                .and_then(|i| i.integrity_rid)
                .map(crate::sid::integrity_level),
            ProcessSource::Pml(..) => self.pml_proc().map(|p| p.integrity.as_ref()),
        }
    }

    /// Logon session id as `HighPart:LowPart` (`emAuthId`).
    pub fn auth_id(&self) -> Option<String> {
        match &self.proc {
            ProcessSource::Live(_) => self
                .info()
                .map(|i| crate::sid::luid_string(i.authentication_id.0, i.authentication_id.1)),
            ProcessSource::Pml(..) => self.pml_proc().map(|p| {
                crate::sid::luid_string(
                    (p.authentication_id >> 32) as i32,
                    p.authentication_id as u32,
                )
            }),
        }
    }

    /// User account `DOMAIN\\User` (`emUser`).
    pub fn user(&self) -> Option<String> {
        match &self.proc {
            ProcessSource::Live(_) => self
                .info()
                .and_then(|i| i.user_sid.as_deref())
                .and_then(crate::sid::account_name),
            ProcessSource::Pml(..) => self.pml_proc().map(|p| p.user.to_string()),
        }
    }

    /// Process command line (`emCommandLine`).
    pub fn command_line(&self) -> Option<&str> {
        match &self.proc {
            ProcessSource::Live(_) => self.info().map(|i| i.command_line.as_str()),
            ProcessSource::Pml(..) => self.pml_proc().map(|p| p.command_line.as_ref()),
        }
    }

    /// DOS image path of the process (`emImagePath`).
    pub fn image_path(&self) -> Option<&str> {
        match &self.proc {
            ProcessSource::Live(_) => self.info().map(|i| i.image_path.as_str()),
            ProcessSource::Pml(..) => self.pml_proc().map(|p| p.image_path.as_ref()),
        }
    }

    /// Process image file name (basename) (`emProcessName`).
    pub fn process_name(&self) -> Option<&str> {
        match &self.proc {
            ProcessSource::Live(_) => self.info().map(|i| {
                let p = &i.image_path;
                p.rsplit(['\\', '/']).next().unwrap_or(p)
            }),
            ProcessSource::Pml(..) => self.pml_proc().map(|p| p.process_name.as_ref()),
        }
    }

    /// Image company name (`emCompany`); for live, `None` until the metadata
    /// worker fills it; for PML, from the capture's process table.
    pub fn company(&self) -> Option<&str> {
        match &self.proc {
            ProcessSource::Live(_) => self
                .live_record()
                .and_then(|r| r.meta())
                .and_then(|m| m.company.as_deref()),
            ProcessSource::Pml(..) => self.pml_proc().map(|p| p.company.as_ref()),
        }
    }

    /// Image description / product name (`emDescription`).
    pub fn description(&self) -> Option<&str> {
        match &self.proc {
            ProcessSource::Live(_) => self
                .live_record()
                .and_then(|r| r.meta())
                .and_then(|m| m.description.as_deref()),
            ProcessSource::Pml(..) => self.pml_proc().map(|p| p.description.as_ref()),
        }
    }

    /// Image version (`emVersion`).
    pub fn version(&self) -> Option<&str> {
        match &self.proc {
            ProcessSource::Live(_) => self
                .live_record()
                .and_then(|r| r.meta())
                .and_then(|m| m.version.as_deref()),
            ProcessSource::Pml(..) => self.pml_proc().map(|p| p.version.as_ref()),
        }
    }

    /// Raw small-icon bytes (`ICONIMAGE`), if present.
    pub fn icon_small(&self) -> Option<&[u8]> {
        match &self.proc {
            ProcessSource::Live(_) => self
                .live_record()
                .and_then(|r| r.meta())
                .and_then(|m| m.icon_small.as_deref()),
            ProcessSource::Pml(reader, _) => self
                .pml_proc()
                .and_then(|p| reader.icon(p.icon_small))
                .map(|i| i.data.as_ref()),
        }
    }

    /// Raw large-icon bytes (`ICONIMAGE`), if present.
    pub fn icon_large(&self) -> Option<&[u8]> {
        match &self.proc {
            ProcessSource::Live(_) => self
                .live_record()
                .and_then(|r| r.meta())
                .and_then(|m| m.icon_large.as_deref()),
            ProcessSource::Pml(reader, _) => self
                .pml_proc()
                .and_then(|p| reader.icon(p.icon_big))
                .map(|i| i.data.as_ref()),
        }
    }

    /// Whether the originating process has exited (`emInvalid`/process tree).
    pub fn process_exited(&self) -> bool {
        match &self.proc {
            ProcessSource::Live(_) => self.live_record().is_some_and(|r| r.is_exited()),
            ProcessSource::Pml(..) => self.pml_proc().is_some_and(|p| p.end_time != 0),
        }
    }

    /// The originating process's exit time in 100-ns ticks, if it has exited.
    pub fn process_exit_time(&self) -> Option<i64> {
        match &self.proc {
            ProcessSource::Live(_) => self.live_record().and_then(|r| r.exit_time()),
            ProcessSource::Pml(..) => self
                .pml_proc()
                .and_then(|p| (p.end_time != 0).then_some(p.end_time as i64)),
        }
    }

    /// The originating process's creation time in 100-ns ticks (0 if unknown).
    pub fn process_create_time(&self) -> i64 {
        match &self.proc {
            ProcessSource::Live(_) => self.live_record().map(|r| r.info.create_time).unwrap_or(0),
            ProcessSource::Pml(..) => self.pml_proc().map(|p| p.start_time as i64).unwrap_or(0),
        }
    }

    /// The loaded modules of this event's process (live record or PML table),
    /// shared via `Arc` — live records hand out their existing `Arc<Module>`s with
    /// no copy; PML builds them once from its table.
    pub fn modules(&self) -> Vec<Arc<crate::process::Module>> {
        match &self.proc {
            ProcessSource::Live(p) => p.as_ref().map(|r| r.modules()).unwrap_or_default(),
            ProcessSource::Pml(..) => self
                .pml_proc()
                .map(|p| {
                    p.modules
                        .iter()
                        .map(|m| {
                            Arc::new(crate::process::Module {
                                base: m.base_address,
                                size: m.size,
                                path: m.image_path.to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// The PRE record's operation-specific data (empty for network events).
    pub(crate) fn pre_data(&self) -> &[u8] {
        match self.pre_entry() {
            // SAFETY: `pre` holds exactly one full record (header + frames +
            // data), compacted by the parser, so the data region is in bounds.
            Some(e) => unsafe { e.event_data() },
            None => &[],
        }
    }

    /// The POST record's operation-specific data, if a completion is attached.
    pub(crate) fn post_data(&self) -> Option<&[u8]> {
        // SAFETY: as `pre_data`, `post` holds exactly one full record.
        self.post_entry().map(|e| unsafe { e.event_data() })
    }

    /// The process record associated with this event, if known.
    pub fn process(&self) -> Option<&Arc<ProcessRecord>> {
        self.live_record()
    }

    /// The raw call-stack frame addresses (empty for network events; symbol
    /// resolution is a GUI concern, matching the C++ SDK).
    pub fn call_stack(&self) -> &[StackFrame] {
        match self.pre_entry() {
            // SAFETY: `pre` holds the full record, so the frame chain is in bounds.
            Some(e) => unsafe { e.frame_chain() },
            None => &[],
        }
    }

    /// Reinterprets this event's PRE record data as `T`, the Rust equivalent of
    /// C++ `TO_EVENT_DATA(T, pEntry)`. `None` if the data is shorter than `T`.
    pub(crate) fn pre_as<T: Copy>(&self) -> Option<&T> {
        crate::kernel_types::cast::<T>(self.pre_data())
    }

    /// Like [`pre_as`](Self::pre_as) but for the correlated POST record's data.
    pub(crate) fn post_as<T: Copy>(&self) -> Option<&T> {
        self.post_data().and_then(crate::kernel_types::cast::<T>)
    }

    /// The operation's canonical display name (cf. C++ `StrMapOperation`, advanced).
    /// For file events the IRP minor function refines it (e.g.
    /// `QueryStandardInformationFile`). This is the stable name used for filtering,
    /// search and export — independent of the GUI's display toggle; the toggle-aware
    /// variant is [`operation_name_advanced`](Self::operation_name_advanced).
    pub fn operation_name(&self) -> &'static str {
        crate::strings::operation(self, true)
    }

    /// Operation name honoring the "Advanced Display" toggle (C++ `bAdvance`): the
    /// friendly detail name when `advance`, otherwise the raw `IRP_MJ_*` name (or the
    /// `FASTIO_*` name when the file record's fast-I/O flag is set).
    pub fn operation_name_advanced(&self, advance: bool) -> &'static str {
        crate::strings::operation(self, advance)
    }

    /// The event category's display name.
    pub fn class_name(&self) -> &'static str {
        match &self.backing {
            Backing::Network(_) => "Network",
            Backing::KernelRecord { .. } => crate::strings::class_event(self.monitor_type()),
        }
    }

    /// The operation result as a Procmon-style string (its `NTSTATUS` name, or
    /// `0x%X` hex if unknown). Network events report `SUCCESS`.
    pub fn result(&self) -> std::borrow::Cow<'static, str> {
        if matches!(self.backing, Backing::Network(_)) {
            return std::borrow::Cow::Borrowed("SUCCESS");
        }
        crate::strings::nt_status_string(self.status_raw())
    }

    /// The operation's target, ready for display: file/process paths are DOS
    /// paths, registry paths are in hive form (`HKLM\...`), and network events are
    /// `local -> remote`. `None` if the operation carries no path.
    pub fn path(&self) -> Option<String> {
        match &self.backing {
            Backing::Network(n) => crate::parse::network::NetView::new(n).path(),
            Backing::KernelRecord { .. } => match self.class() {
                EventClass::File => crate::parse::file::FileView::new(self).path(),
                EventClass::Registry => crate::parse::reg::RegView::new(self).path(),
                EventClass::Process => crate::parse::proc::ProcView::new(self).path(),
                _ => None,
            },
        }
    }

    /// The operation-specific detail string. Empty when there is none to show.
    pub fn detail(&self) -> String {
        match &self.backing {
            Backing::Network(n) => crate::parse::network::NetView::new(n).detail(),
            Backing::KernelRecord { .. } => match self.class() {
                EventClass::File => crate::parse::file::FileView::new(self).detail(),
                EventClass::Registry => crate::parse::reg::RegView::new(self).detail(),
                EventClass::Process => crate::parse::proc::ProcView::new(self).detail(),
                _ => String::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel_types::test_support::entry_bytes;

    #[test]
    fn reads_header_fields() {
        let pre = entry_bytes(3, 25, 42, 0, &[]);
        let ev = Event::from_filter(pre.into_boxed_slice(), None, None).unwrap();
        assert_eq!(ev.class(), EventClass::File);
        assert_eq!(ev.notify_type(), 25);
        assert_eq!(ev.sequence(), 42);
    }

    #[test]
    fn post_status_overrides_pre() {
        let pre = entry_bytes(3, 25, 7, 0, &[]);
        let post = entry_bytes(0, 25, 7, -1073741772, &[]);
        let ev = Event::from_filter(pre.into_boxed_slice(), Some(post.into_boxed_slice()), None)
            .unwrap();
        assert!(ev.has_post());
        assert_eq!(ev.status_raw(), -1073741772);
    }

    #[test]
    fn facade_names() {
        let pre = entry_bytes(3, 20, 1, 0, &[]); // File, CreateFile, SUCCESS
        let ev = Event::from_filter(pre.into_boxed_slice(), None, None).unwrap();
        assert_eq!(ev.operation_name(), "CreateFile");
        assert_eq!(ev.class_name(), "File System");
        assert_eq!(ev.result(), "SUCCESS");
    }

    #[test]
    fn network_event_facade() {
        use crate::network::{NetOp, NetworkEvent};
        let net = NetworkEvent {
            pid: 42,
            is_tcp: true,
            op: NetOp::Connect,
            local: "10.0.0.1:5000".parse().unwrap(),
            remote: "1.2.3.4:443".parse().unwrap(),
            local_name: None,
            remote_name: None,
            length: 0,
            time: 0,
        };
        let ev = Event::from_network(Arc::new(net), crate::event::ProcessSource::Live(None));
        assert_eq!(ev.class(), EventClass::Network);
        assert_eq!(ev.operation_name(), "TCP Connect");
        assert_eq!(ev.pid(), 42);
        assert_eq!(ev.path().as_deref(), Some("10.0.0.1:5000 -> 1.2.3.4:443"));
        assert_eq!(ev.result(), "SUCCESS");
    }
}
