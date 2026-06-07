//! Application configuration edited by the Settings dialog (design `settings.jsx`).
//!
//! Theme/locale live elsewhere (`AppState.theme_mode` + the i18n locale); this
//! holds the rest. Some fields are consumed now (highlight color, hex display);
//! the others (symbols, history, profiling, boot) are stored for the SDK backend.

use gpui::Hsla;

/// The highlight row tint (design Appearance ▸ Highlight Color palette).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
        gpui::rgb(self.rgb()).into()
    }
}

/// Thread-profiling sample interval (design Profiling panel).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProfilingInterval {
    OneSecond,
    HundredMs,
}

/// All Settings-dialog configuration (excluding theme/locale).
#[derive(Clone, Debug)]
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
        }
    }
}
