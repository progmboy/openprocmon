//! SDK performance baseline (CPU + memory), to quantify hot-path optimizations.
//!
//! Run with `cargo bench -p procmon-sdk --bench baseline`. Two data sources:
//!
//! - **PML phases** parse `tests/resources/Logfile.PML` (a large, real capture —
//!   not committed; the phases are skipped when it is absent, e.g. in CI) and
//!   measure open/index, full event materialization, column extraction
//!   (path/detail/operation/result), and filter evaluation.
//! - **live/ingest** feeds synthetic wire-format batches (file create/read/write,
//!   registry open/query/set, with PRE/POST correlation) through `Correlator` —
//!   the exact path the zero-copy refactor targets — so it needs no fixture.
//!
//! Memory is tracked by a counting global allocator (alloc count, bytes
//! allocated, retained = live delta, peak live). The counter adds a small,
//! constant overhead to every allocation; numbers are comparable across runs of
//! this bench, not against externally profiled figures.
//!
//! Under `cargo test --all-targets` (debug) this binary also runs; it then
//! switches to a quick single-iteration mode so CI stays fast.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use procmon_sdk::filter::{Action, Column, FilterSet, Relation, Rule};
use procmon_sdk::kernel_types::{
    file_opt, irp_mj, proc_notify, reg_notify, LogEntry, LogProcessCreate, LogRegCreateOpenKey,
    LogRegQueryValueKey, LogRegSetValueKey, FILE_NOTIFY_BASE, LOG_ENTRY_SIZE, STATUS_PENDING,
};
use procmon_sdk::parse::Correlator;
use procmon_sdk::{Event, PmlReader, ProcessManager};

// ---------------------------------------------------------------------------
// Counting allocator
// ---------------------------------------------------------------------------

static ALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicI64 = AtomicI64::new(0);
static PEAK_BYTES: AtomicI64 = AtomicI64::new(0);

struct CountingAlloc;

impl CountingAlloc {
    fn on_alloc(size: usize) {
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(size as u64, Ordering::Relaxed);
        let live = LIVE_BYTES.fetch_add(size as i64, Ordering::Relaxed) + size as i64;
        PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
    }

    fn on_dealloc(size: usize) {
        LIVE_BYTES.fetch_sub(size as i64, Ordering::Relaxed);
    }
}

// SAFETY: defers all allocation to `System`; the counters are simple atomics.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            Self::on_alloc(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        Self::on_dealloc(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = System.realloc(ptr, layout, new_size);
        if !p.is_null() {
            Self::on_dealloc(layout.size());
            Self::on_alloc(new_size);
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc_zeroed(layout);
        if !p.is_null() {
            Self::on_alloc(layout.size());
        }
        p
    }
}

#[global_allocator]
static ALLOCATOR: CountingAlloc = CountingAlloc;

/// Snapshot of the allocator counters.
#[derive(Clone, Copy)]
struct AllocSnap {
    calls: u64,
    bytes: u64,
    live: i64,
}

fn alloc_snap() -> AllocSnap {
    AllocSnap {
        calls: ALLOC_CALLS.load(Ordering::Relaxed),
        bytes: ALLOC_BYTES.load(Ordering::Relaxed),
        live: LIVE_BYTES.load(Ordering::Relaxed),
    }
}

/// Resets the peak to the current live size (so each phase reports its own peak).
fn reset_peak() {
    PEAK_BYTES.store(LIVE_BYTES.load(Ordering::Relaxed), Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Phase harness
// ---------------------------------------------------------------------------

struct PhaseResult {
    name: &'static str,
    /// Median / best wall time over the timing iterations.
    median: Duration,
    best: Duration,
    /// Work units per timed run (events), for throughput; 0 = not applicable.
    units: u64,
    /// Allocator stats from the dedicated memory run.
    alloc_calls: u64,
    alloc_bytes: u64,
    retained: i64,
    peak_delta: i64,
}

/// Runs `f` `iters` times for timing (results dropped), then once more for the
/// memory measurement, returning that final result so callers can keep it.
fn phase<T>(
    name: &'static str,
    iters: usize,
    units: u64,
    mut f: impl FnMut() -> T,
) -> (PhaseResult, T) {
    let mut times: Vec<Duration> = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t0 = Instant::now();
        let out = f();
        times.push(t0.elapsed());
        drop(out);
    }
    times.sort();

    // Dedicated memory run: prior iterations' results are dropped, so live is
    // back to the phase baseline; reset the peak so it covers this run only.
    reset_peak();
    let before = alloc_snap();
    let t0 = Instant::now();
    let out = f();
    let mem_time = t0.elapsed();
    let after = alloc_snap();
    let peak = PEAK_BYTES.load(Ordering::Relaxed);
    times.push(mem_time);
    times.sort();

    let result = PhaseResult {
        name,
        median: times[times.len() / 2],
        best: times[0],
        units,
        alloc_calls: after.calls - before.calls,
        alloc_bytes: after.bytes - before.bytes,
        retained: after.live - before.live,
        peak_delta: peak - before.live,
    };
    (result, out)
}

fn fmt_count(n: u64) -> String {
    // 1234567 -> "1,234,567"
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn fmt_mb(bytes: i64) -> String {
    format!("{:.1}", bytes as f64 / (1024.0 * 1024.0))
}

fn print_results(results: &[PhaseResult]) {
    println!(
        "\n{:<14} {:>10} {:>10} {:>12} {:>14} {:>10} {:>12} {:>9}",
        "phase", "med(ms)", "min(ms)", "kev/s", "allocs", "allocMB", "retainMB", "peakMB"
    );
    for r in results {
        let throughput = if r.units > 0 {
            format!("{:.0}", r.units as f64 / r.median.as_secs_f64() / 1000.0)
        } else {
            "-".to_string()
        };
        println!(
            "{:<14} {:>10.1} {:>10.1} {:>12} {:>14} {:>10} {:>12} {:>9}",
            r.name,
            r.median.as_secs_f64() * 1000.0,
            r.best.as_secs_f64() * 1000.0,
            throughput,
            fmt_count(r.alloc_calls),
            fmt_mb(r.alloc_bytes as i64),
            fmt_mb(r.retained),
            fmt_mb(r.peak_delta),
        );
    }
}

// ---------------------------------------------------------------------------
// Synthetic live wire-format batches (the Correlator / zero-copy target path)
// ---------------------------------------------------------------------------

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// One complete record: `LogEntry` header (no frame chain) + `data`.
fn entry_bytes(monitor: u16, notify: u16, sequence: i32, status: i32, data: &[u8]) -> Vec<u8> {
    // SAFETY: `LogEntry` is a packed POD of integers; all-zero is a valid value.
    let mut h: LogEntry = unsafe { core::mem::zeroed() };
    h.process_seq = 1; // matches the process-create record's seq below
    h.thread_id = 4321;
    h.monitor_type = monitor;
    h.notify_type = notify;
    h.sequence = sequence;
    h.time = 133_000_000_000_000_000 + sequence as i64;
    h.status = status;
    h.data_length = data.len() as u32;
    // SAFETY: reading the header's bytes for serialization.
    let hb =
        unsafe { core::slice::from_raw_parts(&h as *const LogEntry as *const u8, LOG_ENTRY_SIZE) };
    let mut bytes = Vec::with_capacity(LOG_ENTRY_SIZE + data.len());
    bytes.extend_from_slice(hb);
    bytes.extend_from_slice(data);
    bytes
}

/// `LOG_PROCESSCREATE_INFO` for process seq 1 / pid 1234 (field order mirrors
/// `kernel_types::LogProcessCreate`; the size assert guards layout drift).
fn proc_create_data(image: &str, cmdline: &str) -> Vec<u8> {
    let img = utf16(image);
    let cmd = utf16(cmdline);
    let mut d = Vec::new();
    d.extend(1u32.to_le_bytes()); // seq
    d.extend(1234u32.to_le_bytes()); // process_id
    d.extend(0u32.to_le_bytes()); // parent_proc_seq
    d.extend(4u32.to_le_bytes()); // parent_id
    d.extend(1u32.to_le_bytes()); // session_id
    d.extend(0u32.to_le_bytes()); // is_wow64
    d.extend(0i64.to_le_bytes()); // create_time
    d.extend(0u32.to_le_bytes()); // luid low
    d.extend(0i32.to_le_bytes()); // luid high
    d.extend(0u32.to_le_bytes()); // token_virtualization_enabled
    d.push(0); // sid_length
    d.push(0); // integrity_level_sid_length
    d.extend(((img.len() / 2) as u16).to_le_bytes()); // proc_name_length
    d.extend(((cmd.len() / 2) as u16).to_le_bytes()); // command_line_length
    d.extend(0u16.to_le_bytes()); // unknown
    assert_eq!(d.len(), core::mem::size_of::<LogProcessCreate>());
    d.extend_from_slice(&img);
    d.extend_from_slice(&cmd);
    d
}

/// `LOG_FILE_OPT` data: zeroed head + `FLT_PARAMETERS`, then NameLength/Name,
/// plus the trailing `LOG_FILE_CREATE` for `IRP_MJ_CREATE` records.
fn file_op_data(name: &str, is_create: bool) -> Vec<u8> {
    let n16 = utf16(name);
    let mut d = vec![0u8; file_opt::name_length_offset()];
    d.extend(((n16.len() / 2) as u16).to_le_bytes()); // NameLength (UTF-16 units)
    d.extend([0u8; 2]); // Fill42
    d.extend_from_slice(&n16);
    if is_create {
        d.extend(0x0012_0089u32.to_le_bytes()); // DesiredAccess
        d.extend(0u32.to_le_bytes()); // UserTokenLength
    }
    d
}

/// A registry record whose fixed struct is `fixed_size` bytes: the leading
/// `KeyNameLength` u16, zeroed remainder, then the key name (UTF-16).
fn reg_key_data(fixed_size: usize, key: &str) -> Vec<u8> {
    let k16 = utf16(key);
    let mut d = vec![0u8; fixed_size];
    d[0..2].copy_from_slice(&(((k16.len() / 2) as u16).to_le_bytes()));
    d.extend_from_slice(&k16);
    d
}

/// `LOG_REG_SETVALUEKEY` with a 4-byte `REG_DWORD` payload after the key name.
fn reg_set_value_data(key: &str) -> Vec<u8> {
    let mut d = reg_key_data(core::mem::size_of::<LogRegSetValueKey>(), key);
    d[4..8].copy_from_slice(&4u32.to_le_bytes()); // value_type = REG_DWORD
    d[8..12].copy_from_slice(&4u32.to_le_bytes()); // data_size
    d[12..14].copy_from_slice(&4u16.to_le_bytes()); // copy_size
    d.extend(0xC0FFEEu32.to_le_bytes()); // the DWORD value
    d
}

const FILE_PATHS: [&str; 4] = [
    "\\Device\\HarddiskVolume3\\Windows\\System32\\kernel32.dll",
    "\\Device\\HarddiskVolume3\\Users\\bench\\AppData\\Local\\Temp\\work-item-0001.tmp",
    "\\Device\\HarddiskVolume3\\ProgramData\\Vendor\\App\\settings.json",
    "\\Device\\HarddiskVolume3\\Windows\\Prefetch\\NOTEPAD.EXE-D8414F97.pf",
];

const REG_KEYS: [&str; 4] = [
    "\\REGISTRY\\MACHINE\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion",
    "\\REGISTRY\\MACHINE\\SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters",
    "\\REGISTRY\\USER\\S-1-5-21-1-2-3-1000\\Software\\Classes\\CLSID",
    "\\REGISTRY\\MACHINE\\SOFTWARE\\Policies\\Microsoft\\Windows\\System",
];

/// Events each record group below contributes (create+post correlate into one).
const EVENTS_PER_GROUP: usize = 6;

/// Appends one group of records: file create (PRE pending + POST), read, write,
/// reg open, reg query-value, reg set-value. Returns the next sequence number.
fn push_group(batch: &mut Vec<u8>, mut seq: i32, variant: usize) -> i32 {
    let path = FILE_PATHS[variant % FILE_PATHS.len()];
    let key = REG_KEYS[variant % REG_KEYS.len()];
    let f = |n: u8| FILE_NOTIFY_BASE + n as u16;

    // CreateFile: asynchronous PRE completed by a POST (exercises correlation).
    batch.extend(entry_bytes(
        3,
        f(irp_mj::CREATE),
        seq,
        STATUS_PENDING,
        &file_op_data(path, true),
    ));
    batch.extend(entry_bytes(
        0,
        f(irp_mj::CREATE),
        seq,
        0,
        &1u64.to_le_bytes(),
    ));
    seq += 1;
    for mj in [irp_mj::READ, irp_mj::WRITE] {
        batch.extend(entry_bytes(3, f(mj), seq, 0, &file_op_data(path, false)));
        seq += 1;
    }
    let open = reg_key_data(core::mem::size_of::<LogRegCreateOpenKey>(), key);
    batch.extend(entry_bytes(2, reg_notify::OPENKEYEX, seq, 0, &open));
    seq += 1;
    let query = reg_key_data(core::mem::size_of::<LogRegQueryValueKey>(), key);
    batch.extend(entry_bytes(2, reg_notify::QUERYVALUEKEY, seq, 0, &query));
    seq += 1;
    batch.extend(entry_bytes(
        2,
        reg_notify::SETVALUEKEY,
        seq,
        0,
        &reg_set_value_data(key),
    ));
    seq + 1
}

/// Builds `n_batches` driver-shaped batches (~`batch_target` bytes each), in the
/// `Arc<[u8]>` form the receive thread hands to the parser. The first batch
/// opens with a process-create record so events attach to a tracked process, as
/// in live capture.
fn build_live_batches(n_batches: usize, batch_target: usize) -> (Vec<Arc<[u8]>>, u64) {
    let mut batches = Vec::with_capacity(n_batches);
    let mut seq: i32 = 1;
    let mut events: u64 = 0;
    for i in 0..n_batches {
        let mut batch = Vec::with_capacity(batch_target + 1024);
        if i == 0 {
            let image = "\\Device\\HarddiskVolume3\\Windows\\notepad.exe";
            batch.extend(entry_bytes(
                1,
                proc_notify::CREATE,
                seq,
                0,
                &proc_create_data(image, "notepad.exe bench.txt"),
            ));
            seq += 1;
            events += 1;
        }
        let mut variant = 0usize;
        while batch.len() < batch_target {
            seq = push_group(&mut batch, seq, variant);
            variant += 1;
            events += EVENTS_PER_GROUP as u64;
        }
        batches.push(Arc::from(batch.into_boxed_slice()));
    }
    (batches, events)
}

/// Ingests every batch through a fresh correlator/process table, returning the
/// emitted events (their count is the timed unit of the live/ingest phase).
fn ingest_events(batches: &[Arc<[u8]>]) -> Vec<Event> {
    let mgr = ProcessManager::new();
    let mut correlator = Correlator::new();
    let mut out: Vec<Event> = Vec::new();
    for b in batches {
        correlator.ingest_shared(b, 0..b.len(), &mgr, &mut out);
    }
    correlator.flush(&mgr, &mut out);
    out
}

/// Ingest wrapper for the timed phase: events are dropped so the phase's memory
/// numbers stay comparable across BASELINE.md entries.
fn ingest_all(batches: &[Arc<[u8]>]) -> usize {
    ingest_events(batches).len()
}

/// Sums the display columns of `events` (the shared body of the live/pml
/// columns phases): path, detail, operation, result, time-of-day.
fn sum_columns(events: &[Event]) -> usize {
    let mut sink = 0usize;
    for ev in events {
        sink += ev.path().map_or(0, |p| p.len());
        sink += ev.detail().len();
        sink += ev.operation_name().len();
        sink += ev.result().len();
        sink += ev.time_of_day().len();
    }
    sink
}

// ---------------------------------------------------------------------------
// PML phases
// ---------------------------------------------------------------------------

fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/resources")
        .join("Logfile.PML")
}

/// A representative mixed rule set (case-folded relations over hot columns).
fn bench_filter() -> FilterSet {
    FilterSet::new(vec![
        Rule::new(
            Column::ProcessName,
            Relation::Is,
            "Procmon.exe",
            Action::Exclude,
        ),
        Rule::new(
            Column::Operation,
            Relation::BeginsWith,
            "RegQuery",
            Action::Exclude,
        ),
        Rule::new(Column::Path, Relation::Contains, "windows", Action::Include),
        Rule::new(Column::Path, Relation::EndsWith, ".dll", Action::Include),
    ])
}

fn main() {
    // Quick mode: single iteration, small synthetic load. Used for debug builds
    // (`cargo test --all-targets` executes this binary) and `--quick`.
    let quick = cfg!(debug_assertions) || std::env::args().any(|a| a == "--quick");
    let iters = if quick { 1 } else { 3 };
    let mut results: Vec<PhaseResult> = Vec::new();

    println!(
        "== procmon-sdk baseline bench ({} mode) ==",
        if quick { "quick" } else { "full" }
    );

    // --- live/ingest (no fixture needed) ----------------------------------
    let (batches, expected_events) = build_live_batches(if quick { 8 } else { 256 }, 110 * 1024);
    let total_bytes: usize = batches.iter().map(|b| b.len()).sum();

    // Self-check: the synthetic wire format must parse into the expected events.
    let parsed = procmon_sdk::parse_block(&batches[0]);
    assert!(
        parsed
            .iter()
            .any(|e| e.operation_name() == "CreateFile" && e.path().is_some()),
        "synthetic batch failed to parse: no CreateFile with a path"
    );
    println!(
        "live: {} batches, {} bytes, {} events",
        batches.len(),
        fmt_count(total_bytes as u64),
        fmt_count(expected_events)
    );

    let (r, emitted) = phase(
        "live/ingest",
        if quick { 1 } else { 5 },
        expected_events,
        || ingest_all(&batches),
    );
    assert_eq!(
        emitted as u64, expected_events,
        "ingest emitted an unexpected event count"
    );
    results.push(r);

    // Column extraction over live-mode events: unlike the PML phases (whose
    // strings are mostly PML-ASCII), this exercises the UTF-16 wire decode.
    let live_events = ingest_events(&batches);
    let (r, _) = phase("live/columns", iters, expected_events, || {
        sum_columns(&live_events)
    });
    results.push(r);
    drop(live_events);

    // --- PML phases --------------------------------------------------------
    let path = fixture_path();
    if path.exists() {
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        println!("pml: {} ({} MB)", path.display(), size / (1024 * 1024));

        let (r, reader) = phase("pml/open", iters, 0, || {
            std::sync::Arc::new(PmlReader::open(&path).expect("open Logfile.PML"))
        });
        results.push(r);

        let n_events = reader.len() as u64;
        let (r, events) = phase("pml/parse", iters, n_events, || {
            reader.events().collect::<Vec<Event>>()
        });
        results.push(r);
        println!(
            "pml: {} events materialized",
            fmt_count(events.len() as u64)
        );

        let (r, _) = phase("pml/columns", iters, n_events, || sum_columns(&events));
        results.push(r);

        let filter = bench_filter();
        let (r, visible) = phase("pml/filter", if quick { 1 } else { 5 }, n_events, || {
            events.iter().filter(|e| filter.matches(*e)).count()
        });
        results.push(r);
        println!(
            "pml: {} / {} events pass the bench filter",
            fmt_count(visible as u64),
            fmt_count(n_events)
        );
    } else {
        println!(
            "pml: fixture {} not found — PML phases skipped",
            path.display()
        );
    }

    print_results(&results);
}
