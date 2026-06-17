//! Self-elevation: detect whether we're already admin, relaunch ourselves
//! elevated via UAC, and describe which operations need elevation. Windows-only.

use serde::Serialize;

/// Whether the current process token is elevated (running as administrator).
#[cfg(windows)]
pub fn is_elevated() -> bool {
    use std::mem::size_of;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    // SAFETY: we open our own process token, query its elevation flag, and close
    // the handle. All pointers point at locals that outlive the calls.
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut size = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut core::ffi::c_void),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

/// One row of the capability matrix returned by `driver-status`.
#[derive(Clone, Debug, Serialize)]
pub struct ToolCap {
    pub name: &'static str,
    pub requires_elevation: bool,
    pub notes: &'static str,
}

/// The capability matrix: which tools need elevation. Mirrors the MCP tool set.
pub fn capability_matrix() -> Vec<ToolCap> {
    vec![
        ToolCap {
            name: "capture",
            requires_elevation: true,
            notes: "Loads driver + connects port; auto-RunAs (UAC) when unelevated",
        },
        ToolCap {
            name: "start_capture",
            requires_elevation: true,
            notes: "Background session, same as capture; stop_capture stops it over the pipe",
        },
        ToolCap {
            name: "stop_capture",
            requires_elevation: true,
            notes: "Signals the elevated worker over the pipe to finalize and exit",
        },
        ToolCap {
            name: "query_events",
            requires_elevation: false,
            notes: "Reads a PML; no elevation",
        },
        ToolCap {
            name: "get_event",
            requires_elevation: false,
            notes: "Reads a PML; no elevation",
        },
        ToolCap {
            name: "get_process",
            requires_elevation: false,
            notes: "Reads a PML; no elevation",
        },
        ToolCap {
            name: "list_processes",
            requires_elevation: false,
            notes: "Reads a PML; no elevation",
        },
        ToolCap {
            name: "process_tree",
            requires_elevation: false,
            notes: "Reads a PML; no elevation",
        },
        ToolCap {
            name: "summary",
            requires_elevation: false,
            notes: "Reads a PML; no elevation",
        },
        ToolCap {
            name: "export",
            requires_elevation: false,
            notes: "Reads a PML; no elevation",
        },
        ToolCap {
            name: "pml_info",
            requires_elevation: false,
            notes: "Reads a PML; no elevation",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_elevated_does_not_panic() {
        let _ = is_elevated();
    }

    #[test]
    fn capability_matrix_marks_capture_tools_elevated() {
        let m = capability_matrix();
        let cap = m.iter().find(|t| t.name == "capture").unwrap();
        assert!(cap.requires_elevation);
        let q = m.iter().find(|t| t.name == "query_events").unwrap();
        assert!(!q.requires_elevation);
        // serde round-trips.
        serde_json::to_string(&m).unwrap();
    }
}
