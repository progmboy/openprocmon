//! The filter vocabulary (`list_filter_columns`): the exact column names,
//! relation names, and per-category operation names the AI must use verbatim in
//! `query_events` filters. Derived from the SDK so it never drifts.

use procmon_sdk::kernel_types::{proc_notify, reg_notify, FILE_NOTIFY_BASE};
use procmon_sdk::{strings, Column, NetOp, Relation};
use serde::Serialize;

/// The complete filter vocabulary for building `query_events` clauses.
#[derive(Clone, Debug, Serialize)]
pub struct FilterVocab {
    /// Column names usable in a clause's `column` field.
    pub columns: Vec<String>,
    /// Relation names usable in a clause's `relation` field.
    pub relations: Vec<String>,
    /// Exact `Operation` column values, grouped by category — so the AI filters
    /// `Operation is WriteFile` with a real name, not a guess.
    pub operations: Operations,
}

/// Operation names per category.
#[derive(Clone, Debug, Serialize)]
pub struct Operations {
    pub process: Vec<String>,
    pub file: Vec<String>,
    pub registry: Vec<String>,
    pub network: Vec<String>,
}

/// Collects the distinct, known (`!= "<Unknown>"`) names a code-range maps to.
fn distinct(names: impl Iterator<Item = &'static str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for n in names {
        if n != "<Unknown>" && !out.iter().any(|x| x == n) {
            out.push(n.to_string());
        }
    }
    out
}

/// Builds the vocabulary (no event data needed).
pub fn filter_vocab() -> FilterVocab {
    let columns = Column::ALL.iter().map(|c| c.label().to_string()).collect();
    let relations = Relation::ALL
        .iter()
        .map(|r| r.label().to_string())
        .collect();

    let process = distinct((0..=proc_notify::SYSTEM_PERFORMANCE).map(strings::process_operation));
    let registry = distinct((0..=reg_notify::QUERYKEYSECURITY).map(strings::reg_operation));
    // File ops: the IRP major names (advanced display, minor 0 — major-level
    // names are what the Operation column carries for filtering).
    let file = distinct(
        (0u16..0x40).map(|maj| strings::file_operation(FILE_NOTIFY_BASE + maj, 0, false, true)),
    );
    let network = distinct((0u16..=9).map(|c| NetOp::from_pml(c).name()));

    FilterVocab {
        columns,
        relations,
        operations: Operations {
            process,
            file,
            registry,
            network,
        },
    }
}
