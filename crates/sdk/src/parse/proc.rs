//! Process operation path/detail and process-table tracking
//! (cf. C++ `procopt.cxx` + `viewer.cxx`).
//!
//! A process-create record's data is [`LogProcessCreate`](crate::kernel_types::LogProcessCreate)
//! followed by, in order: the user SID, the integrity-level SID, the image path
//! (UTF-16), and the command line (UTF-16). String lengths are UTF-16 unit
//! counts, matching the C++ `CString::Append(.., Length)` usage.

use crate::event::Event;
use crate::kernel_types::proc_notify as pn;
use crate::kernel_types::{
    cast, LogEntry, LogLoadImage, LogProcessBasic, LogProcessCreate, LogProcessStart, LogThreadExit,
};
use crate::parse::{read_detail_str, str_field_len, DetailMode, OperationView};
use crate::process::{Module, ProcessInfo, ProcessManager, ProcessRecord};
use crate::strings;
use core::mem::size_of;
use std::sync::Arc;

/// Applies a process lifecycle record to the process table. `metadata` (live
/// capture only) resolves the new process's image metadata (version + icon) as
/// it is inserted — like Procmon, which resolves the process described by the
/// CREATE/INIT record (the *new* process; a CREATE event is attributed to the
/// parent, so the child would otherwise never be resolved).
pub(crate) fn track(
    mgr: &ProcessManager,
    metadata: Option<&crate::metadata::MetadataCache>,
    entry: &LogEntry,
    data: &[u8],
) {
    match entry.notify() {
        pn::INIT | pn::CREATE => {
            if let Some(info) = create_info(data, DetailMode::Live) {
                let pid = info.pid;
                let rec = ProcessRecord::new(info);
                if let Some(metadata) = metadata {
                    // Off-thread: a cache hit fills the record now; a first
                    // sighting is queued to the metadata worker so this (parse)
                    // thread never blocks on image I/O. `rec.meta()` is `None`
                    // until the worker lands (the GUI re-reads per frame; the
                    // PML writer backfills at finalize).
                    metadata.resolve_deferred(Arc::clone(&rec));
                    // Seed the module list only for INIT — a process already
                    // running at capture start, whose image-loads predate capture,
                    // so an enumeration is the only source for its pre-existing
                    // modules. A CREATE process's modules all arrive as image-load
                    // events, so Procmon and the C++ both deliberately skip the
                    // snapshot for it (`v21 = notify==0` gate in IDA
                    // `sub_140085160`; `IsProcessFromInit()` in `propstack.cpp`).
                    // Live capture only (`metadata` is None for offline PML replay).
                    if entry.notify() == pn::INIT {
                        seed_init_modules(&rec, pid);
                    }
                }
                mgr.insert(rec);
            }
        }
        pn::EXIT => {
            // Mark exited (keep the record) so this exit event and later events
            // still resolve the process, matching C++ `CProcMgr::Remove`.
            if let Ok(seq) = u32::try_from(entry.process_seq) {
                mgr.mark_exited(seq, entry.time);
            }
        }
        pn::IMAGE_LOAD => {
            if let (Ok(seq), Some(module)) = (
                u32::try_from(entry.process_seq),
                image_module(data, DetailMode::Live),
            ) {
                mgr.add_module(seq, module);
            }
        }
        _ => {}
    }
}

/// The System process PID, which owns the loaded kernel drivers.
const SYSTEM_PID: u32 = 4;

/// Seeds an INIT (already-running) process's loaded modules for later call-stack
/// resolution. The System process (PID 4) holds the *kernel* drivers, which
/// Toolhelp can't enumerate — seed those from `NtQuerySystemInformation`; the
/// driver then attributes every driver loaded *during* capture to PID 4 as an
/// image-load event, which appends here and keeps the list current. Every other
/// process seeds its user-mode modules from a Toolhelp snapshot.
fn seed_init_modules(rec: &ProcessRecord, pid: u32) {
    if pid == SYSTEM_PID {
        for m in crate::system::kernel_modules() {
            rec.add_module(Module {
                base: m.base,
                size: m.size,
                path: m.path,
            });
        }
    } else {
        for module in crate::system::snapshot_modules(pid) {
            rec.add_module(module);
        }
    }
}

/// Parses a process-create record into [`ProcessInfo`]. Returns `None` if the
/// fixed header or the image-path region is truncated.
pub(crate) fn create_info(data: &[u8], mode: DetailMode) -> Option<ProcessInfo> {
    let ci = cast::<LogProcessCreate>(data)?;
    let fixed = size_of::<LogProcessCreate>();
    let sid_len = ci.sid_length as usize;
    let int_sid_len = ci.integrity_level_sid_length as usize;

    let user_sid = data.get(fixed..fixed + sid_len).map(<[u8]>::to_vec);

    let int_sid_off = fixed + sid_len;
    let integrity_rid = data
        .get(int_sid_off..int_sid_off + int_sid_len)
        .and_then(sid_rid);

    // Image path then command line trail the SIDs; their byte lengths are
    // mode-dependent (PML packs ASCII strings 1 byte/char).
    let name_off = fixed + sid_len + int_sid_len;
    let (_, name_bytes) = str_field_len(ci.proc_name_length, mode);
    if name_off + name_bytes > data.len() {
        return None;
    }
    let (name, _) = read_detail_str(data, name_off, ci.proc_name_length, mode);
    // Live records hold NT device paths (convert to DOS); PML already stores DOS.
    let image_path = if mode == DetailMode::Pml {
        name
    } else {
        crate::path::nt_to_dos(&name)
    };

    let cmd_off = name_off + name_bytes;
    let (command_line, _) = read_detail_str(data, cmd_off, ci.command_line_length, mode);

    let luid = ci.authentication_id;
    Some(ProcessInfo {
        seq: ci.seq,
        pid: ci.process_id,
        parent_seq: ci.parent_proc_seq,
        parent_pid: ci.parent_id,
        session_id: ci.session_id,
        // The driver's `IsWow64` field is inverted relative to its name: it is
        // SET for 64-bit native processes and CLEAR for WoW64 (32-bit) ones. The
        // C++ reference reads `!IsWow64` to recover the real WoW64 flag
        // (procmonsdk/viewer.cxx `CProcInfoView::IsWow64`). Mirror that so
        // `is_wow64` genuinely means "32-bit process on 64-bit Windows".
        is_wow64: ci.is_wow64 == 0,
        create_time: ci.create_time,
        authentication_id: (luid.high_part, luid.low_part),
        user_sid,
        integrity_rid,
        is_virtualized: ci.token_virtualization_enabled != 0,
        image_path,
        command_line,
    })
}

/// Parses an image-load record into a [`Module`]. The image name (UTF-16 units)
/// immediately follows the fixed struct.
fn image_module(data: &[u8], mode: DetailMode) -> Option<Module> {
    let info = cast::<LogLoadImage>(data)?;
    let name_off = size_of::<LogLoadImage>();
    let (raw_name, _) = read_detail_str(data, name_off, info.image_name_length, mode);
    // Live records hold NT device paths (convert to DOS); PML already stores DOS.
    let path = if raw_name.is_empty() {
        String::new()
    } else if mode == DetailMode::Pml {
        raw_name
    } else {
        crate::path::nt_to_dos(&raw_name)
    };
    Some(Module {
        base: info.image_base,
        size: info.image_size,
        path,
    })
}

/// Serializes this process event's live detail blob into PML form: the driver
/// blob verbatim with the image path moved to DOS form (the command line, SIDs and
/// other fields are preserved byte-exact). See [`crate::parse::transcode`].
pub(crate) fn pml_detail(ev: &Event) -> Option<Vec<u8>> {
    use crate::parse::transcode::{live_str, splice, PathEdit};
    use core::mem::offset_of;
    let data = ev.pre_data();
    if data.is_empty() {
        return None;
    }
    let mut edits = Vec::new();
    match ev.notify_type() {
        pn::INIT | pn::CREATE => {
            if let Some(ci) = cast::<LogProcessCreate>(data) {
                // Image path follows the fixed struct + user SID + integrity SID.
                let name_off = size_of::<LogProcessCreate>()
                    + ci.sid_length as usize
                    + ci.integrity_level_sid_length as usize;
                let raw = ci.proc_name_length;
                let name = live_str(data, name_off, raw);
                if !name.is_empty() {
                    edits.push(PathEdit {
                        len_field_off: offset_of!(LogProcessCreate, proc_name_length),
                        data_off: name_off,
                        raw_units: raw as usize,
                        text: crate::path::nt_to_dos(&name),
                    });
                }
            }
        }
        pn::IMAGE_LOAD => {
            if let Some(info) = cast::<LogLoadImage>(data) {
                let name_off = size_of::<LogLoadImage>();
                let raw = info.image_name_length;
                let name = live_str(data, name_off, raw);
                if !name.is_empty() {
                    edits.push(PathEdit {
                        len_field_off: offset_of!(LogLoadImage, image_name_length),
                        data_off: name_off,
                        raw_units: raw as usize,
                        text: crate::path::nt_to_dos(&name),
                    });
                }
            }
        }
        _ => {}
    }
    Some(splice(data, edits))
}

/// Extracts the RID (last sub-authority) from raw SID bytes.
fn sid_rid(sid: &[u8]) -> Option<u32> {
    let count = *sid.get(1)? as usize; // SubAuthorityCount
    if count == 0 {
        return None;
    }
    // 1 revision + 1 count + 6 identifier authority, then `count` u32 sub-auths.
    let last = 8 + (count - 1) * 4;
    let bytes = sid.get(last..last + 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

/// Operation view over a process record.
pub(crate) struct ProcView<'a> {
    ev: &'a Event,
}

impl<'a> ProcView<'a> {
    pub(crate) fn new(ev: &'a Event) -> Self {
        Self { ev }
    }
}

impl OperationView for ProcView<'_> {
    fn path(&self) -> Option<String> {
        let data = self.ev.pre_data();
        let mode = self.ev.mode();
        match self.ev.notify_type() {
            pn::INIT | pn::CREATE => create_info(data, mode)
                .map(|i| i.image_path)
                .filter(|p| !p.is_empty()),
            pn::IMAGE_LOAD => image_module(data, mode)
                .map(|m| m.path)
                .filter(|p| !p.is_empty()),
            // Process Start carries no image path in its data; use the started
            // process's image path from the process table (already DOS form).
            pn::START => self
                .ev
                .process()
                .map(|p| p.info.image_path.clone())
                .filter(|p| !p.is_empty()),
            _ => None,
        }
    }

    fn detail(&self) -> String {
        let data = self.ev.pre_data();
        let mode = self.ev.mode();
        match self.ev.notify_type() {
            pn::INIT | pn::CREATE => match create_info(data, mode) {
                Some(i) => format!("PID: {}, Command line: {}", i.pid, i.command_line),
                None => String::new(),
            },
            pn::IMAGE_LOAD => match image_module(data, mode) {
                Some(m) => format!("Image Base: 0x{:x}, Image Size: 0x{:x}", m.base, m.size),
                None => String::new(),
            },
            pn::START => start_detail(data, mode),
            pn::THREAD_CREATE => match data.get(0..4) {
                Some(b) => format!("Thread ID: {}", u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                None => String::new(),
            },
            pn::THREAD_EXIT => match cast::<LogThreadExit>(data) {
                Some(t) => format!(
                    "Exit Status: {}, User Time: {}, Kernel Time: {}",
                    strings::nt_status_string(t.exit_status as i32),
                    filetime_secs(t.user_time),
                    filetime_secs(t.kernel_time),
                ),
                None => String::new(),
            },
            // Process Exit and Process Performance both carry LOG_PROCESSBASIC_INFO.
            pn::EXIT | pn::PERFORMANCE => match cast::<LogProcessBasic>(data) {
                Some(b) => format!(
                    "Exit Status: {}, User Time: {}, Kernel Time: {}, Private Bytes: {}, Working Set: {}",
                    strings::nt_status_string(b.exit_status as i32),
                    filetime_secs(b.user_time),
                    filetime_secs(b.kernel_time),
                    { b.pagefile_usage },
                    { b.working_set_size },
                ),
                None => String::new(),
            },
            _ => String::new(),
        }
    }
}

/// Detail for `NOTIFY_PROCESS_START`: parent PID plus the command line and current
/// directory that trail the fixed struct (the environment block is skipped). The
/// string lengths are mode-dependent (PML packs ASCII strings 1 byte/char).
fn start_detail(data: &[u8], mode: DetailMode) -> String {
    let Some(info) = cast::<LogProcessStart>(data) else {
        return String::new();
    };
    let fixed = size_of::<LogProcessStart>();
    let (cmd, cmd_bytes) = read_detail_str(data, fixed, info.command_line_length, mode);
    let (cwd, _) = read_detail_str(data, fixed + cmd_bytes, info.current_directory_length, mode);
    format!(
        "Parent PID: {}, Command line: {}, Current directory: {}",
        { info.parent_id },
        cmd,
        cwd
    )
}

/// Formats a Windows `LARGE_INTEGER` time span (100 ns ticks) as fractional
/// seconds, matching Procmon's `User Time`/`Kernel Time` columns (7 decimals).
fn filetime_secs(ticks: i64) -> String {
    let t = ticks.max(0) as u64;
    format!("{}.{:07}", t / 10_000_000, t % 10_000_000)
}
