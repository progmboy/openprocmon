//! Loading and unloading the kernel miniFilter (cf. C++ `drvload.cxx`).
//!
//! Loading a filter driver from user mode requires: an elevated process, the
//! `SeLoadDriverPrivilege`, a service registry key describing the driver and its
//! minifilter instance/altitude, and finally `NtLoadDriver` against that key.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};

use windows::core::{HSTRING, PCWSTR};
use windows::Wdk::System::SystemServices::{NtLoadDriver, NtUnloadDriver};
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, STATUS_IMAGE_ALREADY_LOADED, STATUS_SUCCESS, UNICODE_STRING, WIN32_ERROR,
};
use windows::Win32::Security::{
    AdjustTokenPrivileges, GetTokenInformation, LookupPrivilegeValueW, TokenElevation,
    LUID_AND_ATTRIBUTES, SE_LOAD_DRIVER_NAME, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES,
    TOKEN_ELEVATION, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_WRITE, REG_DWORD,
    REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Service key location (relative to `HKEY_LOCAL_MACHINE`).
const SERVICE_KEY_ROOT: &str = "SYSTEM\\CurrentControlSet\\Services\\";
/// Service key as an NT object path, for `NtLoadDriver`.
const NT_SERVICE_PREFIX: &str = "\\Registry\\Machine\\System\\CurrentControlSet\\Services\\";
/// Minifilter instance name and altitude registered under `Instances`.
const INSTANCE_NAME: &str = "Process Monitor 24 Instance";
const ALTITUDE: &str = "385200";
/// NT namespace prefix for the `ImagePath` value.
const IMAGE_PATH_PREFIX: &str = "\\??\\";

/// Loads/unloads the OpenProcessMonitor driver by name and on-disk `.sys` path.
pub struct DriverLoader {
    name: String,
    sys_path: PathBuf,
}

impl DriverLoader {
    /// Creates a loader for a driver service `name` whose binary is at `sys_path`.
    pub fn new(name: impl Into<String>, sys_path: impl AsRef<Path>) -> Self {
        Self {
            name: name.into(),
            sys_path: sys_path.as_ref().to_path_buf(),
        }
    }

    /// Ensures the driver is loaded: verifies elevation, enables the load-driver
    /// privilege, writes the service key, and calls `NtLoadDriver`. An already
    /// loaded driver is treated as success.
    pub fn ensure_loaded(&self) -> Result<()> {
        if !is_elevated()? {
            return Err(Error::NotElevated);
        }
        enable_privilege(SE_LOAD_DRIVER_NAME)?;
        self.create_service_key()?;
        load_driver(&self.registry_path())
    }

    /// Unloads the driver via `NtUnloadDriver`.
    pub fn unload(&self) -> Result<()> {
        let path = to_unicode(&self.registry_path());
        // SAFETY: `path` is a valid UNICODE_STRING backed by a live buffer.
        let status = unsafe { NtUnloadDriver(&path.value) };
        if status == STATUS_SUCCESS {
            Ok(())
        } else {
            Err(Error::DriverLoad(status))
        }
    }

    /// The driver's service key as an NT object path.
    fn registry_path(&self) -> String {
        format!("{NT_SERVICE_PREFIX}{}", self.name)
    }

    /// Creates the service registry key and its minifilter `Instances` subkeys.
    fn create_service_key(&self) -> Result<()> {
        let image_path = format!("{IMAGE_PATH_PREFIX}{}", normalize_sys_path(&self.sys_path));

        let service = create_key(
            HKEY_LOCAL_MACHINE,
            &format!("{SERVICE_KEY_ROOT}{}", self.name),
        )?;
        let result = (|| {
            set_string(service, "ImagePath", &image_path, REG_EXPAND_SZ)?;
            set_dword(service, "Type", 1)?; // SERVICE_KERNEL_DRIVER
            set_dword(service, "ErrorControl", 1)?; // SERVICE_ERROR_NORMAL
            set_dword(service, "Start", 3)?; // SERVICE_DEMAND_START
            set_dword(service, "SupportedFeatures", 3)?;

            let instances = create_key(service, "Instances")?;
            let r = (|| {
                set_string(instances, "DefaultInstance", INSTANCE_NAME, REG_SZ)?;
                let instance = create_key(instances, INSTANCE_NAME)?;
                let r2 = (|| {
                    set_string(instance, "Altitude", ALTITUDE, REG_SZ)?;
                    set_dword(instance, "Flags", 0)
                })();
                close_key(instance);
                r2
            })();
            close_key(instances);
            r
        })();
        close_key(service);
        result
    }
}

/// A `UNICODE_STRING` together with the `HSTRING` buffer it points into.
struct OwnedUnicode {
    value: UNICODE_STRING,
    // Kept alive (and NUL-terminated) for as long as `value.Buffer` is used.
    _buf: HSTRING,
}

/// Builds a `UNICODE_STRING` over `s`. The driver-load APIs only read it, so the
/// `PWSTR` buffer may point at the (immutable, NUL-terminated) `HSTRING` storage.
fn to_unicode(s: &str) -> OwnedUnicode {
    let buf = HSTRING::from(s);
    let len_bytes = (buf.as_wide().len() * 2) as u16; // excludes the NUL
    let value = UNICODE_STRING {
        Length: len_bytes,
        MaximumLength: len_bytes + 2, // includes room for the NUL terminator
        Buffer: windows::core::PWSTR(buf.as_ptr() as *mut u16),
    };
    OwnedUnicode { value, _buf: buf }
}

/// Calls `NtLoadDriver`, treating "already loaded" as success.
fn load_driver(registry_path: &str) -> Result<()> {
    let path = to_unicode(registry_path);
    // SAFETY: `path` is a valid UNICODE_STRING backed by a live buffer.
    let status = unsafe { NtLoadDriver(&path.value) };
    if status == STATUS_SUCCESS || status == STATUS_IMAGE_ALREADY_LOADED {
        Ok(())
    } else {
        Err(Error::DriverLoad(status))
    }
}

/// Whether the current process token is elevated.
fn is_elevated() -> Result<bool> {
    let mut token = HANDLE::default();
    // SAFETY: querying the current process token; `token` receives the handle.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
        .map_err(Error::PrivilegeDenied)?;
    let mut elevation = TOKEN_ELEVATION::default();
    let mut ret_len = 0u32;
    // SAFETY: `elevation` is sized correctly and `ret_len` receives the length.
    let result = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut core::ffi::c_void),
            core::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        )
    };
    // SAFETY: closing a handle we opened.
    unsafe {
        let _ = CloseHandle(token);
    }
    result.map_err(Error::PrivilegeDenied)?;
    Ok(elevation.TokenIsElevated != 0)
}

/// Enables a named privilege (e.g. `SeLoadDriverPrivilege`) on the current token.
fn enable_privilege(name: PCWSTR) -> Result<()> {
    let mut token = HANDLE::default();
    // SAFETY: opening the current process token for privilege adjustment.
    unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
    }
    .map_err(Error::PrivilegeDenied)?;

    let mut luid = Default::default();
    // SAFETY: `name` is a static privilege-name constant; `luid` receives the id.
    let lookup = unsafe { LookupPrivilegeValueW(PCWSTR::null(), name, &mut luid) };
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
    adjust.map_err(Error::PrivilegeDenied)
}

// --- Registry helpers --------------------------------------------------------

/// Creates (or opens) a subkey under `parent`, returning the open handle.
fn create_key(parent: HKEY, subkey: &str) -> Result<HKEY> {
    let mut key = HKEY::default();
    // SAFETY: `&HSTRING` is a valid NUL-terminated wide string; `key` receives
    // the opened handle.
    let status = unsafe {
        RegCreateKeyExW(
            parent,
            &HSTRING::from(subkey),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        )
    };
    win32_ok(status).map(|()| key)
}

fn close_key(key: HKEY) {
    // SAFETY: `key` was opened by `create_key`.
    unsafe {
        let _ = RegCloseKey(key);
    }
}

/// Sets a string value (`REG_SZ`/`REG_EXPAND_SZ`), including the trailing NUL.
fn set_string(
    key: HKEY,
    name: &str,
    value: &str,
    kind: windows::Win32::System::Registry::REG_VALUE_TYPE,
) -> Result<()> {
    let data = reg_sz_bytes(value);
    // SAFETY: `&HSTRING` is a valid value name; `data` is a valid byte buffer.
    let status = unsafe { RegSetValueExW(key, &HSTRING::from(name), 0, kind, Some(&data)) };
    win32_ok(status)
}

/// Sets a `REG_DWORD` value.
fn set_dword(key: HKEY, name: &str, value: u32) -> Result<()> {
    // SAFETY: `&HSTRING` is a valid value name; the data slice is 4 bytes.
    let status = unsafe {
        RegSetValueExW(
            key,
            &HSTRING::from(name),
            0,
            REG_DWORD,
            Some(&value.to_le_bytes()),
        )
    };
    win32_ok(status)
}

/// Maps a `WIN32_ERROR` registry result to our `Result`.
fn win32_ok(status: WIN32_ERROR) -> Result<()> {
    if status == windows::Win32::Foundation::ERROR_SUCCESS {
        Ok(())
    } else {
        Err(reg_err(status))
    }
}

fn reg_err(status: WIN32_ERROR) -> Error {
    Error::ServiceConfig(windows::core::Error::from_hresult(status.to_hresult()))
}

/// Encodes a string as `REG_SZ` value data: UTF-16 (via `HSTRING`) plus the
/// trailing NUL terminator the registry expects. There is no windows-crate API
/// for value *data*, so only the registry-specific terminator is added here.
fn reg_sz_bytes(value: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = HSTRING::from(value)
        .as_wide()
        .iter()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    bytes.extend_from_slice(&[0, 0]);
    bytes
}

/// Returns an absolute DOS path string without the `\\?\` verbatim prefix that
/// `canonicalize` adds, so it can be wrapped in the NT `\??\` form.
fn normalize_sys_path(path: &Path) -> String {
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let s = abs.to_string_lossy().to_string();
    s.strip_prefix("\\\\?\\").map(str::to_string).unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_path_format() {
        let loader = DriverLoader::new("OpenProcmon24", "C:\\drv\\procmon.sys");
        assert_eq!(
            loader.registry_path(),
            "\\Registry\\Machine\\System\\CurrentControlSet\\Services\\OpenProcmon24"
        );
    }

    #[test]
    fn reg_sz_bytes_has_terminator() {
        // "A" -> 0x41 0x00 then NUL 0x00 0x00.
        assert_eq!(reg_sz_bytes("A"), vec![0x41, 0x00, 0x00, 0x00]);
    }
}
