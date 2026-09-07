//! Application configuration edited by the Settings dialog (design `settings.jsx`).
//!
//! Theme/locale live elsewhere (`AppState.theme_mode` + the i18n locale); this
//! holds the rest. Some fields are consumed now (highlight color, hex display);
//! the others (symbols, history, profiling, boot) are stored for the SDK backend.

use gpui_kit::Hsla;
use serde::{Deserialize, Serialize};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

/// The highlight row tint (design Appearance ▸ Highlight Color palette).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum HighlightColor {
    Amber,
    Blue,
    Green,
    Red,
    Purple,
    Cyan,
}

impl HighlightColor {
    pub const ALL: [HighlightColor; 6] = [
        HighlightColor::Amber,
        HighlightColor::Blue,
        HighlightColor::Green,
        HighlightColor::Red,
        HighlightColor::Purple,
        HighlightColor::Cyan,
    ];

    /// The design swatch color (`settings.jsx` `HL_COLORS`).
    pub fn rgb(self) -> u32 {
        match self {
            HighlightColor::Amber => 0xf0c36b,
            HighlightColor::Blue => 0x4f8cf7,
            HighlightColor::Green => 0x6ee59a,
            HighlightColor::Red => 0xf0816b,
            HighlightColor::Purple => 0xb794f6,
            HighlightColor::Cyan => 0x34d3c0,
        }
    }

    pub fn hsla(self) -> Hsla {
        gpui_kit::rgb(self.rgb()).into()
    }
}

/// Thread-profiling sample interval (design Profiling panel).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ProfilingInterval {
    OneSecond,
    HundredMs,
}

/// All Settings-dialog configuration (excluding theme/locale).
///
/// `#[serde(default)]` keeps loading resilient: missing/old fields fall back to the
/// per-field default, so a partial or older `config.json` still loads.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub highlight_color: HighlightColor,
    pub hex_file_offset: bool,
    pub hex_thread_proc_id: bool,
    pub profiling_enabled: bool,
    pub profiling_interval: ProfilingInterval,
    pub boot_capture: bool,
    pub symbols_path: String,
    pub dbghelp_path: String,
    pub history_ring: bool,
    pub history_mb: usize,
    pub history_min: usize,
    /// Absolute path to the main-window background image; empty means none.
    pub bg_image: String,
    /// Background image opacity in percent. Kept low by default so text stays
    /// legible over it (design Appearance ▸ Background Opacity).
    pub bg_opacity: u8,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            highlight_color: HighlightColor::Amber,
            hex_file_offset: false,
            hex_thread_proc_id: false,
            profiling_enabled: false,
            profiling_interval: ProfilingInterval::OneSecond,
            boot_capture: false,
            symbols_path: "srv*C:\\Symbols*https://msdl.microsoft.com/download/symbols".to_string(),
            dbghelp_path: "C:\\Program Files\\Windows Kits\\10\\Debuggers\\x64\\dbghelp.dll"
                .to_string(),
            history_ring: false,
            history_mb: 512,
            history_min: 60,
            bg_image: String::new(),
            bg_opacity: 35,
        }
    }
}

/// `%USERPROFILE%\openprocmon\config.json` — the per-user config location.
pub fn config_path() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE")?;
    Some(PathBuf::from(home).join("openprocmon").join("config.json"))
}

impl AppConfig {
    /// Loads the persisted config from [`config_path`], falling back to defaults on
    /// any error (missing file, parse failure). The result is sanitized.
    ///
    /// SECURITY: this file lives in a user-writable directory, so its contents are
    /// **untrusted** — a lower-integrity process running as the same user can edit
    /// it. Numeric ranges are clamped here as defense-in-depth. Note `dbghelp_path`
    /// is `LoadLibrary`d as-is (user-selected, by design), so a tampered config could
    /// point it at a malicious DLL — that path is treated as user-trusted input.
    pub fn load() -> Self {
        config_path()
            .and_then(|p| Self::load_from(&p).ok())
            .unwrap_or_default()
    }

    /// Reads and parses a config from `path` (used by Import). Sanitized on success.
    pub fn load_from(path: &Path) -> Result<Self, String> {
        let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut cfg: Self = serde_json::from_str(&data).map_err(|e| e.to_string())?;
        cfg.sanitize();
        Ok(cfg)
    }

    /// Persists the config to [`config_path`] (called when Settings are applied).
    pub fn save(&self) -> io::Result<()> {
        let path = config_path()
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "no user profile directory"))?;
        self.save_to(&path)
    }

    /// Writes the config to `path` as pretty JSON (used by Export and [`save`]).
    ///
    /// SECURITY: the write is performed as the logged-on user (see [`write_as_user`])
    /// — when elevated we impersonate the user's medium-integrity token first. So if a
    /// lower-integrity process plants a junction/symlink to redirect our write onto a
    /// protected file, the OS access check runs as the user and the write simply fails
    /// with access-denied. This closes that elevation-of-privilege "confused deputy"
    /// without the check-then-write TOCTOU of validating the path up front.
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;
        write_as_user(path, json.as_bytes())
    }

    /// Clamps untrusted numeric fields to sane ranges (defense-in-depth for values
    /// read from the user-writable config file).
    fn sanitize(&mut self) {
        self.history_mb = self.history_mb.max(1);
        self.history_min = self.history_min.max(1);
        self.bg_opacity = self.bg_opacity.clamp(BG_OPACITY_MIN, BG_OPACITY_MAX);
    }

    /// The background image to paint, or `None` when none is configured or the
    /// file is no longer there.
    ///
    /// A path that stops resolving falls back to "no background" rather than to
    /// an error: the image is decoration and the file can disappear at any time
    /// (the user deletes it, a removable drive goes away). This only reports;
    /// [`resolve_background_image`](Self::resolve_background_image) is the one
    /// that also forgets a dead path.
    pub fn background_image(&self) -> Option<PathBuf> {
        let path = Path::new(self.bg_image.trim());
        (!self.bg_image.trim().is_empty() && path.is_file()).then(|| path.to_path_buf())
    }

    /// [`background_image`](Self::background_image), forgetting a path that no
    /// longer resolves.
    ///
    /// Call this where the config is taken on (load, Apply) rather than per
    /// frame — it touches the filesystem. Dropping the dead path is what puts the
    /// app back in the plain no-background state everywhere at once: the window
    /// stops thinning its chrome and the Settings dialog stops naming a file that
    /// is not there.
    pub fn resolve_background_image(&mut self) -> Option<PathBuf> {
        let resolved = self.background_image();
        if resolved.is_none() {
            self.bg_image.clear();
        }
        resolved
    }
}

/// Opacity range of the background image, in percent (design slider bounds).
pub const BG_OPACITY_MIN: u8 = 5;
pub const BG_OPACITY_MAX: u8 = 100;

/// Writes `bytes` to `path` (creating parent directories) as the logged-on user.
///
/// When the process is fully elevated (a UAC-split admin's high token), the write is
/// done while impersonating the same user's medium-integrity *linked* token, so the
/// file-system access check runs as the user, not as admin — closing the confused-
/// deputy elevation-of-privilege described on [`AppConfig::save_to`]. When the process
/// is not elevated, the current token already is the user's, so it writes directly.
fn write_as_user(path: &Path, bytes: &[u8]) -> io::Result<()> {
    // The guard impersonates for the duration of the write and reverts on drop
    // (even on error). `None` => not elevated, write with the current token.
    #[cfg(windows)]
    let _guard = match elevated_linked_token() {
        Some(token) => Some(Impersonation::begin(token)?),
        None => None,
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, bytes)
}

#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    Security::{
        GetTokenInformation, ImpersonateLoggedOnUser, RevertToSelf, TokenElevationType,
        TokenElevationTypeFull, TokenLinkedToken, TOKEN_DUPLICATE, TOKEN_ELEVATION_TYPE,
        TOKEN_LINKED_TOKEN, TOKEN_QUERY,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

/// The current user's medium-integrity *linked* token, but only when the process is
/// fully elevated (so the linked token is the lower-privileged one). `None` on any
/// failure or when not fully elevated — the caller then writes with the current token.
///
/// The elevation-type check is essential: on a non-elevated token, `TokenLinkedToken`
/// would return the *higher* token, and impersonating that would elevate the write.
#[cfg(windows)]
fn elevated_linked_token() -> Option<HANDLE> {
    // SAFETY: standard token queries; the process token is closed before returning and
    // the returned linked-token handle is owned by the caller (closed via Drop).
    unsafe {
        let mut process: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY | TOKEN_DUPLICATE,
            &mut process,
        ) == 0
        {
            return None;
        }
        let mut etype: TOKEN_ELEVATION_TYPE = 0;
        let mut len = 0u32;
        let ok = GetTokenInformation(
            process,
            TokenElevationType,
            &mut etype as *mut _ as *mut core::ffi::c_void,
            core::mem::size_of::<TOKEN_ELEVATION_TYPE>() as u32,
            &mut len,
        );
        if ok == 0 || etype != TokenElevationTypeFull {
            CloseHandle(process);
            return None;
        }
        let mut linked = TOKEN_LINKED_TOKEN {
            LinkedToken: std::ptr::null_mut(),
        };
        let mut len2 = 0u32;
        let ok2 = GetTokenInformation(
            process,
            TokenLinkedToken,
            &mut linked as *mut _ as *mut core::ffi::c_void,
            core::mem::size_of::<TOKEN_LINKED_TOKEN>() as u32,
            &mut len2,
        );
        CloseHandle(process);
        (ok2 != 0).then_some(linked.LinkedToken)
    }
}

/// RAII thread impersonation: owns the linked token, reverts and closes it on drop.
#[cfg(windows)]
struct Impersonation {
    token: HANDLE,
}

#[cfg(windows)]
impl Impersonation {
    fn begin(token: HANDLE) -> io::Result<Self> {
        // SAFETY: impersonating a valid linked-token handle on the current thread.
        if unsafe { ImpersonateLoggedOnUser(token) } == 0 {
            let err = io::Error::last_os_error();
            // SAFETY: close the token we own before bailing out.
            unsafe { CloseHandle(token) };
            return Err(err);
        }
        Ok(Self { token })
    }
}

#[cfg(windows)]
impl Drop for Impersonation {
    fn drop(&mut self) {
        // SAFETY: revert the impersonation begun in `begin`, then close the token.
        unsafe {
            RevertToSelf();
            CloseHandle(self.token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The image is decoration and its file can vanish under us, so a path that
    /// stops resolving has to put the app back in the plain no-background state
    /// rather than leave a half-applied setting behind.
    #[test]
    fn a_deleted_background_image_falls_back_to_none() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        let mut config = AppConfig {
            bg_image: file.path().to_string_lossy().to_string(),
            ..Default::default()
        };
        assert_eq!(
            config.resolve_background_image(),
            Some(file.path().to_path_buf())
        );

        drop(file); // the user deletes the image
        assert_eq!(config.resolve_background_image(), None);
        assert!(
            config.bg_image.is_empty(),
            "the dead path is forgotten, so the Settings dialog stops naming it"
        );
    }

    #[test]
    fn a_blank_background_path_is_no_background() {
        let mut config = AppConfig {
            bg_image: "   ".to_string(),
            ..Default::default()
        };
        assert_eq!(config.resolve_background_image(), None);
    }

    /// `config.json` is user-writable, so its opacity is untrusted (see [`load`]).
    #[test]
    fn background_opacity_from_the_config_file_is_clamped() {
        let mut config = AppConfig {
            bg_opacity: 240,
            ..Default::default()
        };
        config.sanitize();
        assert_eq!(config.bg_opacity, BG_OPACITY_MAX);

        config.bg_opacity = 0;
        config.sanitize();
        assert_eq!(config.bg_opacity, BG_OPACITY_MIN);
    }
}
