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
use procmon_sdk::{DriverLoader, MonitorController, MonitorFlags, ProcessRecord};

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
        // Kernel call-stack frames resolve against System (PID 4) drivers.
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
/// module (name/`module+offset`/path) via the process's module ranges; symbol
/// resolution is deferred. Kernel vs user is inferred from the high address bits.
fn sdk_stack(
    ev: &procmon_sdk::Event,
    proc_mods: &[ModuleRow],
    kernel_mods: &[ModuleRow],
) -> Vec<StackRow> {
    ev.call_stack()
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let addr = f.address();
            let kind = if addr >= 0xFFFF_0000_0000_0000 {
                FrameKind::Kernel
            } else {
                FrameKind::User
            };
            let (module, location, path) = frame_module(addr, proc_mods, kernel_mods);
            StackRow {
                frame: i as u32,
                kind,
                module,
                location,
                address: addr,
                path,
            }
        })
        .collect()
}

/// Resolves a frame address to `(module basename, "module+offset", full path)`.
/// User-mode frames resolve against the originating process's modules; kernel
/// frames against the System (PID 4) driver modules. Frames outside any known
/// module fall back to "UNKNOWN" with the raw address and empty path.
fn frame_module(
    addr: u64,
    proc_mods: &[ModuleRow],
    kernel_mods: &[ModuleRow],
) -> (gpui::SharedString, gpui::SharedString, gpui::SharedString) {
    for m in proc_mods.iter().chain(kernel_mods.iter()) {
        if m.size > 0 && addr >= m.base && addr < m.base.saturating_add(m.size) {
            let location = format!("{} + 0x{:x}", m.name, addr - m.base).into();
            return (m.name.clone(), location, m.path.clone());
        }
    }
    (
        "<UNKNOWN>".into(),
        format!("0x{addr:016x}").into(),
        "".into(),
    )
}

/// Builds the parent→child process tree from a flat snapshot (by PID).
fn build_tree(records: &[Arc<ProcessRecord>]) -> Vec<ProcessNode> {
    let pids: std::collections::HashSet<u32> = records.iter().map(|r| r.info.pid).collect();
    fn node_of(rec: &ProcessRecord, records: &[Arc<ProcessRecord>]) -> ProcessNode {
        let mut n = record_node(rec);
        n.children = records
            .iter()
            .filter(|r| r.info.parent_pid == rec.info.pid && r.info.pid != rec.info.pid)
            .map(|r| node_of(r, records))
            .collect();
        n
    }
    // Roots are processes whose parent isn't in the snapshot.
    records
        .iter()
        .filter(|r| !pids.contains(&r.info.parent_pid) || r.info.parent_pid == r.info.pid)
        .map(|r| node_of(r, records))
        .collect()
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

fn basename(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_string()
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
    let pids: std::collections::HashSet<u32> = procs.iter().map(|p| p.pid).collect();
    fn node(
        p: &procmon_sdk::PmlProcess,
        procs: &[&procmon_sdk::PmlProcess],
        reader: &procmon_sdk::PmlReader,
    ) -> ProcessNode {
        let mut n = pml_process_node(p, reader);
        n.children = procs
            .iter()
            .filter(|c| c.parent_pid == p.pid && c.pid != p.pid)
            .map(|c| node(c, procs, reader))
            .collect();
        n
    }
    procs
        .iter()
        .filter(|p| !pids.contains(&p.parent_pid) || p.parent_pid == p.pid)
        .map(|p| node(p, &procs, reader))
        .collect()
}

// --- CSV / XML export (cf. Procmon's Save As) -------------------------------

/// Writes the selected rows as Procmon-style CSV (BOM + fully-quoted CRLF columns),
/// using the `csv` crate so commas/quotes/newlines inside fields are escaped right.
pub(crate) fn export_csv(rows: &[&CapturedEvent], path: &str) -> Result<(), String> {
    let mut buf: Vec<u8> = vec![0xEF, 0xBB, 0xBF]; // UTF-8 BOM (matches Procmon)
    {
        let mut w = csv::WriterBuilder::new()
            .quote_style(csv::QuoteStyle::Always)
            .terminator(csv::Terminator::CRLF)
            .from_writer(&mut buf);
        w.write_record([
            "Time of Day",
            "Process Name",
            "PID",
            "Operation",
            "Path",
            "Result",
            "Detail",
            "User",
        ])
        .map_err(|e| e.to_string())?;
        for row in rows {
            let (time, name, op, path_col, result, detail) = (
                row.time(),
                row.process_name(),
                row.operation(),
                row.path(),
                row.result(),
                row.detail(),
            );
            let pid = row.pid().to_string();
            let user = row.event().user().unwrap_or_default();
            w.write_record([
                time.as_ref(),
                name.as_ref(),
                pid.as_str(),
                op.as_ref(),
                path_col.as_ref(),
                result.as_ref(),
                detail.as_ref(),
                user.as_str(),
            ])
            .map_err(|e| e.to_string())?;
        }
        w.flush().map_err(|e| e.to_string())?;
    }
    std::fs::write(path, buf).map_err(|e| e.to_string())
}

/// Writes the selected rows as Procmon-style XML (process list + event list, with
/// optional symbolized stacks), using `quick-xml` so all text is escaped correctly.
/// `kernel_mods` symbolizes kernel-mode frames.
pub(crate) fn export_xml(
    rows: &[&CapturedEvent],
    include_stacks: bool,
    kernel_mods: &[ModuleRow],
    symbols: Option<&procmon_sdk::SymbolResolver>,
    path: &str,
) -> Result<(), String> {
    use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, Event as Xml};
    use quick_xml::writer::Writer;

    // Assign a 1-based ProcessIndex per distinct pid, in first-seen order.
    let mut order: Vec<u32> = Vec::new();
    let mut index_of: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    let mut sample: std::collections::HashMap<u32, &procmon_sdk::Event> =
        std::collections::HashMap::new();
    for row in rows {
        let ev = row.event();
        let pid = ev.pid();
        index_of.entry(pid).or_insert_with(|| {
            order.push(pid);
            sample.insert(pid, ev);
            order.len()
        });
    }

    let mut w = Writer::new(Vec::<u8>::new());
    let start = |w: &mut Writer<Vec<u8>>, n: &str| {
        w.write_event(Xml::Start(BytesStart::new(n)))
            .map_err(|err| err.to_string())
    };
    let end = |w: &mut Writer<Vec<u8>>, n: &str| {
        w.write_event(Xml::End(BytesEnd::new(n)))
            .map_err(|err| err.to_string())
    };

    w.write_event(Xml::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .map_err(|err| err.to_string())?;
    start(&mut w, "procmon")?;
    start(&mut w, "processlist")?;
    for &pid in &order {
        let ev = sample[&pid];
        let parent = ev.parent_pid().unwrap_or(0);
        let parent_index = index_of.get(&parent).copied().unwrap_or(0);
        start(&mut w, "process")?;
        leaf(&mut w, "ProcessIndex", &index_of[&pid].to_string())?;
        leaf(&mut w, "ProcessId", &pid.to_string())?;
        leaf(&mut w, "ParentProcessId", &parent.to_string())?;
        leaf(&mut w, "ParentProcessIndex", &parent_index.to_string())?;
        leaf(
            &mut w,
            "AuthenticationId",
            &ev.auth_id().unwrap_or_default(),
        )?;
        leaf(&mut w, "CreateTime", &ev.process_create_time().to_string())?;
        leaf(
            &mut w,
            "FinishTime",
            &ev.process_exit_time().unwrap_or(0).to_string(),
        )?;
        leaf(
            &mut w,
            "IsVirtualized",
            bit(ev.is_virtualized() == Some(true)),
        )?;
        leaf(&mut w, "Is64bit", bit(ev.is_wow64() != Some(true)))?;
        leaf(&mut w, "Integrity", ev.integrity().unwrap_or(""))?;
        leaf(&mut w, "Owner", &ev.user().unwrap_or_default())?;
        leaf(&mut w, "ProcessName", ev.process_name().unwrap_or(""))?;
        leaf(&mut w, "ImagePath", ev.image_path().unwrap_or(""))?;
        leaf(&mut w, "CommandLine", ev.command_line().unwrap_or(""))?;
        leaf(&mut w, "CompanyName", ev.company().unwrap_or(""))?;
        leaf(&mut w, "Version", ev.version().unwrap_or(""))?;
        leaf(&mut w, "Description", ev.description().unwrap_or(""))?;
        start(&mut w, "modulelist")?;
        for m in ev.modules() {
            start(&mut w, "module")?;
            leaf(&mut w, "Timestamp", "0")?;
            leaf(&mut w, "BaseAddress", &format!("0x{:x}", m.base))?;
            leaf(&mut w, "Size", &m.size.to_string())?;
            leaf(&mut w, "Path", &m.path)?;
            leaf(&mut w, "Version", "")?;
            leaf(&mut w, "Company", "")?;
            leaf(&mut w, "Description", "")?;
            end(&mut w, "module")?;
        }
        end(&mut w, "modulelist")?;
        end(&mut w, "process")?;
    }
    end(&mut w, "processlist")?;
    start(&mut w, "eventlist")?;
    for row in rows {
        let ev = row.event();
        let pidx = index_of.get(&ev.pid()).copied().unwrap_or(0);
        start(&mut w, "event")?;
        leaf(&mut w, "ProcessIndex", &pidx.to_string())?;
        leaf(&mut w, "Time_of_Day", &row.time())?;
        leaf(&mut w, "Process_Name", &row.process_name())?;
        leaf(&mut w, "PID", &ev.pid().to_string())?;
        leaf(&mut w, "Operation", &row.operation())?;
        leaf(&mut w, "Path", &row.path())?;
        leaf(&mut w, "Result", &row.result())?;
        leaf(&mut w, "Detail", &row.detail())?;
        leaf(&mut w, "User", &ev.user().unwrap_or_default())?;
        if include_stacks {
            let proc_mods = rows_from_modules(ev.modules());
            // Combined module view for symbolization (proc modules + kernel drivers).
            let symmods: Vec<procmon_sdk::SymModule> = proc_mods
                .iter()
                .chain(kernel_mods.iter())
                .map(|m| procmon_sdk::SymModule {
                    base: m.base,
                    size: m.size,
                    path: m.path.as_ref(),
                })
                .collect();
            start(&mut w, "stack")?;
            for (depth, frame) in ev.call_stack().iter().enumerate() {
                let addr = frame.address();
                let (_, fallback, fpath) = frame_module(addr, &proc_mods, kernel_mods);
                // Prefer a resolved symbol; fall back to "module+offset" otherwise.
                let location = symbols
                    .and_then(|r| r.resolve(addr, &symmods))
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| fallback.to_string());
                start(&mut w, "frame")?;
                leaf(&mut w, "depth", &depth.to_string())?;
                leaf(&mut w, "address", &format!("0x{:x}", addr))?;
                leaf(&mut w, "path", &fpath)?;
                leaf(&mut w, "location", &location)?;
                end(&mut w, "frame")?;
            }
            end(&mut w, "stack")?;
        }
        end(&mut w, "event")?;
    }
    end(&mut w, "eventlist")?;
    end(&mut w, "procmon")?;

    let mut out: Vec<u8> = vec![0xEF, 0xBB, 0xBF]; // UTF-8 BOM
    out.extend_from_slice(&w.into_inner());
    std::fs::write(path, out).map_err(|err| err.to_string())
}

fn bit(on: bool) -> &'static str {
    if on {
        "1"
    } else {
        "0"
    }
}

/// Writes a leaf `<name>escaped(text)</name>` element via quick-xml.
fn leaf(w: &mut quick_xml::writer::Writer<Vec<u8>>, name: &str, text: &str) -> Result<(), String> {
    use quick_xml::events::BytesText;
    w.create_element(name)
        .write_text_content(BytesText::new(text))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn open_fixture() -> Arc<procmon_sdk::PmlReader> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../sdk/tests/resources/CompressedLogFileUTC64FilesystemPML");
        let raw = std::fs::read(path).expect("fixture");
        let mut buf = Vec::new();
        flate2::read::ZlibDecoder::new(&raw[..])
            .read_to_end(&mut buf)
            .expect("unzip");
        // Unique temp name per call: tests run in parallel and the reader mmaps the
        // file, so a shared name races (one writes while another holds the map).
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let tmp = std::env::temp_dir().join(format!(
            "gui-pml-test-{}-{}.pml",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&tmp, &buf).expect("write");
        Arc::new(procmon_sdk::PmlReader::open(tmp).expect("open"))
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
    fn export_csv_and_xml_produce_expected_shape() {
        let reader = open_fixture();
        let evs: Vec<CapturedEvent> = reader
            .events()
            .map(|e| CapturedEvent::from_event(e, 0))
            .collect();
        let refs: Vec<&CapturedEvent> = evs.iter().collect();
        assert!(refs.len() > 1);

        let dir = std::env::temp_dir();
        let csv_path = dir.join(format!("gui-export-{}.csv", std::process::id()));
        export_csv(&refs, csv_path.to_str().unwrap()).expect("csv");
        let csv = std::fs::read_to_string(&csv_path).expect("read csv");
        assert!(csv.contains("\"Time of Day\",\"Process Name\",\"PID\""));
        assert!(csv.lines().count() > 1);

        let xml_path = dir.join(format!("gui-export-{}.xml", std::process::id()));
        export_xml(&refs, true, &[], None, xml_path.to_str().unwrap()).expect("xml");
        let xml = std::fs::read_to_string(&xml_path).expect("read xml");
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
