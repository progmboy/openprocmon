//! The real SDK-backed event sources: live capture (`SdkSource`) and offline `.PML`
//! viewing (`PmlSource`).
//!
//! Owns a `procmon_sdk::MonitorController`; a relay thread pulls the SDK's
//! `Receiver<Event>` and wraps each event into a [`CapturedEvent`] (moving the
//! non-`Clone` `Event` in — no eager string conversion), forwarding it over the
//! `SourceEvent` channel the app drains. Display columns stay lazy.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender};
use procmon_sdk::{basename, DriverLoader, MonitorController, MonitorFlags, ProcessRecord};

use crate::app::MonitorToggles;
use crate::model::domain::{
    CapturedEvent, EventDetail, FrameKind, ModuleRow, ProcessNode, StackRow,
};
use crate::model::filter::FilterModel;
use crate::model::source::{EventSource, SourceEvent};

/// The driver service name registered when loading the `.sys` on demand.
const DRIVER_NAME: &str = "OpenProcmon24";

pub struct SdkSource {
    /// Kept alive while the relay thread pulls events (Drop stops the driver).
    controller: Option<MonitorController>,
    flags: MonitorFlags,
    /// Relay gate: when false the relay drops events (driver keeps running, like
    /// Process Monitor's pause). Also gates the deferred initial connect.
    capturing: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    /// The output channel, kept so a deferred connect (first play) can relay onto it.
    tx: Option<Sender<SourceEvent>>,
}

impl SdkSource {
    pub fn new() -> Self {
        Self {
            controller: None,
            flags: flags_from(MonitorToggles::default()),
            // Default paused: the driver is not connected until capture is enabled.
            capturing: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
            handle: None,
            tx: None,
        }
    }

    /// Connects the driver and spawns the relay thread. Called from `start()` only
    /// when launching unpaused, otherwise on the first `set_capturing(true)`.
    fn connect_and_run(&mut self, tx: Sender<SourceEvent>) {
        // Build the driver loader. The port is connected first inside
        // `connect_with_driver`; an embedded image is only dropped to
        // System32\Drivers if that connect misses and the driver must be loaded.
        let loader = make_loader();
        let mut controller = match MonitorController::connect_with_driver(loader) {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(SourceEvent::Error(driver_error_message(&e)));
                return;
            }
        };
        let sdk_rx = match controller.start_with(self.flags) {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(SourceEvent::Error(
                    rust_i18n::t!("capture.err.start", detail = e.to_string())
                        .to_string()
                        .into(),
                ));
                return;
            }
        };
        self.controller = Some(controller);

        let capturing = Arc::clone(&self.capturing);
        let stop = Arc::clone(&self.stop);
        self.handle = Some(thread::spawn(move || {
            let mut seq: u64 = 1;
            while !stop.load(Ordering::Relaxed) {
                match sdk_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(ev) => {
                        // Paused: drop the event (the driver keeps running, like
                        // Process Monitor's pause).
                        if capturing.load(Ordering::Relaxed) {
                            let row = CapturedEvent::from_event(ev, seq);
                            seq += 1;
                            if tx.send(SourceEvent::Row(row)).is_err() {
                                break;
                            }
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        }));
    }
}

impl Default for SdkSource {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSource for SdkSource {
    fn start(&mut self) -> Receiver<SourceEvent> {
        let (tx, rx): (Sender<SourceEvent>, Receiver<SourceEvent>) = bounded(8192);
        self.tx = Some(tx.clone());
        // Defer the driver connection until capture is actually enabled, so a paused
        // launch (the default) never loads the driver or surfaces a connect error.
        if self.capturing.load(Ordering::Relaxed) {
            self.connect_and_run(tx);
        }
        rx
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        // Drop the controller (its Drop stops the driver / tears down the pipeline).
        self.controller = None;
    }

    fn set_capturing(&mut self, on: bool) {
        self.capturing.store(on, Ordering::Relaxed);
        // First play: lazily connect the driver and spawn the relay thread.
        if on && self.controller.is_none() {
            if let Some(tx) = self.tx.clone() {
                self.connect_and_run(tx);
            }
        }
    }

    fn set_monitor(&mut self, flags: MonitorToggles) {
        // TODO: live category changes require restarting the controller (new
        // receiver); for now the new flags apply on the next start().
        self.flags = flags_from(flags);
    }

    fn set_filter(&mut self, _filter: FilterModel) {
        // GUI-side filtering is authoritative (the buffer rebuilds its view); a
        // driver-level FilterSet push is a future optimization.
    }

    fn detail_for(&self, row: &CapturedEvent) -> EventDetail {
        // Kernel call-stack frames resolve against System (PID 4)'s modules — the
        // loaded kernel drivers (seeded from NtQuerySystemInformation at INIT and
        // kept current by image-load events; see `proc::seed_init_modules`).
        let kernel_mods = self
            .controller
            .as_ref()
            .and_then(|c| c.processes().by_pid(4))
            .map(|r| sdk_modules(&r))
            .unwrap_or_default();
        event_detail(row.event(), row, &kernel_mods)
    }

    fn process_tree(&self) -> Vec<ProcessNode> {
        let Some(ctrl) = &self.controller else {
            return Vec::new();
        };
        build_tree(&ctrl.processes().snapshot())
    }

    fn kernel_modules(&self) -> Vec<ModuleRow> {
        self.controller
            .as_ref()
            .and_then(|c| c.processes().by_pid(4))
            .map(|r| sdk_modules(&r))
            .unwrap_or_default()
    }

    fn live_processes(&self) -> Option<Arc<procmon_sdk::ProcessManager>> {
        self.controller.as_ref().map(|c| Arc::clone(c.processes()))
    }

    fn live_module_versions(&self) -> Option<Arc<procmon_sdk::ModuleVersionCache>> {
        self.controller
            .as_ref()
            .map(|c| Arc::clone(c.metadata().module_versions()))
    }
}

/// Maps the GUI's per-category toggles to the SDK's `MonitorFlags` (profiling has
/// no driver flag — it's GUI-only).
fn flags_from(t: MonitorToggles) -> MonitorFlags {
    let mut f = MonitorFlags::empty();
    if t.process {
        f |= MonitorFlags::PROCESS;
    }
    if t.file {
        f |= MonitorFlags::FILE;
    }
    if t.registry {
        f |= MonitorFlags::REGISTRY;
    }
    if t.network {
        f |= MonitorFlags::NETWORK;
    }
    f
}

/// Builds the [`DriverLoader`]. Without the `embedded-driver` feature it loads a
/// `procmon.sys` from next to the executable; with it, the driver image is embedded
/// in the binary and dropped to `%SystemRoot%\System32\Drivers` on demand.
/// Maps an SDK error to a localized, user-facing toast message. The actionable
/// driver-load failures get a dedicated string; everything else falls back to the
/// SDK's (English) `Display` wrapped in a localized prefix.
fn driver_error_message(e: &procmon_sdk::Error) -> gpui::SharedString {
    use procmon_sdk::Error as E;
    match e {
        E::NotElevated => rust_i18n::t!("driver.err.not_elevated").to_string(),
        E::OtherVersionLoaded => rust_i18n::t!("driver.err.other_version").to_string(),
        E::AlreadyMonitoring => rust_i18n::t!("driver.err.already_monitoring").to_string(),
        other => rust_i18n::t!("driver.err.generic", detail = other.to_string()).to_string(),
    }
    .into()
}

#[cfg(not(feature = "embedded-driver"))]
fn make_loader() -> DriverLoader {
    // `procmon.sys` next to the executable (where the build/installer places it).
    let sys = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("procmon.sys")))
        .unwrap_or_else(|| PathBuf::from("procmon.sys"));
    DriverLoader::new(DRIVER_NAME, sys)
}

#[cfg(feature = "embedded-driver")]
fn make_loader() -> DriverLoader {
    // The signed driver image (repo `bin/PROCMON24.SYS`), embedded at build time.
    // It is only dropped to %SystemRoot%\System32\Drivers if the driver actually
    // needs loading (connect-first miss) — see `DriverLoader::from_embedded`.
    const DRIVER_IMAGE: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../bin/PROCMON24.SYS"
    ));
    DriverLoader::from_embedded(DRIVER_NAME, "PROCMON24.SYS", DRIVER_IMAGE)
}

/// Builds the rich detail for an SDK event (Event/Process/Stack tabs).
/// `kernel_mods` is the System (PID 4) driver list, used to resolve kernel frames.
fn event_detail(
    ev: &procmon_sdk::Event,
    row: &CapturedEvent,
    kernel_mods: &[ModuleRow],
) -> EventDetail {
    let modules = rows_from_modules(ev.modules());
    let stack = sdk_stack(ev, &modules, kernel_mods);
    EventDetail {
        category: row.category(),
        operation: row.operation(),
        time: row.time(),
        date: ev.date().into(),
        duration: ev.duration().map(Into::into),
        pid: ev.pid(),
        tid: ev.thread_id(),
        path: row.path(),
        result: row.result(),
        result_kind: row.result_kind(),
        other_details: row.detail(),
        // Target-file version/company/signature are not yet surfaced by the SDK.
        target_version: None,
        target_company: None,
        signed: None,
        process: sdk_process_node(ev),
        modules,
        stack,
    }
}

/// The originating process as a `ProcessNode`, from the event's zero-copy accessors.
fn sdk_process_node(ev: &procmon_sdk::Event) -> ProcessNode {
    ProcessNode {
        pid: ev.pid(),
        name: ev.process_name().unwrap_or("").to_string().into(),
        company: ev.company().unwrap_or("").to_string().into(),
        version: ev.version().unwrap_or("").to_string().into(),
        running: !ev.process_exited(),
        integrity: ev.integrity().unwrap_or("").into(),
        arch: if ev.is_wow64() == Some(true) {
            "32-bit"
        } else {
            "64-bit"
        }
        .into(),
        parent_pid: ev.parent_pid().unwrap_or(0),
        session_id: ev.session_id().unwrap_or(0),
        virtualized: ev.is_virtualized().unwrap_or(false),
        user: ev.user().unwrap_or_default().into(),
        start_time: "".into(),
        image_path: ev.image_path().unwrap_or("").to_string().into(),
        command_line: ev.command_line().unwrap_or("").to_string().into(),
        icon: ev.icon_large().map(crate::components::app_image),
        children: Vec::new(),
    }
}

/// Loaded modules of a live process as `ModuleRow`s (kernel/PID 4 frame resolution).
fn sdk_modules(rec: &ProcessRecord) -> Vec<ModuleRow> {
    rows_from_modules(rec.modules())
}

/// Converts shared SDK [`procmon_sdk::Module`]s to display `ModuleRow`s. The
/// `Arc<Module>`s are borrowed (no module is deep-copied); only the per-row
/// display strings the table needs are materialized.
fn rows_from_modules(mods: Vec<Arc<procmon_sdk::Module>>) -> Vec<ModuleRow> {
    mods.iter()
        .map(|m| ModuleRow {
            name: basename(&m.path).into(),
            path: m.path.as_str().into(),
            base: m.base,
            size: m.size as u64,
        })
        .collect()
}

/// The call stack as `StackRow`s. Each frame address is resolved to its loaded
/// module (name/`module+offset`/path) via [`procmon_sdk::resolve_frame`]:
/// user-mode frames against the originating process's modules, kernel frames
/// against the System (PID 4) driver modules. Symbol resolution is deferred.
fn sdk_stack(
    ev: &procmon_sdk::Event,
    proc_mods: &[ModuleRow],
    kernel_mods: &[ModuleRow],
) -> Vec<StackRow> {
    let proc_views = sym_views(proc_mods);
    let kernel_views = sym_views(kernel_mods);
    ev.call_stack()
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let addr = f.address();
            let r = procmon_sdk::resolve_frame_full(addr, &proc_views, &kernel_views);
            StackRow {
                frame: i as u32,
                kind: if r.kernel {
                    FrameKind::Kernel
                } else {
                    FrameKind::User
                },
                module: r.module.to_string().into(),
                location: r.location.into(),
                address: addr,
                path: r.path.to_string().into(),
            }
        })
        .collect()
}

/// Borrowed [`procmon_sdk::SymModule`] views over display `ModuleRow`s (the
/// one GUI-side adapter; core has its own over its serializable rows).
pub(crate) fn sym_views(mods: &[ModuleRow]) -> Vec<procmon_sdk::SymModule<'_>> {
    mods.iter()
        .map(|m| procmon_sdk::SymModule {
            base: m.base,
            size: m.size,
            path: m.path.as_ref(),
        })
        .collect()
}

/// Builds the parent→child process tree from a flat snapshot (by PID).
fn build_tree(records: &[Arc<ProcessRecord>]) -> Vec<ProcessNode> {
    procmon_sdk::build_forest(
        records,
        |r| (r.info.pid, r.info.parent_pid),
        |r, children| {
            let mut n = record_node(r);
            n.children = children;
            n
        },
    )
}

/// A `ProcessNode` from a tracked record (integrity/user need SID resolution not
/// exposed here, so they are left blank in the tree view).
fn record_node(rec: &ProcessRecord) -> ProcessNode {
    let info = &rec.info;
    let meta = rec.meta();
    let s = |o: Option<&String>| -> gpui::SharedString {
        o.map(|v| v.as_str().to_string()).unwrap_or_default().into()
    };
    ProcessNode {
        pid: info.pid,
        name: basename(&info.image_path).into(),
        company: s(meta.and_then(|m| m.company.as_ref())),
        version: s(meta.and_then(|m| m.version.as_ref())),
        running: !rec.is_exited(),
        integrity: "".into(),
        arch: if info.is_wow64 { "32-bit" } else { "64-bit" }.into(),
        parent_pid: info.parent_pid,
        session_id: info.session_id,
        virtualized: info.is_virtualized,
        user: "".into(),
        start_time: "".into(),
        image_path: info.image_path.clone().into(),
        command_line: info.command_line.clone().into(),
        icon: meta
            .and_then(|m| m.icon_large.as_ref())
            .map(|b| crate::components::app_image(b)),
        children: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// PmlSource — offline File ▸ Open .PML viewing.
// ---------------------------------------------------------------------------

/// Loads a `.PML` file and streams its events as rows. Offline/static: capture
/// toggles, monitor flags and filters are no-ops (the GUI buffer still filters).
pub struct PmlSource {
    path: PathBuf,
    reader: Option<Arc<procmon_sdk::PmlReader>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl PmlSource {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            reader: None,
            stop: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }
}

impl EventSource for PmlSource {
    fn as_pml_reader(&self) -> Option<Arc<procmon_sdk::PmlReader>> {
        self.reader.clone()
    }

    fn start(&mut self) -> Receiver<SourceEvent> {
        let (tx, rx): (Sender<SourceEvent>, Receiver<SourceEvent>) = bounded(8192);

        let reader = match procmon_sdk::PmlReader::open(&self.path) {
            Ok(r) => Arc::new(r),
            Err(e) => {
                let _ = tx.send(SourceEvent::Error(
                    format!("Failed to open .PML: {e}").into(),
                ));
                return rx;
            }
        };
        self.reader = Some(Arc::clone(&reader));

        let stop = Arc::clone(&self.stop);
        self.handle = Some(thread::spawn(move || {
            // Each event is synthesized as a unified `Event` sharing the reader Arc.
            for (seq, ev) in (1_u64..).zip(reader.events()) {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let row = CapturedEvent::from_event(ev, seq);
                if tx.send(SourceEvent::Row(row)).is_err() {
                    break;
                }
            }
        }));

        rx
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    fn set_capturing(&mut self, _on: bool) {}
    fn set_monitor(&mut self, _flags: MonitorToggles) {}
    fn set_filter(&mut self, _filter: FilterModel) {}

    fn detail_for(&self, row: &CapturedEvent) -> EventDetail {
        let kernel_mods = self
            .reader
            .as_ref()
            .map(|r| pml_kernel_modules(r))
            .unwrap_or_default();
        event_detail(row.event(), row, &kernel_mods)
    }

    fn process_tree(&self) -> Vec<ProcessNode> {
        match &self.reader {
            Some(reader) => pml_tree(reader),
            None => Vec::new(),
        }
    }

    fn kernel_modules(&self) -> Vec<ModuleRow> {
        self.reader
            .as_ref()
            .map(|r| pml_kernel_modules(r))
            .unwrap_or_default()
    }
}

fn pml_process_node(p: &procmon_sdk::PmlProcess, reader: &procmon_sdk::PmlReader) -> ProcessNode {
    ProcessNode {
        pid: p.pid,
        name: p.process_name.as_ref().into(),
        company: p.company.as_ref().into(),
        version: p.version.as_ref().into(),
        running: p.end_time == 0,
        integrity: p.integrity.as_ref().into(),
        arch: if p.is_64bit { "64-bit" } else { "32-bit" }.into(),
        parent_pid: p.parent_pid,
        session_id: p.session,
        virtualized: p.virtualized,
        user: p.user.as_ref().into(),
        start_time: "".into(),
        image_path: p.image_path.as_ref().into(),
        command_line: p.command_line.as_ref().into(),
        icon: reader
            .icon(p.icon_big)
            .or_else(|| reader.icon(p.icon_small))
            .map(|i| crate::components::app_image(&i.data)),
        children: Vec::new(),
    }
}

fn pml_modules(p: &procmon_sdk::PmlProcess) -> Vec<ModuleRow> {
    p.modules
        .iter()
        .map(|m| ModuleRow {
            name: basename(&m.image_path).into(),
            path: m.image_path.as_ref().into(),
            base: m.base_address,
            size: m.size as u64,
        })
        .collect()
}

/// The System (PID 4) process's modules — i.e. the loaded kernel drivers — used
/// to resolve kernel-mode call-stack frames. Empty if System isn't tracked.
fn pml_kernel_modules(reader: &procmon_sdk::PmlReader) -> Vec<ModuleRow> {
    reader
        .processes()
        .find(|p| p.pid == 4)
        .map(pml_modules)
        .unwrap_or_default()
}

/// Builds the process tree from the PML's process table (by PID).
fn pml_tree(reader: &procmon_sdk::PmlReader) -> Vec<ProcessNode> {
    let procs: Vec<&procmon_sdk::PmlProcess> = reader.processes().collect();
    procmon_sdk::build_forest(
        &procs,
        |p| (p.pid, p.parent_pid),
        |p, children| {
            let mut n = pml_process_node(p, reader);
            n.children = children;
            n
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// A decompressed fixture: the reader plus the temp file's delete-on-drop
    /// guard. Field order matters — the reader (and its mmap) drops first, so
    /// the guard's delete actually succeeds on Windows. Anything holding the
    /// reader (rows, events) must be declared after the fixture.
    struct Fixture {
        reader: Arc<procmon_sdk::PmlReader>,
        _path: tempfile::TempPath,
    }

    impl std::ops::Deref for Fixture {
        type Target = Arc<procmon_sdk::PmlReader>;
        fn deref(&self) -> &Self::Target {
            &self.reader
        }
    }

    fn open_fixture() -> Fixture {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../sdk/tests/resources/CompressedLogFileUTC64FilesystemPML");
        let raw = std::fs::read(path).expect("fixture");
        let mut buf = Vec::new();
        flate2::read::ZlibDecoder::new(&raw[..])
            .read_to_end(&mut buf)
            .expect("unzip");
        let tmp = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(tmp.path(), &buf).expect("write");
        let path = tmp.into_temp_path();
        let reader = Arc::new(procmon_sdk::PmlReader::open(&path).expect("open"));
        Fixture {
            reader,
            _path: path,
        }
    }

    #[test]
    fn pml_summary_rows_have_varied_pids() {
        let reader = open_fixture();
        let mut pids = std::collections::HashSet::new();
        for ev in reader.events() {
            pids.insert(CapturedEvent::from_event(ev, 0).pid());
        }
        assert!(
            pids.len() > 1,
            "expected multiple pids in summary rows, got {pids:?}"
        );
    }

    #[test]
    fn export_via_core_produces_expected_shape() {
        // Mirrors `do_save`: selected rows -> &Event -> the core encoders.
        let reader = open_fixture();
        let rows: Vec<CapturedEvent> = reader
            .events()
            .map(|e| CapturedEvent::from_event(e, 0))
            .collect();
        let events: Vec<&procmon_sdk::Event> = rows.iter().map(|r| r.event()).collect();
        assert!(events.len() > 1);

        let csv_file = tempfile::Builder::new()
            .suffix(".csv")
            .tempfile()
            .expect("csv temp");
        procmon_core::export_csv(&events, csv_file.path().to_str().unwrap()).expect("csv");
        let csv = std::fs::read_to_string(csv_file.path()).expect("read csv");
        assert!(csv.contains("\"Time of Day\",\"Process Name\",\"PID\""));
        assert!(csv.lines().count() > 1);

        let xml_file = tempfile::Builder::new()
            .suffix(".xml")
            .tempfile()
            .expect("xml temp");
        let sym = procmon_core::StackSymbolizer::default();
        procmon_core::export_xml(&events, true, &sym, xml_file.path().to_str().unwrap())
            .expect("xml");
        let xml = std::fs::read_to_string(xml_file.path()).expect("read xml");
        assert!(xml.contains("<procmon><processlist>"));
        assert!(xml.contains("</processlist><eventlist>"));
        assert!(xml.contains("<event>"));
        assert!(xml.contains("<ProcessName>"));
    }

    #[test]
    fn pml_detail_carries_modules() {
        let reader = open_fixture();
        for i in 0..reader.len() {
            let ev = reader.event_as_event(i).expect("event");
            if ev.modules().is_empty() {
                continue;
            }
            let row = CapturedEvent::from_event(ev, (i + 1) as u64);
            let detail = event_detail(row.event(), &row, &[]);
            assert!(
                !detail.modules.is_empty(),
                "EventDetail.modules empty at event {i}"
            );
            return;
        }
        panic!("no event with modules in fixture");
    }
}
