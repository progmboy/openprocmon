//! App-specific icon set + asset source.
//!
//! gpui-component's `IconName`/`Icon` are Lucide glyphs bundled by
//! `gpui-component-assets`. The Process Monitor design ships its *own* curated
//! icon set (`docs/design/gui-design-v2/icons.jsx`) that differs from Lucide — a
//! floppy-disk save, a play/pause capture toggle, a funnel filter, dedicated
//! per-category glyphs, etc. We embed those SVGs under `assets/icons/pm-*.svg`
//! and expose them as [`PmIcon`], which implements gpui-component's [`IconNamed`]
//! trait so it drops into `Icon::new(..)` / raw `svg().path(..)` exactly like the
//! built-in enum.
//!
//! [`Assets`] embeds our SVGs and falls back to `gpui-component-assets` for the
//! glyphs gpui-component's own components load internally (Select chevrons, the
//! Input clear button, scrollbars, dialog close, …).

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};
use gpui_component::IconNamed;
use rust_embed::RustEmbed;

/// Embeds this crate's `assets/` folder (the design's SVG icon set).
#[derive(RustEmbed)]
#[folder = "assets"]
#[include = "icons/**/*.svg"]
struct GuiAssets;

/// The application asset source: our embedded icons first, gpui-component's
/// bundled Lucide icons as the fallback for built-in component glyphs.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        if let Some(f) = GuiAssets::get(path) {
            return Ok(Some(f.data));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut out: Vec<SharedString> = GuiAssets::iter()
            .filter_map(|p| p.starts_with(path).then(|| p.into()))
            .collect();
        out.extend(gpui_component_assets::Assets.list(path)?);
        Ok(out)
    }
}

/// The design's icon set. Each variant maps to an embedded `assets/icons/pm-*.svg`.
///
/// This mirrors the full design catalog (`icons.jsx`); a few glyphs are not wired
/// into a view yet (detail-field accents, context-menu actions), so the catalog is
/// intentionally complete rather than minimal.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PmIcon {
    // Toolbar.
    Open,
    Save,
    Play,
    Pause,
    Scroll,
    Trash,
    Filter,
    FilterFill,
    Highlight,
    Crosshair,
    Tree,
    Search,
    Jump,
    Sun,
    Moon,
    // Monitor categories.
    Registry,
    Filesys,
    Network,
    ProcThread,
    Perf,
    // Detail / dialogs / misc.
    FileText,
    X,
    Copy,
    Plus,
    Minus,
    Chevron,
    ChevronDown,
    Props,
    Check,
    Clock,
    Cpu,
    Layers,
    Info,
    Refresh,
    User,
    // Menu / settings glyphs.
    Download,
    Upload,
    Logout,
    Ban,
    Bookmark,
    Globe,
    Pin,
    Help,
    Settings,
    Palette,
    Power,
    Hash,
}

impl IconNamed for PmIcon {
    fn path(self) -> SharedString {
        use PmIcon::*;
        let name = match self {
            Open => "open",
            Save => "save",
            Play => "play",
            Pause => "pause",
            Scroll => "scroll",
            Trash => "trash",
            Filter => "filter",
            FilterFill => "filter-fill",
            Highlight => "highlight",
            Crosshair => "crosshair",
            Tree => "tree",
            Search => "search",
            Jump => "jump",
            Sun => "sun",
            Moon => "moon",
            Registry => "registry",
            Filesys => "filesys",
            Network => "network",
            ProcThread => "procthread",
            Perf => "perf",
            FileText => "file-text",
            X => "x",
            Copy => "copy",
            Plus => "plus",
            Minus => "minus",
            Chevron => "chevron",
            ChevronDown => "chevron-down",
            Props => "props",
            Check => "check",
            Clock => "clock",
            Cpu => "cpu",
            Layers => "layers",
            Info => "info",
            Refresh => "refresh",
            User => "user",
            Download => "download",
            Upload => "upload",
            Logout => "logout",
            Ban => "ban",
            Bookmark => "bookmark",
            Globe => "globe",
            Pin => "pin",
            Help => "help",
            Settings => "settings",
            Palette => "palette",
            Power => "power",
            Hash => "hash",
        };
        format!("icons/pm-{name}.svg").into()
    }
}
