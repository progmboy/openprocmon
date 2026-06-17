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

/// A handle to an elevated child process started via `relaunch_elevated`. The
/// parent can wait on it but cannot terminate it (lower integrity level), which
/// is why control flows over the pipe instead.
///
/// The raw handle value is stored as `isize` (not `HANDLE`) so the struct is
/// `Send` — a process handle is process-wide and safe to use from any thread,
/// and the MCP server holds this inside a shared, `Send`-required session map.
#[cfg(windows)]
pub struct ElevatedChild {
    handle: isize,
}

#[cfg(windows)]
impl ElevatedChild {
    fn raw(&self) -> windows::Win32::Foundation::HANDLE {
        windows::Win32::Foundation::HANDLE(self.handle as *mut core::ffi::c_void)
    }

    /// Blocks until the child exits.
    pub fn wait(&self) -> std::io::Result<()> {
        use windows::Win32::System::Threading::{WaitForSingleObject, INFINITE};
        // SAFETY: handle is a valid process handle owned by self until Drop.
        let r = unsafe { WaitForSingleObject(self.raw(), INFINITE) };
        if r.0 == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
}

#[cfg(windows)]
impl Drop for ElevatedChild {
    fn drop(&mut self) {
        use windows::Win32::Foundation::CloseHandle;
        // SAFETY: closing our own handle; the child keeps running independently.
        unsafe {
            let _ = CloseHandle(self.raw());
        }
    }
}

/// UTF-16, NUL-terminated.
#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Relaunches the current exe elevated (UAC prompt) with `args`, returning a
/// handle to the elevated process. `Err` with kind `PermissionDenied` if the
/// user declined the UAC prompt (`ERROR_CANCELLED`).
#[cfg(windows)]
pub fn relaunch_elevated(args: &[String]) -> std::io::Result<ElevatedChild> {
    use std::mem::size_of;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{GetLastError, ERROR_CANCELLED};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let exe = std::env::current_exe()?;
    let exe_w = wide(&exe.to_string_lossy());
    // Quote each arg so paths with spaces survive; join with spaces.
    let params = args
        .iter()
        .map(|a| format!("\"{}\"", a.replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(" ");
    let params_w = wide(&params);
    let verb_w = wide("runas");

    let mut sei = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb_w.as_ptr()),
        lpFile: PCWSTR(exe_w.as_ptr()),
        lpParameters: PCWSTR(params_w.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };

    // SAFETY: sei is fully initialized; string buffers outlive the call.
    let res = unsafe { ShellExecuteExW(&mut sei) };
    if res.is_err() {
        let code = unsafe { GetLastError() };
        if code == ERROR_CANCELLED {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "UAC elevation was declined",
            ));
        }
        return Err(std::io::Error::last_os_error());
    }
    if sei.hProcess.is_invalid() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "elevation returned no process handle",
        ));
    }
    Ok(ElevatedChild {
        handle: sei.hProcess.0 as isize,
    })
}

/// Spawns a thread that blocks until the parent process (`parent_pid`) exits,
/// then calls `on_exit`. Backup to pipe-EOF for orphan protection. Windows-only.
#[cfg(windows)]
pub fn watch_parent(parent_pid: u32, on_exit: impl FnOnce() + Send + 'static) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE,
    };
    std::thread::spawn(move || {
        // SAFETY: PROCESS_SYNCHRONIZE only; handle closed before returning.
        unsafe {
            if let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, parent_pid) {
                let _ = WaitForSingleObject(h, INFINITE);
                let _ = CloseHandle(h);
                on_exit();
            }
        }
    });
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
