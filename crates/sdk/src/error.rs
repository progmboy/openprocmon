//! Error and result types for the SDK.
//!
//! Errors wrap the underlying `windows` crate error or `NTSTATUS` rather than
//! re-encoding them, so callers can inspect the original failure. All messages
//! are in English to keep log output and `Display` consistent across locales.

use windows::core::Error as WinError;
use windows::Win32::Foundation::NTSTATUS;

/// Anything that can go wrong while loading the driver, connecting to the port,
/// exchanging messages, or driving the ETW network session.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Loading the kernel driver requires an elevated (administrator) process.
    #[error("administrator privileges are required")]
    NotElevated,

    /// `SE_LOAD_DRIVER_NAME` (or another required privilege) could not be enabled.
    #[error("failed to enable privilege: {0:?}")]
    PrivilegeDenied(WinError),

    /// `NtLoadDriver`/`NtUnloadDriver` returned a failing status.
    #[error("driver load failed: {0:?}")]
    DriverLoad(NTSTATUS),

    /// Writing the driver's service registry key failed.
    #[error("failed to configure driver service: {0:?}")]
    ServiceConfig(WinError),

    /// `FilterConnectCommunicationPort` failed for a reason other than a missing
    /// driver (a missing driver triggers an automatic load + retry instead).
    #[error("failed to connect to filter port: {0:?}")]
    PortConnect(WinError),

    /// `FilterSendMessage` failed.
    #[error("failed to send control message: {0:?}")]
    SendMessage(WinError),

    /// `FilterGetMessage` failed.
    #[error("failed to receive message: {0:?}")]
    GetMessage(WinError),

    /// An ETW trace session call (`StartTraceW`/`OpenTraceW`/`ProcessTrace`/
    /// `ControlTraceW`) failed.
    #[error("etw trace error: {0:?}")]
    Etw(WinError),

    /// A single record could not be parsed; carried so the pipeline can log and
    /// skip without tearing down the receive/parse threads.
    #[error("event parse error: {0}")]
    Parse(String),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;
