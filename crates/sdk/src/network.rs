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
    CloseTrace, ControlTraceW, OpenTraceW, ProcessTrace, StartTraceW, CONTROLTRACE_HANDLE,
    EVENT_RECORD, EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_FLAG_NETWORK_TCPIP, EVENT_TRACE_LOGFILEW,
    EVENT_TRACE_PROPERTIES, EVENT_TRACE_REAL_TIME_MODE, KERNEL_LOGGER_NAMEW,
    PROCESS_TRACE_MODE_EVENT_RECORD, PROCESS_TRACE_MODE_REAL_TIME, WNODE_FLAG_TRACED_GUID,
};

/// TCP/IP MOF provider GUID (classic kernel TcpIp events).
const TCPIP_GUID: GUID = GUID::from_u128(0x9a280ac0_c8e0_11d1_84e2_00c04fb998a2);
/// UDP/IP MOF provider GUID (classic kernel UdpIp events).
const UDPIP_GUID: GUID = GUID::from_u128(0xbf3a50c5_a9c9_4988_a005_2df0b7c80f80);

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
}

/// Context handed to the ETW callback (via `EVENT_RECORD::UserContext`).
struct CallbackCtx {
    tx: Sender<NetworkEvent>,
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

        let ctx = Box::into_raw(Box::new(CallbackCtx { tx }));
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

/// Starts the NT Kernel Logger with the TCP/IP flag enabled.
fn start_session() -> Result<()> {
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
    win32_ok(status)
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

    if let Some(ev) = decode(r) {
        // Never block the ETW callback; drop if the consumer is backed up.
        let _ = ctx.tx.try_send(ev);
    }
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
}
