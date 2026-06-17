//! Driver-error messages for the CLI: the actionable load failures get a clear,
//! user-facing line (cf. the GUI's `driver_error_message`); everything else uses
//! the SDK's `Display`.

use procmon_sdk::Error;

/// Maps an SDK error to a CLI-friendly message.
pub fn describe_error(e: &Error) -> String {
    match e {
        Error::NotElevated => {
            "live capture requires Administrator — re-run this command in an elevated \
             terminal (PML analysis works without elevation)"
                .to_string()
        }
        Error::OtherVersionLoaded => {
            "another Process Monitor driver is already loaded; stop it (or close \
             Procmon) and retry"
                .to_string()
        }
        Error::AlreadyMonitoring => {
            "the driver is already in use by another capture; stop it and retry".to_string()
        }
        // Access denied connecting to an existing driver port is also an
        // elevation problem (the port exists but a non-elevated process can't
        // open it). `0x80070005` = E_ACCESSDENIED.
        Error::PortConnect(w) if w.code().0 == 0x8007_0005u32 as i32 => {
            "live capture requires Administrator — re-run this command in an elevated \
             terminal (PML analysis works without elevation)"
                .to_string()
        }
        other => other.to_string(),
    }
}
