//! P0 verification: PML analysis over a real fixture (the committed,
//! zlib-compressed bench PML), exercising query/group_by, get_event,
//! get_process, process_tree, pml_info, summary, and serde_json round-trips.

use std::io::Read;
use std::sync::Arc;

use procmon_core::{
    filter_vocab, get_event, get_process, list_processes, parse_filter, pml_info, process_tree,
    query, summary, Field,
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
    let r = query(&f.reader, None, &[], &[], None, 0, 50, false);
    assert_eq!(r.total_matched, total, "no filter matches every event");
    assert_eq!(r.events.len(), 50.min(total as usize));
    assert!(r.groups.is_empty());
    serde_json::to_string(&r).unwrap();
}

#[test]
fn query_filter_and_group_by_path() {
    let f = fixture();
    // "what files were written" — Class=File & Op∈{write ops} grouped by Path.
    let filter = parse_filter(
        r#"Category == "File System" && Operation in (WriteFile, SetEndOfFileInformationFile)"#,
    )
    .expect("parse");
    let grouped = query(
        &f.reader,
        Some(&filter),
        &[],
        &[Field::Col(Column::Path)],
        None,
        0,
        20,
        false,
    );
    // Grouped result: distinct paths with counts, no raw events.
    assert!(grouped.events.is_empty());
    // Every group row's count must not exceed the total matched.
    for g in &grouped.groups {
        assert!(g.count <= grouped.total_matched);
    }
    // Raw (ungrouped) of the same filter returns events whose count == total.
    let raw = query(&f.reader, Some(&filter), &[], &[], None, 0, 1000, false);
    assert_eq!(raw.total_matched, grouped.total_matched);
}

#[test]
fn group_by_multi_column_and_metric() {
    let f = fixture();
    // Multi-column group-by: one `values` entry per grouped column, no aggregates
    // without a metric.
    let multi = query(
        &f.reader,
        None,
        &[],
        &[
            Field::Col(Column::ProcessName),
            Field::Col(Column::Operation),
        ],
        None,
        0,
        10,
        false,
    );
    assert!(!multi.groups.is_empty());
    for g in &multi.groups {
        assert_eq!(g.values.len(), 2, "two group columns -> two values");
        assert!(g.sum.is_none() && g.first_time.is_none());
    }
    // With a numeric metric every bucket carries sum/avg/min/max + first/last time.
    let agg = query(
        &f.reader,
        None,
        &[],
        &[Field::Col(Column::ProcessName)],
        Some(Field::Col(Column::Sequence)),
        0,
        10,
        false,
    );
    assert!(!agg.groups.is_empty());
    for g in &agg.groups {
        let (min, max, sum) = (g.min.unwrap(), g.max.unwrap(), g.sum.unwrap());
        assert!(min <= max, "min <= max");
        assert!(sum >= max, "sum of non-negative sequences >= max");
        assert!(g.avg.is_some() && g.first_time.is_some() && g.last_time.is_some());
    }
}

#[test]
fn clause_semantics_and_or() {
    // Cross-clause AND, in-clause OR, via the expression parser.
    let f = fixture();
    let a = parse_filter(r#"Category == "File System""#).unwrap();
    let raw = query(&f.reader, Some(&a), &[], &[], None, 0, 5, false);
    // Every returned event is a File event.
    for ev in &raw.events {
        assert_eq!(ev.category, procmon_core::Category::File);
    }
    // A contradictory AND (File AND Registry) matches nothing.
    let none_expr = parse_filter(r#"Category == "File System" && Category == Registry"#).unwrap();
    let none = query(&f.reader, Some(&none_expr), &[], &[], None, 0, 5, false);
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
fn get_event_resolves_kernel_and_user_stack_frames() {
    let f = fixture();
    // Requesting only the stack (not "process") must still resolve frames: user
    // frames against the originating process's modules, kernel frames against the
    // System (PID 4) driver modules. Scan early events for kernel frames.
    let parts = vec!["stack".to_string()];
    let (mut kernel_seen, mut kernel_resolved) = (0usize, 0usize);
    let (mut user_seen, mut user_resolved) = (0usize, 0usize);
    for seq in 0..200 {
        let Some(d) = get_event(&f.reader, seq, &parts) else {
            continue;
        };
        let Some(stack) = d.stack else { continue };
        for fr in &stack {
            let resolved = fr.module != "<UNKNOWN>";
            match fr.kind {
                "K" => {
                    kernel_seen += 1;
                    kernel_resolved += usize::from(resolved);
                }
                _ => {
                    user_seen += 1;
                    user_resolved += usize::from(resolved);
                }
            }
        }
    }
    assert!(kernel_seen > 0, "fixture should have kernel stack frames");
    // Every kernel frame in this fixture maps to a loaded driver (ntoskrnl.exe, …);
    // before the fix these were all `<UNKNOWN>` because no kernel module list was
    // consulted.
    assert_eq!(
        kernel_seen, kernel_resolved,
        "all kernel frames should resolve against System (PID 4) modules"
    );
    // User frames resolve even though "process" wasn't requested (a handful may sit
    // outside any tracked module); require the overwhelming majority to resolve.
    assert!(user_seen > 0, "fixture should have user stack frames");
    assert!(
        user_resolved * 10 >= user_seen * 9,
        "most user frames should resolve ({user_resolved}/{user_seen})"
    );
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
fn exclude_noise_drops_metadata_and_self() {
    let f = fixture();
    let all = query(&f.reader, None, &[], &[], None, 0, 1, false).total_matched;
    let clean = query(
        &f.reader,
        None,
        &procmon_core::default_noise(),
        &[],
        None,
        0,
        1,
        false,
    )
    .total_matched;
    assert!(clean <= all, "noise filter never adds events");
    // No surviving event is an NTFS-metadata path or a monitoring-tool process.
    let sample = query(
        &f.reader,
        None,
        &procmon_core::default_noise(),
        &[],
        None,
        0,
        500,
        false,
    );
    for ev in &sample.events {
        assert!(!ev.path.ends_with("$Mft"), "Mft leaked: {}", ev.path);
        assert_ne!(ev.process_name, "Procmon.exe");
        assert_ne!(ev.process_name, "procmon-cli.exe");
    }
}

#[test]
fn export_csv_and_xml_and_pml_roundtrip() {
    use procmon_core::{export, Format};
    let f = fixture();
    let filter = parse_filter(r#"Category == "File System""#).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("out.csv");
    let n = export(
        &f.reader,
        Format::Csv,
        Some(&filter),
        &[],
        false,
        csv.to_str().unwrap(),
    )
    .unwrap();
    let csv_text = std::fs::read_to_string(&csv).unwrap();
    assert!(csv_text.starts_with('\u{feff}'), "CSV has UTF-8 BOM");
    assert!(csv_text.contains("\"Time of Day\",\"Process Name\",\"PID\""));
    assert_eq!(
        csv_text.matches("\r\n").count(),
        n + 1,
        "header + n CRLF rows"
    );

    let xml = dir.path().join("out.xml");
    export(
        &f.reader,
        Format::Xml,
        Some(&filter),
        &[],
        true,
        xml.to_str().unwrap(),
    )
    .unwrap();
    let xml_text = std::fs::read_to_string(&xml).unwrap();
    assert!(xml_text.contains("<procmon><processlist>"));
    assert!(xml_text.contains("</processlist><eventlist>"));
    assert!(xml_text.contains("<Operation>"));

    // PML subset export re-opens and has only the filtered (File) events.
    let pml = dir.path().join("out.pml");
    let written = export(
        &f.reader,
        Format::Pml,
        Some(&filter),
        &[],
        false,
        pml.to_str().unwrap(),
    )
    .unwrap();
    let sub = PmlReader::open(&pml).expect("reopen subset");
    assert_eq!(sub.len(), written, "subset event count round-trips");
    assert!(written > 0);
}

#[test]
fn vocab_lists_real_operations() {
    let v = filter_vocab();
    assert!(v
        .columns
        .iter()
        .any(|c| c.name == "Process Name" && !c.description.is_empty()));
    assert!(v.relations.iter().any(|r| r == "contains"));
    assert!(v.operations.file.iter().any(|o| o == "WriteFile"));
    assert!(v.operations.registry.iter().any(|o| o.starts_with("Reg")));
    serde_json::to_string(&v).unwrap();
}
