//! Wire-format definitions shared with the kernel miniFilter (`kernel/logsdk.h`).
//!
//! This module is the single source of truth for the kernel/user-mode contract:
//! the communication constants, the packed record structures, the operation
//! enumerations, and the helpers that walk a received batch. Every struct here is
//! `#[repr(C, packed)]` (alignment 1) so its byte layout matches the driver
//! exactly; scalar fields are therefore read by value (an unaligned load), never
//! by reference. The static assertions at the bottom of the file fail the build
//! if any layout drifts from `logsdk.h`.

use core::mem::size_of;
use windows::Win32::Storage::InstallableFileSystems::FILTER_MESSAGE_HEADER;

// ---------------------------------------------------------------------------
// Communication constants (logsdk.h)
// ---------------------------------------------------------------------------

/// Name of the driver's Filter Manager communication port (OPENPROCMON build).
pub const PORT_NAME: &str = "\\ProcessMonitor24Port";

/// Maximum size, in bytes, of a single batch the driver delivers per message.
pub const MAX_MESSAGE_LEN: usize = 0x20000;

/// Control code selecting the monitor enable/disable message.
pub const CTLCODE_MONITOR: u32 = 0;
/// Control code selecting the thread-profiling interval message.
pub const CTLCODE_THREADPROFILING: u32 = 1;

/// Monitor enable flags (`CTL_MONITOR_*`), combined into `FLTMSG_CONTROL_FLAGS`.
pub mod monitor_flags {
    pub const ALL_CLOSE: u32 = 0x00;
    pub const PROC_ON: u32 = 0x01;
    pub const FILE_ON: u32 = 0x02;
    pub const REG_ON: u32 = 0x04;
    pub const OLDREG_ON: u32 = 0x08;
    pub const EXTLOG_ON: u32 = 0x10;
}

/// `STATUS_PENDING`: a PRE record with this status will be completed later by a
/// matching POST record (correlated by `Sequence`).
pub const STATUS_PENDING: i32 = 0x0000_0103;

/// Kernel pointer width. The driver is x64, so call-stack frames and the
/// `CALC_ENTRY_SIZE` formula use 8-byte pointers; the SDK must be built for x64
/// to match (`size_of::<usize>() == 8`).
pub const PTR_SIZE: usize = size_of::<usize>();

/// Size of [`LogEntry`], the common record header.
pub const LOG_ENTRY_SIZE: usize = size_of::<LogEntry>();

// ---------------------------------------------------------------------------
// Monitor type / notify type enumerations (logsdk.h)
// ---------------------------------------------------------------------------

/// High-level source of a record (`LOG_MONITOR_TYPE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorType {
    /// Completion record (`MONITOR_TYPE_POST`): correlates with a pending PRE.
    Post,
    Process,
    Reg,
    File,
    Profiling,
    /// Any value the driver may emit that this SDK does not model.
    Unknown(u16),
}

impl MonitorType {
    pub fn from_u16(v: u16) -> Self {
        match v {
            0 => Self::Post,
            1 => Self::Process,
            2 => Self::Reg,
            3 => Self::File,
            4 => Self::Profiling,
            other => Self::Unknown(other),
        }
    }
}

/// Process monitor operations (`LOG_PROCESS_NOTIFY_TYPE`).
pub mod proc_notify {
    pub const INIT: u16 = 0;
    pub const CREATE: u16 = 1;
    pub const EXIT: u16 = 2;
    pub const THREAD_CREATE: u16 = 3;
    pub const THREAD_EXIT: u16 = 4;
    pub const IMAGE_LOAD: u16 = 5;
    pub const THREAD_PERFORMANCE: u16 = 6;
    pub const START: u16 = 7;
    pub const PERFORMANCE: u16 = 8;
    pub const SYSTEM_PERFORMANCE: u16 = 9;
}

/// Registry monitor operations (`LOG_REG_NOTIFY_TYPE`).
pub mod reg_notify {
    pub const OPENKEYEX: u16 = 0;
    pub const CREATEKEYEX: u16 = 1;
    pub const KEYHANDLECLOSE: u16 = 2;
    pub const QUERYKEY: u16 = 3;
    pub const SETVALUEKEY: u16 = 4;
    pub const QUERYVALUEKEY: u16 = 5;
    pub const ENUMERATEVALUEKEY: u16 = 6;
    pub const ENUMERATEKEY: u16 = 7;
    pub const SETINFORMATIONKEY: u16 = 8;
    pub const DELETEKEY: u16 = 9;
    pub const DELETEVALUEKEY: u16 = 10;
    pub const FLUSHKEY: u16 = 11;
    pub const LOADKEY: u16 = 12;
    pub const UNLOADKEY: u16 = 13;
    pub const RENAMEKEY: u16 = 14;
    pub const QUERYMULTIPLEVALUEKEY: u16 = 15;
    pub const SETKEYSECURITY: u16 = 16;
    pub const QUERYKEYSECURITY: u16 = 17;
}

/// File records encode their operation as `IRP MajorFunction + FILE_NOTIFY_BASE`,
/// so the major function is recovered with `notify_type - FILE_NOTIFY_BASE`.
pub const FILE_NOTIFY_BASE: u16 = 20;

/// IRP major function codes used when formatting file operations (`wdm.h`).
pub mod irp_mj {
    pub const CREATE: u8 = 0x00;
    pub const CREATE_NAMED_PIPE: u8 = 0x01;
    pub const CLOSE: u8 = 0x02;
    pub const READ: u8 = 0x03;
    pub const WRITE: u8 = 0x04;
    pub const QUERY_INFORMATION: u8 = 0x05;
    pub const SET_INFORMATION: u8 = 0x06;
    pub const QUERY_EA: u8 = 0x07;
    pub const SET_EA: u8 = 0x08;
    pub const FLUSH_BUFFERS: u8 = 0x09;
    pub const QUERY_VOLUME_INFORMATION: u8 = 0x0a;
    pub const SET_VOLUME_INFORMATION: u8 = 0x0b;
    pub const DIRECTORY_CONTROL: u8 = 0x0c;
    pub const FILE_SYSTEM_CONTROL: u8 = 0x0d;
    pub const DEVICE_CONTROL: u8 = 0x0e;
    pub const INTERNAL_DEVICE_CONTROL: u8 = 0x0f;
    pub const SHUTDOWN: u8 = 0x10;
    pub const LOCK_CONTROL: u8 = 0x11;
    pub const CLEANUP: u8 = 0x12;
    pub const CREATE_MAILSLOT: u8 = 0x13;
    pub const QUERY_SECURITY: u8 = 0x14;
    pub const SET_SECURITY: u8 = 0x15;
    pub const POWER: u8 = 0x16;
    pub const SYSTEM_CONTROL: u8 = 0x17;
    pub const DEVICE_CHANGE: u8 = 0x18;
    pub const QUERY_QUOTA: u8 = 0x19;
    pub const SET_QUOTA: u8 = 0x1a;
    pub const PNP: u8 = 0x1b;

    // Fast-I/O / cache-manager pseudo majors (`fltKernel.h`, defined as
    // `(UCHAR)-N`), reported by the minifilter alongside the IRP majors above.
    pub const ACQUIRE_FOR_SECTION_SYNCHRONIZATION: u8 = 0xff; // -1
    pub const RELEASE_FOR_SECTION_SYNCHRONIZATION: u8 = 0xfe; // -2
    pub const ACQUIRE_FOR_MOD_WRITE: u8 = 0xfd; // -3
    pub const RELEASE_FOR_MOD_WRITE: u8 = 0xfc; // -4
    pub const ACQUIRE_FOR_CC_FLUSH: u8 = 0xfb; // -5
    pub const RELEASE_FOR_CC_FLUSH: u8 = 0xfa; // -6
    pub const QUERY_OPEN: u8 = 0xf9; // -7
    pub const FAST_IO_CHECK_IF_POSSIBLE: u8 = 0xf3; // -13
    pub const NETWORK_QUERY_OPEN: u8 = 0xf2; // -14
    pub const MDL_READ: u8 = 0xf1; // -15
    pub const MDL_READ_COMPLETE: u8 = 0xf0; // -16
    pub const PREPARE_MDL_WRITE: u8 = 0xef; // -17
    pub const MDL_WRITE_COMPLETE: u8 = 0xee; // -18
    pub const VOLUME_MOUNT: u8 = 0xed; // -19
    pub const VOLUME_DISMOUNT: u8 = 0xec; // -20
}

/// File information classes used to refine `IRP_MJ_QUERY_INFORMATION` /
/// `IRP_MJ_SET_INFORMATION` operations by their minor function (`FILE_INFORMATION_CLASS`).
///
/// Each entry is `(minor, fast_io_name, show_name)`, mirroring the C++
/// `FILE_OPT_SUB_DESC { Minjor, lpszFastIoName, lpszShowName }`: `show_name` is the
/// friendly "Advanced Display" name, `fast_io_name` the `FASTIO_*` name used when
/// the operation arrived via the fast-I/O path. See [`crate::strings::operation`].
pub mod file_info_class {
    // Query-information minor functions (`FILE_INFORMATION_CLASS`), cf. C++
    // `gFileSubMapQueryInfo`.
    pub const QUERY: &[(u8, &str, &str)] = &[
        (
            0x04,
            "FASTIO_QUERY_INFORMATION",
            "QueryBasicInformationFile",
        ),
        (
            0x05,
            "FASTIO_QUERY_INFORMATION",
            "QueryStandardInformationFile",
        ),
        (
            0x06,
            "FASTIO_QUERY_INFORMATION",
            "QueryFileInternalInformationFile",
        ),
        (0x07, "FASTIO_QUERY_INFORMATION", "QueryEaInformationFile"),
        (0x09, "FASTIO_QUERY_INFORMATION", "QueryNameInformationFile"),
        (
            0x0e,
            "FASTIO_QUERY_INFORMATION",
            "QueryPositionInformationFile",
        ),
        (0x12, "FASTIO_QUERY_INFORMATION", "QueryAllInformationFile"),
        (0x14, "FASTIO_QUERY_INFORMATION", "QueryEndOfFile"),
        (
            0x16,
            "FASTIO_QUERY_INFORMATION",
            "QueryStreamInformationFile",
        ),
        (
            0x1c,
            "FASTIO_QUERY_INFORMATION",
            "QueryCompressionInformationFile",
        ),
        (0x1d, "FASTIO_QUERY_INFORMATION", "QueryId"),
        (
            0x1f,
            "FASTIO_QUERY_INFORMATION",
            "QueryMoveClusterInformationFile",
        ),
        (
            0x22,
            "FASTIO_QUERY_INFORMATION",
            "QueryNetworkOpenInformationFile",
        ),
        (0x23, "FASTIO_QUERY_INFORMATION", "QueryAttributeTagFile"),
        (0x25, "FASTIO_QUERY_INFORMATION", "QueryIdBothDirectory"),
        (0x27, "FASTIO_QUERY_INFORMATION", "QueryValidDataLength"),
        (
            0x28,
            "FASTIO_QUERY_INFORMATION",
            "QueryShortNameInformationFile",
        ),
        (0x2b, "FASTIO_QUERY_INFORMATION", "QueryIoPriorityHint"),
        (0x2e, "FASTIO_QUERY_INFORMATION", "QueryLinks"),
        (
            0x30,
            "FASTIO_QUERY_INFORMATION",
            "QueryNormalizedNameInformationFile",
        ),
        (
            0x31,
            "FASTIO_QUERY_INFORMATION",
            "QueryNetworkPhysicalNameInformationFile",
        ),
        (
            0x32,
            "FASTIO_QUERY_INFORMATION",
            "QueryIdGlobalTxDirectoryInformation",
        ),
        (
            0x33,
            "FASTIO_QUERY_INFORMATION",
            "QueryIsRemoteDeviceInformation",
        ),
        (
            0x34,
            "FASTIO_QUERY_INFORMATION",
            "QueryAttributeCacheInformation",
        ),
        (0x35, "FASTIO_QUERY_INFORMATION", "QueryNumaNodeInformation"),
        (
            0x36,
            "FASTIO_QUERY_INFORMATION",
            "QueryStandardLinkInformation",
        ),
        (
            0x37,
            "FASTIO_QUERY_INFORMATION",
            "QueryRemoteProtocolInformation",
        ),
        (
            0x38,
            "FASTIO_QUERY_INFORMATION",
            "QueryRenameInformationBypassAccessCheck",
        ),
        (
            0x39,
            "FASTIO_QUERY_INFORMATION",
            "QueryLinkInformationBypassAccessCheck",
        ),
        (
            0x3a,
            "FASTIO_QUERY_INFORMATION",
            "QueryVolumeNameInformation",
        ),
        (0x3b, "FASTIO_QUERY_INFORMATION", "QueryIdInformation"),
        (
            0x3c,
            "FASTIO_QUERY_INFORMATION",
            "QueryIdExtdDirectoryInformation",
        ),
        (
            0x3e,
            "FASTIO_QUERY_INFORMATION",
            "QueryHardLinkFullIdInformation",
        ),
        (
            0x3f,
            "FASTIO_QUERY_INFORMATION",
            "QueryIdExtdBothDirectoryInformation",
        ),
        (
            0x43,
            "FASTIO_QUERY_INFORMATION",
            "QueryDesiredStorageClassInformation",
        ),
        (0x44, "FASTIO_QUERY_INFORMATION", "QueryStatInformation"),
        (
            0x45,
            "FASTIO_QUERY_INFORMATION",
            "QueryMemoryPartitionInformation",
        ),
    ];
    // Set-information minor functions, cf. C++ `gFileSubMapSetInfo`.
    pub const SET: &[(u8, &str, &str)] = &[
        (0x04, "FASTIO_SET_INFORMATION", "SetBasicInformationFile"),
        (0x0a, "FASTIO_SET_INFORMATION", "SetRenameInformationFile"),
        (0x0b, "FASTIO_SET_INFORMATION", "SetLinkInformationFile"),
        (
            0x0d,
            "FASTIO_SET_INFORMATION",
            "SetDispositionInformationFile",
        ),
        (0x0e, "FASTIO_SET_INFORMATION", "SetPositionInformationFile"),
        (
            0x13,
            "FASTIO_SET_INFORMATION",
            "SetAllocationInformationFile",
        ),
        (
            0x14,
            "FASTIO_SET_INFORMATION",
            "SetEndOfFileInformationFile",
        ),
        (0x16, "FASTIO_SET_INFORMATION", "SetFileStreamInformation"),
        (0x17, "FASTIO_SET_INFORMATION", "SetPipeInformation"),
        (
            0x27,
            "FASTIO_SET_INFORMATION",
            "SetValidDataLengthInformationFile",
        ),
        (0x28, "FASTIO_SET_INFORMATION", "SetShortNameInformation"),
        (
            0x3d,
            "FASTIO_SET_INFORMATION",
            "SetReplaceCompletionInformation",
        ),
        (
            0x40,
            "FASTIO_SET_INFORMATION",
            "SetDispositionInformationEx",
        ),
        (0x41, "FASTIO_SET_INFORMATION", "SetRenameInformationEx"),
        (
            0x42,
            "FASTIO_SET_INFORMATION",
            "SetRenameInformationExBypassAccessCheck",
        ),
    ];
    // Volume-information minor functions.
    pub const VOLUME: &[(u8, &str, &str)] = &[
        (
            0x01,
            "FASTIO_QUERY_VOLUME_INFORMATION",
            "QueryInformationVolume",
        ),
        (
            0x02,
            "FASTIO_QUERY_VOLUME_INFORMATION",
            "QueryLabelInformationVolume",
        ),
        (
            0x03,
            "FASTIO_QUERY_VOLUME_INFORMATION",
            "QuerySizeInformationVolume",
        ),
        (
            0x04,
            "FASTIO_QUERY_VOLUME_INFORMATION",
            "QueryDeviceInformationVolume",
        ),
        (
            0x05,
            "FASTIO_QUERY_VOLUME_INFORMATION",
            "QueryAttributeInformationVolume",
        ),
        (
            0x06,
            "FASTIO_QUERY_VOLUME_INFORMATION",
            "QueryControlInformationVolume",
        ),
        (
            0x07,
            "FASTIO_QUERY_VOLUME_INFORMATION",
            "QueryFullSizeInformationVolume",
        ),
        (
            0x08,
            "FASTIO_QUERY_VOLUME_INFORMATION",
            "QueryObjectIdInformationVolume",
        ),
    ];
    // Directory-control minor functions.
    pub const DIRECTORY: &[(u8, &str, &str)] = &[
        (0x01, "FASTIO_DIRECTORY_CONTROL", "QueryDirectory"),
        (0x02, "FASTIO_DIRECTORY_CONTROL", "NotifyChangeDirectory"),
    ];
    // Lock-control minor functions (fast-I/O lock operations); each has its own
    // `FASTIO_*` name (cf. C++ `gFileSubMapLockControl`).
    pub const LOCK: &[(u8, &str, &str)] = &[
        (0x01, "FASTIO_LOCK", "LockFile"),
        (0x02, "FASTIO_UNLOCK_SINGLE", "UnlockFileSingle"),
        (0x03, "FASTIO_UNLOCK_ALL", "UnlockFileAll"),
        (0x04, "FASTIO_UNLOCK_ALL_BY_KEY", "UnlockFileByKey"),
    ];
    // Plug-and-play minor functions, cf. C++ `gFileSubMapPnp` (whose fast-I/O name
    // is the raw `IRP_MJ_PNP`).
    pub const PNP: &[(u8, &str, &str)] = &[
        (0x00, "IRP_MJ_PNP", "StartDevice"),
        (0x01, "IRP_MJ_PNP", "QueryRemoveDevice"),
        (0x02, "IRP_MJ_PNP", "RemoveDevice"),
        (0x03, "IRP_MJ_PNP", "CancelRemoveDevice"),
        (0x04, "IRP_MJ_PNP", "StopDevice"),
        (0x05, "IRP_MJ_PNP", "QueryStopDevice"),
        (0x06, "IRP_MJ_PNP", "CancelStopDevice"),
        (0x07, "IRP_MJ_PNP", "QueryDeviceRelations"),
        (0x08, "IRP_MJ_PNP", "QueryInterface"),
        (0x09, "IRP_MJ_PNP", "QueryCapabilities"),
        (0x0a, "IRP_MJ_PNP", "QueryResources"),
        (0x0b, "IRP_MJ_PNP", "QueryResourceRequirements"),
        (0x0c, "IRP_MJ_PNP", "QueryDeviceText"),
        (0x0d, "IRP_MJ_PNP", "FilterResourceRequirements"),
        (0x0f, "IRP_MJ_PNP", "ReadConfig"),
        (0x10, "IRP_MJ_PNP", "WriteConfig"),
        (0x11, "IRP_MJ_PNP", "Eject"),
        (0x12, "IRP_MJ_PNP", "SetLock"),
        (0x13, "IRP_MJ_PNP", "QueryId"),
        (0x14, "IRP_MJ_PNP", "QueryPnpDeviceState"),
        (0x15, "IRP_MJ_PNP", "QueryBusInformation"),
        (0x16, "IRP_MJ_PNP", "DeviceUsageNotification"),
        (0x17, "IRP_MJ_PNP", "SurpriseRemoval"),
        (0x18, "IRP_MJ_PNP", "QueryLegacyBusInformation"),
    ];
    // Read/Write carry a single fast-I/O wildcard entry (`0xFF`): the minor
    // function never refines the displayed name, so any minor maps to the base
    // verb. Kept as tables (rather than only the major name) to mirror C++
    // `gFileSubMapRead`/`gFileSubMapWrite` and to exercise the `0xFF` wildcard.
    pub const READ: &[(u8, &str, &str)] = &[(SUB_WILDCARD, "FASTIO_READ", "ReadFile")];
    pub const WRITE: &[(u8, &str, &str)] = &[(SUB_WILDCARD, "FASTIO_WRITE", "WriteFile")];

    /// Sentinel minor that matches any minor function (cf. C++ sub-map `0xff`).
    pub const SUB_WILDCARD: u8 = 0xFF;
}

// ---------------------------------------------------------------------------
// Common record header (LOG_ENTRY) + call-stack frames
// ---------------------------------------------------------------------------

/// The fixed 0x34-byte header that precedes every record's frame chain and data.
///
/// Layout mirrors `_LOG_ENTRY`. `field_*` members are unused by the SDK but kept
/// so the struct size and field offsets match the driver byte-for-byte.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogEntry {
    pub process_seq: i32,
    pub thread_id: u32,
    pub monitor_type: u16,
    field_a: u16,
    pub notify_type: u16,
    field_e: u16,
    pub sequence: i32,
    field_14: u32,
    field_18: u32,
    pub time: i64,
    pub status: i32,
    pub n_frame_chain: u16,
    field_2a: u16,
    pub data_length: u32,
    field_30: u32,
}

/// One raw call-stack frame address. Packed so a `&[StackFrame]` can borrow the
/// frame chain in place even though it starts at the unaligned offset 0x34.
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct StackFrame(u64);

impl StackFrame {
    /// A frame from a raw instruction-pointer address — for owned stacks that
    /// don't borrow the wire buffer (e.g. a network event's frames, decoded
    /// from a PML blob rather than referenced in place).
    pub fn from_addr(addr: u64) -> Self {
        StackFrame(addr)
    }

    /// The frame's instruction pointer (a raw address; symbol resolution is a
    /// concern of the GUI layer, matching the C++ SDK).
    pub fn address(self) -> u64 {
        self.0
    }
}

impl LogEntry {
    /// Borrows the header at `off` in `buf`, validating that a full header fits.
    ///
    /// Returns `None` for a truncated buffer so callers can stop walking a
    /// corrupt batch instead of reading out of bounds.
    pub fn view(buf: &[u8], off: usize) -> Option<&LogEntry> {
        if off.checked_add(LOG_ENTRY_SIZE)? > buf.len() {
            return None;
        }
        // SAFETY: the slice holds at least `LOG_ENTRY_SIZE` bytes at `off`, and
        // `LogEntry` is `#[repr(C, packed)]` (alignment 1) of plain integers, so
        // any such region is a valid, initialized view.
        Some(unsafe { &*(buf.as_ptr().add(off) as *const LogEntry) })
    }

    /// Category of this record.
    pub fn monitor(&self) -> MonitorType {
        MonitorType::from_u16(self.monitor_type)
    }

    /// Operation discriminant within the category.
    pub fn notify(&self) -> u16 {
        self.notify_type
    }

    /// Number of call-stack frames stored between the header and the data.
    pub fn frame_count(&self) -> usize {
        self.n_frame_chain as usize
    }

    /// Length, in bytes, of the operation-specific data trailing the frames.
    pub fn data_len(&self) -> usize {
        self.data_length as usize
    }

    /// Byte offset of the operation-specific data from the start of this header
    /// (header + frame chain). Equivalent to the `TO_EVENT_DATA` macro's stride.
    pub fn data_offset(&self) -> usize {
        LOG_ENTRY_SIZE + self.frame_count() * PTR_SIZE
    }

    /// Total size of this record (header + frame chain + data). Equivalent to the
    /// `CALC_ENTRY_SIZE` macro; used to advance to the next record in a batch.
    pub fn entry_size(&self) -> usize {
        self.data_offset() + self.data_len()
    }

    /// Borrows the call-stack frames (cf. `TO_EVENT_DATA` minus the data step).
    ///
    /// # Safety
    /// The caller must guarantee this header was obtained from a buffer that holds
    /// the full record, i.e. at least [`entry_size`](Self::entry_size) bytes from
    /// the header onward (the batch iterator upholds this).
    pub unsafe fn frame_chain(&self) -> &[StackFrame] {
        let base = (self as *const LogEntry as *const u8).add(LOG_ENTRY_SIZE);
        core::slice::from_raw_parts(base as *const StackFrame, self.frame_count())
    }

    /// Borrows the operation-specific data (cf. the `TO_EVENT_DATA` macro).
    ///
    /// # Safety
    /// Same invariant as [`frame_chain`](Self::frame_chain): the backing buffer
    /// must contain the full record.
    pub unsafe fn event_data(&self) -> &[u8] {
        let base = (self as *const LogEntry as *const u8).add(self.data_offset());
        core::slice::from_raw_parts(base, self.data_len())
    }
}

// ---------------------------------------------------------------------------
// Control messages (FLTMSG_CONTROL_*) and the per-message header
// ---------------------------------------------------------------------------

/// `FLTMSG_CONTROL_FLAGS`: enables/disables monitor sources (`CtlCode == 0`).
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct FltmsgControlFlags {
    pub ctl_code: u32,
    pub flags: u32,
}

/// `FLTMSG_CONTROL_THREADPROFILING`: sets the profiling interval (`CtlCode == 1`).
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct FltmsgControlThreadProfiling {
    pub ctl_code: u32,
    pub thread_profile: i64,
}

/// `PROCMON_MESSAGE_HEADER`: the Filter Manager header plus the batch length the
/// driver prepends to each delivered message (`#pragma pack(4)`).
#[repr(C, packed(4))]
#[derive(Clone, Copy)]
pub struct ProcmonMessageHeader {
    pub header: FILTER_MESSAGE_HEADER,
    pub length: u32,
}

impl ProcmonMessageHeader {
    /// Byte offset at which the record batch begins in a received buffer.
    pub const BATCH_OFFSET: usize = size_of::<ProcmonMessageHeader>();

    /// Reads the batch length from a received buffer, or `None` if the buffer is
    /// too small to contain a full header.
    pub fn batch_len(buf: &[u8]) -> Option<u32> {
        if buf.len() < Self::BATCH_OFFSET {
            return None;
        }
        // SAFETY: the buffer is large enough for the header; `length` is read by
        // value (unaligned) and never as a reference into the packed struct.
        let header = unsafe { &*(buf.as_ptr() as *const ProcmonMessageHeader) };
        Some(header.length)
    }
}

// ---------------------------------------------------------------------------
// Per-operation data structures (everything under the LOG_ENTRY data region)
// ---------------------------------------------------------------------------

/// `LUID` (logon session id) carried by process-create records.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Luid {
    pub low_part: u32,
    pub high_part: i32,
}

/// `LOG_PROCESSCREATE_INFO`. Trailing variable data follows in this order:
/// user SID, integrity-level SID, image name (UTF-16), command line (UTF-16).
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogProcessCreate {
    pub seq: u32,
    pub process_id: u32,
    pub parent_proc_seq: u32,
    pub parent_id: u32,
    pub session_id: u32,
    pub is_wow64: u32,
    pub create_time: i64,
    pub authentication_id: Luid,
    pub token_virtualization_enabled: u32,
    pub sid_length: u8,
    pub integrity_level_sid_length: u8,
    pub proc_name_length: u16,
    pub command_line_length: u16,
    unknown1: u16,
}

/// `LOG_PROCESSSTART_INFO`. Trailing data: command line, current directory,
/// environment block.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogProcessStart {
    pub parent_id: u32,
    pub command_line_length: u16,
    pub current_directory_length: u16,
    pub environment_length: u32,
}

/// `LOG_THREADEXIT_INFO` / process-basic exit accounting prefix.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogThreadExit {
    pub exit_status: u32,
    pub kernel_time: i64,
    pub user_time: i64,
}

/// `LOG_PROCESSBASIC_INFO`: a process's exit/performance accounting, sent for
/// `NOTIFY_PROCESS_EXIT` and `NOTIFY_PROCESS_PERFORMANCE`. The `SIZE_T` members
/// are 8 bytes on the x64 builds this SDK targets.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogProcessBasic {
    pub exit_status: u32,
    pub kernel_time: i64,
    pub user_time: i64,
    pub working_set_size: u64,
    pub peak_working_set_size: u64,
    pub pagefile_usage: u64,
    pub peak_pagefile_usage: u64,
}

/// `LOG_PROCESS_PROFILING_INFO`.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogProcessProfiling {
    pub user_time: i64,
    pub kernel_time: i64,
    pub working_set_size: u64,
    pub pagefile_usage: u64,
}

/// `LOG_THREAD_PROFILING_INFO`.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogThreadProfiling {
    pub user_time_change: u32,
    pub kernel_time_change: u32,
    pub context_switches_change: u32,
}

/// `LOG_LOADIMAGE_INFO`. The image name (UTF-16) follows the fixed fields.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogLoadImage {
    pub image_base: u64,
    pub image_size: u32,
    pub image_name_length: u16,
    fill_0e: u16,
}

/// Fixed prefix of `LOG_FILE_OPT`, up to (but excluding) the `FLT_PARAMETERS`
/// union. The union, then `NameLength`/`Name`, follow; their offsets are derived
/// with [`file_opt::name_length_offset`] using the windows-crate `FLT_PARAMETERS`
/// size so we never hard-code the union's byte width.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogFileOptHead {
    pub minor_function: u8,
    fill1: [u8; 7],
    pub iopb_flag: u32,
    pub flags: u32,
}

/// Offsets within a `LOG_FILE_OPT` record's data region.
pub mod file_opt {
    use super::*;
    use windows::Wdk::Storage::FileSystem::Minifilters::FLT_PARAMETERS;

    /// Offset of the embedded `FLT_PARAMETERS` union (right after the fixed head).
    pub const FLT_PARAMS_OFFSET: usize = size_of::<LogFileOptHead>();

    /// Byte length of the `FLT_PARAMETERS` union (from the windows crate, so it
    /// always matches the kernel's `fltKernel.h` definition).
    pub const FLT_PARAMS_SIZE: usize = size_of::<FLT_PARAMETERS>();

    /// Offset of the `NameLength` (`u16`) field that follows the union.
    pub const fn name_length_offset() -> usize {
        FLT_PARAMS_OFFSET + FLT_PARAMS_SIZE
    }

    /// Offset of the `Name` (UTF-16) buffer: `NameLength` + 2-byte fill (`Fill42`).
    pub const fn name_offset() -> usize {
        name_length_offset() + size_of::<u16>() + 2
    }
}

/// `LOG_FILE_CREATE`: trails the file name for `IRP_MJ_CREATE` records.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogFileCreate {
    pub desired_access: u32,
    pub user_token_length: u32,
}

/// POST data for `IRP_MJ_CREATE`: the `IoStatus.Information` (a `ULONG_PTR`),
/// whose low 32 bits are the create result disposition (cf. C++ reading
/// `*(ULONG_PTR*)TO_EVENT_DATA(pPostEntry)`). Packed (alignment 1) so it can be
/// borrowed from an unaligned offset in the record.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogFileCreatePost {
    pub information: u64,
}

/// `LOG_REG_CREATEOPENKEY`.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogRegCreateOpenKey {
    pub key_name_length: u16,
    fill02: u16,
    pub desired_access: u32,
}

/// `LOG_REG_POSTCREATEOPENKEY`: the completion data for create/open key.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogRegPostCreateOpenKey {
    pub granted_access: u32,
    pub disposition: u32,
}

/// `LOG_REG_SETVALUEKEY`.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogRegSetValueKey {
    pub key_name_length: u16,
    fill02: u16,
    pub value_type: u32,
    pub data_size: u32,
    pub copy_size: u16,
    fill0e: u16,
}

/// `LOG_REG_SETINFORMATIONKEY`.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogRegSetInformationKey {
    pub key_name_length: u16,
    fill02: u16,
    pub key_set_information_class: u32,
    pub key_set_information_length: u32,
    pub copy_size: u16,
    fill0e: u16,
}

/// `LOG_REG_RENAMEKEY`.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogRegRenameKey {
    pub key_name_length: u16,
    pub new_name_length: u16,
}

/// `LOG_REG_ENUMERATEKEY`.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogRegEnumerateKey {
    pub key_name_length: u16,
    fill02: u16,
    pub length: u32,
    pub index: u32,
    pub key_information_class: u32,
}

/// `LOG_REG_ENUMERATEVALUEKEY`.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogRegEnumerateValueKey {
    pub key_name_length: u16,
    fill02: u16,
    pub length: u32,
    pub index: u32,
    pub key_value_information_class: u32,
}

/// `LOG_REG_QUERYKEY`.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogRegQueryKey {
    pub key_name_length: u16,
    fill02: u16,
    pub length: u32,
    pub key_information_class: u32,
}

/// `LOG_REG_QUERYVALUEKEY`.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogRegQueryValueKey {
    pub key_name_length: u16,
    fill02: u16,
    pub length: u32,
    pub key_value_information_class: u32,
}

/// `LOG_REG_LOADKEY`. Trailing data: key name, then source file name.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogRegLoadKey {
    pub key_name_length: u16,
    pub source_file_length: u16,
}

/// `LOG_REG_CONNMON` / `LOG_REG_DELETEVALUEKEY` / `LOG_REG_UNLOADKEY`: records
/// that carry only a key name (a bare `KeyNameLength` prefix).
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct LogRegKeyOnly {
    pub key_name_length: u16,
}

// ---------------------------------------------------------------------------
// Casting helper
// ---------------------------------------------------------------------------

/// Borrows a packed (alignment-1) record view from the front of `bytes`, or
/// `None` if the slice is too short.
///
/// Only sound for `#[repr(C, packed)]` types of plain integers (every `Log*`
/// struct in this module); such types have alignment 1, so any sufficiently
/// large byte region is a valid view, and their fields are always read by value.
pub(crate) fn cast<T: Copy>(bytes: &[u8]) -> Option<&T> {
    if bytes.len() < size_of::<T>() {
        return None;
    }
    // SAFETY: length checked above; `T` is a packed integer record (alignment 1),
    // so the pointer is always suitably aligned and the region is initialized.
    Some(unsafe { &*(bytes.as_ptr() as *const T) })
}

// ---------------------------------------------------------------------------
// Static layout assertions: fail the build if any struct drifts from logsdk.h
// ---------------------------------------------------------------------------

const _: () = {
    assert!(LOG_ENTRY_SIZE == 0x34);
    assert!(size_of::<StackFrame>() == 8);
    assert!(size_of::<FltmsgControlFlags>() == 8);
    assert!(size_of::<FltmsgControlThreadProfiling>() == 12);
    // PROCMON_MESSAGE_HEADER under pack(4): FILTER_MESSAGE_HEADER (16) + u32.
    assert!(ProcmonMessageHeader::BATCH_OFFSET == 20);
    assert!(size_of::<LogProcessCreate>() == 0x34);
    assert!(size_of::<LogProcessStart>() == 0x0c);
    assert!(size_of::<LogLoadImage>() == 0x10);
    assert!(size_of::<LogFileCreate>() == 8);
    assert!(size_of::<LogFileOptHead>() == 16);
    assert!(size_of::<LogRegCreateOpenKey>() == 8);
    assert!(size_of::<LogRegPostCreateOpenKey>() == 8);
    assert!(size_of::<LogRegSetValueKey>() == 0x10);
    assert!(size_of::<LogRegSetInformationKey>() == 0x10);
    assert!(size_of::<LogRegEnumerateKey>() == 0x10);
    assert!(size_of::<LogRegEnumerateValueKey>() == 0x10);
    assert!(size_of::<LogRegQueryKey>() == 0x0c);
    assert!(size_of::<LogRegQueryValueKey>() == 0x0c);
    assert!(size_of::<LogRegRenameKey>() == 4);
    assert!(size_of::<LogRegLoadKey>() == 4);
    // The SDK must be built for a 64-bit target to match the driver's pointers.
    assert!(PTR_SIZE == 8);
};

/// Builds a synthetic minifilter record (`LogEntry` header + `data`, no frame
/// chain) from individual fields, so PML-sourced event data can flow through the
/// same [`crate::event::Event`] parsing as live records (the PML detail blob is
/// the driver's `EventData`, modulo PML's string re-encoding handled by the
/// detail views' `DetailMode`).
#[allow(dead_code)] // used by the PML detail decode path (round-trip / comparison tests)
pub(crate) fn synth_record(
    monitor_type: u16,
    notify_type: u16,
    status: i32,
    data: &[u8],
) -> Vec<u8> {
    // SAFETY: `LogEntry` is a packed struct of integers; all-zero is valid.
    let mut h: LogEntry = unsafe { core::mem::zeroed() };
    h.monitor_type = monitor_type;
    h.notify_type = notify_type;
    h.status = status;
    h.n_frame_chain = 0;
    h.data_length = data.len() as u32;
    let ptr = &h as *const LogEntry as *const u8;
    // SAFETY: reading `size_of::<LogEntry>()` bytes from a valid `LogEntry`.
    let mut bytes =
        unsafe { core::slice::from_raw_parts(ptr, core::mem::size_of::<LogEntry>()) }.to_vec();
    bytes.extend_from_slice(data);
    bytes
}

/// Builds a synthetic [`LogEntry`] header describing `n_frames` stack frames
/// and `data_len` bytes of trailing data — the PML→kernel-record bridge
/// (unset fields stay zero). Returned by value: the header is `Copy` and lives
/// inline in a borrowed [`crate::event::Record`], no allocation.
pub(crate) fn synth_log_entry(
    monitor_type: u16,
    notify_type: u16,
    status: i32,
    thread_id: u32,
    time: i64,
    n_frames: usize,
    data_len: usize,
) -> LogEntry {
    // SAFETY: `LogEntry` is a packed struct of integers; all-zero is valid.
    let mut h: LogEntry = unsafe { core::mem::zeroed() };
    h.monitor_type = monitor_type;
    h.notify_type = notify_type;
    h.status = status;
    h.thread_id = thread_id;
    h.time = time;
    h.n_frame_chain = n_frames as u16;
    h.data_length = data_len as u32;
    h
}

/// Borrows `n` packed [`StackFrame`]s from the start of `bytes`, or `None` if
/// the region is too short — the borrowed analog of [`LogEntry::frame_chain`]
/// for frame chains living outside a kernel record (a PML event body in the
/// reader's mmap).
pub(crate) fn frame_slice(bytes: &[u8], n: usize) -> Option<&[StackFrame]> {
    if bytes.len() < n.checked_mul(PTR_SIZE)? {
        return None;
    }
    // SAFETY: `StackFrame` is `#[repr(C, packed)]` (alignment 1) over a u64, so
    // any region holding `n * 8` bytes (checked above) is a valid view.
    Some(unsafe { core::slice::from_raw_parts(bytes.as_ptr() as *const StackFrame, n) })
}

/// Builds one complete record (header + frame chain + data) from decoded
/// scalars, as owned bytes. The PML read path borrows the mmap instead (see
/// [`crate::event::Record`]); this remains as the reference construction the
/// borrowed layout is tested against. Returns an `Arc<[u8]>` written in place —
/// exactly one allocation per record (an `Arc` cannot be built from a
/// `Vec`/`Box` without re-copying, since its refcount header precedes the data).
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn synth_record_full(
    monitor_type: u16,
    notify_type: u16,
    status: i32,
    thread_id: u32,
    time: i64,
    frames: &[u64],
    data: &[u8],
) -> std::sync::Arc<[u8]> {
    use core::mem::MaybeUninit;

    let h = synth_log_entry(
        monitor_type,
        notify_type,
        status,
        thread_id,
        time,
        frames.len(),
        data.len(),
    );

    let len = LOG_ENTRY_SIZE + frames.len() * PTR_SIZE + data.len();
    let mut arc = std::sync::Arc::<[u8]>::new_uninit_slice(len);
    let dst: &mut [MaybeUninit<u8>] =
        std::sync::Arc::get_mut(&mut arc).expect("freshly allocated, no other refs");
    let mut off = 0usize;
    let mut put = |bytes: &[u8]| {
        // SAFETY: `MaybeUninit<u8>` is layout-identical to `u8`; the parts sum to
        // `len` (asserted below), so the writes stay in bounds.
        unsafe {
            core::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                dst.as_mut_ptr().add(off) as *mut u8,
                bytes.len(),
            );
        }
        off += bytes.len();
    };
    // SAFETY: reading `LOG_ENTRY_SIZE` bytes from a valid `LogEntry`.
    put(unsafe { core::slice::from_raw_parts(&h as *const LogEntry as *const u8, LOG_ENTRY_SIZE) });
    for f in frames {
        put(&f.to_le_bytes()[..PTR_SIZE]);
    }
    put(data);
    assert_eq!(off, len);
    // SAFETY: every byte was written by `put` (off == len).
    unsafe { arc.assume_init() }
}

// ---------------------------------------------------------------------------
// Test helpers for building wire-format bytes without a live driver
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// A zeroed [`LogEntry`] for tests to fill in the fields they care about.
    pub fn zeroed_header() -> LogEntry {
        // SAFETY: `LogEntry` is a packed struct of integers; all-zero is valid.
        unsafe { core::mem::zeroed() }
    }

    /// Serializes a [`LogEntry`] header to its raw bytes.
    pub fn header_bytes(h: &LogEntry) -> Vec<u8> {
        let ptr = h as *const LogEntry as *const u8;
        // SAFETY: reading `LOG_ENTRY_SIZE` bytes out of a valid `LogEntry`.
        unsafe { core::slice::from_raw_parts(ptr, LOG_ENTRY_SIZE) }.to_vec()
    }

    /// Builds one complete record: header + zeroed frame chain + `data`.
    pub fn entry_bytes(
        monitor_type: u16,
        notify_type: u16,
        sequence: i32,
        status: i32,
        data: &[u8],
    ) -> Vec<u8> {
        let mut h = zeroed_header();
        h.monitor_type = monitor_type;
        h.notify_type = notify_type;
        h.sequence = sequence;
        h.status = status;
        h.data_length = data.len() as u32;
        let mut bytes = header_bytes(&h);
        bytes.extend_from_slice(data);
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_full_roundtrips_scalars_and_frames() {
        // monitor=File(3), notify=20, status=0, tid=7, time=123, frames=[0xAA,0xBB], data=[1,2,3]
        let bytes = synth_record_full(3, 20, 0, 7, 123, &[0xAA, 0xBB], &[1, 2, 3]);
        let e = LogEntry::view(&bytes, 0).expect("header");
        // Copy packed fields to locals before asserting (can't reference packed fields).
        let (mt, nt, tid, time, nfc, dl) = (
            e.monitor_type,
            e.notify_type,
            e.thread_id,
            e.time,
            e.n_frame_chain,
            e.data_length,
        );
        assert_eq!(mt, 3);
        assert_eq!(nt, 20);
        assert_eq!(tid, 7);
        assert_eq!(time, 123);
        assert_eq!(nfc, 2);
        assert_eq!(dl, 3);
        // SAFETY: bytes is one complete record.
        let frames = unsafe { e.frame_chain() };
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].address(), 0xAA);
        assert_eq!(frames[1].address(), 0xBB);
        // SAFETY: same as above.
        assert_eq!(unsafe { e.event_data() }, &[1, 2, 3]);
    }

    #[test]
    fn entry_size_includes_frames_and_data() {
        let mut h = test_support::zeroed_header();
        h.n_frame_chain = 3;
        h.data_length = 10;
        assert_eq!(h.data_offset(), LOG_ENTRY_SIZE + 3 * PTR_SIZE);
        assert_eq!(h.entry_size(), LOG_ENTRY_SIZE + 3 * PTR_SIZE + 10);
    }

    #[test]
    fn view_rejects_truncated_buffer() {
        let buf = vec![0u8; LOG_ENTRY_SIZE - 1];
        assert!(LogEntry::view(&buf, 0).is_none());
    }

    #[test]
    fn monitor_type_roundtrip() {
        let bytes = test_support::entry_bytes(3, 25, 1, 0, &[]);
        let e = LogEntry::view(&bytes, 0).unwrap();
        assert_eq!(e.monitor(), MonitorType::File);
        assert_eq!(e.notify(), 25);
    }

    #[test]
    fn name_offset_uses_flt_parameters_size() {
        // Sanity: the name field sits past the head + union + length + fill.
        assert!(file_opt::name_offset() > file_opt::FLT_PARAMS_OFFSET + 4);
    }
}
