//! P0 verification: PML analysis over a real fixture (the committed,
//! zlib-compressed bench PML), exercising query/group_by, get_event,
//! get_process, process_tree, pml_info, summary, and serde_json round-trips.

use std::io::Read;
use std::sync::Arc;

use procmon_core::{
    filter_vocab, get_event, get_process, list_processes, parse_clause_str, pml_info, process_tree,
    query, resolve_clauses, summary, Clause, RawClause,
};
use procmon_sdk::{Column, PmlReader};

/// Decompresses a committed fixture to a self-deleting temp file and opens it.
struct Fixture {
    reader: Arc<PmlReader>,
    _path: tempfile::TempPath,
}

fn open(name: &str) -> Fixture {
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../sdk/tests/resources")
            .join(name),
    )
    .expect("read fixture");
    let mut buf = Vec::new();
    flate2::read::ZlibDecoder::new(&raw[..])
        .read_to_end(&mut buf)
        .expect("unzip");
    let tmp = tempfile::NamedTempFile::new().expect("temp");
    std::fs::write(tmp.path(), &buf).expect("write");
    let path = tmp.into_temp_path();
    let reader = Arc::new(PmlReader::open(&path).expect("open"));
    Fixture {
        reader,
        _path: path,
    }
}

/// The large bench fixture has the most variety; fall back to the filesystem
/// fixture name if the bench one is absent.
fn fixture() -> Fixture {
    let bench = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../sdk/tests/resources/CompressedLogFileBench64PML");
    if bench.exists() {
        open("CompressedLogFileBench64PML")
    } else {
        open("CompressedLogFileUTC64FilesystemPML")
    }
}

#[test]
fn pml_info_and_processes() {
    let f = fixture();
    let info = pml_info(&f.reader);
    assert!(info.event_count > 0, "fixture has events");
    assert!(info.process_count > 0, "fixture has processes");
    assert!(!list_processes(&f.reader).is_empty());
    assert!(!process_tree(&f.reader).is_empty());
    // serde round-trips.
    serde_json::to_string(&info).unwrap();
}

#[test]
fn query_unfiltered_paginates() {
    let f = fixture();
    let total = pml_info(&f.reader).event_count as u64;
    let r = query(&f.reader, &[], &[], None, 0, 50, false);
    assert_eq!(r.total_matched, total, "no filter matches every event");
    assert_eq!(r.events.len(), 50.min(total as usize));
    assert!(r.groups.is_empty());
    serde_json::to_string(&r).unwrap();
}

#[test]
fn query_filter_and_group_by_path() {
    let f = fixture();
    // "what files were written" — Class=File & Op∈{write ops} grouped by Path.
    let clauses: Vec<Clause> = resolve_clauses(vec![
        RawClause {
            column: "Category".into(),
            relation: "is".into(),
            value: procmon_core::query::OneOrMany::One("File System".into()),
        },
        RawClause {
            column: "Operation".into(),
            relation: "is".into(),
            value: procmon_core::query::OneOrMany::Many(vec![
                "WriteFile".into(),
                "SetEndOfFileInformationFile".into(),
            ]),
        },
    ])
    .expect("resolve");
    let grouped = query(&f.reader, &clauses, &[], Some(Column::Path), 0, 20, false);
    // Grouped result: distinct paths with counts, no raw events.
    assert!(grouped.events.is_empty());
    // Every group row's count must not exceed the total matched.
    for g in &grouped.groups {
        assert!(g.count <= grouped.total_matched);
    }
    // Raw (ungrouped) of the same filter returns events whose count == total.
    let raw = query(&f.reader, &clauses, &[], None, 0, 1000, false);
    assert_eq!(raw.total_matched, grouped.total_matched);
}

#[test]
fn clause_semantics_and_or() {
    // Cross-clause AND, in-clause OR, via the public string parser.
    let f = fixture();
    let a = parse_clause_str("Category is File System").unwrap();
    let raw = query(&f.reader, std::slice::from_ref(&a), &[], None, 0, 5, false);
    // Every returned event is a File event.
    for ev in &raw.events {
        assert_eq!(ev.category, procmon_core::Category::File);
    }
    // A contradictory AND (File AND Registry) matches nothing.
    let b = parse_clause_str("Category is Registry").unwrap();
    let none = query(&f.reader, &[a, b], &[], None, 0, 5, false);
    assert_eq!(none.total_matched, 0, "File AND Registry is empty");
}

#[test]
fn get_event_detail_and_process() {
    let f = fixture();
    // First event's detail, all parts.
    let parts = vec![
        "event".to_string(),
        "process".to_string(),
        "stack".to_string(),
    ];
    let d = get_event(&f.reader, 0, &parts).expect("event 0 detail");
    assert_eq!(d.event.seq, 0);
    assert!(d.event.detail.is_some(), "detail requested");
    serde_json::to_string(&d).unwrap();

    // get_process for that event's pid resolves with modules where present.
    let pid = d.event.pid;
    if let Some(p) = get_process(&f.reader, pid) {
        assert_eq!(p.pid, pid);
        serde_json::to_string(&p).unwrap();
    }
}

#[test]
fn summary_matches_pml_total() {
    let f = fixture();
    let s = summary(&f.reader, 6);
    let total = pml_info(&f.reader).event_count as u64;
    assert_eq!(s.total, total, "summary total == event count");
    let by_cat: u64 = s.by_category.iter().map(|(_, n)| n).sum();
    assert_eq!(by_cat, total, "category counts sum to total");
    assert_eq!(s.rate.len(), 24);
    assert!(s.top_processes.len() <= 6);
    serde_json::to_string(&s).unwrap();
}

#[test]
fn vocab_lists_real_operations() {
    let v = filter_vocab();
    assert!(v.columns.iter().any(|c| c == "Process Name"));
    assert!(v.relations.iter().any(|r| r == "contains"));
    assert!(v.operations.file.iter().any(|o| o == "WriteFile"));
    assert!(v.operations.registry.iter().any(|o| o.starts_with("Reg")));
    serde_json::to_string(&v).unwrap();
}
