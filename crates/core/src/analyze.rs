//! PML analysis: every read tool is a pure function over an `Arc<PmlReader>`.
//!
//! A live capture is just "produce a PML"; once finalized, all analysis reads
//! the file. `PmlReader` already provides random access, the process table with
//! modules, and lazy mmap, so these are thin projections over it.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;

use procmon_sdk::{Column, Event, PmlReader, Relation, Result};
use serde::Serialize;

use crate::query::{Clause, Expr, Field, GroupRow, Grouper};
use crate::record::{EventRecord, ModuleRow, ProcessDetail, ProcessNode, StackFrameRow};

/// Opens (and indexes) a `.PML` file. The reader is shared (`Arc`) because
/// `events()` requires it and callers cache it per file.
pub fn open_pml(path: impl AsRef<Path>) -> Result<Arc<PmlReader>> {
    Ok(Arc::new(PmlReader::open(path)?))
}

/// Result of [`query`]: either a page of events or, when grouped, the
/// aggregation rows. `total_matched` is the count of events passing the filter
/// (and noise) before paging; `truncated` means more rows existed than returned.
#[derive(Clone, Debug, Serialize)]
pub struct QueryResult {
    pub total_matched: u64,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<EventRecord>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<GroupRow>,
}

/// The universal query. `filter` (a parsed filter expression, `None` = match
/// all) selects events; `noise` drops events matching ANY noise clause (the
/// `exclude_noise` rule set, empty to disable). Without `group_by` it returns a
/// page of events; with it, the distinct values of that column + counts (top
/// `limit`, after `offset`).
#[allow(clippy::too_many_arguments)]
pub fn query(
    reader: &Arc<PmlReader>,
    filter: Option<&Expr>,
    noise: &[Clause],
    group_by: &[Field],
    metric: Option<Field>,
    offset: usize,
    limit: usize,
    include_detail: bool,
) -> QueryResult {
    let passes = |ev: &Event| event_passes(ev, filter, noise);

    if !group_by.is_empty() {
        let mut grouper = Grouper::new(group_by.to_vec(), metric);
        let mut total = 0u64;
        for ev in reader.events() {
            if passes(&ev) {
                total += 1;
                grouper.observe(&ev);
            }
        }
        let all = grouper.into_rows();
        let truncated = all.len() > offset + limit;
        let groups = all.into_iter().skip(offset).take(limit).collect();
        QueryResult {
            total_matched: total,
            truncated,
            events: Vec::new(),
            groups,
        }
    } else {
        let mut total = 0u64;
        let mut events = Vec::new();
        for (i, ev) in reader.events().enumerate() {
            if !passes(&ev) {
                continue;
            }
            let matched = total as usize;
            if matched >= offset && events.len() < limit {
                events.push(EventRecord::from_event(&ev, i as u64, include_detail));
            }
            total += 1;
        }
        let truncated = total as usize > offset + limit;
        QueryResult {
            total_matched: total,
            truncated,
            events,
            groups: Vec::new(),
        }
    }
}

/// Whether `ev` passes `filter` and is not matched by any `noise` clause,
/// sharing one per-event column memo across both (the default noise set alone
/// has 13 `Path` clauses — without the memo each derives Path independently).
pub(crate) fn event_passes(ev: &Event, filter: Option<&Expr>, noise: &[Clause]) -> bool {
    let mut memo = procmon_sdk::ColumnMemo::new();
    filter.is_none_or(|f| f.matches_memo(ev, &mut memo))
        && !noise.iter().any(|c| c.matches_memo(ev, &mut memo))
}

/// State-changing file / registry / process operations — the "significant"
/// activity a timeline keeps when reads / queries / closes are folded away.
/// Network is kept wholesale (every endpoint touch matters), handled separately.
const SIGNIFICANT_OPS: &[&str] = &[
    "CreateFile",
    "WriteFile",
    "SetEndOfFileInformationFile",
    "SetAllocationInformationFile",
    "SetRenameInformationFile",
    "SetBasicInformationFile",
    "SetDispositionInformationFile",
    "DeleteFile",
    "RegCreateKey",
    "RegSetValue",
    "RegDeleteKey",
    "RegDeleteValue",
    "RegRenameKey",
    "RegSetInfoKey",
    "Process Create",
    "Process Exit",
    "Process Start",
    "Thread Create",
    "Load Image",
];

/// A process's activity as a time-ordered list (the "process timeline"). By
/// default (`include_reads = false`) only state-changing file/registry/process
/// operations plus all network activity are kept — reads / queries / closes are
/// folded away — and the default noise filter is applied. `include_reads = true`
/// returns every event for the pid. Events come back in capture (= time) order.
pub fn process_timeline(
    reader: &Arc<PmlReader>,
    pid: u32,
    include_reads: bool,
    limit: usize,
) -> QueryResult {
    let pid_clause = Expr::Clause(Clause {
        column: Field::Col(Column::Pid),
        relation: Relation::Is,
        values: vec![pid.to_string()],
    });
    let filter = if include_reads {
        pid_clause
    } else {
        // All network activity OR any state-changing file/registry/process op.
        let network = Expr::Clause(Clause {
            column: Field::Col(Column::Class),
            relation: Relation::Is,
            values: vec!["Network".to_string()],
        });
        let significant = Expr::Clause(Clause {
            column: Field::Col(Column::Operation),
            relation: Relation::Is,
            values: SIGNIFICANT_OPS.iter().map(|s| s.to_string()).collect(),
        });
        Expr::And(vec![pid_clause, Expr::Or(vec![network, significant])])
    };
    let noise = crate::default_noise();
    query(reader, Some(&filter), &noise, &[], None, 0, limit, false)
}

/// A window of events around a center `seq` (the "context" view): the events just
/// before and after it, optionally restricted to the same process.
#[derive(Clone, Debug, Serialize)]
pub struct EventWindow {
    pub center_seq: u64,
    pub events: Vec<EventRecord>,
}

/// Up to `before` events preceding `seq` and `after` following it (plus `seq`
/// itself), optionally restricted to the center event's process. Uses a sliding
/// pre-window and stops once `after` are collected, so it never scans the whole
/// file past the window. `None` if `seq` is out of range.
pub fn event_window(
    reader: &Arc<PmlReader>,
    seq: usize,
    before: usize,
    after: usize,
    same_process: bool,
) -> Option<EventWindow> {
    let pid = reader.event_as_event(seq).ok()?.pid();
    let mut pre: VecDeque<EventRecord> = VecDeque::with_capacity(before + 1);
    let mut center = None;
    let mut post: Vec<EventRecord> = Vec::with_capacity(after);
    for (i, ev) in reader.events().enumerate() {
        if same_process && ev.pid() != pid {
            continue;
        }
        match i.cmp(&seq) {
            std::cmp::Ordering::Less => {
                pre.push_back(EventRecord::from_event(&ev, i as u64, false));
                if pre.len() > before {
                    pre.pop_front();
                }
            }
            std::cmp::Ordering::Equal => {
                center = Some(EventRecord::from_event(&ev, i as u64, false));
            }
            std::cmp::Ordering::Greater => {
                if post.len() >= after {
                    break;
                }
                post.push(EventRecord::from_event(&ev, i as u64, false));
            }
        }
    }
    let mut events: Vec<EventRecord> = pre.into_iter().collect();
    events.extend(center);
    events.extend(post);
    Some(EventWindow {
        center_seq: seq as u64,
        events,
    })
}

/// Full detail of one event (the "detail panel"): the event, its originating
/// process, and the resolved call stack. `parts` selects which to include to
/// bound token cost.
#[derive(Clone, Debug, Serialize)]
pub struct EventDetail {
    pub event: EventRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process: Option<ProcessDetail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<Vec<StackFrameRow>>,
}

/// Looks up event `seq` and builds its detail. `parts` ⊆ {"event","process",
/// "stack"}; "event" is always present. Returns `None` if the index is invalid.
pub fn get_event(reader: &Arc<PmlReader>, seq: usize, parts: &[String]) -> Option<EventDetail> {
    let ev = reader.event_as_event(seq).ok()?;
    let want = |p: &str| parts.is_empty() || parts.iter().any(|x| x.eq_ignore_ascii_case(p));

    let process = want("process")
        .then(|| find_process(reader, ev.pid()))
        .flatten();
    let stack = want("stack").then(|| {
        // A stack always resolves against the originating process's user modules
        // and the System (PID 4) kernel-driver modules — independent of whether the
        // `process` detail itself was requested (else every frame is `<UNKNOWN>`).
        // Reuse the process detail if it was already fetched.
        let proc_mods = process
            .as_ref()
            .map(|p| p.modules.clone())
            .or_else(|| find_process(reader, ev.pid()).map(|p| p.modules))
            .unwrap_or_default();
        // Kernel-mode frames resolve against the System (PID 4) driver modules,
        // exactly as the GUI's `frame_module` does.
        let kernel_mods = find_process(reader, 4)
            .map(|p| p.modules)
            .unwrap_or_default();
        resolve_stack(&ev, &proc_mods, &kernel_mods)
    });
    Some(EventDetail {
        event: EventRecord::from_event(&ev, seq as u64, true),
        process,
        stack,
    })
}

/// Full detail (+ modules) of the process with `pid` (first match if a PID was
/// reused).
pub fn get_process(reader: &Arc<PmlReader>, pid: u32) -> Option<ProcessDetail> {
    find_process(reader, pid)
}

fn find_process(reader: &Arc<PmlReader>, pid: u32) -> Option<ProcessDetail> {
    reader
        .processes()
        .find(|p| p.pid == pid)
        .map(ProcessDetail::from_pml)
}

/// All processes seen in the capture, flat.
pub fn list_processes(reader: &Arc<PmlReader>) -> Vec<ProcessNode> {
    reader.processes().map(ProcessNode::from_pml).collect()
}

/// The parent→child process tree. Roots are processes whose parent pid is not
/// itself a captured process (or is self-parented).
pub fn process_tree(reader: &Arc<PmlReader>) -> Vec<ProcessNode> {
    let nodes: Vec<ProcessNode> = reader.processes().map(ProcessNode::from_pml).collect();
    let pids: rustc_hash::FxHashSet<u32> = nodes.iter().map(|n| n.pid).collect();

    fn build(parent_pid: u32, nodes: &[ProcessNode]) -> Vec<ProcessNode> {
        nodes
            .iter()
            .filter(|n| n.parent_pid == parent_pid && n.pid != parent_pid)
            .map(|n| {
                let mut node = n.clone();
                node.children = build(n.pid, nodes);
                node
            })
            .collect()
    }

    nodes
        .iter()
        .filter(|n| !pids.contains(&n.parent_pid) || n.parent_pid == n.pid)
        .map(|n| {
            let mut node = n.clone();
            node.children = build(n.pid, &nodes);
            node
        })
        .collect()
}

/// `.PML` metadata, read from the header without scanning events.
#[derive(Clone, Debug, Serialize)]
pub struct PmlInfo {
    pub event_count: u32,
    pub computer_name: String,
    pub is_64bit: bool,
    pub windows_build: u32,
    pub process_count: usize,
}

pub fn pml_info(reader: &Arc<PmlReader>) -> PmlInfo {
    let h = reader.header();
    PmlInfo {
        event_count: h.number_of_events,
        computer_name: h.computer_name.clone(),
        is_64bit: h.is_64bit,
        windows_build: h.windows_build,
        process_count: reader.processes().count(),
    }
}

/// Resolves each call-stack frame address to `module+offset` (cf. the GUI's
/// `frame_module`). User-mode frames resolve against the originating process's
/// modules, kernel-mode frames against the System (PID 4) driver modules; both
/// lists are searched so either kind resolves. Kernel vs user is inferred from the
/// high address bits.
fn resolve_stack(
    ev: &Event,
    proc_mods: &[ModuleRow],
    kernel_mods: &[ModuleRow],
) -> Vec<StackFrameRow> {
    ev.call_stack()
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let addr = f.address();
            let kind = if addr >= 0xFFFF_0000_0000_0000 {
                "K"
            } else {
                "U"
            };
            let (module, location, path) = proc_mods
                .iter()
                .chain(kernel_mods.iter())
                .find(|m| m.size > 0 && addr >= m.base && addr < m.base.saturating_add(m.size))
                .map(|m| {
                    (
                        m.name.clone(),
                        format!("{} + 0x{:x}", m.name, addr - m.base),
                        m.path.clone(),
                    )
                })
                .unwrap_or_else(|| {
                    (
                        "<UNKNOWN>".to_string(),
                        format!("0x{addr:016x}"),
                        String::new(),
                    )
                });
            StackFrameRow {
                frame: i as u32,
                kind,
                module,
                location,
                address: format!("0x{addr:x}"),
                path,
            }
        })
        .collect()
}
