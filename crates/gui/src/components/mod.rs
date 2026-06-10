//! The fixed window-chrome regions: menu bar, toolbar, monitor bar, status bar
//! (and, in later milestones, the event table and detail panel).
//!
//! Each region exposes a `render(..)` that returns an `impl IntoElement`. Regions
//! are pure render + event-dispatch: they read [`crate::app::AppState`] and wire
//! interactions to [`crate::app::AppView`] methods via `cx.listener`, keeping all
//! state mutation in `app.rs`.

pub(crate) mod detail_panel;
pub(crate) mod event_table;
pub(crate) mod menubar;
pub(crate) mod monitorbar;
pub(crate) mod statusbar;
pub(crate) mod toolbar;

use std::sync::Arc;

use gpui::{
    div, img, prelude::FluentBuilder, px, white, AnyElement, App, Div, Hsla, Image, ImageFormat,
    InteractiveElement, IntoElement, ParentElement, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled,
};
use gpui_component::{
    scroll::{Scrollbar, ScrollbarAxis, ScrollbarShow},
    ActiveTheme, StyledExt,
};

/// The application's own icon — the embedded `.ico` Explorer/the taskbar show
/// (`assets/res/icon1.ico`, the same file `build.rs` embeds as the exe icon). Used
/// for the title-bar brand mark and the About card so they match the app identity.
pub(crate) const APP_ICON: &[u8] = include_bytes!("../../assets/res/icon2.ico");

/// Renders the application icon as a rounded square of `size` px (radius = 25% of
/// the edge, matching the design's 4px@16 brand mark and 14px@56 About logo).
pub(crate) fn brand_icon(size: f32) -> impl IntoElement {
    img(Arc::new(Image::from_bytes(
        ImageFormat::Ico,
        APP_ICON.to_vec(),
    )))
    .size(px(size))
    .rounded(px(size * 0.25))
    .flex_shrink_0()
}

/// Wraps raw process-icon bytes (`RT_ICON`/`ICONIMAGE` or a full `.ico`) as a
/// renderable [`gpui::Image`]. Do this ONCE per icon and reuse the `Arc`:
/// assembling the `.ico` and content-hashing it on every frame is what the
/// per-row caches exist to avoid.
pub(crate) fn app_image(bytes: &[u8]) -> Arc<Image> {
    Arc::new(Image::from_bytes(ImageFormat::Ico, ico_bytes(bytes)))
}

/// The shell's default `.exe` icon as a prepared [`gpui::Image`], resolved and
/// wrapped once per process lifetime.
fn default_app_image() -> Option<Arc<Image>> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Option<Arc<Image>>> = OnceLock::new();
    CACHE
        .get_or_init(|| crate::sysicon::default_app_icon().map(|b| app_image(&b)))
        .clone()
}

/// Renders a process app-icon: the prepared image when present, else the shell's
/// default `.exe` icon (cf. C++ `UtilGetDefaultIcon`), and only if even that is
/// unavailable a colored rounded square with the name's first letter. `size` is
/// the square's edge in px. Callers pass an already-wrapped [`app_image`] (cached
/// per row/node), so rendering a frame never re-assembles or re-hashes icon bytes.
pub(crate) fn app_icon(
    icon: Option<&Arc<Image>>,
    name: &str,
    color: Hsla,
    size: f32,
) -> AnyElement {
    let edge = px(size);
    let radius = px((size * 0.25).clamp(3., 6.));
    // Fall back to the system default `.exe` icon when the process carries none
    // (a render-time overlay — it is never persisted to a PML).
    match icon.cloned().or_else(default_app_image) {
        Some(image) => img(image)
            .size(edge)
            .rounded(radius)
            .flex_shrink_0()
            .into_any_element(),
        None => {
            let letter = name
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase())
                .unwrap_or(' ')
                .to_string();
            div()
                .size(edge)
                .flex_shrink_0()
                .rounded(radius)
                .flex()
                .items_center()
                .justify_center()
                .bg(color)
                .text_color(white())
                .text_size(px(size * 0.55))
                .font_bold()
                .child(letter)
                .into_any_element()
        }
    }
}

/// Normalizes process-icon bytes into a complete `.ico` for gpui's ICO decoder.
///
/// A real `.ico` file starts with `ICONDIR` (`00 00 01 00`), but the SDK live
/// backend and the PML reader hand over raw `RT_ICON` /
/// `ICONIMAGE` resource bytes (a bare `BITMAPINFOHEADER`+pixels, or a PNG) with no
/// directory header — gpui can't decode those. We detect the latter and wrap them
/// in a one-entry `.ico`. The result is deterministic, so gpui's content-hash
/// decode cache still applies.
pub(crate) fn ico_bytes(bytes: &[u8]) -> Vec<u8> {
    // A real .ico begins with ICONDIR: reserved=0, type=1 (little-endian u16s).
    let is_ico =
        bytes.len() >= 4 && bytes[0] == 0 && bytes[1] == 0 && bytes[2] == 1 && bytes[3] == 0;
    if is_ico {
        return bytes.to_vec();
    }

    // Derive the directory entry's width/height/planes/bit-depth from the image
    // header (PNG IHDR or BITMAPINFOHEADER). 0 width/height legitimately means 256.
    const PNG_SIG: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    let (w, h, planes, bpp) = if bytes.len() >= 24 && bytes[..8] == PNG_SIG {
        let be =
            |o: usize| u32::from_be_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        (be(16) as u8, be(20) as u8, 1u16, 32u16)
    } else if bytes.len() >= 16 {
        // BITMAPINFOHEADER: width@4 (i32), height@8 (i32, doubled for XOR+AND masks),
        // planes@12 (u16), bitcount@14 (u16) — all little-endian.
        let w = i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let h = i32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) / 2;
        let planes = u16::from_le_bytes([bytes[12], bytes[13]]);
        let bpp = u16::from_le_bytes([bytes[14], bytes[15]]);
        (w as u8, h as u8, planes, bpp)
    } else {
        (0, 0, 1, 32)
    };

    let mut out = Vec::with_capacity(22 + bytes.len());
    out.extend_from_slice(&[0, 0, 1, 0, 1, 0]); // ICONDIR: reserved, type=1, count=1
    out.push(w);
    out.push(h);
    out.push(0); // color count (0 when >= 8bpp)
    out.push(0); // reserved
    out.extend_from_slice(&planes.to_le_bytes());
    out.extend_from_slice(&bpp.to_le_bytes());
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes()); // bytes in resource
    out.extend_from_slice(&22u32.to_le_bytes()); // image offset (after the 22-byte header)
    out.extend_from_slice(bytes);
    out
}

/// A thin vertical separator used between toolbar button groups (design
/// `.tbar-sep`: 1×22 with 5px horizontal margins).
pub(crate) fn separator(cx: &App) -> Div {
    div()
        // `flex_shrink_0` is required: as a 1px flex child it would otherwise
        // collapse to zero width whenever the toolbar row is tight (the icon
        // buttons are flex_shrink_0, so the separators absorb the deficit).
        .flex_shrink_0()
        .w(px(1.))
        .h(px(22.))
        .mx(px(5.))
        .bg(cx.theme().border)
}

/// A horizontally scrollable area with an always-visible horizontal scrollbar
/// whose **vertical mouse-wheel is not captured** (it bubbles to the page).
///
/// The built-in `.overflow_x_scrollbar()` wrapper can't express this: it doesn't
/// expose `restrict_scroll_to_axis` (so a vertical wheel would scroll it
/// horizontally) nor a per-instance always-show (its default only shows while
/// scrolling — which never happens once the wheel is restricted). So we assemble
/// the scroll-area + `Scrollbar` ourselves, once, here.
pub(crate) fn h_scroll_area(
    name: &'static str,
    scroll: &ScrollHandle,
    content: impl IntoElement,
) -> impl IntoElement {
    div()
        .relative()
        .w_full()
        .child(
            div()
                .id(SharedString::from(format!("{name}-scroll")))
                .flex()
                .w_full()
                .overflow_x_scroll()
                // The horizontal scrollbar is an overlay pinned to the bottom edge,
                // so reserve its height below the content — otherwise it covers the
                // last row.
                .pb(px(14.))
                .map(|mut d| {
                    d.style().restrict_scroll_to_axis = Some(true);
                    d
                })
                .track_scroll(scroll)
                .child(content),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .child(
                    Scrollbar::new(scroll)
                        .id(SharedString::from(format!("{name}-bar")))
                        .axis(ScrollbarAxis::Horizontal)
                        .scrollbar_show(ScrollbarShow::Always),
                ),
        )
}
