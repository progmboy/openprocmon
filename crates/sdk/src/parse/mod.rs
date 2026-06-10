//! Turning raw record batches into [`Event`]s.
//!
//! The flow mirrors the C++ `CEventMgr`: walk a batch record by record, route
//! POST records to the pending PRE they complete (correlated by `Sequence`), feed
//! process lifecycle records into the [`ProcessManager`], and emit finished
//! events. The batch buffer is `Arc`-shared: every emitted event holds a
//! [`Record`] (buffer + offset) into it, so ingestion performs no per-record
//! copies — a batch stays alive exactly as long as some event references it.
//!
//! Path/detail formatting is organized with the [`OperationView`] trait, which
//! `file`/`reg`/`proc` implement and [`Event`] dispatches to statically.

pub mod file;
pub(crate) mod network;
pub mod proc;
pub mod reg;
pub(crate) mod transcode;

use crate::event::{Event, Record};
use crate::kernel_types::{MonitorType, STATUS_PENDING};
use crate::message::entry_offsets;
use crate::process::{ProcessManager, ProcessRecord};
use rustc_hash::FxHashMap;
use std::ops::Range;
use std::sync::Arc;

/// Produces an operation's path and detail strings from its event bytes
/// (cf. the virtual `CLogEvent::GetPath` / `GetDetail`). Implemented by the
/// per-class views in `file`/`reg`/`proc`.
pub(crate) trait OperationView {
    /// Target path/key/image, or `None` if the operation carries none.
    fn path(&self) -> Option<String>;
    /// Operation-specific detail, or empty if there is none to show.
    fn detail(&self) -> String;
}

/// Where an event's detail bytes came from. The two forms differ ONLY in string
/// encoding: the live driver wire form is plain UTF-16 with a plain unit-count
/// length; Procmon's PML re-serializes strings as `bit15 = ASCII flag, low15 =
/// char count`, storing ASCII strings 1 byte/char. All other fields (structs,
/// op-codes, `FLT_PARAMETERS`) are identical, so the per-operation views are
/// shared and only branch at string reads via [`str_field_len`]/[`read_detail_str`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DetailMode {
    Live,
    Pml,
}

/// Interprets a raw string-length field for `mode`: `(char_count, byte_len)`.
/// For [`DetailMode::Live`] this is exactly `(raw, raw * 2)` (unchanged behavior).
pub(crate) fn str_field_len(raw: u16, mode: DetailMode) -> (usize, usize) {
    match mode {
        DetailMode::Live => (raw as usize, raw as usize * 2),
        DetailMode::Pml => {
            let count = (raw & 0x7fff) as usize;
            let bytes = if raw >> 15 == 1 { count } else { count * 2 };
            (count, bytes)
        }
    }
}

/// Reads a detail string at `data[off..]` whose length is the raw field `raw`,
/// returning the decoded string and the byte length it occupied (so callers can
/// advance past it — PML ASCII strings are half the bytes of UTF-16).
pub(crate) fn read_detail_str(
    data: &[u8],
    off: usize,
    raw: u16,
    mode: DetailMode,
) -> (String, usize) {
    let (count, bytes) = str_field_len(raw, mode);
    let s = match data.get(off..off + bytes) {
        Some(b) if mode == DetailMode::Pml && raw >> 15 == 1 => {
            let mut out = String::with_capacity(count);
            out.extend(b.iter().map(|&c| c as char));
            out
        }
        Some(b) => decode_utf16(b),
        None => String::new(),
    };
    (s, bytes)
}

/// Serializes a live [`Event`] into its PML `(operation_code, detail_blob)`,
/// dispatching to the per-category serializer (`parse::{file,reg,proc,network}::
/// pml_detail`) — the write-side mirror of the per-category [`OperationView`]s.
/// The operation code is the event's notify type, except network, which uses the
/// PML `NetworkOperation` code. `detail` is `None` for categories with no blob.
pub(crate) fn pml_serialize(ev: &Event) -> (u16, Option<Vec<u8>>) {
    use crate::event::EventClass;
    match ev.class() {
        EventClass::File => (ev.notify_type(), file::pml_detail(ev)),
        EventClass::Registry => (ev.notify_type(), reg::pml_detail(ev)),
        EventClass::Process => (ev.notify_type(), proc::pml_detail(ev)),
        EventClass::Network => {
            let op = ev.network().map(|n| n.op.to_pml()).unwrap_or(0);
            (op, network::pml_detail(ev))
        }
        _ => (ev.notify_type(), None),
    }
}

/// Decodes a little-endian UTF-16 byte slice into a `String`, stopping at the
/// first NUL if present. Lossy on unpaired surrogates. Decodes by streaming the
/// units — one allocation (the `String`), no intermediate `Vec<u16>`; the
/// capacity covers ASCII-only content exactly (1 UTF-8 byte per unit).
pub(crate) fn decode_utf16(bytes: &[u8]) -> String {
    let units = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0);
    let mut out = String::with_capacity(bytes.len() / 2);
    for c in char::decode_utf16(units) {
        out.push(c.unwrap_or(char::REPLACEMENT_CHARACTER));
    }
    out
}

/// Correlates pending PRE records with their POST completions across one or more
/// batches. The pipeline keeps a single long-lived instance (a POST may arrive in
/// a later batch); offline callers ingest one batch and then [`flush`](Self::flush).
pub struct Correlator {
    /// PRE records awaiting completion, keyed by `Sequence`. A pending record
    /// keeps its batch buffer alive until the POST arrives or `flush` runs.
    /// FxHashMap: one insert + remove per asynchronous operation.
    pending: FxHashMap<i32, Record>,
    /// Single-entry cache for the per-event process lookup: events arrive in
    /// bursts from one process, so consecutive events usually share a record —
    /// a hit skips the process table's lock + hash entirely. Only positive
    /// lookups are cached: a process tracked *after* its first events
    /// (startup INIT races) must not be masked by a stale miss, and a seq's
    /// record never changes once inserted, so a hit cannot go stale.
    last_proc: Option<(i32, Arc<ProcessRecord>)>,
}

impl Correlator {
    pub fn new() -> Self {
        Self {
            pending: FxHashMap::default(),
            last_proc: None,
        }
    }

    /// Parses every record in `batch`, updating `mgr` and appending finished
    /// events to `out`. Copies the batch into a shared buffer once; the live
    /// pipeline avoids even that by calling [`ingest_shared`](Self::ingest_shared)
    /// with the receive buffer itself.
    pub fn ingest(&mut self, batch: &[u8], mgr: &ProcessManager, out: &mut Vec<Event>) {
        let buf: Arc<[u8]> = Arc::from(batch);
        let range = 0..buf.len();
        self.ingest_shared(&buf, range, mgr, out);
    }

    /// Parses every record in `buf[range]` without copying: each emitted event
    /// holds a [`Record`] referencing `buf`. Records that fail validation
    /// (truncated/corrupt tail) are skipped.
    pub fn ingest_shared(
        &mut self,
        buf: &Arc<[u8]>,
        range: Range<usize>,
        mgr: &ProcessManager,
        out: &mut Vec<Event>,
    ) {
        let batch = match buf.get(range.clone()) {
            Some(b) => b,
            None => return,
        };
        for off in entry_offsets(batch) {
            let Some(record) = Record::new(Arc::clone(buf), range.start + off) else {
                continue; // truncated or corrupt tail
            };
            let entry = record.entry();

            // Copy packed header fields to locals (no references into packed data).
            let sequence = entry.sequence;
            let status = entry.status;
            let monitor = entry.monitor();

            // Only process/file/registry (and their POST completions) are
            // surfaced; profiling and any unmodeled categories are dropped, as in
            // C++ `CEventMgr::ProcessEntry`.
            match monitor {
                MonitorType::Post => {
                    // Completion: pair it with the PRE it finishes, if we have it.
                    if let Some(pre) = self.pending.remove(&sequence) {
                        self.emit(pre, Some(record), mgr, out);
                    }
                    continue;
                }
                MonitorType::Process | MonitorType::File | MonitorType::Reg => {}
                MonitorType::Profiling | MonitorType::Unknown(_) => continue,
            }

            // PRE record: process lifecycle records update the process table.
            if monitor == MonitorType::Process {
                proc::track(mgr, record.entry(), record.data());
            }

            if status == STATUS_PENDING {
                // Asynchronous: hold it until the matching POST arrives.
                self.pending.insert(sequence, record);
            } else {
                self.emit(record, None, mgr, out);
            }
        }
    }

    /// Emits any PRE records still awaiting a completion as PRE-only events. Use
    /// after the final batch (or in single-batch offline parsing) so nothing is
    /// silently dropped.
    pub fn flush(&mut self, mgr: &ProcessManager, out: &mut Vec<Event>) {
        for (_, pre) in std::mem::take(&mut self.pending) {
            self.emit(pre, None, mgr, out);
        }
    }

    /// Builds an event from PRE (+ optional POST) records, attaching the
    /// originating process snapshot (via the single-entry cache), and appends
    /// it to `out`.
    fn emit(
        &mut self,
        pre: Record,
        post: Option<Record>,
        mgr: &ProcessManager,
        out: &mut Vec<Event>,
    ) {
        let seq = pre.entry().process_seq;
        let proc = match &self.last_proc {
            Some((cached_seq, rec)) if *cached_seq == seq => Some(Arc::clone(rec)),
            _ => {
                let found = u32::try_from(seq).ok().and_then(|s| mgr.by_seq(s));
                if let Some(rec) = &found {
                    self.last_proc = Some((seq, Arc::clone(rec)));
                }
                found
            }
        };
        out.push(Event::from_records(pre, post, proc));
    }
}

impl Default for Correlator {
    fn default() -> Self {
        Self::new()
    }
}

/// Parses a single batch into events using a throwaway process table. Intended
/// for offline use (tests, recorded fixtures); process snapshots are available
/// only for processes created within the same batch.
pub fn parse_block(batch: &[u8]) -> Vec<Event> {
    let mgr = ProcessManager::new();
    parse_block_tracked(batch, &mgr)
}

/// Parses a single batch into events, updating the caller's persistent process
/// table. Pending PRE records with no completion in this batch are flushed as
/// PRE-only events (single-batch semantics).
pub fn parse_block_tracked(batch: &[u8], mgr: &ProcessManager) -> Vec<Event> {
    let mut correlator = Correlator::new();
    let mut out = Vec::new();
    correlator.ingest(batch, mgr, &mut out);
    correlator.flush(mgr, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel_types::test_support::entry_bytes;

    #[test]
    fn correlates_pre_and_post() {
        // A pending PRE (File CreateFile) followed by its POST completion.
        let pre = entry_bytes(3, 20, 99, STATUS_PENDING, &[]);
        let post = entry_bytes(0, 20, 99, 0, &[]);
        let mut batch = pre;
        batch.extend_from_slice(&post);
        let events = parse_block(&batch);
        assert_eq!(events.len(), 1);
        assert!(events[0].has_post());
        assert_eq!(events[0].status_raw(), 0);
    }

    #[test]
    fn synchronous_pre_emits_immediately() {
        let pre = entry_bytes(3, 20, 1, 0, &[]); // status SUCCESS, no POST expected
        let events = parse_block(&pre);
        assert_eq!(events.len(), 1);
        assert!(!events[0].has_post());
    }

    #[test]
    fn unmatched_pending_is_flushed() {
        let pre = entry_bytes(3, 20, 7, STATUS_PENDING, &[]); // no POST in batch
        let events = parse_block(&pre);
        assert_eq!(events.len(), 1);
        assert!(!events[0].has_post());
    }

    // --- Realistic record assembly: validates field offsets fully offline. ---

    use crate::kernel_types::{irp_mj, proc_notify, LogFileOptHead, FILE_NOTIFY_BASE};
    use core::mem::size_of;
    use windows::Wdk::Storage::FileSystem::Minifilters::FLT_PARAMETERS;

    fn utf16(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    /// Assembles a `LOG_FILE_OPT` create record data region using the real
    /// `FLT_PARAMETERS` layout, exercising the `size_of`-derived name offset.
    fn file_create_data(name: &str, desired: u32, options: u32, attrs: u16, share: u16) -> Vec<u8> {
        let mut d = vec![0u8; size_of::<LogFileOptHead>()];
        // SAFETY: FLT_PARAMETERS is POD for our purposes; zeroed is valid.
        let mut params: FLT_PARAMETERS = unsafe { core::mem::zeroed() };
        // Writing union fields is safe (only reads are unsafe).
        params.Create.Options = options;
        params.Create.FileAttributes = attrs;
        params.Create.ShareAccess = share;
        params.Create.AllocationSize = 0;
        // SAFETY: read the union's bytes for serialization.
        let pb = unsafe {
            core::slice::from_raw_parts(
                &params as *const _ as *const u8,
                size_of::<FLT_PARAMETERS>(),
            )
        };
        d.extend_from_slice(pb);
        let name16 = utf16(name);
        d.extend(((name16.len() / 2) as u16).to_le_bytes()); // NameLength (units)
        d.extend(0u16.to_le_bytes()); // Fill42
        d.extend_from_slice(&name16);
        d.extend(desired.to_le_bytes()); // LOG_FILE_CREATE.DesiredAccess
        d.extend(0u32.to_le_bytes()); // UserTokenLength
        d
    }

    #[test]
    fn file_create_path_and_detail() {
        use windows::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_SHARE_READ,
        };
        let name = "\\Device\\HarddiskVolume999\\Windows\\test.txt";
        // Disposition byte (high 8 bits) = 1 => "Open".
        let options = 1u32 << 24;
        let data = file_create_data(
            name,
            FILE_GENERIC_READ.0,
            options,
            FILE_ATTRIBUTE_NORMAL.0 as u16,
            FILE_SHARE_READ.0 as u16,
        );
        let pre = entry_bytes(
            3,
            FILE_NOTIFY_BASE + irp_mj::CREATE as u16,
            1,
            STATUS_PENDING,
            &data,
        );
        // POST carries IO_STATUS.Information (ret disposition) = 1 => "Opened".
        let post = entry_bytes(
            0,
            FILE_NOTIFY_BASE + irp_mj::CREATE as u16,
            1,
            0,
            &1u64.to_le_bytes(),
        );
        let mut batch = pre;
        batch.extend_from_slice(&post);

        let events = parse_block(&batch);
        assert_eq!(events.len(), 1);
        let ev = &events[0];
        assert_eq!(ev.operation_name(), "CreateFile");
        assert_eq!(ev.path().as_deref(), Some(name));
        let detail = ev.detail();
        assert!(detail.contains("Disposition: Open"), "detail: {detail}");
        assert!(detail.contains("ShareMode: Read"), "detail: {detail}");
        assert!(detail.contains("OpenResult: Opened"), "detail: {detail}");
    }

    fn proc_create_data(seq: u32, pid: u32, image: &str, cmdline: &str) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend(seq.to_le_bytes());
        d.extend(pid.to_le_bytes());
        d.extend(1u32.to_le_bytes()); // parent_proc_seq
        d.extend(4u32.to_le_bytes()); // parent_id
        d.extend(1u32.to_le_bytes()); // session_id
        d.extend(0u32.to_le_bytes()); // is_wow64
        d.extend(0i64.to_le_bytes()); // create_time
        d.extend(0u32.to_le_bytes()); // luid low
        d.extend(0i32.to_le_bytes()); // luid high
        d.extend(0u32.to_le_bytes()); // token_virtualization_enabled
        d.push(0u8); // sid_length
        d.push(0u8); // integrity_level_sid_length
        let img = utf16(image);
        let cmd = utf16(cmdline);
        d.extend(((img.len() / 2) as u16).to_le_bytes()); // proc_name_length
        d.extend(((cmd.len() / 2) as u16).to_le_bytes()); // command_line_length
        d.extend(0u16.to_le_bytes()); // unknown1
        assert_eq!(d.len(), size_of::<crate::kernel_types::LogProcessCreate>());
        d.extend_from_slice(&img);
        d.extend_from_slice(&cmd);
        d
    }

    #[test]
    fn process_create_path_detail_and_tracking() {
        let image = "\\Device\\HarddiskVolume999\\Windows\\notepad.exe";
        let data = proc_create_data(5, 1234, image, "notepad.exe foo.txt");
        let mut pre = entry_bytes(1, proc_notify::CREATE, 5, 0, &data);
        // The header's ProcessSeq (offset 0) links the event to its process
        // record; set it to match the created process's sequence (5).
        pre[0..4].copy_from_slice(&5i32.to_le_bytes());

        let mgr = ProcessManager::new();
        let events = parse_block_tracked(&pre, &mgr);
        assert_eq!(events.len(), 1);
        let ev = &events[0];
        assert_eq!(ev.operation_name(), "Process Create");
        assert_eq!(ev.path().as_deref(), Some(image));
        let detail = ev.detail();
        assert!(detail.contains("PID: 1234"), "detail: {detail}");
        assert!(detail.contains("notepad.exe foo.txt"), "detail: {detail}");

        // The process table now knows process seq 5 / pid 1234.
        assert!(mgr.by_seq(5).is_some());
        assert_eq!(mgr.by_pid(1234).unwrap().info.image_path, image);
        // And the event carries the process snapshot.
        assert_eq!(ev.process().unwrap().info.pid, 1234);
    }

    #[test]
    fn reg_set_value_detail_decodes_data() {
        use crate::kernel_types::reg_notify;
        use windows::Win32::System::Registry::REG_DWORD;

        // LOG_REG_SETVALUEKEY (16 bytes) + key name + 4-byte DWORD value (= 1).
        let name = utf16("HKLM\\X");
        let units = (name.len() / 2) as u16;
        let mut d = Vec::new();
        d.extend(units.to_le_bytes()); // key_name_length
        d.extend(0u16.to_le_bytes()); // fill02
        d.extend(REG_DWORD.0.to_le_bytes()); // value_type
        d.extend(4u32.to_le_bytes()); // data_size
        d.extend(4u16.to_le_bytes()); // copy_size
        d.extend(0u16.to_le_bytes()); // fill0e
        d.extend_from_slice(&name);
        d.extend(1u32.to_le_bytes()); // the DWORD value

        let pre = entry_bytes(2, reg_notify::SETVALUEKEY, 1, 0, &d);
        let events = parse_block(&pre);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].operation_name(), "RegSetValue");
        assert_eq!(events[0].detail(), "Type: REG_DWORD, Length: 4, Data: 1");
    }

    #[test]
    fn process_exit_thread_create_and_start_details() {
        use crate::kernel_types::proc_notify;

        // Thread Create: a bare ULONG thread id.
        let pre = entry_bytes(1, proc_notify::THREAD_CREATE, 1, 0, &1234u32.to_le_bytes());
        let ev = &parse_block(&pre)[0];
        assert_eq!(ev.operation_name(), "Thread Create");
        assert_eq!(ev.detail(), "Thread ID: 1234");

        // Process Exit: LOG_PROCESSBASIC_INFO (exit status + times + memory).
        let mut d = Vec::new();
        d.extend(0u32.to_le_bytes()); // exit_status = SUCCESS
        d.extend(0i64.to_le_bytes()); // kernel_time
        d.extend(10_000_000i64.to_le_bytes()); // user_time = 1.0s (100ns ticks)
        d.extend(4096u64.to_le_bytes()); // working_set_size
        d.extend(0u64.to_le_bytes()); // peak_working_set_size
        d.extend(8192u64.to_le_bytes()); // pagefile_usage
        d.extend(0u64.to_le_bytes()); // peak_pagefile_usage
        let pre = entry_bytes(1, proc_notify::EXIT, 1, 0, &d);
        let ev = &parse_block(&pre)[0];
        assert_eq!(ev.operation_name(), "Process Exit");
        assert_eq!(
            ev.detail(),
            "Exit Status: SUCCESS, User Time: 1.0000000, Kernel Time: 0.0000000, \
             Private Bytes: 8192, Working Set: 4096"
        );

        // Process Start: LOG_PROCESSSTART_INFO + command line + current directory.
        let cmd = utf16("a.exe -x");
        let cwd = utf16("C:\\Dir");
        let mut d = Vec::new();
        d.extend(100u32.to_le_bytes()); // parent_id
        d.extend(((cmd.len() / 2) as u16).to_le_bytes()); // command_line_length
        d.extend(((cwd.len() / 2) as u16).to_le_bytes()); // current_directory_length
        d.extend(0u32.to_le_bytes()); // environment_length
        d.extend_from_slice(&cmd);
        d.extend_from_slice(&cwd);
        let pre = entry_bytes(1, proc_notify::START, 1, 0, &d);
        let ev = &parse_block(&pre)[0];
        assert_eq!(ev.operation_name(), "Process Start");
        assert_eq!(
            ev.detail(),
            "Parent PID: 100, Command line: a.exe -x, Current directory: C:\\Dir"
        );
    }
}
