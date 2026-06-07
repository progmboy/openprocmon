//! Filter Manager port communication (cf. C++ `CMonitorController` / `CRecvThread`).
//!
//! Wraps the three fltlib calls the SDK needs — connect, send a control message,
//! and receive an event batch — over an overlapped I/O event so the receive can
//! be interrupted by a stop signal without blocking forever.

use crate::error::{Error, Result};
use crate::kernel_types::{
    FltmsgControlFlags, ProcmonMessageHeader, CTLCODE_MONITOR, MAX_MESSAGE_LEN, PORT_NAME,
};
use core::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_IO_PENDING, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Storage::InstallableFileSystems::{
    FilterConnectCommunicationPort, FilterGetMessage, FilterSendMessage, FILTER_MESSAGE_HEADER,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};
use windows::Win32::System::IO::{CancelIoEx, OVERLAPPED};

/// Size of the receive scratch buffer: the largest batch plus its header.
pub const MESSAGE_BUFFER_LEN: usize = MAX_MESSAGE_LEN + size_of::<ProcmonMessageHeader>();

/// How long the receive wait blocks before re-checking the stop flag.
const WAIT_SLICE_MS: u32 = 500;

/// A connected handle to the driver's communication port.
pub struct FilterPort {
    handle: HANDLE,
    /// Auto-reset event signaling overlapped receive completion.
    event: HANDLE,
}

// SAFETY: the handles are owned by this struct and a Windows HANDLE has no thread
// affinity. The struct is shared as `Arc<FilterPort>` between the receive thread
// (which calls `recv`) and the controller (which calls `send_control`); the
// Filter Manager supports a concurrent `FilterGetMessage`/`FilterSendMessage` on
// one port, and the overlapped event is touched only by `recv`, so concurrent
// `&self` use is sound.
unsafe impl Send for FilterPort {}
unsafe impl Sync for FilterPort {}

impl FilterPort {
    /// Connects to the driver's communication port. Returns the raw connect error
    /// (callers map `ERROR_FILE_NOT_FOUND` to a driver-load + retry).
    pub fn connect() -> Result<Self> {
        let context: u32 = 0;
        // SAFETY: `&HSTRING` is a valid NUL-terminated port name; `context` is a
        // 4-byte scratch value matching the size argument; no security attributes.
        let handle = unsafe {
            FilterConnectCommunicationPort(
                &HSTRING::from(PORT_NAME),
                0,
                Some(&context as *const u32 as *const core::ffi::c_void),
                size_of::<u32>() as u16,
                None,
            )
        }
        .map_err(Error::PortConnect)?;

        // SAFETY: an unnamed, auto-reset, initially non-signaled event.
        let event = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }
            .map_err(Error::PortConnect)?;

        Ok(Self { handle, event })
    }

    /// Sends a monitor enable/disable control message (`FLTMSG_CONTROL_FLAGS`).
    pub fn send_control(&self, flags: u32) -> Result<()> {
        let message = FltmsgControlFlags {
            ctl_code: CTLCODE_MONITOR,
            flags,
        };
        let mut returned = 0u32;
        // SAFETY: `message` outlives the synchronous call; no output is requested.
        unsafe {
            FilterSendMessage(
                self.handle,
                &message as *const FltmsgControlFlags as *const core::ffi::c_void,
                size_of::<FltmsgControlFlags>() as u32,
                None,
                0,
                &mut returned,
            )
        }
        .map_err(Error::SendMessage)
    }

    /// Receives one batch into `buf`, returning the batch byte length (records
    /// begin at [`ProcmonMessageHeader::BATCH_OFFSET`]), or `None` if `stop` was
    /// set while waiting.
    ///
    /// On stop the pending overlapped read is cancelled and awaited before
    /// returning, so the kernel never writes into `buf` after this call returns.
    pub fn recv(&self, buf: &mut [u8], stop: &AtomicBool) -> Result<Option<usize>> {
        let mut overlapped = OVERLAPPED {
            hEvent: self.event,
            ..Default::default()
        };
        let header = buf.as_mut_ptr() as *mut FILTER_MESSAGE_HEADER;

        // SAFETY: `buf` is at least header-sized and outlives the call;
        // `overlapped` lives until the wait below completes or is cancelled.
        let result = unsafe {
            FilterGetMessage(self.handle, header, buf.len() as u32, Some(&mut overlapped))
        };

        match result {
            Ok(()) => {}
            Err(err) if err.code() == ERROR_IO_PENDING.to_hresult() => {
                loop {
                    // SAFETY: `event` is a valid auto-reset event handle.
                    let wait = unsafe { WaitForSingleObject(self.event, WAIT_SLICE_MS) };
                    if wait == WAIT_OBJECT_0 {
                        break;
                    }
                    if wait == WAIT_TIMEOUT {
                        if stop.load(Ordering::Relaxed) {
                            self.cancel_and_drain(&mut overlapped);
                            return Ok(None);
                        }
                    } else {
                        return Err(Error::GetMessage(windows::core::Error::from_win32()));
                    }
                }
            }
            Err(err) => return Err(Error::GetMessage(err)),
        }

        Ok(ProcmonMessageHeader::batch_len(buf).map(|len| len as usize))
    }

    /// Cancels a pending overlapped read and waits for it to settle, ensuring the
    /// kernel will not touch the receive buffer after we return.
    fn cancel_and_drain(&self, overlapped: &mut OVERLAPPED) {
        // SAFETY: cancelling the I/O we issued on this handle/overlapped, then
        // waiting once for the event to ensure the operation has fully settled.
        unsafe {
            let _ = CancelIoEx(self.handle, Some(overlapped));
            let _ = WaitForSingleObject(self.event, INFINITE);
        }
    }
}

impl Drop for FilterPort {
    fn drop(&mut self) {
        // SAFETY: both handles were created by this struct and are closed once.
        unsafe {
            let _ = CloseHandle(self.handle);
            let _ = CloseHandle(self.event);
        }
    }
}
