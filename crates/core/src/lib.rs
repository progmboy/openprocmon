//! `procmon-core`: the gpui-free capture + PML-analysis core shared by the
//! OpenProcMon CLI and MCP server.
//!
//! OpenProcMon is a capture-then-analyze tool: a live capture writes a
//! Procmon-compatible `.PML`, and every analysis reads that file. This crate
//! owns the capture driver (P2) and the analysis layer — a set of pure
//! functions over a [`procmon_sdk::PmlReader`] returning serde-serializable
//! projections, consumed identically by the CLI (JSON) and the MCP tools.
//!
//! The one query primitive is [`analyze::query`] (filter + optional group-by),
//! which subsumes per-path / per-process / cross-reference aggregations; the
//! filter vocabulary is in [`vocab`]. The GUI is intentionally left untouched —
//! the aggregation/export math here is a parity-tested re-port, not a shared
//! extraction.

pub mod analyze;
pub mod capture;
pub mod export;
pub mod noise;
pub mod query;
pub mod record;
pub mod scope;
pub mod summary;
pub mod vocab;

pub use analyze::{
    get_event, get_process, list_processes, open_pml, pml_info, process_tree, query, EventDetail,
    PmlInfo, QueryResult,
};
pub use capture::{
    capture, parse_monitors, CaptureLimits, CaptureOutcome, CaptureSession, StoppedReason,
    TargetSpec,
};
pub use export::{export, Format};
pub use noise::default_noise;
pub use query::{parse_column, parse_filter, Clause, Expr, GroupRow};
pub use record::{Category, EventRecord, ModuleRow, ProcessDetail, ProcessNode, StackFrameRow};
pub use summary::{summary, ProcCount, Summary};
pub use vocab::{filter_vocab, FilterVocab};
