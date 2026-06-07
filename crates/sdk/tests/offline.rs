//! Offline parser tests over recorded fixtures.
//!
//! Each `.bin` file under `tests/fixtures/` is a raw event batch captured by the
//! `record` example (the bytes beginning at `ProcmonMessageHeader::BATCH_OFFSET`).
//! Feeding them through `parse_block` exercises the full parsing path with real
//! kernel data and no live driver. When no fixtures are present the test is a
//! no-op, so the suite passes on a clean checkout and gains coverage once
//! fixtures are recorded.

use procmon_sdk::parse_block;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn fixture_files() -> Vec<PathBuf> {
    let dir = fixtures_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "bin").unwrap_or(false))
        .collect()
}

#[test]
fn fixtures_parse_without_panicking() {
    let files = fixture_files();
    if files.is_empty() {
        eprintln!("no fixtures recorded; run `cargo run -p procmon-example --example record`");
        return;
    }
    for path in files {
        let bytes = std::fs::read(&path).expect("read fixture");
        let events = parse_block(&bytes);
        // Every recorded batch should yield at least one event, and each event's
        // accessors must be callable (they would panic on a bad offset).
        assert!(
            !events.is_empty(),
            "no events parsed from {}",
            path.display()
        );
        for ev in &events {
            let _ = ev.operation_name();
            let _ = ev.result();
            let _ = ev.path();
            let _ = ev.detail();
            let _ = ev.call_stack();
        }
        eprintln!("{}: {} events", path.display(), events.len());
    }
}
