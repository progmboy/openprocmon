//! Executable metadata extraction (cf. design §7, C++ `CProcInfo`).
//!
//! Both the version strings (description/company/version) and the small/large
//! icons come from the image's PE resources — a disk read (plus `LoadLibraryEx`
//! for icons) per distinct image. Live capture resolves them **off-thread**
//! ([`MetadataCache::resolve_deferred`], Procmon-style): a cache hit is applied
//! to the record immediately, a miss is queued to a background worker, so the
//! parse thread never blocks on image I/O during the capture-start INIT burst.
//!
//! Late-arrival semantics: `ProcessRecord::meta()` is the tri-state — `None`
//! while pending, `Some` once final (a `Some` with empty fields means the image
//! really has none). The GUI reads icons per frame, so late metadata appears on
//! the next frame; the PML writer prefers the worker's result and resolves
//! synchronously at finalize for records still pending (saves stay complete);
//! `EventSource::process_meta` is the resolve-it-now accessor for consumers
//! that want the value immediately. Filters on Company/Version/Description
//! evaluate against the resolved-so-far state, exactly like Procmon.
//!
//! Icons are raw `RT_ICON` resource bytes (no GDI), mirroring C++ `UtilExtractIcon`.

use crate::process::{ProcessMeta, ProcessRecord};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{FreeLibrary, BOOL, HMODULE};
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::Win32::System::LibraryLoader::{
    EnumResourceNamesW, FindResourceW, LoadLibraryExW, LoadResource, LockResource, SizeofResource,
    LOAD_LIBRARY_AS_DATAFILE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, LookupIconIdFromDirectoryEx, LR_DEFAULTCOLOR, RT_GROUP_ICON, RT_ICON,
    SM_CXICON, SM_CXSMICON, SM_CYICON, SM_CYSMICON,
};

/// Resolves and caches executable metadata, keyed by image path.
///
/// Entries are shared via `Arc`: `resolve` hands out a clone of the `Arc` (a
/// refcount bump), so all processes of the same image — and the cache itself —
/// share one allocation of the version strings and icon bytes rather than each
/// holding a deep copy.
///
/// The map itself is `Arc`-shared with the lazily spawned background worker
/// ([`resolve_deferred`](Self::resolve_deferred)); dropping the cache drops the
/// job channel's sender, which ends the worker thread.
pub struct MetadataCache {
    cache: Arc<RwLock<HashMap<String, Arc<ProcessMeta>>>>,
    /// Background resolver, spawned on the first deferred miss. Jobs are the
    /// process records to fill; the worker extracts, caches, and `set_meta`s.
    worker: OnceLock<crossbeam_channel::Sender<Arc<ProcessRecord>>>,
    /// Module (per-loaded-DLL) version strings, pre-warmed from image-load
    /// events during the capture so a PML save is all cache hits. Shared with
    /// the writer via [`module_versions`](Self::module_versions).
    module_versions: Arc<ModuleVersionCache>,
}

impl MetadataCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            worker: OnceLock::new(),
            module_versions: Arc::new(ModuleVersionCache::new()),
        }
    }

    /// The shared module-version cache. The capture pipeline calls
    /// [`ModuleVersionCache::prewarm`] on it as image-load events arrive; the
    /// PML writer resolves against the same instance at save time.
    pub fn module_versions(&self) -> &Arc<ModuleVersionCache> {
        &self.module_versions
    }

    /// Resolves an image's metadata (version strings + icons) **synchronously**,
    /// caching by path so only the first request per image reads from disk. The
    /// returned `Arc` is shared with the cache (and every other process of the
    /// same image), so this is a refcount bump, not a deep copy. Empty path
    /// yields an all-`None` result.
    pub fn resolve(&self, image_path: &str) -> Arc<ProcessMeta> {
        resolve_in(&self.cache, image_path)
    }

    /// Resolves `rec`'s image metadata without blocking the calling thread: a
    /// cache hit (or empty path) is applied to the record immediately — so
    /// respawns of a known image stay deterministic — while a miss is queued to
    /// the background worker, which extracts, caches, and `set_meta`s. Until
    /// then `rec.meta()` reads `None` ("still pending"); once set it is final.
    pub fn resolve_deferred(&self, rec: Arc<ProcessRecord>) {
        let path = &rec.info.image_path;
        if path.is_empty() {
            rec.set_meta(Arc::new(ProcessMeta::default()));
            return;
        }
        if let Some(m) = self.cache.read().get(&path.to_ascii_lowercase()) {
            rec.set_meta(Arc::clone(m));
            return;
        }
        let tx = self.worker.get_or_init(|| {
            let (tx, rx) = crossbeam_channel::unbounded::<Arc<ProcessRecord>>();
            let cache = Arc::clone(&self.cache);
            // Ends when the sender (owned by this cache) is dropped. Failure to
            // spawn (thread exhaustion) falls back to inline resolution below.
            let _ = std::thread::Builder::new()
                .name("procmon-metadata".into())
                .spawn(move || {
                    for rec in rx {
                        rec.set_meta(resolve_in(&cache, &rec.info.image_path));
                    }
                });
            tx
        });
        if let Err(e) = tx.send(rec) {
            // Worker unavailable (thread spawn failed, so the receiver is
            // gone): resolve inline rather than losing the record.
            let rec = e.into_inner();
            rec.set_meta(resolve_in(&self.cache, &rec.info.image_path));
        }
    }
}

/// The shared body of [`MetadataCache::resolve`]: look up, extract on miss,
/// insert-or-share (a concurrent inserter's entry wins so all callers share
/// one allocation).
fn resolve_in(
    cache: &RwLock<HashMap<String, Arc<ProcessMeta>>>,
    image_path: &str,
) -> Arc<ProcessMeta> {
    if image_path.is_empty() {
        return Arc::new(ProcessMeta::default());
    }
    let key = image_path.to_ascii_lowercase();
    if let Some(m) = cache.read().get(&key) {
        return Arc::clone(m);
    }
    let meta = Arc::new(extract(image_path));
    let mut cache = cache.write();
    Arc::clone(cache.entry(key).or_insert(meta))
}

impl Default for MetadataCache {
    fn default() -> Self {
        Self::new()
    }
}

/// A module's PE version strings, shared cheaply (one `Arc<str>` allocation
/// per field per distinct image; absent fields are the empty string, matching
/// Procmon's blank module columns for unversioned images).
#[derive(Clone)]
pub struct ModuleVersion {
    pub version: Arc<str>,
    pub company: Arc<str>,
    pub description: Arc<str>,
}

impl Default for ModuleVersion {
    fn default() -> Self {
        Self {
            version: Arc::from(""),
            company: Arc::from(""),
            description: Arc::from(""),
        }
    }
}

/// Resolves and caches module version strings, keyed case-insensitively by
/// path. No icon extraction (a PML save touches thousands of module images;
/// icons are a process-level concern), so it stays cheap enough to run over
/// every module list — `ntdll.dll` and friends are read once however many
/// processes load them.
///
/// Version-resource reads are ~1ms each and a system-wide capture references
/// thousands of distinct modules — seconds of I/O if all done at save time. So
/// live capture **pre-warms during the capture** ([`prewarm`](Self::prewarm)):
/// as image-load events arrive, their paths are queued to a background worker,
/// so by save time the [`resolve`](Self::resolve) calls are cache hits. The
/// cache is `Arc`-shared between the capture pipeline (which pre-warms) and the
/// PML writer (which reads); dropping the last handle ends the worker.
pub struct ModuleVersionCache {
    cache: Arc<RwLock<HashMap<String, ModuleVersion>>>,
    /// Paths already cached or in flight — dedups the pre-warm queue (ntdll is
    /// loaded by every process; we resolve it once, not thousands of times).
    seen: Arc<RwLock<std::collections::HashSet<String>>>,
    worker: OnceLock<crossbeam_channel::Sender<String>>,
}

impl ModuleVersionCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            seen: Arc::new(RwLock::new(std::collections::HashSet::new())),
            worker: OnceLock::new(),
        }
    }

    /// The version strings of the image at `path` (a DOS path). Unreadable or
    /// unversioned images resolve to empty strings (cached too, so a missing
    /// file is only probed once). Synchronous — used by the writer at finalize;
    /// after [`prewarm`](Self::prewarm) most calls are cache hits.
    pub fn resolve(&self, path: &str) -> ModuleVersion {
        if path.is_empty() {
            return ModuleVersion::default();
        }
        let key = path.to_ascii_lowercase();
        if let Some(v) = self.cache.read().get(&key) {
            return v.clone();
        }
        let v = extract_module_version(path);
        self.cache.write().entry(key).or_insert(v).clone()
    }

    /// Queues `paths` (DOS module paths) for background version resolution,
    /// deduped against everything already cached or queued. Non-blocking: the
    /// worker (lazily spawned) does the I/O off the capture's parse thread, so
    /// the version strings are ready by save time. Empty until the first call.
    pub fn prewarm<I: IntoIterator<Item = String>>(&self, paths: I) {
        let tx = self.worker.get_or_init(|| {
            let (tx, rx) = crossbeam_channel::unbounded::<String>();
            let cache = Arc::clone(&self.cache);
            let _ = std::thread::Builder::new()
                .name("procmon-modversion".into())
                .spawn(move || {
                    for path in rx {
                        let key = path.to_ascii_lowercase();
                        if cache.read().contains_key(&key) {
                            continue;
                        }
                        let v = extract_module_version(&path);
                        cache.write().entry(key).or_insert(v);
                    }
                });
            tx
        });
        let mut seen = self.seen.write();
        for p in paths {
            if p.is_empty() {
                continue;
            }
            let key = p.to_ascii_lowercase();
            if seen.insert(key) {
                let _ = tx.send(p);
            }
        }
    }

    /// Pre-resolves `paths` with a bounded thread pool so subsequent
    /// [`resolve`](Self::resolve) calls all hit the cache. A system-wide PML
    /// finalize references thousands of distinct images at ~1ms of version-
    /// resource I/O each — resolved serially that is seconds per save; the
    /// reads parallelize well (page cache), so warming cuts it to a fraction.
    pub fn warm<I: IntoIterator<Item = String>>(&self, paths: I) {
        // Distinct, not-yet-cached paths only (case-insensitive key).
        let todo: Vec<String> = {
            let seen = self.cache.read();
            let mut keys = std::collections::HashSet::new();
            paths
                .into_iter()
                .filter(|p| !p.is_empty())
                .filter(|p| {
                    let k = p.to_ascii_lowercase();
                    !seen.contains_key(&k) && keys.insert(k)
                })
                .collect()
        };
        if todo.is_empty() {
            return;
        }
        let workers = std::thread::available_parallelism()
            .map(|n| n.get().min(8))
            .unwrap_or(4)
            .min(todo.len());
        let next = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..workers {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(p) = todo.get(i) else { break };
                    // `resolve` re-checks the cache and inserts — idempotent
                    // under the pool and against concurrent resolvers.
                    let _ = self.resolve(p);
                });
            }
        });
    }
}

impl Default for ModuleVersionCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Reads just the version strings of a module image into a [`ModuleVersion`]
/// (absent fields → empty string), the shared body of
/// [`ModuleVersionCache::resolve`] and its background worker.
fn extract_module_version(path: &str) -> ModuleVersion {
    let meta = extract_version(path);
    let arc = |s: Option<String>| -> Arc<str> { Arc::from(s.as_deref().unwrap_or("")) };
    ModuleVersion {
        version: arc(meta.version),
        company: arc(meta.company),
        description: arc(meta.description),
    }
}

/// Extracts an image's version strings and icons (cf. C++ `CProcInfo::Parse`).
fn extract(path: &str) -> ProcessMeta {
    let mut meta = extract_version(path);
    let (small, large) = extract_icons(path);
    meta.icon_small = small;
    meta.icon_large = large;
    meta
}

/// Reads the version resource into a [`ProcessMeta`]'s version fields, aligned with C++
/// `VerQueryByTranslation`: it uses the first `\VarFileInfo\Translation` pair and
/// reads `FileDescription` / `CompanyName` / `FileVersion`, falling back to
/// codepage `0x04E4` (1252) if the file's own codepage has no such string.
fn extract_version(path: &str) -> ProcessMeta {
    let mut meta = ProcessMeta::default();
    let wpath = HSTRING::from(path);

    // SAFETY: `&wpath` is a valid NUL-terminated path.
    let size = unsafe { GetFileVersionInfoSizeW(&wpath, None) };
    if size == 0 {
        return meta;
    }
    let mut block = vec![0u8; size as usize];
    // SAFETY: `block` is `size` bytes; the path is valid.
    if unsafe { GetFileVersionInfoW(&wpath, 0, size, block.as_mut_ptr() as *mut _) }.is_err() {
        return meta;
    }

    // Like C++, only query when a translation table is present (no synthetic
    // default codepage).
    let Some((lang, codepage)) = translation(&block) else {
        return meta;
    };
    let query = |name: &str| query_translated(&block, lang, codepage, name);
    meta.description = query("FileDescription");
    meta.company = query("CompanyName");
    meta.version = query("FileVersion");
    meta
}

/// Queries a version string for `(lang, codepage)`, falling back to codepage
/// `0x04E4` (cf. C++ `VerQueryByTranslation`/`VerQueryByCodePage`).
fn query_translated(block: &[u8], lang: u16, codepage: u16, name: &str) -> Option<String> {
    query_string(
        block,
        &format!("\\StringFileInfo\\{lang:04x}{codepage:04x}\\{name}"),
    )
    .or_else(|| query_string(block, &format!("\\StringFileInfo\\{lang:04x}04e4\\{name}")))
}

/// Reads the first `\VarFileInfo\Translation` (language, codepage) pair.
fn translation(block: &[u8]) -> Option<(u16, u16)> {
    let mut ptr: *mut core::ffi::c_void = core::ptr::null_mut();
    let mut len: u32 = 0;
    // SAFETY: `block` is a valid version-info block; `ptr`/`len` receive a view
    // into it (an array of u16 language/codepage pairs).
    let ok = unsafe {
        VerQueryValueW(
            block.as_ptr() as *const _,
            &HSTRING::from("\\VarFileInfo\\Translation"),
            &mut ptr,
            &mut len,
        )
    };
    if !ok.as_bool() || ptr.is_null() || len < 4 {
        return None;
    }
    // SAFETY: at least one pair is present (len >= 4 bytes).
    let pair = unsafe { core::slice::from_raw_parts(ptr as *const u16, 2) };
    Some((pair[0], pair[1]))
}

/// Queries a string value from the version block, or `None` if absent/empty.
fn query_string(block: &[u8], sub_block: &str) -> Option<String> {
    let mut ptr: *mut core::ffi::c_void = core::ptr::null_mut();
    let mut len: u32 = 0;
    // SAFETY: `block` is a valid version-info block; on success `ptr`/`len`
    // describe a UTF-16 string view into it (`len` is a unit count).
    let ok = unsafe {
        VerQueryValueW(
            block.as_ptr() as *const _,
            &HSTRING::from(sub_block),
            &mut ptr,
            &mut len,
        )
    };
    if !ok.as_bool() || ptr.is_null() || len == 0 {
        return None;
    }
    // SAFETY: `ptr`/`len` are valid per the successful query above.
    let units = unsafe { core::slice::from_raw_parts(ptr as *const u16, len as usize) };
    let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
    let s = String::from_utf16_lossy(&units[..end]);
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

// --- Icon extraction (cf. C++ `UtilExtractIcon` / `GetMatchIconBuffer`) -------
//
// The image is loaded as a data file, its `RT_GROUP_ICON` directory is searched
// for the entries best matching the small/large system metric sizes, and the raw
// `RT_ICON` resource bytes for those entries are copied out. No GDI is involved;
// the GUI turns these bytes into a displayable icon.

/// Accumulates the matched small/large icon bytes during enumeration.
#[derive(Default)]
struct IconAccum {
    small: Option<Vec<u8>>,
    large: Option<Vec<u8>>,
}

/// Extracts the small and large icon resource bytes from `path`.
fn extract_icons(path: &str) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let wpath = HSTRING::from(path);
    let mut acc = IconAccum::default();
    // SAFETY: load the image purely as a data file (no code runs).
    let module = match unsafe { LoadLibraryExW(&wpath, None, LOAD_LIBRARY_AS_DATAFILE) } {
        Ok(m) if !m.is_invalid() => m,
        _ => return (None, None),
    };

    // SAFETY: enumerate RT_GROUP_ICON resources; `acc` outlives the call and is
    // passed as the callback context.
    unsafe {
        let _ = EnumResourceNamesW(
            module,
            RT_GROUP_ICON,
            Some(enum_icon_groups),
            &mut acc as *mut IconAccum as isize,
        );
        let _ = FreeLibrary(module);
    }
    (acc.small, acc.large)
}

/// `EnumResourceNamesW` callback: for each icon group, copy the small/large icon
/// resources, stopping once both have been found. Returns TRUE to continue.
unsafe extern "system" fn enum_icon_groups(
    module: HMODULE,
    _ty: PCWSTR,
    name: PCWSTR,
    lparam: isize,
) -> BOOL {
    // SAFETY: `lparam` is the `&mut IconAccum` passed to EnumResourceNamesW.
    let param = unsafe { &mut *(lparam as *mut IconAccum) };

    let Some((dir, _)) = (unsafe { lock_resource(module, name, RT_GROUP_ICON) }) else {
        return BOOL(1); // skip this group, keep enumerating
    };

    let (cx_small, cy_small) = (GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON));
    let (cx_large, cy_large) = (GetSystemMetrics(SM_CXICON), GetSystemMetrics(SM_CYICON));
    if param.small.is_none() {
        param.small = unsafe { match_icon(module, dir, cx_small, cy_small) };
    }
    if param.large.is_none() {
        param.large = unsafe { match_icon(module, dir, cx_large, cy_large) };
    }

    // Stop (FALSE) once both sizes are found, else continue (TRUE).
    BOOL((param.small.is_none() || param.large.is_none()) as i32)
}

/// Finds the `RT_ICON` best matching `cx`/`cy` in a group-icon directory and
/// returns its raw resource bytes.
///
/// # Safety
/// `dir` must point at a valid `RT_GROUP_ICON` directory resource of `module`.
unsafe fn match_icon(module: HMODULE, dir: *const u8, cx: i32, cy: i32) -> Option<Vec<u8>> {
    // SAFETY: `dir` is a valid icon directory per the caller's contract.
    let id = unsafe { LookupIconIdFromDirectoryEx(dir, true, cx, cy, LR_DEFAULTCOLOR) };
    if id == 0 {
        return None;
    }
    // MAKEINTRESOURCE(id): a PCWSTR whose pointer value is the resource id.
    let name = PCWSTR(id as u16 as usize as *const u16);
    // SAFETY: looking up the RT_ICON resource we just resolved.
    let (ptr, size) = unsafe { lock_resource(module, name, RT_ICON)? };
    // SAFETY: `ptr`/`size` describe the resource bytes for the module's lifetime.
    Some(unsafe { core::slice::from_raw_parts(ptr, size as usize) }.to_vec())
}

/// Locks a resource and returns a pointer to its bytes and its size.
///
/// # Safety
/// `module` must be a valid loaded module handle.
unsafe fn lock_resource(module: HMODULE, name: PCWSTR, ty: PCWSTR) -> Option<(*const u8, u32)> {
    // SAFETY: standard resource lookup on a valid module.
    let res = unsafe { FindResourceW(module, name, ty) };
    if res.is_invalid() {
        return None;
    }
    // SAFETY: `res` is valid; LoadResource/LockResource read it.
    let global = unsafe { LoadResource(module, res) }.ok()?;
    let ptr = unsafe { LockResource(global) };
    if ptr.is_null() {
        return None;
    }
    let size = unsafe { SizeofResource(module, res) };
    if size == 0 {
        return None;
    }
    Some((ptr as *const u8, size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{ProcessInfo, ProcessRecord};

    fn system_image(name: &str) -> String {
        format!(
            "{}\\System32\\{name}",
            std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into())
        )
    }

    #[test]
    fn resolve_deferred_fills_meta_off_thread_then_hits_cache() {
        let cache = MetadataCache::new();
        let rec = ProcessRecord::new(ProcessInfo {
            seq: 1,
            pid: 100,
            image_path: system_image("ntdll.dll"),
            ..Default::default()
        });
        cache.resolve_deferred(Arc::clone(&rec));
        // Bounded wait: the first sighting of an image resolves off-thread.
        for _ in 0..500 {
            if rec.meta().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let meta = rec.meta().expect("worker resolves within the bound");
        assert!(
            meta.company.as_deref().unwrap_or("").contains("Microsoft"),
            "company from the version resource, got {:?}",
            meta.company
        );

        // Same image again: the cache hit is applied synchronously, so a
        // respawn of a known image never reads as pending.
        let rec2 = ProcessRecord::new(ProcessInfo {
            seq: 2,
            pid: 101,
            image_path: system_image("ntdll.dll"),
            ..Default::default()
        });
        cache.resolve_deferred(Arc::clone(&rec2));
        assert!(rec2.meta().is_some(), "cache hit must be synchronous");
    }

    #[test]
    fn empty_path_resolves_immediately_to_none_fields() {
        let cache = MetadataCache::new();
        let rec = ProcessRecord::new(ProcessInfo::default());
        cache.resolve_deferred(Arc::clone(&rec));
        let meta = rec.meta().expect("empty path is immediate");
        assert!(meta.company.is_none() && meta.version.is_none());
    }
}
