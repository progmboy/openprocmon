//! Network monitoring via ETW (cf. design §11).
//!
//! Procmon's network events do not come through the minifilter; they come from an
//! ETW kernel trace session with the TCP/IP provider enabled. This module starts
//! the NT Kernel Logger with `EVENT_TRACE_FLAG_NETWORK_TCPIP`, consumes it in
//! real time on a dedicated thread, and decodes the classic TCP/UDP MOF events
//! into [`NetworkEvent`]s.
//!
//! The MOF record layout used here is the documented `TcpIp_TypeGroup1` /
//! `UdpIp_TypeGroup1` header (PID, size, dest addr, source addr, dest port,
//! source port); addresses and ports are in network byte order. IPv6 events use
//! 16-byte addresses and are decoded analogously.

use crate::error::{Error, Result};
use core::mem::size_of;
use crossbeam_channel::Sender;
use std::net::SocketAddr;
use std::thread::JoinHandle;

use windows::core::{GUID, PWSTR};
use windows::Win32::Foundation::WIN32_ERROR;
use windows::Win32::System::Diagnostics::Etw::{
    CloseTrace, ControlTraceW, OpenTraceW, ProcessTrace, StartTraceW, TraceSetInformation,
    TraceStackTracingInfo, CLASSIC_EVENT_ID, CONTROLTRACE_HANDLE, EVENT_RECORD,
    EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_FLAG_NETWORK_TCPIP, EVENT_TRACE_LOGFILEW,
    EVENT_TRACE_PROPERTIES, EVENT_TRACE_REAL_TIME_MODE, KERNEL_LOGGER_NAMEW,
    PROCESS_TRACE_MODE_EVENT_RECORD, PROCESS_TRACE_MODE_REAL_TIME, WNODE_FLAG_TRACED_GUID,
};

/// TCP/IP MOF provider GUID (classic kernel TcpIp events).
const TCPIP_GUID: GUID = GUID::from_u128(0x9a280ac0_c8e0_11d1_84e2_00c04fb998a2);
/// UDP/IP MOF provider GUID (classic kernel UdpIp events).
const UDPIP_GUID: GUID = GUID::from_u128(0xbf3a50c5_a9c9_4988_a005_2df0b7c80f80);
/// StackWalk MOF provider GUID. When stack tracing is enabled for an event, the
/// kernel emits a separate StackWalk event carrying the call stack, correlated
/// to its triggering event by timestamp (see [`parse_stackwalk`]).
const STACKWALK_GUID: GUID = GUID::from_u128(0xdef2fe46_7bd6_4b80_bd94_f57fe20d0ce3);

/// A network operation, independent of protocol (the TCP/UDP distinction is the
/// [`NetworkEvent::is_tcp`] flag). Covers both the ETW classic events and the
/// richer set Procmon records in PML (`consts.py:NetworkOperation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetOp {
    Unknown,
    Other,
    Send,
    Receive,
    Accept,
    Connect,
    Disconnect,
    Reconnect,
    Retransmit,
    TcpCopy,
}

impl NetOp {
    /// Base operation name with no protocol prefix (cf. Procmon's
    /// `NetworkOperation`); the displayed label is built by
    /// [`crate::parse::network::op_label`] which prepends `TCP`/`UDP`.
    pub fn name(self) -> &'static str {
        match self {
            NetOp::Unknown => "Unknown",
            NetOp::Other => "Other",
            NetOp::Send => "Send",
            NetOp::Receive => "Receive",
            NetOp::Accept => "Accept",
            NetOp::Connect => "Connect",
            NetOp::Disconnect => "Disconnect",
            NetOp::Reconnect => "Reconnect",
            NetOp::Retransmit => "Retransmit",
            NetOp::TcpCopy => "TCPCopy",
        }
    }

    /// Maps a PML `NetworkOperation` code (0..=9) to a [`NetOp`].
    pub fn from_pml(code: u16) -> Self {
        match code {
            1 => NetOp::Other,
            2 => NetOp::Send,
            3 => NetOp::Receive,
            4 => NetOp::Accept,
            5 => NetOp::Connect,
            6 => NetOp::Disconnect,
            7 => NetOp::Reconnect,
            8 => NetOp::Retransmit,
            9 => NetOp::TcpCopy,
            _ => NetOp::Unknown,
        }
    }

    /// The PML `NetworkOperation` code for this op (inverse of [`from_pml`](Self::from_pml)).
    pub fn to_pml(self) -> u16 {
        match self {
            NetOp::Unknown => 0,
            NetOp::Other => 1,
            NetOp::Send => 2,
            NetOp::Receive => 3,
            NetOp::Accept => 4,
            NetOp::Connect => 5,
            NetOp::Disconnect => 6,
            NetOp::Reconnect => 7,
            NetOp::Retransmit => 8,
            NetOp::TcpCopy => 9,
        }
    }
}

/// A decoded network event, from ETW (live) or a PML detail blob. Owned (ETW
/// allocates per event and network volume is far lower than file/registry), so it
/// is shared behind an `Arc` when attached to an [`crate::event::Event`].
///
/// `local`/`remote` are the numeric endpoints; `local_name`/`remote_name` hold the
/// resolved `host:port` display string when known (PML carries name/port tables;
/// live ETW leaves them `None`, rendering numeric). Rendering is centralized in
/// [`crate::parse::network::NetView`].
#[derive(Debug, Clone)]
pub struct NetworkEvent {
    pub pid: u32,
    pub is_tcp: bool,
    pub op: NetOp,
    pub local: SocketAddr,
    pub remote: SocketAddr,
    pub local_name: Option<std::sync::Arc<str>>,
    pub remote_name: Option<std::sync::Arc<str>>,
    pub length: u32,
    /// Event time as a FILETIME (100-ns ticks since 1601), the same clock as the
    /// driver's `LogEntry.time`, so network and driver events order together. The
    /// ETW session requests this with `ClientContext = 2` (see [`start_session`]).
    pub time: i64,
    /// Extra ETW MOF properties rendered in the Detail column, as ordered
    /// `(name, value)` pairs — the fields beyond the endpoints/length that
    /// Procmon carries (`seqnum`/`connid` on every op; `mss`/`sackopt`/… on
    /// TCP connect/accept; `startime`/`endtime` on send). PML captures decode
    /// these from the detail blob's trailing string list; live ETW leaves it
    /// empty for now (the endpoints/length are decoded, the MOF extras are not
    /// yet extracted). See [`crate::parse::network`].
    pub extra: Vec<(std::sync::Arc<str>, std::sync::Arc<str>)>,
    /// Call-stack frames (raw addresses). Procmon records a stack for network
    /// events too; a PML capture carries them in the event body before the
    /// detail blob, decoded here. Live ETW leaves it empty (stack-walk capture
    /// is not enabled yet). Owned (not borrowed from the wire) — network volume
    /// is low, so the copy is cheap.
    pub stack: Vec<crate::kernel_types::StackFrame>,
}

/// Context handed to the ETW callback (via `EVENT_RECORD::UserContext`).
///
/// `pending` holds network events awaiting their StackWalk event — the kernel
/// emits the stack as a separate event right after the one that triggered it,
/// correlated by timestamp (Procmon's exact mechanism). Only the single
/// consumer thread touches this via the callback, so a `RefCell` suffices (no
/// cross-thread sharing).
struct CallbackCtx {
    tx: Sender<NetworkEvent>,
    pending: std::cell::RefCell<Vec<NetworkEvent>>,
}

impl CallbackCtx {
    /// Buffers a network event to await its StackWalk. First flushes any older
    /// pending event: seeing a newer timestamp is the watermark that its stack
    /// (which would share the older timestamp) is not coming, so it goes out
    /// stackless rather than lingering — lossless and bounded (the buffer holds
    /// ~1 entry in practice, since the StackWalk immediately follows its event).
    fn push_network(&self, ev: NetworkEvent) {
        let mut pending = self.pending.borrow_mut();
        let now = ev.time;
        let mut i = 0;
        while i < pending.len() {
            if pending[i].time < now {
                let _ = self.tx.try_send(pending.remove(i));
            } else {
                i += 1;
            }
        }
        pending.push(ev);
    }

    /// Attaches a decoded StackWalk's frames to the pending network event with
    /// the matching timestamp, then sends it. A StackWalk for an event we did
    /// not buffer (a non-network event) simply matches nothing.
    fn attach_stack(&self, timestamp: i64, frames: Vec<crate::kernel_types::StackFrame>) {
        let mut pending = self.pending.borrow_mut();
        if let Some(pos) = pending.iter().position(|p| p.time == timestamp) {
            let mut ev = pending.remove(pos);
            ev.stack = frames;
            let _ = self.tx.try_send(ev);
        }
    }

    /// Flushes every still-pending event (the session is stopping, so no more
    /// StackWalks will arrive).
    fn flush_pending(&self) {
        for ev in self.pending.borrow_mut().drain(..) {
            let _ = self.tx.try_send(ev);
        }
    }
}

/// A running ETW network session and its consumer thread.
pub struct NetworkMonitor {
    consumer: Option<JoinHandle<()>>,
    /// Leaked callback context; reclaimed in `stop`.
    ctx: *mut CallbackCtx,
}

// SAFETY: `ctx` is only dereferenced by the ETW callback on the consumer thread;
// `NetworkMonitor` itself just owns the pointer for cleanup.
unsafe impl Send for NetworkMonitor {}

impl NetworkMonitor {
    /// Starts the kernel TCP/IP trace and a consumer thread that forwards decoded
    /// events to `tx`. Any pre-existing kernel-logger session is stopped first.
    pub fn start(tx: Sender<NetworkEvent>) -> Result<Self> {
        // A stale "NT Kernel Logger" would make StartTraceW fail with
        // ERROR_ALREADY_EXISTS; stop it first and ignore the result.
        let _ = stop_session();

        start_session()?;

        let ctx = Box::into_raw(Box::new(CallbackCtx {
            tx,
            pending: std::cell::RefCell::new(Vec::new()),
        }));
        // Raw pointers are not `Send`; move the address as a `usize` instead.
        let ctx_addr = ctx as usize;
        let consumer = std::thread::Builder::new()
            .name("procmon-etw".into())
            .spawn(move || consume(ctx_addr as *mut CallbackCtx))
            .map_err(|e| {
                Error::Etw(windows::core::Error::new(
                    windows::core::HRESULT(-1),
                    e.to_string(),
                ))
            })?;

        Ok(Self {
            consumer: Some(consumer),
            ctx,
        })
    }

    /// Stops the trace session, joins the consumer, and frees the context.
    pub fn stop(mut self) {
        let _ = stop_session();
        if let Some(handle) = self.consumer.take() {
            let _ = handle.join();
        }
        if !self.ctx.is_null() {
            // SAFETY: the consumer thread has joined, so no one else can touch
            // `ctx`; it was created by `Box::into_raw` in `start`.
            unsafe { drop(Box::from_raw(self.ctx)) };
            self.ctx = std::ptr::null_mut();
        }
    }
}

impl Drop for NetworkMonitor {
    fn drop(&mut self) {
        // If `stop` was not called explicitly, ensure the session and thread are
        // torn down so the context can be freed.
        if self.consumer.is_some() || !self.ctx.is_null() {
            let _ = stop_session();
            if let Some(handle) = self.consumer.take() {
                let _ = handle.join();
            }
            if !self.ctx.is_null() {
                // SAFETY: consumer joined above; sole owner of `ctx`.
                unsafe { drop(Box::from_raw(self.ctx)) };
                self.ctx = std::ptr::null_mut();
            }
        }
    }
}

/// Builds an `EVENT_TRACE_PROPERTIES` buffer sized to hold the logger name.
fn properties_buffer() -> Vec<u8> {
    // Name length in bytes including the trailing NUL.
    let name_bytes = ("NT Kernel Logger".len() + 1) * 2;
    vec![0u8; size_of::<EVENT_TRACE_PROPERTIES>() + name_bytes]
}

/// Starts the NT Kernel Logger with the TCP/IP flag enabled and turns on stack
/// tracing for the network events, returning the session handle.
fn start_session() -> Result<CONTROLTRACE_HANDLE> {
    let mut buf = properties_buffer();
    let props = buf.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES;
    // SAFETY: `props` points at a zeroed, correctly sized buffer.
    unsafe {
        (*props).Wnode.BufferSize = buf.len() as u32;
        (*props).Wnode.Flags = WNODE_FLAG_TRACED_GUID;
        (*props).Wnode.Guid = windows::Win32::System::Diagnostics::Etw::SystemTraceControlGuid;
        // System-time (FILETIME) timestamps, not QPC (1): EventHeader.TimeStamp is
        // then 100-ns ticks since 1601 — the same clock as the driver's
        // LogEntry.time, so network events sort/order together with driver events
        // (otherwise QPC values write garbage timestamps into the PML).
        (*props).Wnode.ClientContext = 2;
        (*props).LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
        (*props).EnableFlags = EVENT_TRACE_FLAG_NETWORK_TCPIP;
        (*props).LoggerNameOffset = size_of::<EVENT_TRACE_PROPERTIES>() as u32;
    }
    let mut handle = CONTROLTRACE_HANDLE::default();
    // SAFETY: name is a static wide string; `props` is valid for the call.
    let status = unsafe { StartTraceW(&mut handle, KERNEL_LOGGER_NAMEW, props) };
    win32_ok(status)?;
    enable_stack_tracing(handle);
    Ok(handle)
}

/// Enables stack tracing for the network events, so the kernel emits a
/// StackWalk event carrying the call stack after each one — exactly how Procmon
/// captures network stacks. `TraceSetInformation(TraceStackTracingInfo, …)`
/// takes a list of [`CLASSIC_EVENT_ID`] (event GUID + opcode); we enable every
/// TcpIp/UdpIp opcode we decode, plus its IPv6 variant (opcode + 16). Best
/// effort: a failure just means no stacks, not a broken capture.
fn enable_stack_tracing(handle: CONTROLTRACE_HANDLE) {
    use crate::parse::network::{ET_ACCEPT, ET_CONNECT, ET_DISCONNECT, ET_RECV, ET_SEND};
    const IPV6: u8 = 16; // IPv6 opcodes are the IPv4 opcode + this offset.
    let id = |guid: GUID, ty: u8| CLASSIC_EVENT_ID {
        EventGuid: guid,
        Type: ty,
        Reserved: [0; 7],
    };
    let mut ids = Vec::new();
    for &ty in &[ET_SEND, ET_RECV, ET_CONNECT, ET_DISCONNECT, ET_ACCEPT] {
        ids.push(id(TCPIP_GUID, ty));
        ids.push(id(TCPIP_GUID, ty + IPV6));
    }
    for &ty in &[ET_SEND, ET_RECV] {
        ids.push(id(UDPIP_GUID, ty));
        ids.push(id(UDPIP_GUID, ty + IPV6));
    }
    let bytes = std::mem::size_of_val(ids.as_slice()) as u32;
    // SAFETY: `ids` is a valid, `bytes`-long array of CLASSIC_EVENT_ID for the
    // live session handle; TraceSetInformation only reads it.
    let _ = unsafe {
        TraceSetInformation(
            handle,
            TraceStackTracingInfo,
            ids.as_ptr() as *const core::ffi::c_void,
            bytes,
        )
    };
}

/// Stops the NT Kernel Logger (by name).
fn stop_session() -> Result<()> {
    let mut buf = properties_buffer();
    let props = buf.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES;
    // SAFETY: zeroed, correctly sized buffer.
    unsafe {
        (*props).Wnode.BufferSize = buf.len() as u32;
        (*props).LoggerNameOffset = size_of::<EVENT_TRACE_PROPERTIES>() as u32;
    }
    // SAFETY: stopping by name with a null control handle is permitted.
    let status = unsafe {
        ControlTraceW(
            CONTROLTRACE_HANDLE::default(),
            KERNEL_LOGGER_NAMEW,
            props,
            EVENT_TRACE_CONTROL_STOP,
        )
    };
    win32_ok(status)
}

/// Consumer thread body: opens the real-time trace and pumps it until the
/// session is stopped, at which point `ProcessTrace` returns.
//
// Fields are set after `Default` because the union members (Anonymous1/2) are
// assigned via their union fields, which a struct literal cannot express cleanly.
#[allow(clippy::field_reassign_with_default)]
fn consume(ctx: *mut CallbackCtx) {
    let mut logfile = EVENT_TRACE_LOGFILEW::default();
    // Reuse the crate's static logger-name literal; the field is typed `PWSTR`
    // but ETW only reads it, so pointing it at the read-only buffer is sound.
    logfile.LoggerName = PWSTR(KERNEL_LOGGER_NAMEW.as_ptr() as *mut u16);
    logfile.Anonymous1.ProcessTraceMode =
        PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
    logfile.Anonymous2.EventRecordCallback = Some(event_callback);
    logfile.Context = ctx as *mut core::ffi::c_void;

    // SAFETY: `logfile` is fully initialized; `OpenTraceW` returns a handle whose
    // value is `INVALID_PROCESSTRACE_HANDLE` (u64::MAX) on failure.
    let handle = unsafe { OpenTraceW(&mut logfile) };
    if handle.Value == u64::MAX {
        return;
    }
    // SAFETY: `handle` is valid; ProcessTrace blocks until the session stops.
    unsafe {
        let _ = ProcessTrace(&[handle], None, None);
        let _ = CloseTrace(handle);
    }
    // The session stopped: no more StackWalk events will arrive, so send any
    // network events still waiting for a stack. SAFETY: `ctx` outlives the
    // consumer thread (freed only after this thread joins).
    unsafe { &*ctx }.flush_pending();
}

/// ETW per-event callback: decode TCP/UDP records and forward them.
unsafe extern "system" fn event_callback(record: *mut EVENT_RECORD) {
    if record.is_null() {
        return;
    }
    // SAFETY: ETW guarantees a valid record pointer for the callback duration.
    let r = unsafe { &*record };
    let ctx = r.UserContext as *const CallbackCtx;
    if ctx.is_null() {
        return;
    }
    // SAFETY: `ctx` is the pointer we set as `logfile.Context`, alive until the
    // consumer thread (which owns this callback) joins.
    let ctx = unsafe { &*ctx };

    // A StackWalk event carries the call stack of an earlier event, keyed by
    // that event's timestamp — attach it to the matching pending network event.
    if r.EventHeader.ProviderId == STACKWALK_GUID {
        if let Some((timestamp, frames)) = parse_stackwalk(r) {
            ctx.attach_stack(timestamp, frames);
        }
        return;
    }

    if let Some(ev) = decode(r) {
        // Buffer to await its StackWalk (never blocks; a full channel drops the
        // event via the pending flush's `try_send`).
        ctx.push_network(ev);
    }
}

/// Parses a StackWalk event into `(triggering-event timestamp, frames)`. The
/// MOF layout is `EventTimeStamp(8) + StackProcess(4) + StackThread(4)` then the
/// frame addresses (pointer-sized). `EventTimeStamp` is the timestamp of the
/// event the stack belongs to (matches [`NetworkEvent::time`]). `None` if the
/// payload is too short.
fn parse_stackwalk(r: &EVENT_RECORD) -> Option<(i64, Vec<crate::kernel_types::StackFrame>)> {
    if r.UserData.is_null() {
        return None;
    }
    // SAFETY: ETW provides `UserData`/`UserDataLength` describing a valid buffer.
    let data =
        unsafe { core::slice::from_raw_parts(r.UserData as *const u8, r.UserDataLength as usize) };
    decode_stackwalk(data)
}

/// The pure byte-layout half of [`parse_stackwalk`] (testable off the FFI
/// record): `EventTimeStamp(8) + StackProcess(4) + StackThread(4)` then 8-byte
/// frame addresses. Returns `(timestamp, frames)`; `None` if shorter than the
/// header.
fn decode_stackwalk(data: &[u8]) -> Option<(i64, Vec<crate::kernel_types::StackFrame>)> {
    use crate::kernel_types::StackFrame;
    const HEADER: usize = 16; // EventTimeStamp(8) + StackProcess(4) + StackThread(4)
    if data.len() < HEADER {
        return None;
    }
    let timestamp = i64::from_le_bytes(data.get(0..8)?.try_into().ok()?);
    // x64: each frame is an 8-byte address. `chunks_exact` drops any tail.
    let frames = data[HEADER..]
        .chunks_exact(8)
        .map(|c| StackFrame::from_addr(u64::from_le_bytes(c.try_into().unwrap())))
        .collect();
    Some((timestamp, frames))
}

/// Decodes one ETW record into a [`NetworkEvent`], or `None` if it is not a
/// TCP/UDP event we model or the payload is too short. The MOF byte layout and
/// opcode classification live in [`crate::parse::network`].
fn decode(r: &EVENT_RECORD) -> Option<NetworkEvent> {
    use crate::parse::network::{classify_etw, parse_group1_v4, parse_group1_v6};
    let guid = r.EventHeader.ProviderId;
    let is_tcp = guid == TCPIP_GUID;
    let is_udp = guid == UDPIP_GUID;
    if !is_tcp && !is_udp {
        return None;
    }
    // Determine the operation AND address family from the opcode. The IPv4/IPv6
    // family must come from the opcode, not the payload length: larger IPv4 event
    // groups can exceed the IPv6 `TypeGroup1` size and would otherwise be misparsed.
    let (op, is_ipv6) = classify_etw(is_tcp, r.EventHeader.EventDescriptor.Opcode)?;

    if r.UserData.is_null() || r.UserDataLength == 0 {
        return None;
    }
    // SAFETY: ETW provides `UserData`/`UserDataLength` describing a valid buffer.
    let data =
        unsafe { core::slice::from_raw_parts(r.UserData as *const u8, r.UserDataLength as usize) };
    let (local, remote, length) = if is_ipv6 {
        parse_group1_v6(data)?
    } else {
        parse_group1_v4(data)?
    };
    let pid = u32::from_le_bytes(data.get(0..4)?.try_into().ok()?);
    Some(NetworkEvent {
        pid,
        is_tcp,
        op,
        local,
        remote,
        local_name: None,
        remote_name: None,
        length,
        // FILETIME because the session uses ClientContext = 2 (see start_session).
        time: r.EventHeader.TimeStamp,
        extra: crate::parse::network::group1_extra(data, is_ipv6),
        stack: Vec::new(),
    })
}

fn win32_ok(status: WIN32_ERROR) -> Result<()> {
    if status == windows::Win32::Foundation::ERROR_SUCCESS {
        Ok(())
    } else {
        Err(Error::Etw(windows::core::Error::from_hresult(
            status.to_hresult(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netop_from_pml_codes() {
        assert_eq!(NetOp::from_pml(5), NetOp::Connect);
        assert_eq!(NetOp::from_pml(2), NetOp::Send);
        assert_eq!(NetOp::from_pml(9), NetOp::TcpCopy);
        assert_eq!(NetOp::from_pml(99), NetOp::Unknown);
        assert_eq!(NetOp::Connect.name(), "Connect");
        assert_eq!(NetOp::TcpCopy.name(), "TCPCopy");
    }

    #[test]
    fn decode_stackwalk_parses_timestamp_and_frames() {
        // EventTimeStamp(8) + StackProcess(4) + StackThread(4) + 3 frames(8 each).
        let mut d = Vec::new();
        d.extend_from_slice(&0x1122_3344_5566_7788i64.to_le_bytes()); // timestamp
        d.extend_from_slice(&4u32.to_le_bytes()); // StackProcess
        d.extend_from_slice(&99u32.to_le_bytes()); // StackThread
        for a in [
            0xffff_f800_0000_0011u64,
            0x7ff6_0000_0000_2222,
            0x0000_0000_dead_beef,
        ] {
            d.extend_from_slice(&a.to_le_bytes());
        }
        let (ts, frames) = decode_stackwalk(&d).expect("decode");
        assert_eq!(ts, 0x1122_3344_5566_7788);
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].address(), 0xffff_f800_0000_0011);
        assert_eq!(frames[2].address(), 0x0000_0000_dead_beef);
        // Too short for the header → None.
        assert!(decode_stackwalk(&d[..12]).is_none());
    }

    fn net_at(time: i64) -> NetworkEvent {
        NetworkEvent {
            pid: 1,
            is_tcp: true,
            op: NetOp::Connect,
            local: "10.0.0.1:1".parse().unwrap(),
            remote: "1.2.3.4:2".parse().unwrap(),
            local_name: None,
            remote_name: None,
            length: 0,
            time,
            extra: Vec::new(),
            stack: Vec::new(),
        }
    }

    #[test]
    fn stackwalk_correlates_by_timestamp() {
        use crate::kernel_types::StackFrame;
        let (tx, rx) = crossbeam_channel::unbounded();
        let ctx = CallbackCtx {
            tx,
            pending: std::cell::RefCell::new(Vec::new()),
        };
        // Event arrives, then its StackWalk (same timestamp) → sent with stack.
        ctx.push_network(net_at(100));
        assert!(
            rx.try_recv().is_err(),
            "held pending until its stack arrives"
        );
        ctx.attach_stack(100, vec![StackFrame::from_addr(0xabc)]);
        let ev = rx.try_recv().expect("sent after stack");
        assert_eq!(ev.time, 100);
        assert_eq!(ev.stack.len(), 1);

        // A later event is the watermark that flushes an older stackless one.
        ctx.push_network(net_at(200));
        ctx.push_network(net_at(300));
        let ev = rx
            .try_recv()
            .expect("older event flushed on newer timestamp");
        assert_eq!(ev.time, 200);
        assert!(ev.stack.is_empty(), "no stack came for it");
        // 300 is still pending; flush on session end.
        assert!(rx.try_recv().is_err());
        ctx.flush_pending();
        assert_eq!(rx.try_recv().expect("flushed").time, 300);
    }
}
