//! The receive/parse pipeline (cf. design §3, C++ `CRecvThread` + `COPtThread`).
//!
//! Two threads, mirroring real Procmon's split:
//! - the **receive** thread blocks in `FilterGetMessage` on a per-batch buffer
//!   and hands the buffer itself (as `Arc<[u8]>` + record range) onto a bounded
//!   channel (channel A) — no compaction copy; events later reference slices of
//!   this same allocation;
//! - the **parse** thread drains channel A, correlates PRE/POST records, updates
//!   the process table, requests metadata, and emits [`Event`]s onto channel B,
//!   also merging network events from the ETW consumer.
//!
//! Channel A is bounded, so a slow parser back-pressures the receiver (and thus
//! the kernel's buffering) instead of growing without bound.

use crate::event::Event;
use crate::kernel_types::{proc_notify, MonitorType, ProcmonMessageHeader};
use crate::metadata::MetadataCache;
use crate::network::NetworkEvent;
use crate::parse::Correlator;
use crate::port::{FilterPort, MESSAGE_BUFFER_LEN};
use crate::process::ProcessManager;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Default capacity of channel A (raw batches in flight).
const CHANNEL_A_CAP: usize = 256;

/// Batches smaller than this are compacted into an exact-size buffer instead of
/// sharing the full receive buffer, so a trickle of tiny batches (idle system)
/// does not pin a `MESSAGE_BUFFER_LEN` allocation each per surviving event.
const COMPACT_THRESHOLD: usize = 16 * 1024;

/// One received batch: the shared receive buffer and the byte range of its
/// records within it.
struct RawBatch {
    buf: Arc<[u8]>,
    records: Range<usize>,
}

/// Shared state the parse thread needs to enrich events.
pub(crate) struct Enrichment {
    pub mgr: Arc<ProcessManager>,
    pub metadata: Arc<MetadataCache>,
}

/// Handle to the running pipeline threads.
pub(crate) struct Pipeline {
    stop: Arc<AtomicBool>,
    recv_thread: Option<JoinHandle<()>>,
    parse_thread: Option<JoinHandle<()>>,
}

impl Pipeline {
    /// Spawns the receive and parse threads and returns the event receiver
    /// (channel B). `net_rx` carries decoded ETW network events to merge, if the
    /// network source is enabled.
    pub(crate) fn start(
        port: Arc<FilterPort>,
        enrich: Enrichment,
        net_rx: Option<Receiver<NetworkEvent>>,
    ) -> (Self, Receiver<Event>) {
        let stop = Arc::new(AtomicBool::new(false));
        let (tx_a, rx_a) = bounded::<RawBatch>(CHANNEL_A_CAP);
        let (tx_b, rx_b) = unbounded::<Event>();

        let recv_stop = Arc::clone(&stop);
        let recv_thread = std::thread::Builder::new()
            .name("procmon-recv".into())
            .spawn(move || receive_loop(port, tx_a, recv_stop))
            .expect("spawn receive thread");

        let parse_thread = std::thread::Builder::new()
            .name("procmon-parse".into())
            .spawn(move || parse_loop(rx_a, net_rx, enrich, tx_b))
            .expect("spawn parse thread");

        (
            Self {
                stop,
                recv_thread: Some(recv_thread),
                parse_thread: Some(parse_thread),
            },
            rx_b,
        )
    }

    /// Signals the threads to stop and joins them. The caller stops the network
    /// session separately so the parse thread's merge sees both inputs end.
    pub(crate) fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.recv_thread.take() {
            let _ = t.join();
        }
        if let Some(t) = self.parse_thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Receive thread body: pull batches from the driver into channel A. Each batch
/// gets its own buffer, handed off whole (`Box` → `Arc` is a move, not a copy);
/// only near-empty batches are compacted, per [`COMPACT_THRESHOLD`].
fn receive_loop(port: Arc<FilterPort>, tx_a: Sender<RawBatch>, stop: Arc<AtomicBool>) {
    loop {
        let mut buf = vec![0u8; MESSAGE_BUFFER_LEN].into_boxed_slice();
        match port.recv(&mut buf, &stop) {
            Ok(Some(len)) => {
                let start = ProcmonMessageHeader::BATCH_OFFSET;
                let end = (start + len).min(buf.len());
                if end <= start {
                    continue;
                }
                let batch = if end - start < COMPACT_THRESHOLD {
                    RawBatch {
                        buf: Arc::from(&buf[start..end]),
                        records: 0..end - start,
                    }
                } else {
                    RawBatch {
                        buf: Arc::from(buf),
                        records: start..end,
                    }
                };
                // Back-pressure: blocks if the parser is behind. Stops if the
                // channel is disconnected (parser gone) or stop was requested.
                if tx_a.send(batch).is_err() || stop.load(Ordering::Relaxed) {
                    break;
                }
            }
            Ok(None) => break, // stop requested during the receive wait
            Err(e) => {
                tracing::warn!("receive failed: {e}");
                break;
            }
        }
    }
}

/// Parse thread body: correlate records and emit events, merging network events.
fn parse_loop(
    rx_a: Receiver<RawBatch>,
    net_rx: Option<Receiver<NetworkEvent>>,
    enrich: Enrichment,
    tx_b: Sender<Event>,
) {
    let mut correlator = Correlator::new();
    // Reused across batches so a steady event stream doesn't regrow it each time.
    let mut events: Vec<Event> = Vec::new();
    // A "parked" channel whose sender we keep alive: selecting on it blocks
    // forever (never ready, never disconnected). We swap the network branch to it
    // once the real network source ends, so a disconnected `net_rx` cannot busy-
    // loop the select, and it stands in when there is no network source at all.
    let (park_tx, park_rx) = unbounded::<NetworkEvent>();
    let mut net_rx = net_rx.unwrap_or_else(|| park_rx.clone());

    loop {
        crossbeam_channel::select! {
            recv(rx_a) -> msg => match msg {
                // The filter source is the lifeline: when it closes, shut down.
                Ok(batch) => handle_batch(batch, &mut correlator, &mut events, &enrich, &tx_b),
                Err(_) => break,
            },
            recv(net_rx) -> msg => match msg {
                Ok(net) => emit_network(net, &enrich, &tx_b),
                Err(_) => net_rx = park_rx.clone(), // disable a dead network branch
            },
        }
    }

    drop(park_tx); // keep the park sender alive until the loop exits

    // Flush any PRE records still awaiting completion.
    let mut tail = Vec::new();
    correlator.flush(&enrich.mgr, &mut tail);
    for ev in tail {
        let _ = tx_b.send(ev);
    }
}

/// Parses one batch into events, enriches them, and forwards them. `events` is
/// a reusable scratch vector (drained, not dropped).
fn handle_batch(
    batch: RawBatch,
    correlator: &mut Correlator,
    events: &mut Vec<Event>,
    enrich: &Enrichment,
    tx_b: &Sender<Event>,
) {
    correlator.ingest_shared(&batch.buf, batch.records, &enrich.mgr, events);
    for ev in events.drain(..) {
        // Drop events whose originating process is not tracked, matching C++
        // `CEventMgr::Process` (which emits a view only when the process is
        // known). Process create/exit/image-load always carry their process.
        if ev.process().is_none() {
            continue;
        }
        resolve_metadata_if_process(&ev, enrich);
        if tx_b.send(ev).is_err() {
            break;
        }
    }
}

/// Wraps a network event with its process snapshot and forwards it.
fn emit_network(net: NetworkEvent, enrich: &Enrichment, tx_b: &Sender<Event>) {
    let proc = enrich.mgr.by_pid(net.pid);
    let _ = tx_b.send(Event::from_network(
        Arc::new(net),
        crate::event::ProcessSource::Live(proc),
    ));
}

/// On a process create/init event, resolves the image metadata (version strings
/// and icons) synchronously and attaches it to the process record. Cached by
/// image path, so only the first process of each image reads from disk.
fn resolve_metadata_if_process(ev: &Event, enrich: &Enrichment) {
    if ev.monitor_type() != MonitorType::Process {
        return;
    }
    if !matches!(ev.notify_type(), proc_notify::CREATE | proc_notify::INIT) {
        return;
    }
    if let Some(rec) = ev.process() {
        if rec.meta().is_none() {
            // `image_path` is already a DOS path (converted at parse time).
            rec.set_meta(enrich.metadata.resolve(&rec.info.image_path));
        }
    }
}
