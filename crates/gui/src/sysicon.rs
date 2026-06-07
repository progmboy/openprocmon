//! The shell's default application icon (cf. C++ `UtilGetDefaultIcon`).
//!
//! When a process exposes no embedded icon, Process Monitor falls back to the
//! shell's generic `.exe` icon. This is a *display-time* fallback only: it is
//! never written into a PML (a log captured without an icon stores empty, and the
//! fallback is re-applied when that log is viewed), so it lives in the GUI rather
//! than the SDK.
//!
//! The shell exposes this icon only as an `HICON` (`SHGFI_ICON`) — there is no
//! file/resource to read the bytes from (`SHGFI_ICONLOCATION` returns an empty
//! path for `.exe`, since the generic icon lives in the system image list, not a
//! file). So we convert the `HICON`'s color + mask bitmaps into raw `ICONIMAGE`
//! (DIB) bytes — the same shape the SDK/PML backends deliver — and let
//! [`crate::components::app_icon`]'s existing ICO wrapper render it unchanged.

use std::sync::Arc;

/// The shell's default `.exe` icon as raw `ICONIMAGE` (DIB) bytes, resolved once
/// and cached. `None` when the platform has no such icon or the conversion fails
/// (callers then fall back to the colored letter tile).
pub(crate) fn default_app_icon() -> Option<Arc<[u8]>> {
    #[cfg(windows)]
    {
        use std::sync::OnceLock;
        static CACHE: OnceLock<Option<Arc<[u8]>>> = OnceLock::new();
        CACHE.get_or_init(win::extract).clone()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
mod win {
    use std::sync::Arc;

    use windows_sys::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
    use windows_sys::Win32::UI::Shell::{
        SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_USEFILEATTRIBUTES,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

    /// Resolves the default `.exe` icon to `ICONIMAGE` bytes (cf. `UtilGetDefaultIcon`).
    pub(super) fn extract() -> Option<Arc<[u8]>> {
        // ".exe" as a NUL-terminated wide string; `SHGFI_USEFILEATTRIBUTES` means
        // the path need not exist — only the extension drives the lookup.
        let ext: Vec<u16> = ".exe\0".encode_utf16().collect();
        let mut sfi: SHFILEINFOW = unsafe { core::mem::zeroed() };
        // SAFETY: `ext` is a valid NUL-terminated wide string; `sfi` is a valid
        // out-parameter sized by the `size_of` argument.
        let ret = unsafe {
            SHGetFileInfoW(
                ext.as_ptr(),
                FILE_ATTRIBUTE_NORMAL,
                &mut sfi,
                core::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_USEFILEATTRIBUTES | SHGFI_ICON | SHGFI_LARGEICON,
            )
        };
        if ret == 0 || sfi.hIcon.is_null() {
            return None;
        }
        // SAFETY: `sfi.hIcon` is a valid icon handle from the call above.
        let bytes = unsafe { hicon_to_iconimage(sfi.hIcon) };
        // SAFETY: we own `hIcon` (SHGFI_ICON) and must free it.
        unsafe { DestroyIcon(sfi.hIcon) };
        bytes.map(Arc::from)
    }

    /// Converts an `HICON` into raw `ICONIMAGE` bytes: a `BITMAPINFOHEADER` whose
    /// height covers the XOR (color) and AND (mask) planes, the 32bpp color bits,
    /// then the 1bpp mask — exactly what an `.ico` directory entry points at.
    ///
    /// # Safety
    /// `hicon` must be a valid icon handle.
    unsafe fn hicon_to_iconimage(hicon: HICON) -> Option<Vec<u8>> {
        let mut ii: ICONINFO = unsafe { core::mem::zeroed() };
        // SAFETY: `hicon` is valid; `ii` receives owned bitmap handles we free below.
        if unsafe { GetIconInfo(hicon, &mut ii) } == 0 {
            return None;
        }
        let result = unsafe { convert(&ii) };
        // SAFETY: `GetIconInfo` allocated these bitmaps; free both (a monochrome
        // icon leaves `hbmColor` null, which `DeleteObject` tolerates).
        unsafe {
            if !ii.hbmColor.is_null() {
                DeleteObject(ii.hbmColor as _);
            }
            if !ii.hbmMask.is_null() {
                DeleteObject(ii.hbmMask as _);
            }
        }
        result
    }

    /// Reads the color and mask bitmaps from `ii` and assembles the `ICONIMAGE`.
    ///
    /// # Safety
    /// `ii.hbmColor` / `ii.hbmMask` must be valid bitmap handles.
    unsafe fn convert(ii: &ICONINFO) -> Option<Vec<u8>> {
        if ii.hbmColor.is_null() {
            return None; // monochrome icon — fall back to the letter tile
        }

        // Dimensions from the color bitmap.
        let mut bmp: BITMAP = unsafe { core::mem::zeroed() };
        // SAFETY: `hbmColor` is a valid bitmap; `bmp` is sized by the count arg.
        let got = unsafe {
            GetObjectW(
                ii.hbmColor as _,
                core::mem::size_of::<BITMAP>() as i32,
                &mut bmp as *mut _ as *mut core::ffi::c_void,
            )
        };
        if got == 0 || bmp.bmWidth <= 0 || bmp.bmHeight <= 0 {
            return None;
        }
        let w = bmp.bmWidth;
        let h = bmp.bmHeight;

        // SAFETY: a screen DC; released below.
        let hdc = unsafe { GetDC(core::ptr::null_mut()) };

        // 32bpp BGRA color bits (positive height => bottom-up rows, as `.ico` stores).
        let color_row = w as usize * 4;
        let mut color = vec![0u8; color_row * h as usize];
        let mut bi: BITMAPINFO = unsafe { core::mem::zeroed() };
        bi.bmiHeader = header(w, h, 32);
        // SAFETY: `hbmColor` is valid; `color` is sized for `h` rows of `w` pixels.
        let ok_color = unsafe {
            GetDIBits(
                hdc,
                ii.hbmColor as _,
                0,
                h as u32,
                color.as_mut_ptr() as *mut core::ffi::c_void,
                &mut bi,
                DIB_RGB_COLORS,
            )
        };

        // 1bpp AND mask, rows padded to a 4-byte boundary.
        let mask_row = (w as usize).div_ceil(32) * 4;
        let mut mask = vec![0u8; mask_row * h as usize];
        if !ii.hbmMask.is_null() {
            let mut mbi: BITMAPINFO = unsafe { core::mem::zeroed() };
            mbi.bmiHeader = header(w, h, 1);
            // SAFETY: `hbmMask` is valid; `mask` is sized for `h` padded rows.
            unsafe {
                GetDIBits(
                    hdc,
                    ii.hbmMask as _,
                    0,
                    h as u32,
                    mask.as_mut_ptr() as *mut core::ffi::c_void,
                    &mut mbi,
                    DIB_RGB_COLORS,
                );
            }
        }

        // SAFETY: release the screen DC obtained above.
        unsafe { ReleaseDC(core::ptr::null_mut(), hdc) };

        if ok_color == 0 {
            return None;
        }

        // `BI_RGB` leaves the alpha byte undefined; if the color plane carries no
        // alpha, derive it from the AND mask (a set mask bit means transparent).
        let has_alpha = color.iter().skip(3).step_by(4).any(|&a| a != 0);
        if !has_alpha {
            for y in 0..h as usize {
                for x in 0..w as usize {
                    let bit = (mask[y * mask_row + (x / 8)] >> (7 - (x % 8))) & 1;
                    color[y * color_row + x * 4 + 3] = if bit == 1 { 0 } else { 255 };
                }
            }
        }

        // ICONIMAGE = header (height doubled for XOR+AND) + color (XOR) + mask (AND).
        let mut out = Vec::with_capacity(40 + color.len() + mask.len());
        out.extend_from_slice(&header_bytes(w, h * 2, 32));
        out.extend_from_slice(&color);
        out.extend_from_slice(&mask);
        Some(out)
    }

    /// A `BITMAPINFOHEADER` for `GetDIBits` (positive height = bottom-up, `BI_RGB`).
    fn header(width: i32, height: i32, bpp: u16) -> BITMAPINFOHEADER {
        BITMAPINFOHEADER {
            biSize: core::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: height,
            biPlanes: 1,
            biBitCount: bpp,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        }
    }

    /// The 40-byte `BITMAPINFOHEADER` for the assembled `ICONIMAGE`, little-endian.
    fn header_bytes(width: i32, height: i32, bpp: u16) -> [u8; 40] {
        let mut b = [0u8; 40];
        b[0..4].copy_from_slice(&40u32.to_le_bytes());
        b[4..8].copy_from_slice(&width.to_le_bytes());
        b[8..12].copy_from_slice(&height.to_le_bytes());
        b[12..14].copy_from_slice(&1u16.to_le_bytes()); // planes
        b[14..16].copy_from_slice(&bpp.to_le_bytes());
        // Remaining fields (compression=BI_RGB(0), sizes, ppm, palette) stay zero.
        b
    }

    #[cfg(test)]
    mod tests {
        /// The default icon must resolve and, once wrapped as an `.ico`, decode to a
        /// real raster — proving the whole render path (extract → wrap → decode).
        #[test]
        fn default_icon_decodes() {
            let icon = super::extract().expect("default .exe icon should resolve");
            let ico = crate::components::ico_bytes(&icon);
            let img = image::load_from_memory_with_format(&ico, image::ImageFormat::Ico)
                .expect("wrapped bytes should decode as ICO");
            assert!(img.width() > 0 && img.height() > 0);
            // A real icon is not fully transparent.
            let opaque = img.to_rgba8().pixels().any(|p| p[3] != 0);
            assert!(opaque, "decoded icon is fully transparent");
        }
    }
}
