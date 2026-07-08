//! Call-stack symbol resolution via a dynamically-loaded `dbghelp.dll`
//! (cf. the C++ GUI's `propstack.cpp::LookupSymbolByAddress`).
//!
//! Resolution turns a raw frame address into `module!symbol+0xoffset`. It is slow
//! (it maps each module image and asks dbghelp to load matching PDBs, possibly from
//! a symbol server) but highly cacheable, so a single [`SymbolResolver`] is meant to
//! live for the whole session and be shared (it serializes calls internally because
//! the dbghelp API is single-threaded).
//!
//! The dbghelp DLL is loaded from a *configured* path — the Debugging Tools / WDK
//! build supports symbol servers, unlike the system one — so resolution only runs
//! when that path exists.
//!
//! Two caches keep it cheap:
//! * a per-module image cache (`path -> our mapped base`): each module is mapped
//!   (`LoadLibraryEx` + `SymLoadModuleEx`) once for the resolver's lifetime, instead
//!   of the C++ load/unload per frame;
//! * a symbol cache keyed by *our* mapped address (`base + offset`, stable across
//!   the originating processes — the raw target VA is not, due to ASLR), storing
//!   both hits and misses (negative cache).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use widestring::U16CString;
use windows::core::{s, PCWSTR};
use windows::Win32::Foundation::{FreeLibrary, BOOL, HANDLE, HMODULE};
use windows::Win32::System::Diagnostics::Debug::{
    SYMBOL_INFOW, SYMOPT_DEFERRED_LOADS, SYMOPT_UNDNAME,
};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LoadLibraryW, DONT_RESOLVE_DLL_REFERENCES,
};
use windows::Win32::System::Threading::GetCurrentProcess;

/// dbghelp's documented maximum symbol name length (in `WCHAR`s).
const MAX_SYM_NAME: usize = 2000;

/// A lightweight, borrowed view of one loaded module, decoupling the resolver from
/// the SDK's [`crate::Module`], the GUI's `ModuleRow`, and the PML module type.
#[derive(Clone, Copy)]
pub struct SymModule<'a> {
    pub base: u64,
    pub size: u64,
    pub path: &'a str,
}

/// Whether `addr` is a kernel-mode address (the canonical upper half) — the
/// single source for the frame-kind threshold used everywhere a call stack is
/// displayed or exported.
pub fn is_kernel_address(addr: u64) -> bool {
    addr >= 0xFFFF_0000_0000_0000
}

/// Resolves a frame address to `(module basename, "module + 0xoffset", full
/// path)`, searching the originating process's modules then the System (PID 4)
/// kernel-driver modules (so user- and kernel-mode frames both resolve).
/// Outside every module it falls back to `("<UNKNOWN>", "0x{addr:016x}", "")`.
/// Module-range resolution only — symbol (PDB) resolution is the separate,
/// optional [`SymbolResolver`] overlay.
pub fn resolve_frame<'a>(
    addr: u64,
    proc_mods: &'a [SymModule<'a>],
    kernel_mods: &'a [SymModule<'a>],
) -> (&'a str, String, &'a str) {
    proc_mods
        .iter()
        .chain(kernel_mods.iter())
        .find(|m| m.size > 0 && addr >= m.base && addr < m.base.saturating_add(m.size))
        .map(|m| {
            let name = basename(m.path);
            (name, format!("{} + 0x{:x}", name, addr - m.base), m.path)
        })
        .unwrap_or_else(|| ("<UNKNOWN>", format!("0x{addr:016x}"), ""))
}

// dbghelp entry points we resolve dynamically from the configured DLL (so the call
// goes to *that* dbghelp, not whatever the loader would bind statically).
type FnSymSetOptions = unsafe extern "system" fn(u32) -> u32;
type FnSymInitializeW = unsafe extern "system" fn(HANDLE, PCWSTR, BOOL) -> BOOL;
type FnSymLoadModuleExW = unsafe extern "system" fn(
    HANDLE,
    HANDLE,
    PCWSTR,
    PCWSTR,
    u64,
    u32,
    *const core::ffi::c_void,
    u32,
) -> u64;
type FnSymFromAddrW = unsafe extern "system" fn(HANDLE, u64, *mut u64, *mut SYMBOL_INFOW) -> BOOL;
type FnSymUnloadModule64 = unsafe extern "system" fn(HANDLE, u64) -> BOOL;
type FnSymCleanup = unsafe extern "system" fn(HANDLE) -> BOOL;

struct Procs {
    load_module: FnSymLoadModuleExW,
    from_addr: FnSymFromAddrW,
    unload_module: FnSymUnloadModule64,
    cleanup: FnSymCleanup,
}

/// One mapped module image kept alive for the resolver's lifetime.
struct Loaded {
    /// The `HMODULE` from `LoadLibraryEx` (and our symbol base).
    handle: HMODULE,
    /// `true` once `SymLoadModuleExW` attached symbols at `handle`'s base.
    sym_loaded: bool,
}

struct Inner {
    /// The loaded `dbghelp.dll` (freed on drop).
    dbghelp: HMODULE,
    /// Pseudo-handle used as the dbghelp "process" key.
    process: HANDLE,
    procs: Procs,
    /// Mapped module images, keyed by image path.
    modules: HashMap<String, Loaded>,
    /// Symbol cache keyed by our mapped address (`base + offset`); `None` = a
    /// resolved miss (negative cache).
    cache: HashMap<u64, Option<Arc<str>>>,
}

// SAFETY: every field is only touched under the resolver's `Mutex`, and the handles
// are process-global. dbghelp itself is single-threaded, which the mutex enforces.
unsafe impl Send for Inner {}

/// Resolves frame addresses to `module!symbol+0xoffset` via a configured dbghelp.
pub struct SymbolResolver {
    inner: Mutex<Inner>,
}

impl SymbolResolver {
    /// Loads `dbghelp_path` and initializes a symbol session that searches
    /// `symbols_path` (a `srv*...` spec or a plain directory list). Returns `None`
    /// when `dbghelp_path` is empty or does not exist, so callers can treat symbol
    /// resolution as an optional overlay.
    pub fn new(dbghelp_path: &str, symbols_path: &str) -> Option<Self> {
        if dbghelp_path.is_empty() || !Path::new(dbghelp_path).exists() {
            return None;
        }
        // SAFETY: standard LoadLibrary/GetProcAddress sequence; every entry point is
        // checked for presence and the wide strings outlive the calls.
        unsafe {
            let dll_w = U16CString::from_str(dbghelp_path).ok()?;
            let dbghelp = LoadLibraryW(PCWSTR(dll_w.as_ptr())).ok()?;

            let set_options: FnSymSetOptions =
                core::mem::transmute(GetProcAddress(dbghelp, s!("SymSetOptions"))?);
            let initialize: FnSymInitializeW =
                core::mem::transmute(GetProcAddress(dbghelp, s!("SymInitializeW"))?);
            let load_module: FnSymLoadModuleExW =
                core::mem::transmute(GetProcAddress(dbghelp, s!("SymLoadModuleExW"))?);
            let from_addr: FnSymFromAddrW =
                core::mem::transmute(GetProcAddress(dbghelp, s!("SymFromAddrW"))?);
            let unload_module: FnSymUnloadModule64 =
                core::mem::transmute(GetProcAddress(dbghelp, s!("SymUnloadModule64"))?);
            let cleanup: FnSymCleanup =
                core::mem::transmute(GetProcAddress(dbghelp, s!("SymCleanup"))?);

            set_options(SYMOPT_DEFERRED_LOADS | SYMOPT_UNDNAME);

            let process = GetCurrentProcess();
            // We load modules explicitly, so don't invade the current process; pass
            // the configured search path (symbol server / directories).
            let search = U16CString::from_str(symbols_path).unwrap_or_default();
            let search_ptr = if symbols_path.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(search.as_ptr())
            };
            if !initialize(process, search_ptr, BOOL(0)).as_bool() {
                let _ = FreeLibrary(dbghelp);
                return None;
            }

            Some(Self {
                inner: Mutex::new(Inner {
                    dbghelp,
                    process,
                    procs: Procs {
                        load_module,
                        from_addr,
                        unload_module,
                        cleanup,
                    },
                    modules: HashMap::new(),
                    cache: HashMap::new(),
                }),
            })
        }
    }

    /// Resolves `addr` against `modules` to `module!symbol+0xoffset`, or `None` when
    /// the address is outside every module or no symbol could be found (the caller
    /// keeps the `module+offset` fallback).
    pub fn resolve(&self, addr: u64, modules: &[SymModule]) -> Option<Arc<str>> {
        // Find the module containing the address.
        let m = modules
            .iter()
            .find(|m| m.size > 0 && addr >= m.base && addr < m.base.saturating_add(m.size))?;
        let offset = addr - m.base;

        let mut inner = self.inner.lock();
        let load_base = inner.ensure_loaded(m)?;
        let sym_addr = load_base.wrapping_add(offset);

        if let Some(cached) = inner.cache.get(&sym_addr) {
            return cached.clone();
        }
        let result = inner.lookup(sym_addr, basename(m.path));
        inner.cache.insert(sym_addr, result.clone());
        result
    }
}

impl Inner {
    /// Maps the module image once (cached by path) and returns our symbol base.
    fn ensure_loaded(&mut self, m: &SymModule) -> Option<u64> {
        if let Some(l) = self.modules.get(m.path) {
            return Some(l.handle.0 as u64);
        }
        // SAFETY: `path_w`/`name_w` are valid wide strings kept alive across the
        // calls; the handle is stored for cleanup on drop.
        unsafe {
            let path_w = U16CString::from_str(m.path).ok()?;
            let handle =
                LoadLibraryExW(PCWSTR(path_w.as_ptr()), None, DONT_RESOLVE_DLL_REFERENCES).ok()?;
            let base = handle.0 as u64;

            let name = basename(m.path);
            let name_w = U16CString::from_str(name).unwrap_or_default();
            let name_ptr = PCWSTR(name_w.as_ptr());
            // ImageName = ModuleName = basename; BaseOfDll = our mapped base; let
            // dbghelp read the size from the image when the caller's size is unknown.
            let img = (self.procs.load_module)(
                self.process,
                HANDLE::default(),
                name_ptr,
                name_ptr,
                base,
                m.size as u32,
                core::ptr::null(),
                0,
            );
            let sym_loaded = img == base;
            self.modules
                .insert(m.path.to_string(), Loaded { handle, sym_loaded });
            Some(base)
        }
    }

    /// Queries the symbol at `sym_addr`, formatting `module!symbol+0xoffset`.
    fn lookup(&self, sym_addr: u64, module: &str) -> Option<Arc<str>> {
        // SYMBOL_INFOW has a trailing flexible `Name` array; back it with a
        // u64-aligned buffer of SYMBOL_INFOW + MAX_SYM_NAME WCHARs.
        let words = (size_of::<SYMBOL_INFOW>() + MAX_SYM_NAME * 2) / 8 + 1;
        let mut buf = vec![0u64; words];
        // SAFETY: `sym` points at a correctly sized, aligned buffer; SizeOfStruct and
        // MaxNameLen are set per the dbghelp contract before the call.
        unsafe {
            let sym = buf.as_mut_ptr() as *mut SYMBOL_INFOW;
            (*sym).SizeOfStruct = size_of::<SYMBOL_INFOW>() as u32;
            (*sym).MaxNameLen = MAX_SYM_NAME as u32;
            let mut disp: u64 = 0;
            if !(self.procs.from_addr)(self.process, sym_addr, &mut disp, sym).as_bool() {
                return None;
            }
            let len = (*sym).NameLen as usize;
            let name_slice = std::slice::from_raw_parts((*sym).Name.as_ptr(), len);
            let name = String::from_utf16_lossy(name_slice);
            let text = if disp != 0 {
                format!("{module}!{name}+0x{disp:x}")
            } else {
                format!("{module}!{name}")
            };
            Some(Arc::from(text.as_str()))
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // SAFETY: unload every module we attached, free its mapped image, end the
        // symbol session, then free dbghelp itself.
        unsafe {
            for l in self.modules.values() {
                if l.sym_loaded {
                    let _ = (self.procs.unload_module)(self.process, l.handle.0 as u64);
                }
                let _ = FreeLibrary(l.handle);
            }
            let _ = (self.procs.cleanup)(self.process);
            let _ = FreeLibrary(self.dbghelp);
        }
    }
}

use crate::path::basename;
use core::mem::size_of;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_dbghelp_yields_none() {
        // The "resolve only when the configured dbghelp exists" contract: an empty
        // or non-existent path disables resolution (callers keep module+offset).
        assert!(SymbolResolver::new("", "srv*").is_none());
        assert!(SymbolResolver::new(r"C:\does\not\exist\dbghelp.dll", "").is_none());
    }

    #[test]
    fn resolve_frame_hits_and_falls_back() {
        let mods = [SymModule {
            base: 0x7FF6_0000_0000,
            size: 0x1000,
            path: r"C:\Windows\System32\ntdll.dll",
        }];
        let (name, location, path) = resolve_frame(0x7FF6_0000_0010, &mods, &[]);
        assert_eq!(name, "ntdll.dll");
        assert_eq!(location, "ntdll.dll + 0x10");
        assert_eq!(path, r"C:\Windows\System32\ntdll.dll");

        let (name, location, path) = resolve_frame(0x1000, &mods, &[]);
        assert_eq!(name, "<UNKNOWN>");
        assert_eq!(location, "0x0000000000001000");
        assert_eq!(path, "");
    }

    #[test]
    fn kernel_address_threshold() {
        assert!(is_kernel_address(0xFFFF_0000_0000_0000));
        assert!(is_kernel_address(0xFFFF_8000_0000_0001));
        assert!(!is_kernel_address(0x7FF6_0000_0000));
    }
}
