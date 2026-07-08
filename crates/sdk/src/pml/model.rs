//! Owned result types for the PML reader/writer, mirroring `logs.py`'s
//! `Module` / `Process` / `Event`. Strings are `Arc<str>` (shared, cheap to clone;
//! the reader transcodes each unique string from the dedup table once).

use std::borrow::Cow;
use std::sync::Arc;

use crate::EventClass;

/// A captured process icon, stored inside the PML so it renders without the
/// original executable (which may live on another machine). `data` is a Windows
/// `ICONIMAGE` resource (`BITMAPINFOHEADER` + colors + masks) — load it with
/// `CreateIconFromResourceEx`. `dimension` is the icon's `cx`/`cy` size hint.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PmlIcon {
    pub dimension: u32,
    pub data: Arc<[u8]>,
}

/// The fixed part of an on-disk PML event record (Python-struct
/// `"<IIIHHIQQIHHII"`); the variable body — `[stack frames][details]` — and the
/// optional extra/completion blob follow it. `#[repr(C, packed)]` over plain
/// little-endian integers, so its size and field offsets *are* the wire layout
/// (derived via `size_of`, never hand-counted constants).
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub(crate) struct PmlEventHeader {
    pub process_index: u32,
    pub tid: u32,
    /// The event class as its PML u32 (see [`EventClass::from_u32`]).
    pub class: u32,
    pub operation: u16,
    pub reserved0: u16,
    pub reserved1: u32,
    pub duration: u64,
    pub date_filetime: u64,
    pub result: u32,
    pub stack_depth: u16,
    pub reserved2: u16,
    pub details_size: u32,
    /// Offset of the extra/completion blob from the event start (0 = none).
    pub extra_offset: u32,
}

/// Size of the fixed event part; the body follows at this offset.
pub(crate) const EVENT_HEADER_SIZE: usize = size_of::<PmlEventHeader>();
// Pin the wire layout: fail the build if the struct drifts from the format.
const _: () = assert!(EVENT_HEADER_SIZE == 52);

impl PmlEventHeader {
    /// Borrows the header at `off` in `data`, or `None` if it doesn't fit.
    pub(crate) fn view(data: &[u8], off: usize) -> Option<&Self> {
        crate::kernel_types::cast(data.get(off..)?)
    }

    /// The header's on-disk bytes (the writer emits these verbatim).
    pub(crate) fn as_bytes(&self) -> &[u8] {
        // SAFETY: `PmlEventHeader` is `#[repr(C, packed)]` plain integers
        // (alignment 1, no padding), so its memory is exactly the wire bytes.
        unsafe { core::slice::from_raw_parts(self as *const Self as *const u8, size_of::<Self>()) }
    }
}

/// A loaded module within a process (`logs.py:Module`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PmlModule {
    pub base_address: u64,
    pub size: u32,
    pub image_path: Arc<str>,
    pub version: Arc<str>,
    pub company: Arc<str>,
    pub description: Arc<str>,
    pub timestamp: u32,
}

/// A process seen in the capture (`logs.py:Process`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PmlProcess {
    pub process_index: u32,
    pub pid: u32,
    pub parent_pid: u32,
    pub authentication_id: u64,
    pub session: u32,
    /// FILETIME (100ns since 1601-01-01); `end_time` is 0 while still running.
    pub start_time: u64,
    pub end_time: u64,
    pub virtualized: bool,
    pub is_64bit: bool,
    pub integrity: Arc<str>,
    pub user: Arc<str>,
    pub process_name: Arc<str>,
    pub image_path: Arc<str>,
    pub command_line: Arc<str>,
    pub company: Arc<str>,
    pub version: Arc<str>,
    pub description: Arc<str>,
    /// Indices into the PML icon array ([`crate::PmlReader::icon`]); small = 16px,
    /// big = 32px. The icon bytes live in the reader, not here.
    pub icon_small: u32,
    pub icon_big: u32,
    pub modules: Vec<PmlModule>,
}

/// One captured event (`logs.py:Event`). `details` is the ordered set of extra
/// columns (key → value); `path`/`category` are the Path and Category columns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PmlEvent {
    pub process_index: u32,
    pub tid: u32,
    pub class: EventClass,
    pub operation: u16,
    /// Duration in 100ns intervals.
    pub duration: u64,
    /// FILETIME the event was captured.
    pub date_filetime: u64,
    /// Raw NTSTATUS-ish result code.
    pub result: u32,
    /// Stack-frame return addresses (empty if the trace was disabled).
    pub stack: Vec<u64>,
    pub category: Cow<'static, str>,
    pub path: Arc<str>,
    pub details: Vec<(Cow<'static, str>, String)>,
    /// Operation display name override (e.g. network prepends "TCP "/"UDP ").
    pub op_name: Option<String>,
    /// The PML detail blob for the writer to emit: from `PmlWriter::push_event`
    /// (transcoded from live driver data) or from `PmlReader::event_with_raw` (kept
    /// verbatim for PML→PML round-trip). `None` for `event()` (zero-copy) and
    /// events built from scratch.
    pub raw_detail: Option<Arc<[u8]>>,
    /// The PML extra-detail blob (POST data) for the writer, if any.
    pub raw_extra: Option<Arc<[u8]>>,
}

#[allow(dead_code)] // accessors used by the PML decode/round-trip tests
impl PmlEvent {
    /// The operation's display name for this event's class.
    pub fn operation_name(&self) -> &str {
        match &self.op_name {
            Some(name) => name,
            None => self.class.operation_name(self.operation),
        }
    }

    /// The result's display name (e.g. "SUCCESS", "ACCESS DENIED", or hex),
    /// from the SDK's single NTSTATUS table.
    pub fn result_name(&self) -> Cow<'static, str> {
        crate::strings::nt_status_string(self.result as i32)
    }

    /// Capture time as local `HH:MM:SS.fffffff` (FILETIME is 100 ns since 1601,
    /// same base as the live event timestamp).
    pub fn time_of_day(&self) -> String {
        crate::time::time_of_day(self.date_filetime as i64)
    }

    /// Capture date/time as local `YYYY/MM/DD HH:MM:SS`.
    pub fn date(&self) -> String {
        crate::time::date(self.date_filetime as i64)
    }
}
