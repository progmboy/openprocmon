//! The public monitor controller (cf. C++ `CMonitorController`).
//!
//! [`MonitorController`] owns the connection, the background threads, and the
//! shared state (process table, metadata cache, address resolver, filter). It is
//! an explicit instance — no global singleton — and its `Drop` stops and joins
//! everything.

use crate::driver::DriverLoader;
use crate::error::{Error, Result};
use crate::event::Event;
use crate::filter::FilterSet;
use crate::kernel_types::monitor_flags;
use crate::metadata::MetadataCache;
use crate::network::{NetworkEvent, NetworkMonitor};
use crate::pipeline::{Enrichment, Pipeline};
use crate::port::FilterPort;
use crate::process::ProcessManager;
use crate::resolver::AddressResolver;

use arc_swap::ArcSwap;
use crossbeam_channel::Receiver;
use std::sync::Arc;
use windows::Win32::Foundation::{ERROR_CONNECTION_COUNT_LIMIT, ERROR_FILE_NOT_FOUND};

bitflags::bitflags! {
    /// Sources to monitor. PROCESS/FILE/REGISTRY flow through the minifilter;
    /// NETWORK is collected via ETW (see [`crate::network`]).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MonitorFlags: u32 {
        const PROCESS = 0b0001;
        const FILE = 0b0010;
        const REGISTRY = 0b0100;
        const NETWORK = 0b1000;
    }
}

impl MonitorFlags {
    /// The minifilter control bits (`CTL_MONITOR_*`) for this selection.
    fn minifilter_bits(self) -> u32 {
        let mut bits = monitor_flags::ALL_CLOSE;
        if self.contains(MonitorFlags::PROCESS) {
            bits |= monitor_flags::PROC_ON;
        }
        if self.contains(MonitorFlags::FILE) {
            bits |= monitor_flags::FILE_ON;
        }
        if self.contains(MonitorFlags::REGISTRY) {
            bits |= monitor_flags::REG_ON;
        }
        bits
    }
}

/// Threads/sessions that exist only while monitoring is active.
struct RunState {
    pipeline: Pipeline,
    network: Option<NetworkMonitor>,
}

/// Connects to the driver and streams [`Event`]s.
pub struct MonitorController {
    port: Arc<FilterPort>,
    driver: Option<DriverLoader>,
    flags: MonitorFlags,
    mgr: Arc<ProcessManager>,
    metadata: Arc<MetadataCache>,
    resolver: Arc<AddressResolver>,
    filter: Arc<ArcSwap<FilterSet>>,
    run: Option<RunState>,
}

impl MonitorController {
    /// Connects to the driver port, with no driver auto-load. Use
    /// [`connect_with_driver`](Self::connect_with_driver) to load the driver on
    /// demand when it is not yet running.
    pub fn connect() -> Result<Self> {
        let port = Self::try_connect()?;
        Ok(Self::with_port(port, None))
    }

    /// Connects to the driver port, loading the driver via `loader` if the port
    /// is not present yet (cf. C++ `monctl.cxx` connect/retry). Connect-first
    /// mirrors Process Monitor: an already-running driver is reused without a
    /// reinstall; only a missing port (`ERROR_FILE_NOT_FOUND`) triggers the load.
    pub fn connect_with_driver(loader: DriverLoader) -> Result<Self> {
        match Self::try_connect() {
            Ok(port) => Ok(Self::with_port(port, Some(loader))),
            Err(Error::PortConnect(e)) if e.code() == ERROR_FILE_NOT_FOUND.to_hresult() => {
                loader.ensure_loaded()?;
                let port = Self::try_connect()?;
                Ok(Self::with_port(port, Some(loader)))
            }
            Err(e) => Err(e),
        }
    }

    /// Connects to the port, mapping the single-client "port already connected"
    /// limit (`ERROR_CONNECTION_COUNT_LIMIT`) to [`Error::AlreadyMonitoring`] so the
    /// GUI can tell "someone else is monitoring" apart from a generic failure.
    fn try_connect() -> Result<FilterPort> {
        FilterPort::connect().map_err(|e| match e {
            Error::PortConnect(w) if w.code() == ERROR_CONNECTION_COUNT_LIMIT.to_hresult() => {
                Error::AlreadyMonitoring
            }
            other => other,
        })
    }

    fn with_port(port: FilterPort, driver: Option<DriverLoader>) -> Self {
        Self {
            port: Arc::new(port),
            driver,
            flags: MonitorFlags::empty(),
            mgr: Arc::new(ProcessManager::new()),
            metadata: Arc::new(MetadataCache::new()),
            resolver: Arc::new(AddressResolver::new()),
            filter: Arc::new(ArcSwap::from_pointee(FilterSet::default())),
            run: None,
        }
    }

    /// Selects which sources to monitor. This only records the selection (cf.
    /// C++ `CMonitorController::SetMonitor`); nothing is sent to the driver here.
    /// The selection is pushed when [`start`](Self::start) runs. The default is
    /// all-off, so without a call here `start` enables nothing.
    pub fn set_monitor(&mut self, flags: MonitorFlags) {
        self.flags = flags;
    }

    /// Starts the receive/parse pipeline, enables the selected sources on the
    /// driver, and returns the event stream. Mirrors C++ `Start`, which starts
    /// the threads then sends the control flags. If the NETWORK bit is set, the
    /// ETW session is started here too since it feeds the parse thread.
    pub fn start(&mut self) -> Result<Receiver<Event>> {
        if self.run.is_some() {
            return Err(Error::Parse("monitor already started".into()));
        }

        // Enable SeDebugPrivilege *before* events flow: the driver replays an INIT
        // record for every pre-existing process the moment monitoring starts, and
        // the parse thread snapshots each one's loaded modules (for call-stack
        // resolution). Opening a process in another session / at a higher
        // integrity (services.exe and the SYSTEM svchosts run as SYSTEM/System IL)
        // for that snapshot needs SeDebug — without it those snapshots come back
        // empty and every user-mode frame in those processes stays `<UNKNOWN>`.
        // Token privileges are process-wide, so enabling it here covers the parse
        // thread too. Best-effort: an unelevated caller can't capture anyway.
        let _ = crate::system::enable_privilege(windows::Win32::Security::SE_DEBUG_NAME);

        let (network, net_rx) = if self.flags.contains(MonitorFlags::NETWORK) {
            let (tx, rx) = crossbeam_channel::unbounded::<NetworkEvent>();
            (Some(NetworkMonitor::start(tx)?), Some(rx))
        } else {
            (None, None)
        };

        let enrich = Enrichment {
            mgr: Arc::clone(&self.mgr),
            metadata: Arc::clone(&self.metadata),
        };
        let (pipeline, rx_b) = Pipeline::start(Arc::clone(&self.port), enrich, net_rx);
        self.run = Some(RunState { pipeline, network });

        // Enable the selected sources now that the receiver is running (cf. C++
        // `Start` -> `Control(m_dwControl)`). On error the pipeline is already
        // stored, so `Drop`/`stop` tears it down.
        self.port.send_control(self.flags.minifilter_bits())?;
        Ok(rx_b)
    }

    /// Convenience: override the current selection with `flags`, then
    /// [`start`](Self::start). Equivalent to [`set_monitor`](Self::set_monitor)
    /// followed by `start`.
    pub fn start_with(&mut self, flags: MonitorFlags) -> Result<Receiver<Event>> {
        self.set_monitor(flags);
        self.start()
    }

    /// Stops monitoring: disables all sources, tears down the ETW session, and
    /// joins the pipeline threads.
    pub fn stop(&mut self) {
        // Best-effort: tell the driver to stop emitting before we tear down.
        let _ = self.port.send_control(monitor_flags::ALL_CLOSE);
        if let Some(mut run) = self.run.take() {
            if let Some(net) = run.network.take() {
                net.stop();
            }
            run.pipeline.stop();
        }
    }

    /// Enables or disables reverse-DNS resolution of network addresses.
    pub fn set_resolve_addresses(&self, on: bool) {
        self.resolver.set_enabled(on);
    }

    /// Replaces the active filter (lock-free for the hot path).
    pub fn set_filter(&self, filter: FilterSet) {
        self.filter.store(Arc::new(filter));
    }

    /// The current filter snapshot.
    pub fn filter(&self) -> Arc<FilterSet> {
        self.filter.load_full()
    }

    /// Convenience: whether `ev` is visible under the current filter.
    pub fn is_visible(&self, ev: &Event) -> bool {
        self.filter.load().matches(ev)
    }

    /// The process table (for the GUI's process list / detail view).
    pub fn processes(&self) -> &Arc<ProcessManager> {
        &self.mgr
    }

    /// The shared image-metadata cache (the async worker's backing store; used
    /// by `EventSource::process_meta` to force-resolve on demand).
    pub fn metadata(&self) -> &Arc<MetadataCache> {
        &self.metadata
    }

    /// The address resolver for formatting network hosts.
    pub fn resolver(&self) -> &Arc<AddressResolver> {
        &self.resolver
    }

    /// The driver loader, if this controller was created with one. Lets callers
    /// unload the driver after stopping (the controller never unloads on `Drop`).
    pub fn driver(&self) -> Option<&DriverLoader> {
        self.driver.as_ref()
    }
}

impl Drop for MonitorController {
    fn drop(&mut self) {
        self.stop();
    }
}
