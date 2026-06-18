//! Host system info for the PML header (computer name, OS version, CPU, RAM), so
//! a capture we write carries the same metadata Procmon records — gathered the
//! same way Procmon does (`GetComputerNameW` + `RtlGetVersion` + `GetSystemInfo`
//! + `GlobalMemoryStatusEx`; see IDA `sub_14002EE30`).

/// A snapshot of the host metadata embedded in a PML header.
pub struct HostInfo {
    pub computer_name: String,
    /// Raw `OSVERSIONINFOEXW` bytes (284) from `RtlGetVersion`, stored verbatim.
    pub os_version: Vec<u8>,
    /// `SYSTEM_INFO.lpMaximumApplicationAddress`.
    pub max_app_address: u64,
    pub num_logical_processors: u32,
    pub ram_bytes: u64,
}

/// Gathers the host metadata for a PML header, the same way Procmon does.
pub fn host_info() -> HostInfo {
    use windows::Wdk::System::SystemServices::RtlGetVersion;
    use windows::Win32::System::SystemInformation::{
        GetSystemInfo, GlobalMemoryStatusEx, MEMORYSTATUSEX, OSVERSIONINFOEXW, OSVERSIONINFOW,
        SYSTEM_INFO,
    };

    // OS version as the raw OSVERSIONINFOEXW the header stores verbatim. Size is
    // set to the EX struct (284) so RtlGetVersion fills the extended fields too.
    let mut osv = OSVERSIONINFOEXW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOEXW>() as u32,
        ..Default::default()
    };
    // SAFETY: RtlGetVersion writes into the (EX-sized) struct; the base-type
    // cast is the documented way to pass an OSVERSIONINFOEXW.
    unsafe {
        let _ = RtlGetVersion(&mut osv as *mut _ as *mut OSVERSIONINFOW);
    }
    // SAFETY: OSVERSIONINFOEXW is repr(C), fully initialized; copy its bytes.
    let os_version = unsafe {
        std::slice::from_raw_parts(
            &osv as *const _ as *const u8,
            std::mem::size_of::<OSVERSIONINFOEXW>(),
        )
    }
    .to_vec();

    let mut si = SYSTEM_INFO::default();
    // SAFETY: GetSystemInfo fills the SYSTEM_INFO in place.
    unsafe { GetSystemInfo(&mut si) };

    let mut mem = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: dwLength is set; GlobalMemoryStatusEx fills the struct.
    let ram_bytes = if unsafe { GlobalMemoryStatusEx(&mut mem) }.is_ok() {
        mem.ullTotalPhys
    } else {
        0
    };

    HostInfo {
        computer_name: computer_name(),
        os_version,
        max_app_address: si.lpMaximumApplicationAddress as u64,
        num_logical_processors: si.dwNumberOfProcessors,
        ram_bytes,
    }
}

/// The machine's NetBIOS computer name — what Procmon stores in the PML header
/// (it calls `GetComputerNameW` in `WinMain`). Falls back to `%COMPUTERNAME%`
/// then an empty string if the call fails.
pub fn computer_name() -> String {
    use windows::core::PWSTR;
    use windows::Win32::System::WindowsProgramming::GetComputerNameW;

    // MAX_COMPUTERNAME_LENGTH is 15; 256 is comfortably above any real name.
    let mut buf = [0u16; 256];
    let mut size = buf.len() as u32;
    // SAFETY: buf and size are valid; GetComputerNameW writes up to `size`
    // UTF-16 units and updates `size` to the count actually written.
    let ok = unsafe { GetComputerNameW(PWSTR(buf.as_mut_ptr()), &mut size) }.is_ok();
    if ok {
        String::from_utf16_lossy(&buf[..size as usize])
    } else {
        std::env::var("COMPUTERNAME").unwrap_or_default()
    }
}

/// The Windows build number (e.g. `26100`), via `RtlGetVersion` — the only
/// non-lying source (`GetVersionEx` is capped without an app manifest). Returns
/// `0` if the call fails.
pub fn windows_build() -> u32 {
    use windows::Wdk::System::SystemServices::RtlGetVersion;
    use windows::Win32::System::SystemInformation::OSVERSIONINFOW;

    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is a valid, correctly-sized OSVERSIONINFOW; RtlGetVersion
    // only writes into it and returns an NTSTATUS.
    let status = unsafe { RtlGetVersion(&mut info) };
    if status.is_ok() {
        info.dwBuildNumber
    } else {
        0
    }
}

/// One loaded kernel-mode module (driver / ntoskrnl), as Procmon stores them in
/// the System (PID 4) process so kernel-mode stack frames resolve.
pub struct KernelModule {
    pub base: u64,
    pub size: u32,
    pub path: String,
}

/// `RTL_PROCESS_MODULE_INFORMATION` (not in the `windows` crate). Layout per the
/// NT headers; only `image_base`/`image_size`/`full_path_name` are consumed.
#[repr(C)]
#[allow(dead_code)]
struct RtlProcessModuleInformation {
    section: *mut core::ffi::c_void,
    mapped_base: *mut core::ffi::c_void,
    image_base: *mut core::ffi::c_void,
    image_size: u32,
    flags: u32,
    load_order_index: u16,
    init_order_index: u16,
    load_count: u16,
    offset_to_file_name: u16,
    full_path_name: [u8; 256],
}

/// `RTL_PROCESS_MODULES`: a count followed by that many module entries.
#[repr(C)]
struct RtlProcessModules {
    number_of_modules: u32,
    modules: [RtlProcessModuleInformation; 1],
}

/// Enumerates loaded kernel-mode modules via `NtQuerySystemInformation`
/// (`SystemModuleInformation`, class 11) — exactly how Procmon builds its kernel
/// module list (IDA `sub_1400AEAB0`). Returns an empty vec on failure.
pub fn kernel_modules() -> Vec<KernelModule> {
    use windows::Wdk::System::SystemInformation::{
        NtQuerySystemInformation, SYSTEM_INFORMATION_CLASS,
    };

    const SYSTEM_MODULE_INFORMATION: SYSTEM_INFORMATION_CLASS = SYSTEM_INFORMATION_CLASS(11);
    const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004u32 as i32;

    // Without SeDebugPrivilege, `SystemModuleInformation` zeroes every ImageBase
    // (KASLR mitigation since Win8.1) even for an elevated caller — which makes
    // the kernel module list useless for resolution and crashes Procmon. Enable
    // it first (the elevated worker holds it), like Procmon does.
    let _ = enable_privilege(windows::Win32::Security::SE_DEBUG_NAME);

    // Grow the buffer to exactly the size the API reports, like the C++
    // `CProcessInfo::ListKernelModule` (`dwBytes = dwNeed` on length mismatch).
    let mut cap = 4096usize;
    let mut buf = vec![0u8; cap];
    loop {
        let mut ret = 0u32;
        // SAFETY: buf/cap describe a valid writable region; the call only writes
        // up to `cap` bytes and reports the needed length in `ret`.
        let st = unsafe {
            NtQuerySystemInformation(
                SYSTEM_MODULE_INFORMATION,
                buf.as_mut_ptr() as *mut core::ffi::c_void,
                cap as u32,
                &mut ret,
            )
        };
        if st.is_ok() {
            break;
        }
        if st.0 != STATUS_INFO_LENGTH_MISMATCH {
            return Vec::new();
        }
        cap = ret as usize;
        buf = vec![0u8; cap];
    }

    // SAFETY: on success the buffer holds an RTL_PROCESS_MODULES — a u32 count
    // then `count` RTL_PROCESS_MODULE_INFORMATION entries — all within `cap`.
    let header = unsafe { &*(buf.as_ptr() as *const RtlProcessModules) };
    let count = header.number_of_modules as usize;
    // Guard against a count that would read past the returned data.
    let entry = std::mem::size_of::<RtlProcessModuleInformation>();
    let avail = (buf.len().saturating_sub(8)) / entry;
    let count = count.min(avail);
    // SAFETY: `modules` is the start of the entry array; `count` is clamped to
    // what fits in the buffer.
    let entries = unsafe { std::slice::from_raw_parts(header.modules.as_ptr(), count) };

    entries
        .iter()
        .map(|m| {
            let nul = m
                .full_path_name
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(m.full_path_name.len());
            let nt = String::from_utf8_lossy(&m.full_path_name[..nul]).into_owned();
            KernelModule {
                base: m.image_base as u64,
                size: m.image_size,
                path: kernel_path_to_dos(&nt),
            }
        })
        .collect()
}

/// Snapshots a process's currently-loaded modules via Toolhelp
/// (`CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32)` +
/// `Module32FirstW`/`Module32NextW`) — exactly how Procmon seeds a process's
/// module list (IDA `sub_1400AE0A0`). This is the *only* source for the modules
/// of a process that was already running at capture start (its image-loads
/// happened before we attached); for a process created during capture it is the
/// baseline that later image-load events extend. `szExePath` is already a DOS
/// path. Returns an empty vec on failure (process exited / protected / racing).
pub fn snapshot_modules(pid: u32) -> Vec<crate::process::Module> {
    use windows::Win32::Foundation::{CloseHandle, ERROR_BAD_LENGTH, INVALID_HANDLE_VALUE};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, MODULEENTRY32W, TH32CS_SNAPMODULE,
        TH32CS_SNAPMODULE32,
    };

    // SNAPMODULE32 as well so a 64-bit capture also sees the 32-bit modules of a
    // WoW64 target. CreateToolhelp32Snapshot can fail with ERROR_BAD_LENGTH when
    // the module list changes mid-snapshot; MSDN says retry until it succeeds.
    let mut snapshot = INVALID_HANDLE_VALUE;
    for _ in 0..8 {
        // SAFETY: FFI; returns a snapshot handle or an error.
        match unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid) } {
            Ok(h) => {
                snapshot = h;
                break;
            }
            Err(e) if e.code() == ERROR_BAD_LENGTH.to_hresult() => continue,
            Err(_) => return Vec::new(),
        }
    }
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut entry = MODULEENTRY32W {
        dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: `snapshot` is valid and `entry.dwSize` is set as required.
    if unsafe { Module32FirstW(snapshot, &mut entry) }.is_ok() {
        loop {
            let nul = entry
                .szExePath
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExePath.len());
            out.push(crate::process::Module {
                base: entry.modBaseAddr as u64,
                size: entry.modBaseSize,
                path: String::from_utf16_lossy(&entry.szExePath[..nul]),
            });
            // SAFETY: same valid snapshot/entry; advances to the next module.
            if unsafe { Module32NextW(snapshot, &mut entry) }.is_err() {
                break;
            }
        }
    }
    // SAFETY: closing the snapshot handle we created.
    unsafe {
        let _ = CloseHandle(snapshot);
    }
    out
}

/// Enables a named privilege (e.g. `SeLoadDriverPrivilege` for loading the
/// driver, `SeDebugPrivilege` for reading kernel module bases) on the current
/// process token.
pub(crate) fn enable_privilege(name: windows::core::PCWSTR) -> crate::Result<()> {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
        TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token = HANDLE::default();
    // SAFETY: opening the current process token for privilege adjustment.
    unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
    }
    .map_err(crate::error::Error::PrivilegeDenied)?;

    let mut luid = Default::default();
    // SAFETY: `name` is a privilege-name constant; `luid` receives the id.
    let lookup = unsafe { LookupPrivilegeValueW(windows::core::PCWSTR::null(), name, &mut luid) };
    let tp = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };
    let adjust = lookup.and_then(|()| {
        // SAFETY: `tp` describes exactly one privilege and outlives the call.
        unsafe { AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None) }
    });
    // SAFETY: closing a handle we opened.
    unsafe {
        let _ = CloseHandle(token);
    }
    adjust.map_err(crate::error::Error::PrivilegeDenied)
}

/// Converts a kernel module's NT path to a DOS path (best-effort; the path is
/// for display — base/size drive resolution). Handles `\SystemRoot\` and `\??\`.
fn kernel_path_to_dos(nt: &str) -> String {
    if let Some(rest) = nt.strip_prefix("\\SystemRoot\\") {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        format!("{}\\{}", root.trim_end_matches('\\'), rest)
    } else if let Some(rest) = nt.strip_prefix("\\??\\") {
        rest.to_string()
    } else {
        nt.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_real_windows_build() {
        // Any supported Windows host returns a real build (Win10+ is >= 10000).
        let b = windows_build();
        assert!(b >= 10000, "unexpected build number {b}");
    }

    #[test]
    fn reports_a_computer_name() {
        // GetComputerNameW returns the host's NetBIOS name (never empty here).
        let name = computer_name();
        assert!(!name.is_empty(), "computer name was empty");
        assert!(!name.contains('\0'), "name has embedded NUL: {name:?}");
    }

    #[test]
    fn enumerates_kernel_modules() {
        let mods = kernel_modules();
        assert!(
            mods.len() > 50,
            "expected many kernel modules, got {}",
            mods.len()
        );
        // ntoskrnl is always loaded, in the high kernel address range.
        assert!(
            mods.iter()
                .any(|m| m.path.to_ascii_lowercase().contains("ntoskrnl")),
            "ntoskrnl.exe not found"
        );
        // ImageBase is only populated for elevated callers (KASLR mitigation):
        // it is in the high kernel range when elevated, and 0 otherwise (which is
        // what the non-elevated test runner sees). The real capture worker runs
        // elevated, so it gets the real bases.
        assert!(mods
            .iter()
            .all(|m| m.base == 0 || m.base >= 0xFFFF_8000_0000_0000));
    }

    #[test]
    fn snapshots_own_modules() {
        // The test process always has at least ntdll + the test exe loaded, so a
        // self-snapshot returns several modules with non-zero base and a path.
        let pid = std::process::id();
        let mods = snapshot_modules(pid);
        assert!(
            mods.len() > 2,
            "expected several modules, got {}",
            mods.len()
        );
        assert!(mods.iter().all(|m| m.base != 0 && !m.path.is_empty()));
        assert!(
            mods.iter()
                .any(|m| m.path.to_ascii_lowercase().contains("ntdll")),
            "ntdll.dll not found in own module snapshot"
        );
    }

    #[test]
    fn host_info_is_well_formed() {
        let h = host_info();
        // The OSVERSIONINFOEXW blob is exactly 284 bytes; dwBuildNumber (at +12)
        // is the real build, and num CPUs / RAM are non-zero.
        assert_eq!(h.os_version.len(), 0x11c, "os_version must be 284 bytes");
        let build = u32::from_le_bytes(h.os_version[12..16].try_into().unwrap());
        assert!(build >= 10000, "unexpected build {build}");
        assert!(h.num_logical_processors >= 1);
        assert!(h.ram_bytes > 0);
        assert!(h.max_app_address > 0);
    }
}
