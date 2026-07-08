//! Process and module tracking (cf. C++ `procmgr.cxx` / `process.cxx`).
//!
//! As process create/exit and image-load records arrive, the parse thread keeps
//! a [`ProcessManager`] up to date. Each tracked process is an `Arc<ProcessRecord>`
//! so attaching it to an [`crate::event::Event`] is a refcount bump, never a deep
//! copy of its module list (the per-event copy the C++ SDK paid for).

use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::sync::{Arc, OnceLock};

/// A loaded module (image) within a process.
#[derive(Debug, Clone)]
pub struct Module {
    pub base: u64,
    pub size: u32,
    /// Raw NT image path; convert with [`crate::path::VolumeMap::resolve`].
    pub path: String,
}

/// Identifying and security information captured at process creation.
#[derive(Debug, Clone, Default)]
pub struct ProcessInfo {
    pub seq: u32,
    pub pid: u32,
    pub parent_seq: u32,
    pub parent_pid: u32,
    pub session_id: u32,
    pub is_wow64: bool,
    pub create_time: i64,
    /// Logon session id (`LUID` high/low parts).
    pub authentication_id: (i32, u32),
    /// Raw user SID bytes, resolved to a name lazily (see [`crate::sid`]).
    pub user_sid: Option<Vec<u8>>,
    /// Integrity level RID (last sub-authority of the integrity SID).
    pub integrity_rid: Option<u32>,
    /// Whether token virtualization is enabled for the process.
    pub is_virtualized: bool,
    /// Raw NT image path; convert with [`crate::path::VolumeMap::resolve`].
    pub image_path: String,
    pub command_line: String,
}

/// Executable metadata extracted from the image file's resources (cf. C++
/// `CProcInfo`): version strings and the small/large icon bytes. Extracted
/// synchronously when the process is first seen and cached by image path; `None`
/// fields mean the resource was absent. Icon fields are the raw `RT_ICON` bytes
/// as stored in the PE (cf. `m_SmallIcon`/`m_LargeIcon`); the GUI turns them into
/// a displayable icon.
#[derive(Debug, Clone, Default)]
pub struct ProcessMeta {
    pub description: Option<String>,
    pub company: Option<String>,
    pub version: Option<String>,
    pub icon_small: Option<Vec<u8>>,
    pub icon_large: Option<Vec<u8>>,
}

/// A tracked process: its identity, loaded modules, image metadata, and its exit
/// time once it has exited.
pub struct ProcessRecord {
    pub info: ProcessInfo,
    /// Loaded modules, each behind an `Arc` so [`modules`](Self::modules) shares
    /// them with consumers (GUI detail view, PML writer) by refcount bump rather
    /// than deep-copying every module path on each call.
    modules: RwLock<Vec<Arc<Module>>>,
    /// Version + icon metadata, set synchronously when the process is first seen.
    /// Shared (`Arc`) with the [`crate::metadata::MetadataCache`] so every process
    /// of the same image points at one allocation instead of copying the icon
    /// bytes per process.
    meta: OnceLock<Arc<ProcessMeta>>,
    /// `Some(exit_time_ticks)` once the process has exited. The record is kept
    /// (not removed) so exit events and the process tree retain it, mirroring
    /// C++ `CProcess::MarkExit` / `CProcMgr::Remove`.
    exit: RwLock<Option<i64>>,
}

impl ProcessRecord {
    pub fn new(info: ProcessInfo) -> Arc<Self> {
        Arc::new(Self {
            info,
            modules: RwLock::new(Vec::new()),
            meta: OnceLock::new(),
            exit: RwLock::new(None),
        })
    }

    /// Marks the process as exited at `exit_time` (100-ns ticks).
    pub fn mark_exited(&self, exit_time: i64) {
        *self.exit.write() = Some(exit_time);
    }

    /// Whether the process has exited.
    pub fn is_exited(&self) -> bool {
        self.exit.read().is_some()
    }

    /// The process exit time (100-ns ticks), if it has exited.
    pub fn exit_time(&self) -> Option<i64> {
        *self.exit.read()
    }

    /// Appends a loaded module, skipping a base already present. The Toolhelp
    /// seed (modules already loaded when the process is first seen) and a later
    /// image-load event can both report the same module; keep it once.
    pub fn add_module(&self, module: Module) {
        let mut mods = self.modules.write();
        if mods.iter().any(|m| m.base == module.base) {
            return;
        }
        mods.push(Arc::new(module));
    }

    /// Number of modules loaded so far.
    pub fn module_count(&self) -> usize {
        self.modules.read().len()
    }

    /// Returns a snapshot of the module list (used by the GUI's detail view and
    /// the PML writer). Cloning each `Arc<Module>` is a refcount bump, so the
    /// module paths are shared, not deep-copied.
    pub fn modules(&self) -> Vec<Arc<Module>> {
        self.modules.read().clone()
    }

    /// The image metadata, if it has been resolved yet.
    pub fn meta(&self) -> Option<&ProcessMeta> {
        self.meta.get().map(Arc::as_ref)
    }

    /// Stores the (shared) image metadata; ignored if already set.
    pub fn set_meta(&self, meta: Arc<ProcessMeta>) {
        let _ = self.meta.set(meta);
    }
}

/// The live set of processes, indexed by both kernel sequence and PID.
///
/// The sequence id is the stable key (PIDs are reused), while the PID index maps
/// to the most recent sequence for that PID, mirroring the C++ manager.
pub struct ProcessManager {
    // FxHashMap: looked up once per emitted event; the integer keys are
    // kernel-assigned, so the default hasher's DoS resistance is not needed.
    by_seq: RwLock<FxHashMap<u32, Arc<ProcessRecord>>>,
    by_pid: RwLock<FxHashMap<u32, u32>>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            by_seq: RwLock::new(FxHashMap::default()),
            by_pid: RwLock::new(FxHashMap::default()),
        }
    }

    /// Inserts (or replaces) a process record, updating both indexes.
    pub fn insert(&self, record: Arc<ProcessRecord>) {
        let seq = record.info.seq;
        let pid = record.info.pid;
        self.by_seq.write().insert(seq, record);
        self.by_pid.write().insert(pid, seq);
    }

    /// Looks up a process by its kernel sequence id.
    pub fn by_seq(&self, seq: u32) -> Option<Arc<ProcessRecord>> {
        self.by_seq.read().get(&seq).cloned()
    }

    /// A snapshot of all tracked process records (for the GUI's process tree).
    /// Read-only: read-locks the table and clones the `Arc`s.
    pub fn snapshot(&self) -> Vec<Arc<ProcessRecord>> {
        self.by_seq.read().values().cloned().collect()
    }

    /// Looks up the most recent process for a PID.
    pub fn by_pid(&self, pid: u32) -> Option<Arc<ProcessRecord>> {
        let seq = *self.by_pid.read().get(&pid)?;
        self.by_seq(seq)
    }

    /// Marks a process as exited at `exit_time`, keeping its record so exit
    /// events and the process tree retain it (cf. C++ `CProcMgr::Remove`, which
    /// marks rather than deletes). Records are reclaimed only by [`clear`](Self::clear).
    pub fn mark_exited(&self, seq: u32, exit_time: i64) {
        if let Some(rec) = self.by_seq(seq) {
            rec.mark_exited(exit_time);
        }
    }

    /// Drops all tracked processes (cf. C++ `CProcMgr::Clear`).
    pub fn clear(&self) {
        self.by_seq.write().clear();
        self.by_pid.write().clear();
    }

    /// Appends a module to the process identified by sequence id, if tracked.
    pub fn add_module(&self, seq: u32, module: Module) {
        if let Some(rec) = self.by_seq(seq) {
            rec.add_module(module);
        }
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds a parent→child forest from a flat process list — the one algorithm
/// behind every process-tree view. `ids` yields an item's `(pid, parent_pid)`;
/// `make` builds an output node from an item and its already-built children.
/// Roots are items whose parent is absent from the pid set or self-referential;
/// a self-parented item never becomes its own child. O(n²) over the process
/// count, which is small in any capture.
///
/// Every item is placed at most once. Process tables are keyed by sequence, so
/// a reused pid appears as two items — without the guard, the by-pid child
/// match would attach the same subtree under both same-pid nodes, and a
/// reused-pid ancestry loop (X → Y → X') would recurse until stack overflow.
pub fn build_forest<T, N>(
    items: &[T],
    ids: impl Fn(&T) -> (u32, u32) + Copy,
    make: impl Fn(&T, Vec<N>) -> N + Copy,
) -> Vec<N> {
    fn children_of<T, N>(
        parent: u32,
        items: &[T],
        placed: &mut [bool],
        ids: impl Fn(&T) -> (u32, u32) + Copy,
        make: impl Fn(&T, Vec<N>) -> N + Copy,
    ) -> Vec<N> {
        let mut out = Vec::new();
        for (i, t) in items.iter().enumerate() {
            let (pid, ppid) = ids(t);
            if placed[i] || ppid != parent || pid == parent {
                continue;
            }
            placed[i] = true; // before recursing: breaks pid-reuse cycles
            out.push(make(t, children_of(pid, items, placed, ids, make)));
        }
        out
    }
    let pids: std::collections::HashSet<u32> = items.iter().map(|t| ids(t).0).collect();
    let mut placed = vec![false; items.len()];
    let mut forest = Vec::new();
    for (i, t) in items.iter().enumerate() {
        let (pid, ppid) = ids(t);
        if !pids.contains(&ppid) || ppid == pid {
            placed[i] = true;
            forest.push(make(t, children_of(pid, items, &mut placed, ids, make)));
        }
    }
    forest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(seq: u32, pid: u32) -> Arc<ProcessRecord> {
        ProcessRecord::new(ProcessInfo {
            seq,
            pid,
            ..Default::default()
        })
    }

    #[test]
    fn insert_and_lookup() {
        let mgr = ProcessManager::new();
        mgr.insert(rec(10, 100));
        assert!(mgr.by_seq(10).is_some());
        assert_eq!(mgr.by_pid(100).unwrap().info.seq, 10);
    }

    #[test]
    fn exit_marks_but_keeps_record() {
        let mgr = ProcessManager::new();
        mgr.insert(rec(10, 100));
        mgr.insert(rec(11, 100)); // same PID, new sequence
        mgr.mark_exited(10, 42); // old process exits
        let old = mgr.by_seq(10).expect("exited record retained");
        assert!(old.is_exited());
        assert_eq!(old.exit_time(), Some(42));
        // PID index still resolves to the newer sequence.
        assert_eq!(mgr.by_pid(100).unwrap().info.seq, 11);
    }

    #[test]
    fn build_forest_roots_children_and_self_parent() {
        struct Node(u32, Vec<Node>);
        // (pid, parent): 1 is a root (parent 99 unknown), 2/3 under 1, 4 under 2,
        // 5 is self-parented (root, not its own child).
        let items = [(1u32, 99u32), (2, 1), (3, 1), (4, 2), (5, 5)];
        let forest = build_forest(&items, |t| (t.0, t.1), |t, children| Node(t.0, children));
        assert_eq!(forest.len(), 2, "two roots");
        let one = forest.iter().find(|n| n.0 == 1).expect("root 1");
        assert_eq!(
            one.1.iter().map(|c| c.0).collect::<Vec<_>>(),
            vec![2, 3],
            "children keep list order"
        );
        assert_eq!(one.1[0].1[0].0, 4, "grandchild under 2");
        let five = forest.iter().find(|n| n.0 == 5).expect("root 5");
        assert!(five.1.is_empty(), "self-parent has no children");
    }

    #[test]
    fn build_forest_survives_pid_reuse_cycle() {
        struct Node(u32, Vec<Node>);
        fn count(n: &Node) -> usize {
            1 + n.1.iter().map(count).sum::<usize>()
        }
        // R(1) → X(2) → Y(3) → X'(pid 2 reused, parent 3): matching children by
        // pid alone sees X' under Y and Y under X' — an infinite recursion
        // without the placed guard. Every item must be placed exactly once.
        let items = [(1u32, 0u32), (2, 1), (3, 2), (2, 3)];
        let forest = build_forest(&items, |t| (t.0, t.1), |t, c| Node(t.0, c));
        assert_eq!(forest.len(), 1, "one root");
        assert_eq!(forest[0].0, 1, "root is R");
        assert_eq!(
            forest.iter().map(count).sum::<usize>(),
            items.len(),
            "every item placed exactly once"
        );
    }

    #[test]
    fn module_tracking() {
        let mgr = ProcessManager::new();
        mgr.insert(rec(10, 100));
        mgr.add_module(
            10,
            Module {
                base: 0x1000,
                size: 0x200,
                path: "a.dll".into(),
            },
        );
        assert_eq!(mgr.by_seq(10).unwrap().module_count(), 1);
    }
}
