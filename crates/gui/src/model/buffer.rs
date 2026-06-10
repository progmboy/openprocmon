//! The retained event buffer and its filtered view.
//!
//! `all` holds every captured row (the GUI is the layer that retains millions of
//! events, per the SDK design). `view` is the list of indices into `all` that
//! currently pass the monitor toggles + filter + search; the table renders from
//! `view`. Pushing a row updates the view incrementally; changing the filter,
//! search or toggles rebuilds it.
//!
//! Rows are [`CapturedEvent`]s stored by value (not `Clone`); display columns are
//! produced lazily via accessors. Filtering touches only the columns a rule needs
//! (scalars like pid/category stay zero-copy; only Path rules / search force the
//! lazy string columns). Analytics dialogs snapshot via [`EventSummaryRow`].

use crate::app::MonitorToggles;
use crate::model::domain::{CapturedEvent, CategoryCounts, EventSummaryRow};
use crate::model::filter::{category_enabled, FilterModel};

/// History ring-buffer limits (live capture only): drop the oldest events once the
/// retained bytes exceed `max_bytes` or they fall outside the `max_age_ticks`
/// window (`0` = no age limit). `None` retention means unbounded (offline/PML).
#[derive(Clone, Copy)]
pub struct Retention {
    pub max_bytes: usize,
    pub max_age_ticks: i64,
}

#[derive(Default)]
pub struct EventBuffer {
    all: Vec<CapturedEvent>,
    view: Vec<usize>,
    counts: CategoryCounts,
    filter: FilterModel,
    search: String,
    monitor: MonitorToggles,
    /// Rule set whose matches are marked as highlighted (same shape as `filter`).
    highlight: FilterModel,
    /// Active history limits (live only); `None` = unbounded.
    retention: Option<Retention>,
    /// Sum of `byte_size()` over `all`, maintained for the byte-based limit.
    total_bytes: usize,
}

impl EventBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Total captured rows (visible or not).
    pub fn total(&self) -> usize {
        self.all.len()
    }

    /// Rows currently visible under the active gating.
    pub fn visible_len(&self) -> usize {
        self.view.len()
    }

    /// Per-category running counts (for a future status bar / the SDK's
    /// `CountsChanged` event; the summary dialog aggregates from rows instead).
    #[allow(dead_code)]
    pub fn counts(&self) -> CategoryCounts {
        self.counts
    }

    /// The row at visible index `ix` (table row), if any.
    pub fn visible(&self, ix: usize) -> Option<&CapturedEvent> {
        self.view.get(ix).map(|&i| &self.all[i])
    }

    /// All captured rows (for the Save dialog's "All events" scope).
    pub fn rows(&self) -> &[CapturedEvent] {
        &self.all
    }

    /// Indices into [`rows`](Self::rows) of the currently visible rows.
    pub fn visible_indices(&self) -> &[usize] {
        &self.view
    }

    /// Number of currently highlighted rows (Save dialog "Highlighted" scope).
    pub fn highlighted_count(&self) -> usize {
        self.all.iter().filter(|r| r.highlighted()).count()
    }

    /// Owned, cloneable snapshot of the currently *visible* rows (after monitor
    /// toggles, filter and search) — used by the Tools analytics dialogs so their
    /// statistics reflect the active filter, not every captured event.
    pub fn summary_rows(&self) -> Vec<EventSummaryRow> {
        self.view
            .iter()
            .map(|&i| self.all[i].summary_row())
            .collect()
    }

    /// Appends a row, updating counts and the view if it passes gating, then
    /// applies the history limits (live only).
    pub fn push(&mut self, mut row: CapturedEvent) {
        self.counts.bump(row.category());
        let highlighted = self.is_highlighted(&row);
        row.set_highlighted(highlighted);
        let size = row.byte_size();
        let idx = self.all.len();
        let visible = self.passes(&row);
        self.total_bytes += size;
        self.all.push(row);
        if visible {
            self.view.push(idx);
        }
        if self.retention.is_some() {
            self.trim();
        }
    }

    /// Drops all rows and resets the view/counts (Clear display).
    pub fn clear(&mut self) {
        self.all.clear();
        self.view.clear();
        self.counts = CategoryCounts::default();
        self.total_bytes = 0;
    }

    /// Sets the history ring-buffer limits (`None` = unbounded). Applied immediately.
    pub fn set_retention(&mut self, retention: Option<Retention>) {
        self.retention = retention;
        if self.retention.is_some() {
            self.trim();
        }
    }

    /// Drops the oldest rows until the byte/age limits are satisfied, then rebases
    /// the view indices. Keeps at least the newest row.
    fn trim(&mut self) {
        let (max_bytes, max_age) = match self.retention {
            Some(r) => (r.max_bytes, r.max_age_ticks),
            None => return,
        };
        let n = self.all.len();
        if n == 0 {
            return;
        }
        let mut drop = 0;
        // Age window: drop rows older than (newest - max_age).
        if max_age > 0 {
            let cutoff = self.all[n - 1].time_raw().saturating_sub(max_age);
            while drop < n && self.all[drop].time_raw() < cutoff {
                drop += 1;
            }
        }
        // Byte budget: drop oldest until under, keeping at least one row.
        let mut bytes = self.total_bytes;
        for row in &self.all[..drop] {
            bytes -= row.byte_size();
        }
        while bytes > max_bytes && drop + 1 < n {
            bytes -= self.all[drop].byte_size();
            drop += 1;
        }
        if drop == 0 {
            return;
        }

        // Removing from the front shifts every view index (O(n)). Amortize it:
        // defer until a worthwhile batch has accrued, unless memory is well over
        // budget (so a burst can't blow past the limit). Deferred rows stay counted
        // and are re-checked cheaply (O(drop)) on the next push.
        let batch = (n / 16).max(1024);
        let hard_over = self.total_bytes > max_bytes + max_bytes / 4;
        if drop < batch && !hard_over {
            return;
        }

        self.total_bytes = bytes;
        self.all.drain(0..drop);
        // Rebase the view onto the shifted `all` indices.
        self.view.retain(|&i| i >= drop);
        for v in &mut self.view {
            *v -= drop;
        }
    }

    pub fn set_filter(&mut self, filter: FilterModel) {
        self.filter = filter;
        self.rebuild_view();
    }

    pub fn set_search(&mut self, search: String) {
        self.search = search;
        self.rebuild_view();
    }

    pub fn set_monitor(&mut self, monitor: MonitorToggles) {
        self.monitor = monitor;
        self.rebuild_view();
    }

    /// Sets the highlight rule set and re-marks all existing rows.
    pub fn set_highlight(&mut self, highlight: FilterModel) {
        self.highlight = highlight;
        for i in 0..self.all.len() {
            let h = self.highlight.highlights(&self.all[i]);
            self.all[i].set_highlighted(h);
        }
    }

    fn is_highlighted(&self, row: &CapturedEvent) -> bool {
        self.highlight.highlights(row)
    }

    /// Number of bookmarked rows (for the status bar).
    pub fn bookmark_count(&self) -> usize {
        self.all.iter().filter(|r| r.bookmarked()).count()
    }

    /// Toggles the bookmark flag on the row at visible index `ix`.
    pub fn toggle_bookmark(&mut self, ix: usize) {
        if let Some(&i) = self.view.get(ix) {
            let cur = self.all[i].bookmarked();
            self.all[i].set_bookmarked(!cur);
        }
    }

    fn rebuild_view(&mut self) {
        self.view.clear();
        for (i, row) in self.all.iter().enumerate() {
            if Self::passes_with(row, &self.filter, &self.search, &self.monitor) {
                self.view.push(i);
            }
        }
    }

    fn passes(&self, row: &CapturedEvent) -> bool {
        Self::passes_with(row, &self.filter, &self.search, &self.monitor)
    }

    fn passes_with(
        row: &CapturedEvent,
        filter: &FilterModel,
        search: &str,
        monitor: &MonitorToggles,
    ) -> bool {
        category_enabled(row.category(), monitor)
            && filter.matches(row)
            && search_matches(row, search)
    }
}

/// Case-insensitive search across the most useful columns. Compares in place
/// (ASCII case folding, the same the previous lowercase-copies performed) — no
/// per-row allocations.
fn search_matches(row: &CapturedEvent, search: &str) -> bool {
    if search.is_empty() {
        return true;
    }
    let q = search.as_bytes();
    let mut pid_buf = itoa_buf();
    contains_ci(row.process_name().as_ref(), q)
        || contains_ci(row.operation().as_ref(), q)
        || contains_ci(row.path_str(), q)
        || contains_ci(row.result().as_ref(), q)
        || contains_ci(fmt_u32(row.pid(), &mut pid_buf), q)
}

/// ASCII-case-insensitive substring test (byte windows; correct on UTF-8 since
/// multi-byte sequences are self-synchronizing and only ASCII bytes fold).
fn contains_ci(haystack: &str, needle: &[u8]) -> bool {
    let h = haystack.as_bytes();
    h.len() >= needle.len()
        && h.windows(needle.len())
            .any(|w| w.eq_ignore_ascii_case(needle))
}

/// Stack buffer + formatter for a `u32` (avoids a `to_string` per searched row).
fn itoa_buf() -> [u8; 10] {
    [0; 10]
}
fn fmt_u32(mut v: u32, buf: &mut [u8; 10]) -> &str {
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    // SAFETY-free: the written range is pure ASCII digits.
    std::str::from_utf8(&buf[i..]).expect("ascii digits")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::domain::{CapturedEvent, EventCategory};
    use crate::model::filter::{
        category_enabled, FilterAction, FilterColumn, FilterModel, FilterRelation, FilterRule,
    };
    use std::io::Read;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Loads the filesystem PML fixture and maps it to real `CapturedEvent`s — the
    /// same data path the live/PML sources use (the GUI has no synthetic rows).
    fn fixture_rows() -> Vec<CapturedEvent> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../sdk/tests/resources/CompressedLogFileUTC64FilesystemPML");
        let raw = std::fs::read(path).expect("fixture");
        let mut buf = Vec::new();
        flate2::read::ZlibDecoder::new(&raw[..])
            .read_to_end(&mut buf)
            .expect("unzip");
        // Unique temp name per call (tests run in parallel; the reader mmaps the file).
        static N: AtomicU64 = AtomicU64::new(0);
        let tmp = std::env::temp_dir().join(format!(
            "gui-buf-test-{}-{}.pml",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&tmp, &buf).expect("write");
        let reader = std::sync::Arc::new(procmon_sdk::PmlReader::open(&tmp).expect("open"));
        reader
            .events()
            .enumerate()
            .map(|(i, ev)| CapturedEvent::from_event(ev, i as u64 + 1))
            .collect()
    }

    fn filled() -> EventBuffer {
        let mut buf = EventBuffer::new();
        for r in fixture_rows() {
            buf.push(r);
        }
        assert!(buf.total() > 0, "empty fixture");
        buf
    }

    #[test]
    fn monitor_toggle_gates_view() {
        let mut buf = filled();
        let total = buf.total();
        assert_eq!(
            buf.visible_len(),
            total,
            "all categories visible by default"
        );

        // Snapshot every row's category (the view == all rows initially), then disable
        // the first row's category and assert exactly the still-enabled rows remain.
        let cats: Vec<EventCategory> = (0..total)
            .map(|i| buf.visible(i).unwrap().category())
            .collect();
        let mut m = MonitorToggles::default();
        match cats[0] {
            EventCategory::Registry => m.registry = false,
            EventCategory::File => m.file = false,
            EventCategory::Network => m.network = false,
            EventCategory::Process => m.process = false,
            EventCategory::Profiling => m.profiling = false,
            EventCategory::Other => {}
        }
        let expected = cats.iter().filter(|&&c| category_enabled(c, &m)).count();
        buf.set_monitor(m);
        assert_eq!(buf.visible_len(), expected);
        assert_eq!(
            buf.total(),
            total,
            "totals are unaffected; only the view is gated"
        );
    }

    #[test]
    fn exclude_filter_hides_matches() {
        let mut buf = filled();
        let total = buf.total();
        // Exclude one process name; the view drops exactly its rows.
        let name = buf.visible(0).unwrap().process_name().to_string();
        let n_name = (0..total)
            .filter(|&i| buf.visible(i).unwrap().process_name().as_ref() == name)
            .count();
        assert!(n_name > 0);
        buf.set_filter(FilterModel {
            rules: vec![FilterRule::new(
                FilterColumn::ProcessName,
                FilterRelation::Is,
                &name,
                FilterAction::Exclude,
            )],
        });
        assert_eq!(buf.visible_len(), total - n_name);
    }

    #[test]
    fn search_filters_view() {
        let mut buf = filled();
        // Search a substring present in some row; every visible row matches it.
        let op = buf.visible(0).unwrap().operation().to_string();
        assert!(!op.is_empty());
        let q = op.to_ascii_lowercase();
        buf.set_search(q.clone());
        assert!(buf.visible_len() > 0);
        for i in 0..buf.visible_len() {
            assert!(search_matches(buf.visible(i).unwrap(), &q));
        }
    }

    #[test]
    fn retention_trims_oldest_by_bytes() {
        let rows: Vec<CapturedEvent> = fixture_rows().into_iter().take(60).collect();
        let n = rows.len();
        assert!(n >= 4, "fixture too small");
        // Budget a quarter of the sample's bytes, so pushing them all overshoots the
        // 125% hard cap and forces a trim regardless of per-event size.
        let total_bytes: usize = rows.iter().map(|r| r.byte_size()).sum();
        let mut buf = EventBuffer::new();
        buf.set_retention(Some(Retention {
            max_bytes: total_bytes / 4,
            max_age_ticks: 0,
        }));
        for r in rows {
            buf.push(r);
        }
        assert!(
            buf.total() >= 1 && buf.total() < n,
            "expected trimming, total={}",
            buf.total()
        );
        // The view stays consistent with the retained rows (no filter active).
        assert_eq!(buf.visible_len(), buf.total());
    }
}
