//! Parent-side (unelevated) orchestration of an elevated capture worker over an
//! `interprocess` named pipe. The parent is the pipe server; the elevated child
//! connects as client and is put into worker mode via `--control-pipe`.
//!
//! Completion is detected over the pipe, not by waiting on the child process
//! handle — `ShellExecuteEx`'s `runas` hProcess is unreliable to wait on. The
//! worker sends `Started{pml_path}` on connect and a terminal `Done{...}` once
//! it finalizes; a clean EOF (the worker closed the pipe on exit) is an
//! equivalent "finished" signal. The parent->child direction carries a single
//! `Stop` for background `stop_capture` (one-shot captures self-stop at their
//! duration limit and need no message).

use std::io::BufReader;

use anyhow::{anyhow, Context, Result};
use interprocess::local_socket::{
    prelude::*, GenericNamespaced, ListenerOptions, RecvHalf, SendHalf, Stream,
};

use crate::ipc::{read_msg, write_msg, ChildMsg, ParentMsg};

/// A unique pipe name for one capture session. `seq` disambiguates within a pid.
pub fn pipe_name(seq: u64) -> String {
    format!("procmon-cli-{}-{}.sock", std::process::id(), seq)
}

/// The accepted connection to the elevated worker, split into owned read/write
/// halves, plus the child handle.
pub struct WorkerLink {
    pub reader: BufReader<RecvHalf>,
    pub writer: SendHalf,
    // Held only to keep the elevated process handle open until the link drops
    // (RAII close). Completion is detected over the pipe, never by waiting on it.
    #[cfg(windows)]
    #[allow(dead_code)]
    pub child: crate::elevate::ElevatedChild,
}

#[cfg(windows)]
impl WorkerLink {
    /// Reads the worker's first `Started{pml_path}` message (sent right after it
    /// connects, while it is still alive — so this read does not block on a dead
    /// peer). Returns the PML path it reported, or `None` if the stream closed.
    pub fn read_started(&mut self) -> Result<Option<String>> {
        match read_msg::<ChildMsg, _>(&mut self.reader)? {
            Some(ChildMsg::Started { pml_path }) => Ok(Some(pml_path)),
            _ => Ok(None),
        }
    }

    /// Signals the worker to stop and finalize (background `stop_capture`).
    pub fn send_stop(&mut self) -> std::io::Result<()> {
        write_msg(&mut self.writer, &ParentMsg::Stop)
    }

    /// Reads until the worker's terminal `Done` message (its result) or a clean
    /// EOF — the worker closes the pipe when it exits, so EOF reliably signals
    /// "worker finished" even if the `Done` write was lost. Returns the Done
    /// fields `(events_written, stopped_reason, pml_path)`, or `None` on EOF.
    ///
    /// We deliberately do NOT `WaitForSingleObject` the child handle:
    /// `ShellExecuteEx`'s `runas` hProcess is unreliable to wait on for an
    /// elevated child, so the pipe is the source of truth for completion.
    pub fn read_done(&mut self) -> Result<Option<(u64, String, String)>> {
        loop {
            match read_msg::<ChildMsg, _>(&mut self.reader)? {
                Some(ChildMsg::Done {
                    events_written,
                    stopped_reason,
                    pml_path,
                }) => return Ok(Some((events_written, stopped_reason, pml_path))),
                Some(_) => continue,     // Started / Status
                None => return Ok(None), // EOF: worker exited
            }
        }
    }
}

/// Creates the pipe listener, relaunches self elevated with the worker args, and
/// accepts the worker's connection. `worker_args` is the full argv for the
/// elevated `procmon-cli capture ... --control-pipe <name>` invocation.
#[cfg(windows)]
pub fn launch_worker(name: &str, mut worker_args: Vec<String>) -> Result<WorkerLink> {
    let ns = name
        .to_ns_name::<GenericNamespaced>()
        .context("pipe name")?;
    let listener = ListenerOptions::new()
        .name(ns)
        .create_sync()
        .context("create pipe listener")?;

    // Tell the elevated child which pipe to connect back on — this is what puts
    // it into worker mode. Without it the child would just capture in-process
    // (it is elevated) and never connect, hanging our accept().
    worker_args.push("--control-pipe".into());
    worker_args.push(name.to_string());

    // Relaunch elevated AFTER the listener exists so the child can connect.
    let child = crate::elevate::relaunch_elevated(&worker_args)
        .map_err(|e| anyhow!("elevation failed: {e}"))?;

    // Accept the worker (it connects shortly after launch). Split the duplex
    // stream into an owned reader and writer.
    let conn = listener.accept().context("accept worker connection")?;
    let (rh, sh) = conn.split();
    Ok(WorkerLink {
        reader: BufReader::new(rh),
        writer: sh,
        child,
    })
}

/// Connects to the parent's pipe as the worker (client side). Used in worker mode.
pub fn connect_worker(name: &str) -> Result<(BufReader<RecvHalf>, SendHalf)> {
    let ns = name
        .to_ns_name::<GenericNamespaced>()
        .context("pipe name")?;
    let conn = Stream::connect(ns).context("connect to parent pipe")?;
    let (rh, sh) = conn.split();
    Ok((BufReader::new(rh), sh))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Proves server/client wiring + that a worker reading the pipe sees EOF when
    // the server side is dropped (parent-exit simulation) — all unelevated.
    #[test]
    fn server_drop_gives_worker_eof() {
        let name = format!("procmon-test-{}.sock", std::process::id());
        let ns = name.as_str().to_ns_name::<GenericNamespaced>().unwrap();
        let listener = ListenerOptions::new().name(ns).create_sync().unwrap();

        let cname = name.clone();
        let worker = std::thread::spawn(move || {
            let (mut reader, _writer) = connect_worker(&cname).unwrap();
            // Worker waits for a ParentMsg; gets EOF (None) once the server drops.
            read_msg::<ParentMsg, _>(&mut reader).unwrap()
        });

        let conn = listener.accept().unwrap();
        // Drop the server-side connection AND listener -> client sees EOF.
        drop(conn);
        drop(listener);

        let got = worker.join().unwrap();
        assert_eq!(
            got, None,
            "worker observes clean EOF when parent/server drops"
        );
    }
}
