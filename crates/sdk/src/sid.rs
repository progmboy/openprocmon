//! SID and integrity-level helpers (cf. C++ `StrMapUserNameFromSid` /
//! `StrMapIntegrityLevel`).

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::OnceLock;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, LocalFree, ERROR_INSUFFICIENT_BUFFER, HANDLE, HLOCAL,
};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{
    GetTokenInformation, LookupAccountSidW, TokenUser, PSID, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER,
};
use windows::Win32::System::SystemServices::{
    SECURITY_MANDATORY_HIGH_RID, SECURITY_MANDATORY_LOW_RID, SECURITY_MANDATORY_MEDIUM_RID,
    SECURITY_MANDATORY_PROTECTED_PROCESS_RID, SECURITY_MANDATORY_SYSTEM_RID,
    SECURITY_MANDATORY_UNTRUSTED_RID,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Resolves raw SID bytes to a `DOMAIN\User` string, or `None` if the account
/// cannot be looked up (e.g. an orphaned SID). The input must be a valid SID
/// blob as delivered by the driver.
///
/// Results (including failures) are cached by SID bytes, since `LookupAccountSidW`
/// is a relatively slow call and a system has only a handful of distinct user
/// SIDs that recur across many events.
pub fn account_name(sid: &[u8]) -> Option<String> {
    if sid.is_empty() {
        return None;
    }
    static CACHE: OnceLock<RwLock<HashMap<Vec<u8>, Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    if let Some(hit) = cache.read().get(sid) {
        return hit.clone();
    }
    let resolved = lookup_account_name(sid);
    cache.write().insert(sid.to_vec(), resolved.clone());
    resolved
}

/// Uncached `LookupAccountSidW` resolution; see [`account_name`].
fn lookup_account_name(sid: &[u8]) -> Option<String> {
    let psid = PSID(sid.as_ptr() as *mut core::ffi::c_void);
    let mut name_len: u32 = 0;
    let mut domain_len: u32 = 0;
    let mut use_kind = SID_NAME_USE(0);

    // First call sizes the two buffers; it is expected to fail with
    // ERROR_INSUFFICIENT_BUFFER while writing the required lengths.
    // SAFETY: `psid` points at the caller's valid SID bytes; both name pointers
    // are null on the sizing call, which the API permits.
    let sized = unsafe {
        LookupAccountSidW(
            PCWSTR::null(),
            psid,
            PWSTR::null(),
            &mut name_len,
            PWSTR::null(),
            &mut domain_len,
            &mut use_kind,
        )
    };
    if let Err(e) = sized {
        if e.code() != ERROR_INSUFFICIENT_BUFFER.to_hresult() {
            return None;
        }
    }
    if name_len == 0 {
        return None;
    }

    let mut name = vec![0u16; name_len as usize];
    let mut domain = vec![0u16; domain_len.max(1) as usize];
    // SAFETY: buffers are sized per the first call; lengths are in/out and valid.
    unsafe {
        LookupAccountSidW(
            PCWSTR::null(),
            psid,
            PWSTR(name.as_mut_ptr()),
            &mut name_len,
            PWSTR(domain.as_mut_ptr()),
            &mut domain_len,
            &mut use_kind,
        )
    }
    .ok()?;

    let name = String::from_utf16_lossy(&name[..name_len as usize]);
    let domain = String::from_utf16_lossy(&domain[..domain_len as usize]);
    if domain.is_empty() {
        Some(name)
    } else {
        Some(format!("{domain}\\{name}"))
    }
}

/// Maps an integrity-level RID to its display name (cf. `StrMapIntegrityLevel`).
pub fn integrity_level(rid: u32) -> &'static str {
    let rid = rid as i32;
    if rid >= SECURITY_MANDATORY_PROTECTED_PROCESS_RID {
        "Protected"
    } else if rid >= SECURITY_MANDATORY_SYSTEM_RID {
        "System"
    } else if rid >= SECURITY_MANDATORY_HIGH_RID {
        "High"
    } else if rid >= SECURITY_MANDATORY_MEDIUM_RID {
        "Medium"
    } else if rid >= SECURITY_MANDATORY_LOW_RID {
        "Low"
    } else if rid >= SECURITY_MANDATORY_UNTRUSTED_RID {
        "Untrusted"
    } else {
        "Unknown"
    }
}

/// Formats a logon-session `LUID` as Procmon does (`HighPart:LowPart`).
pub fn luid_string(high_part: i32, low_part: u32) -> String {
    format!("{:08x}:{:08x}", high_part as u32, low_part)
}

/// The current process user's SID in string form (e.g. `S-1-5-21-...`), cached
/// for the lifetime of the process. Used to fold `\REGISTRY\USER\<sid>` into the
/// `HKCU`/`HKCR` hives (cf. C++ `GetUserSid`).
pub fn current_user_sid_string() -> Option<&'static str> {
    static CACHE: OnceLock<Option<String>> = OnceLock::new();
    CACHE.get_or_init(query_current_user_sid).as_deref()
}

fn query_current_user_sid() -> Option<String> {
    let mut token = HANDLE::default();
    // SAFETY: querying the current process token; `token` receives the handle.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.ok()?;

    // First call sizes the TOKEN_USER buffer (it is expected to fail).
    let mut len = 0u32;
    // SAFETY: sizing call with a null buffer is permitted.
    let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut len) };
    let mut buf = vec![0u8; len as usize];
    // SAFETY: `buf` is `len` bytes; `len` is updated in place.
    let info = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr() as *mut _),
            len,
            &mut len,
        )
    };
    // SAFETY: closing the handle we opened.
    unsafe {
        let _ = CloseHandle(token);
    }
    info.ok()?;

    // SAFETY: `buf` holds a TOKEN_USER; its `User.Sid` points within `buf`.
    let token_user = unsafe { &*(buf.as_ptr() as *const TOKEN_USER) };
    let mut string_sid = PWSTR::null();
    // SAFETY: converts the SID to a LocalAlloc'd string written to `string_sid`.
    unsafe { ConvertSidToStringSidW(token_user.User.Sid, &mut string_sid) }.ok()?;
    if string_sid.is_null() {
        return None;
    }
    // SAFETY: `string_sid` is a valid NUL-terminated string from the call above.
    let result = unsafe { string_sid.to_string().ok() };
    // SAFETY: free the buffer allocated by ConvertSidToStringSidW.
    unsafe {
        let _ = LocalFree(HLOCAL(string_sid.0 as *mut core::ffi::c_void));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrity_names() {
        assert_eq!(integrity_level(0), "Untrusted");
        assert_eq!(integrity_level(4096), "Low");
        assert_eq!(integrity_level(8192), "Medium");
        assert_eq!(integrity_level(12288), "High");
        assert_eq!(integrity_level(16384), "System");
    }

    #[test]
    fn luid_format() {
        assert_eq!(luid_string(0, 0x3e7), "00000000:000003e7");
    }
}
