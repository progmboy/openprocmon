//! The default noise-suppression rule set for the analysis layer.
//!
//! These are *exclude* predicates: [`crate::analyze::query`] drops an event when
//! it matches ANY of these clauses (with `exclude_noise = true`).
//!
//! There is one source of truth — `procmon_sdk::default_display_filter` (shared
//! with the GUI's Advanced Output toggle and the SDK example). We project each
//! exclude [`Rule`](procmon_sdk::Rule) into a single-value [`Clause`] so the CLI/MCP
//! `exclude_noise` set can never drift from the GUI's.

use crate::query::Clause;

/// The default noise filter: our own tools (`procmon-gui.exe`, `procmon-cli.exe`,
/// `procmon-example.exe`) and the Sysinternals tools, the System process, the
/// IRP/FastIO bookkeeping operations, and NTFS metadata files — derived from
/// `procmon_sdk::default_display_filter` so it stays in lockstep with the GUI.
pub fn default_noise() -> Vec<Clause> {
    procmon_sdk::default_display_filter()
        .into_iter()
        .map(|r| Clause {
            column: r.column,
            relation: r.relation,
            values: vec![r.value],
        })
        .collect()
}
