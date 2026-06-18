//! Loading and unloading the kernel miniFilter (cf. C++ `drvload.cxx`).
//!
//! Loading a filter driver from user mode requires: an elevated process, the
//! `SeLoadDriverPrivilege`, a service registry key describing the driver and its
//! minifilter instance/altitude, and finally `NtLoadDriver` against that key.

use crate::error::{Error, Result};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

use windows::core::{HSTRING, PCWSTR};
use windows::Wdk::System::SystemServices::{NtLoadDriver, NtUnloadDriver};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_SHARING_VIOLATION, HANDLE, NTSTATUS, STATUS_IMAGE_ALREADY_LOADED,
    STATUS_SUCCESS, UNICODE_STRING, WIN32_ERROR,
};
use windows::Win32::Security::{
    GetTokenInformation, TokenElevation, SE_LOAD_DRIVER_NAME, TOKEN_ELEVATION, TOKEN_QUERY,
};
use windows::Win32::Storage::FileSystem::{SetFileAttributesW, FILE_ATTRIBUTE_NORMAL};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteKeyW, RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE,
    KEY_WRITE, REG_DWORD, REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::System::Services::{
    SERVICE_DEMAND_START, SERVICE_ERROR_NORMAL, SERVICE_FILE_SYSTEM_DRIVER,
};
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// `STATUS_FLT_INSTANCE_ALTITUDE_COLLISION` — a different driver build already
/// occupies this minifilter altitude; replacing it requires a reboot.
const STATUS_FLT_INSTANCE_ALTITUDE_COLLISION: NTSTATUS = NTSTATUS(0xC01F_0011u32 as i32);

/// Service key location (relative to `HKEY_LOCAL_MACHINE`).
const SERVICE_KEY_ROOT: &str = "SYSTEM\\CurrentControlSet\\Services\\";
/// Service key as an NT object path, for `NtLoadDriver`.
const NT_SERVICE_PREFIX: &str = "\\Registry\\Machine\\System\\CurrentControlSet\\Services\\";
/// Minifilter instance name and altitude registered under `Instances`.
const INSTANCE_NAME: &str = "Process Monitor 24 Instance";
const ALTITUDE: &str = "385200";
/// Capability bitmask the user-mode app hands the driver via the service key's
/// `SupportedFeatures` value (read by the driver's `DriverEntry`). It must match
/// the *loaded driver's* expectation, so it is version/driver-specific:
/// `15` (0xF) for the genuine Process Monitor 24 driver currently used; switch to
/// `3` when loading our own driver (cf. `kernel/procmon.inf` `SupportedFeatures`).
const SUPPORTED_FEATURES: u32 = 15;
/// NT namespace prefix for the `ImagePath` value.
const IMAGE_PATH_PREFIX: &str = "\\??\\";

/// The driver image: an on-disk `.sys` path, or bytes embedded in the binary that
/// are dropped to `System32\Drivers` *only when the driver actually needs loading*.
enum Image {
    Path(PathBuf),
    Embedded {
        file_name: String,
        bytes: &'static [u8],
    },
}

/// Loads/unloads the OpenProcessMonitor driver by service `name` and image.
pub struct DriverLoader {
    name: String,
    image: Image,
}

impl DriverLoader {
    /// Creates a loader for a driver service `name` whose binary is at `sys_path`.
    pub fn new(name: impl Into<String>, sys_path: impl AsRef<Path>) -> Self {
        Self {
            name: name.into(),
            image: Image::Path(sys_path.as_ref().to_path_buf()),
        }
    }

    /// Creates a loader from an in-memory driver image (e.g. one embedded via
    /// `include_bytes!`). The bytes are **not** written here — extraction to
    /// `%SystemRoot%\System32\Drivers\<file_name>` is deferred to
    /// [`ensure_loaded`](Self::ensure_loaded), so a connect-first hit (driver
    /// already running) never touches the disk. Mirrors Process Monitor, which only
    /// drops its embedded `.sys` when it has to load.
    pub fn from_embedded(
        name: impl Into<String>,
        file_name: impl Into<String>,
        bytes: &'static [u8],
    ) -> Self {
        Self {
            name: name.into(),
            image: Image::Embedded {
                file_name: file_name.into(),
                bytes,
            },
        }
    }

    /// Ensures the driver is loaded: verifies elevation, enables the load-driver
    /// privilege, resolves the image path (extracting an embedded image to disk on
    /// demand), writes the service key, and calls `NtLoadDriver`. An already loaded
    /// driver is treated as success. Only called after a connect-first miss, so an
    /// embedded image is written to disk lazily — never when the port already exists.
    pub fn ensure_loaded(&self) -> Result<()> {
        if !is_elevated()? {
            return Err(Error::NotElevated);
        }
        crate::system::enable_privilege(SE_LOAD_DRIVER_NAME)?;
        let sys_path = self.resolve_sys_path()?;
        self.create_service_key(&sys_path)?;
        let result = load_driver(&self.registry_path());
        // Remove the system-generated noise subkeys whether or not the load
        // succeeded — Procmon cleans up regardless of NtLoadDriver's result. The
        // load outcome is surfaced afterwards.
        self.cleanup_service_subkeys();
        result
    }

    /// After a successful load, removes the noise subkeys the system auto-creates
    /// under the service key (`Enum`, `Security`) plus any `Parameters`, mirroring
    /// Process Monitor's post-load cleanup. The core service values
    /// (`Type`/`Start`/`ImagePath`) and the `Instances` subtree are kept, so the
    /// driver can still be unloaded via [`unload`](Self::unload). Best-effort: a
    /// missing subkey (e.g. `Parameters`, which we never create) is ignored.
    fn cleanup_service_subkeys(&self) {
        for sub in ["Enum", "Security", "Parameters"] {
            let path = format!("{SERVICE_KEY_ROOT}{}\\{sub}", self.name);
            // SAFETY: `&HSTRING` is a valid NUL-terminated subkey path; the result
            // is intentionally ignored (the subkey may not exist).
            unsafe {
                let _ = RegDeleteKeyW(HKEY_LOCAL_MACHINE, &HSTRING::from(path));
            }
        }
    }

    /// Resolves the on-disk `.sys` path, extracting an embedded image to
    /// `System32\Drivers` on demand (this is the only place an embedded image is
    /// written, so it happens lazily at load time).
    fn resolve_sys_path(&self) -> Result<Cow<'_, Path>> {
        match &self.image {
            Image::Path(p) => Ok(Cow::Borrowed(p)),
            Image::Embedded { file_name, bytes } => {
                Ok(Cow::Owned(extract_to_system32(file_name, bytes)?))
            }
        }
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
    fn create_service_key(&self, sys_path: &Path) -> Result<()> {
        let image_path = format!("{IMAGE_PATH_PREFIX}{}", normalize_sys_path(sys_path));

        let service = create_key(
            HKEY_LOCAL_MACHINE,
            &format!("{SERVICE_KEY_ROOT}{}", self.name),
        )?;
        let result = (|| {
            set_string(service, "ImagePath", &image_path, REG_EXPAND_SZ)?;
            set_dword(service, "Type", SERVICE_FILE_SYSTEM_DRIVER.0)?; // minifilter
            set_dword(service, "ErrorControl", SERVICE_ERROR_NORMAL.0)?;
            set_dword(service, "Start", SERVICE_DEMAND_START.0)?;
            set_dword(service, "SupportedFeatures", SUPPORTED_FEATURES)?;

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

/// Calls `NtLoadDriver`, treating "already loaded" as success and an altitude
/// collision (another driver version) as the distinct [`Error::OtherVersionLoaded`].
fn load_driver(registry_path: &str) -> Result<()> {
    let path = to_unicode(registry_path);
    // SAFETY: `path` is a valid UNICODE_STRING backed by a live buffer.
    let status = unsafe { NtLoadDriver(&path.value) };
    if status == STATUS_SUCCESS || status == STATUS_IMAGE_ALREADY_LOADED {
        Ok(())
    } else if status == STATUS_FLT_INSTANCE_ALTITUDE_COLLISION {
        Err(Error::OtherVersionLoaded)
    } else {
        Err(Error::DriverLoad(status))
    }
}

/// `%SystemRoot%\System32\Drivers` — where boot/runtime kernel drivers live.
fn system_drivers_dir() -> Result<PathBuf> {
    let mut buf = [0u16; 260];
    // SAFETY: `buf` is a valid writable buffer; the call fills it and returns the
    // number of characters written (0 on failure).
    let len = unsafe { GetSystemDirectoryW(Some(&mut buf)) } as usize;
    if len == 0 || len > buf.len() {
        return Err(Error::DriverExtract(std::io::Error::last_os_error()));
    }
    Ok(PathBuf::from(String::from_utf16_lossy(&buf[..len])).join("Drivers"))
}

/// Writes a driver image to `%SystemRoot%\System32\Drivers\<file_name>` and returns
/// its path. Mirrors Process Monitor's resource extraction: it clears any
/// HIDDEN/SYSTEM attributes first (so an existing image can be overwritten) and
/// treats a sharing violation — the file is locked because the driver is already
/// loaded — as success, reusing the on-disk path. The file is intentionally left
/// in place (newer Process Monitor builds keep it too).
pub fn extract_to_system32(file_name: &str, bytes: &[u8]) -> Result<PathBuf> {
    let path = system_drivers_dir()?.join(file_name);
    let wide = HSTRING::from(path.to_string_lossy().as_ref());
    // Best-effort: clear attributes so a previously-dropped HIDDEN/SYSTEM/READONLY
    // image can be overwritten (cf. Procmon's SetFileAttributes(NORMAL)). The file
    // may not exist yet, so the result is ignored.
    // SAFETY: `wide` is a valid NUL-terminated path string.
    unsafe {
        let _ = SetFileAttributesW(&wide, FILE_ATTRIBUTE_NORMAL);
    }
    match std::fs::write(&path, bytes) {
        Ok(()) => Ok(path),
        // The image is present and locked because the driver is already loaded —
        // reuse it (cf. Procmon's `GetLastError() == ERROR_SHARING_VIOLATION`).
        Err(e) if e.raw_os_error() == Some(ERROR_SHARING_VIOLATION.0 as i32) => Ok(path),
        Err(e) => Err(Error::DriverExtract(e)),
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

    #[test]
    fn drivers_dir_under_system32() {
        // GetSystemDirectory needs no privileges, so this is safe in CI.
        let dir = system_drivers_dir().expect("system directory");
        assert!(dir.ends_with("Drivers"), "got {dir:?}");
    }
}
