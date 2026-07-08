//! Capture overview: totals, per-category counts, top processes, and an
//! event-rate sparkline. Ported from the GUI's `SummaryStats::from_rows`
//! (`crates/gui/src/dialogs/summary.rs`) on plain types — a parity test pins the
//! numbers to the GUI's. The GUI's per-process/per-path/xref summaries are NOT
//! ported; `query` + `group_by` covers them.

use std::sync::Arc;

use procmon_sdk::PmlReader;
use serde::Serialize;

use crate::record::Category;

/// Number of time bins in the rate sparklines — shared with the GUI's summary
/// dialog so both views bin identically.
pub const BINS: usize = 24;

/// The bin for event `i` of `total` under index binning: `(i*BINS)/total`,
/// clamped to the last bin (0 when `total` is 0). The one formula behind every
/// rate sparkline.
pub fn bin_index(i: usize, total: usize) -> usize {
    (i * BINS).checked_div(total).map_or(0, |b| b.min(BINS - 1))
}

/// One "top process" row.
#[derive(Clone, Debug, Serialize)]
pub struct ProcCount {
    pub name: String,
    pub count: u64,
}

/// Aggregated capture overview.
#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub total: u64,
    /// `(category, count)`, sorted by count desc.
    pub by_category: Vec<(Category, u64)>,
    /// Top-N processes by event count.
    pub top_processes: Vec<ProcCount>,
    /// Event-rate series (counts per time bin, `BINS` points), binned by event
    /// order over the total — same as the GUI's index binning.
    pub rate: Vec<u64>,
    /// Network-event series, same binning.
    pub net_rate: Vec<u64>,
}

fn cat_index(c: Category) -> usize {
    match c {
        Category::Registry => 0,
        Category::File => 1,
        Category::Network => 2,
        Category::Process => 3,
        Category::Profiling => 4,
        Category::Other => 5,
    }
}

/// Computes the overview over all events in `reader`. `top` caps the process
/// list. One parse pass (collecting per-event category + process name), then
/// index-binning — equivalent to the GUI computing over its retained rows.
pub fn summary(reader: &Arc<PmlReader>, top: usize) -> Summary {
    let mut cat_counts = [0u64; 6];
    let mut proc: rustc_hash::FxHashMap<String, u64> = rustc_hash::FxHashMap::default();
    // Per-event category, kept to bin the rate after we know the total.
    let mut seq: Vec<Category> = Vec::new();

    for ev in reader.events() {
        // Process-table seed records are bookkeeping, not activity.
        if ev.is_process_defined() {
            continue;
        }
        let cat: Category = ev.class().into();
        cat_counts[cat_index(cat)] += 1;
        *proc
            .entry(ev.process_name().unwrap_or("").to_string())
            .or_default() += 1;
        seq.push(cat);
    }

    let total = seq.len() as u64;
    let mut rate = vec![0u64; BINS];
    let mut net_rate = vec![0u64; BINS];
    for (i, cat) in seq.iter().enumerate() {
        let bin = bin_index(i, seq.len());
        rate[bin] += 1;
        if *cat == Category::Network {
            net_rate[bin] += 1;
        }
    }

    let mut by_category: Vec<(Category, u64)> = [
        Category::Registry,
        Category::File,
        Category::Network,
        Category::Process,
        Category::Profiling,
    ]
    .into_iter()
    .map(|c| (c, cat_counts[cat_index(c)]))
    .collect();
    by_category.sort_by_key(|c| std::cmp::Reverse(c.1));

    let mut top_processes: Vec<ProcCount> = proc
        .into_iter()
        .map(|(name, count)| ProcCount { name, count })
        .collect();
    top_processes.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    top_processes.truncate(top);

    Summary {
        total,
        by_category,
        top_processes,
        rate,
        net_rate,
    }
}
