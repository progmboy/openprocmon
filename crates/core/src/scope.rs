//! Capture-time process scoping: which events get written to the PML.
//!
//! The "who" block of a capture is dynamic — a target by name catches future
//! processes of that name, and `include_children` follows descendants whose
//! parent is already in scope. This can't be a static `FilterSet` (children have
//! runtime-discovered pids/names), so it lives here, seeded at capture start and
//! grown per event. Identity uses both pid and the kernel process sequence
//! (stable across pid reuse) where available.

use procmon_sdk::Event;
use rustc_hash::FxHashSet;

/// The live set of in-scope processes during a capture.
pub struct PidScope {
    /// Lowercased target image basenames (match present + future processes).
    names: Vec<String>,
    /// In-scope pids (seeds + discovered children/name-matches).
    pids: FxHashSet<u32>,
    /// In-scope kernel process sequences (stronger identity than pid).
    seqs: FxHashSet<i32>,
    include_children: bool,
    /// No targets given → capture everything.
    capture_all: bool,
}

impl PidScope {
    /// Seeds the scope from target names (lowercased here) and explicit pids.
    /// With no names and no pids, the scope captures everything.
    pub fn new(names: &[String], pids: &[u32], include_children: bool) -> Self {
        PidScope {
            names: names.iter().map(|n| n.to_ascii_lowercase()).collect(),
            pids: pids.iter().copied().collect(),
            seqs: FxHashSet::default(),
            include_children,
            capture_all: names.is_empty() && pids.is_empty(),
        }
    }

    /// Explicitly adds a pid (e.g. a `launch`ed process).
    pub fn add_pid(&mut self, pid: u32) {
        self.pids.insert(pid);
    }

    /// Whether `ev` belongs to an in-scope process.
    pub fn contains(&self, ev: &Event) -> bool {
        self.contains_raw(ev.pid(), ev.process_seq())
    }

    fn contains_raw(&self, pid: u32, seq: i32) -> bool {
        self.capture_all || self.pids.contains(&pid) || (seq != 0 && self.seqs.contains(&seq))
    }

    /// Updates the scope from `ev`: a process whose name matches a target, or
    /// whose parent is already in scope (when following children), is added.
    pub fn observe(&mut self, ev: &Event) {
        self.observe_raw(
            ev.pid(),
            ev.process_seq(),
            ev.parent_pid(),
            ev.process_name(),
        );
    }

    fn observe_raw(&mut self, pid: u32, seq: i32, parent_pid: Option<u32>, name: Option<&str>) {
        if self.capture_all {
            return;
        }
        let name_hit = name.is_some_and(|n| {
            let lower = n.to_ascii_lowercase();
            self.names.contains(&lower)
        });
        let child_hit = self.include_children
            && parent_pid.is_some_and(|p| self.pids.contains(&p))
            && !self.pids.contains(&pid);
        if name_hit || child_hit {
            self.pids.insert(pid);
            if seq != 0 {
                self.seqs.insert(seq);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_targets_capture_all() {
        let s = PidScope::new(&[], &[], true);
        assert!(s.contains_raw(123, 5));
        assert!(s.contains_raw(999, 0));
    }

    #[test]
    fn name_match_seeds_future_process() {
        let mut s = PidScope::new(&["notepad.exe".into()], &[], false);
        assert!(!s.contains_raw(100, 1));
        // A process named NOTEPAD.EXE appears (case-insensitive) — now in scope.
        s.observe_raw(100, 1, Some(4), Some("NOTEPAD.EXE"));
        assert!(s.contains_raw(100, 1));
        // A different process is not.
        assert!(!s.contains_raw(200, 2));
    }

    #[test]
    fn children_followed_when_enabled() {
        let mut s = PidScope::new(&[], &[100], true);
        assert!(s.contains_raw(100, 0)); // seed pid
                                         // A child whose parent (100) is in scope is added.
        s.observe_raw(101, 7, Some(100), Some("child.exe"));
        assert!(s.contains_raw(101, 7));
        // A grandchild (parent 101 now in scope) is added too.
        s.observe_raw(102, 8, Some(101), Some("grand.exe"));
        assert!(s.contains_raw(102, 8));
        // An unrelated process is not.
        s.observe_raw(500, 9, Some(4), Some("other.exe"));
        assert!(!s.contains_raw(500, 9));
    }

    #[test]
    fn children_not_followed_when_disabled() {
        let mut s = PidScope::new(&[], &[100], false);
        s.observe_raw(101, 7, Some(100), Some("child.exe"));
        assert!(!s.contains_raw(101, 7), "children excluded when disabled");
    }
}
