//! NT path normalization (cf. C++ `utils.cxx`).
//!
//! The driver reports file paths as kernel-internal NT device paths (e.g.
//! `\Device\HarddiskVolume3\Windows\notepad.exe`) and registry paths in their
//! `\REGISTRY\MACHINE\...` form. These helpers convert them to the familiar DOS
//! (`C:\...`) and hive (`HKLM\...`) forms shown to users.

use parking_lot::RwLock;
use std::sync::OnceLock;

use windows::core::HSTRING;
use windows::Win32::Storage::FileSystem::{GetLogicalDriveStringsW, QueryDosDeviceW};
use windows::Win32::System::SystemInformation::GetWindowsDirectoryW;

/// Special NT path prefixes that map to a DOS form without a volume lookup.
const REDIRECTOR_PREFIXES: &[&str] = &["\\Device\\LanmanRedirector\\", "\\Device\\Mup\\"];
const DOSDEVICES_PREFIX: &str = "\\??\\";
const SYSTEMROOT_PREFIX: &str = "\\SystemRoot\\";

/// Cached volume table: NT volume device names (`\Device\HarddiskVolumeN`) to DOS
/// drive letters (`C:`), plus the resolved `\SystemRoot` directory.
struct VolumeState {
    /// Device name -> drive letter.
    entries: Vec<(String, String)>,
    system_root: String,
}

/// Process-wide volume cache, lazily built and refreshed on a miss — mirroring
/// the static `gVolumeCache` inside C++ `UtilConvertNtInternalPathToDosPath`.
fn volume_state() -> &'static RwLock<VolumeState> {
    static STATE: OnceLock<RwLock<VolumeState>> = OnceLock::new();
    STATE.get_or_init(|| RwLock::new(load_volume_state()))
}

/// Enumerates the current drive letters and the Windows directory.
fn load_volume_state() -> VolumeState {
    let mut entries = Vec::new();
    for drive in logical_drives() {
        // Strip the trailing "\" so "C:\" becomes the "C:" device alias.
        let alias = drive.trim_end_matches('\\');
        if let Some(device) = query_dos_device(alias) {
            entries.push((device, alias.to_string()));
        }
    }
    VolumeState {
        entries,
        system_root: windows_directory(),
    }
}

/// Converts an NT internal path to its DOS form (cf. C++
/// `UtilConvertNtInternalPathToDosPath`). Uses a process-wide volume cache; on a
/// `\Device\...` volume miss it refreshes the cache once and retries (a drive may
/// have been mounted since). Returns the input unchanged if no rule applies.
pub fn nt_to_dos(nt_path: &str) -> String {
    {
        let state = volume_state().read();
        if let Some(dos) = convert_with(nt_path, &state.entries, &state.system_root) {
            return dos;
        }
    }
    // Unknown volume: rebuild the cache and try once more.
    *volume_state().write() = load_volume_state();
    let state = volume_state().read();
    convert_with(nt_path, &state.entries, &state.system_root).unwrap_or_else(|| nt_path.to_string())
}

/// One conversion attempt against a given volume table; `None` only for a
/// `\Device\...` volume path absent from `entries` (so the caller can refresh).
fn convert_with(nt_path: &str, entries: &[(String, String)], system_root: &str) -> Option<String> {
    for prefix in REDIRECTOR_PREFIXES {
        if let Some(rest) = strip_prefix_ci(nt_path, prefix) {
            return Some(format!("\\\\{rest}"));
        }
    }
    if let Some(rest) = strip_prefix_ci(nt_path, SYSTEMROOT_PREFIX) {
        return Some(format!("{}\\{rest}", system_root.trim_end_matches('\\')));
    }
    if let Some(rest) = strip_prefix_ci(nt_path, DOSDEVICES_PREFIX) {
        return Some(rest.to_string());
    }

    for (device, letter) in entries {
        // Match "\Device\HarddiskVolumeN" followed by a path separator (or end)
        // so volume 1 does not accidentally match volume 10.
        if let Some(rest) = strip_prefix_ci(nt_path, device) {
            if rest.is_empty() || rest.starts_with('\\') {
                return Some(format!("{letter}{rest}"));
            }
        }
    }

    if !nt_path.starts_with("\\Device\\") {
        // Already a DOS path or an unrecognized non-volume path: pass through.
        return Some(nt_path.to_string());
    }
    None
}

/// Converts a kernel-internal registry path (`\REGISTRY\MACHINE\...`) to the hive
/// form (`HKLM\...`, `HKCU\...`, `HKCR\...`, `HKU\<sid>\...`) shown by Procmon.
/// Returns the input unchanged if it has no known prefix.
pub fn reg_internal_to_normal(path: &str) -> String {
    reg_normalize(path, crate::sid::current_user_sid_string())
}

/// Core of [`reg_internal_to_normal`], parameterized by the current user's SID so
/// it can be unit-tested without querying the process token (cf. C++
/// `UtilConvertRegInternalToNormal`).
fn reg_normalize(path: &str, current_sid: Option<&str>) -> String {
    let Some(rest) = strip_prefix_ci(path, "\\REGISTRY\\") else {
        return path.to_string();
    };

    if let Some(machine) = strip_prefix_ci(rest, "MACHINE") {
        return format!("HKLM{machine}");
    }

    if let Some(user) = strip_prefix_ci(rest, "USER") {
        let user = user.trim_start_matches('\\');
        // `\REGISTRY\USER\<current-sid>` is HKCU; its `_Classes` subtree is HKCR;
        // any other SID stays under HKU.
        if let Some(sid) = current_sid {
            if let Some(after) = strip_prefix_ci(user, sid) {
                if let Some(classes) = strip_prefix_ci(after, "_Classes") {
                    return format!("HKCR{classes}");
                }
                if after.is_empty() || after.starts_with('\\') {
                    return format!("HKCU{after}");
                }
                // `sid` was only a prefix of a longer (different) SID: fall through.
            }
        }
        return if user.is_empty() {
            "HKU".to_string()
        } else {
            format!("HKU\\{user}")
        };
    }

    path.to_string()
}

/// Case-insensitive prefix strip (NT paths are case-insensitive). The prefixes
/// are ASCII, so this compares raw bytes — `s` may contain multi-byte UTF-8 and
/// `prefix.len()` need not fall on a char boundary. Since the prefix is ASCII,
/// a byte-length match also lands on a char boundary in `s`, so the `str` slice
/// of the remainder is valid.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.as_bytes().get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix.as_bytes()) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// Enumerates drive root strings (e.g. `C:\`, `D:\`).
fn logical_drives() -> Vec<String> {
    let mut buf = [0u16; 512];
    // SAFETY: `buf` is a valid writable buffer of the length we pass.
    let len = unsafe { GetLogicalDriveStringsW(Some(&mut buf)) } as usize;
    if len == 0 {
        return Vec::new();
    }
    buf[..len]
        .split(|&c| c == 0)
        .filter(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// Resolves a DOS device alias (e.g. `C:`) to its NT target (e.g.
/// `\Device\HarddiskVolume3`).
fn query_dos_device(alias: &str) -> Option<String> {
    let mut buf = [0u16; 512];
    // SAFETY: `&HSTRING` is a valid device name; `buf` is a writable buffer.
    let len = unsafe { QueryDosDeviceW(&HSTRING::from(alias), Some(&mut buf)) } as usize;
    if len == 0 {
        return None;
    }
    // The result is double-NUL terminated; take up to the first NUL.
    let end = buf.iter().position(|&c| c == 0).unwrap_or(len);
    Some(String::from_utf16_lossy(&buf[..end]))
}

/// Returns the Windows directory (target of `\SystemRoot`).
fn windows_directory() -> String {
    let mut buf = [0u16; 260];
    // SAFETY: `buf` is a valid writable buffer of the length we pass.
    let len = unsafe { GetWindowsDirectoryW(Some(&mut buf)) } as usize;
    String::from_utf16_lossy(&buf[..len.min(buf.len())])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an owned volume table for offline testing of [`convert_with`].
    fn table(entries: &[(&str, &str)]) -> Vec<(String, String)> {
        entries
            .iter()
            .map(|(d, l)| (d.to_string(), l.to_string()))
            .collect()
    }

    #[test]
    fn resolves_volume_to_drive_letter() {
        let vols = table(&[("\\Device\\HarddiskVolume3", "C:")]);
        assert_eq!(
            convert_with(
                "\\Device\\HarddiskVolume3\\Windows\\notepad.exe",
                &vols,
                "C:\\Windows"
            ),
            Some("C:\\Windows\\notepad.exe".to_string())
        );
    }

    #[test]
    fn does_not_match_partial_volume_number() {
        let vols = table(&[("\\Device\\HarddiskVolume1", "C:")]);
        // Volume 1 must not match volume 10's path -> a miss (None) so the caller
        // refreshes rather than producing a wrong drive letter.
        assert_eq!(
            convert_with("\\Device\\HarddiskVolume10\\x", &vols, "C:\\Windows"),
            None
        );
    }

    #[test]
    fn resolves_systemroot_and_dosdevices() {
        assert_eq!(
            convert_with("\\SystemRoot\\system32\\ntdll.dll", &[], "C:\\Windows"),
            Some("C:\\Windows\\system32\\ntdll.dll".to_string())
        );
        assert_eq!(
            convert_with("\\??\\C:\\temp\\a.txt", &[], "C:\\Windows"),
            Some("C:\\temp\\a.txt".to_string())
        );
    }

    #[test]
    fn resolves_network_redirector() {
        assert_eq!(
            convert_with("\\Device\\Mup\\server\\share\\f", &[], "C:\\Windows"),
            Some("\\\\server\\share\\f".to_string())
        );
    }

    #[test]
    fn multibyte_path_does_not_panic_on_prefix_boundary() {
        // A path whose multi-byte char straddles a prefix length must not panic
        // (regression: byte-indexed slicing inside `strip_prefix_ci`). 🌏 is 4
        // bytes; place it so it crosses the `\Device\Mup\` / volume prefix lengths.
        // A `\Device\` path with no matching volume legitimately returns None
        // (the caller then refreshes); the point is it doesn't panic.
        assert_eq!(
            convert_with("\\Device\\🌏weird\\path", &[], "C:\\Windows"),
            None
        );
        // A non-\Device path with an emoji passes through unchanged.
        let q = "🌏\\some\\path";
        assert_eq!(convert_with(q, &[], "C:\\Windows"), Some(q.to_string()));
        // And the registry side, with the emoji crossing the `\REGISTRY\` length.
        assert_eq!(reg_normalize("\\REGISTR🌏nope", None), "\\REGISTR🌏nope");
        assert_eq!(reg_normalize("🌏", None), "🌏");

        // Same class of bug for 3-byte CJK: `中` at bytes 8..11 straddles the
        // `\REGISTRY\` prefix length (10), and a CJK segment crossing a volume /
        // device prefix length must also pass through rather than panic.
        assert_eq!(reg_normalize("\\REGISTR中文", None), "\\REGISTR中文");
        assert_eq!(
            reg_normalize("\\REGISTRY\\MACHINE\\软件\\中文键", None),
            "HKLM\\软件\\中文键"
        );
        assert_eq!(
            convert_with("\\Device\\中文卷\\路径", &[], "C:\\Windows"),
            None
        );
        let cjk = "C:\\用户\\文档\\文件.txt";
        assert_eq!(convert_with(cjk, &[], "C:\\Windows"), Some(cjk.to_string()));
    }

    #[test]
    fn registry_hive_abbreviation() {
        let sid = Some("S-1-5-21-1-2-3-1000");
        assert_eq!(
            reg_normalize("\\REGISTRY\\MACHINE\\SOFTWARE", sid),
            "HKLM\\SOFTWARE"
        );
        // A different user's SID stays under HKU.
        assert_eq!(
            reg_normalize("\\REGISTRY\\USER\\S-1-5-18", sid),
            "HKU\\S-1-5-18"
        );
        // The current user's SID folds into HKCU, and its `_Classes` into HKCR.
        assert_eq!(
            reg_normalize("\\REGISTRY\\USER\\S-1-5-21-1-2-3-1000\\Console", sid),
            "HKCU\\Console"
        );
        assert_eq!(
            reg_normalize("\\REGISTRY\\USER\\S-1-5-21-1-2-3-1000_Classes\\CLSID", sid),
            "HKCR\\CLSID"
        );
        assert_eq!(reg_normalize("unchanged", sid), "unchanged");
    }
}
