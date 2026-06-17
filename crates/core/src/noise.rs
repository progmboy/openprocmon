//! The default noise-suppression rule set, ported verbatim from the GUI / SDK
//! example `advanced_display_rules` (`crates/gui/src/model/filter.rs`,
//! `crates/example/src/main.rs`) plus our own `procmon-cli.exe` exclusion.
//!
//! These are *exclude* predicates: [`crate::analyze::query`] drops an event when
//! it matches ANY of these clauses (with `exclude_noise = true`). Each clause
//! OR-s its own values, so the whole set is one OR-of-exclusions, matching the
//! GUI's set of Exclude rules.

use procmon_sdk::{Column, Relation};

use crate::query::Clause;

/// Builds a one-value-or-many exclude clause.
fn clause(column: Column, relation: Relation, values: &[&str]) -> Clause {
    Clause {
        column,
        relation,
        values: values.iter().map(|s| s.to_string()).collect(),
    }
}

/// The default noise filter: the monitoring tools (including this CLI) and the
/// System process, the IRP/FastIO bookkeeping operations, and NTFS metadata
/// files. Mirrors the GUI's `advanced_display_rules`, plus `procmon-cli.exe`.
pub fn default_noise() -> Vec<Clause> {
    vec![
        clause(
            Column::ProcessName,
            Relation::Is,
            &[
                "OpenProcmon.exe",
                "Procmon.exe",
                "Procexp.exe",
                "Autoruns.exe",
                "Procmon64.exe",
                "Procexp64.exe",
                "procmon-cli.exe",
                "System",
            ],
        ),
        clause(
            Column::Operation,
            Relation::BeginsWith,
            &["IRP_MJ_", "FASTIO_", "FAST IO"],
        ),
        clause(Column::Result, Relation::BeginsWith, &["FAST IO"]),
        clause(
            Column::Path,
            Relation::EndsWith,
            &[
                "pagefile.sys",
                "$Mft",
                "$MftMirr",
                "$LogFile",
                "$Volume",
                "$AttrDef",
                "$Root",
                "$Bitmap",
                "$Boot",
                "$BadClus",
                "$Secure",
                "$Upcase",
            ],
        ),
        clause(Column::Path, Relation::Contains, &["$Extend"]),
    ]
}
