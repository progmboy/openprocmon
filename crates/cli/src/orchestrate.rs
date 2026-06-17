//! Parent-side (unelevated) orchestration of an elevated capture worker over an
//! `interprocess` named pipe. The parent is the pipe server; the elevated child
//! connects as client. The parent can `wait` on the child but not kill it, so it
//! stops captures by sending `ParentMsg::Stop` over the pipe.

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
    #[cfg(windows)]
    pub child: crate::elevate::ElevatedChild,
}

/// Creates the pipe listener, relaunches self elevated with the worker args, and
/// accepts the worker's connection. `worker_args` is the full argv for the
/// elevated `procmon-cli capture ... --control-pipe <name>` invocation.
#[cfg(windows)]
pub fn launch_worker(name: &str, worker_args: Vec<String>) -> Result<WorkerLink> {
    let ns = name.to_ns_name::<GenericNamespaced>().context("pipe name")?;
    let listener = ListenerOptions::new()
        .name(ns)
        .create_sync()
        .context("create pipe listener")?;

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
    let ns = name.to_ns_name::<GenericNamespaced>().context("pipe name")?;
    let conn = Stream::connect(ns).context("connect to parent pipe")?;
    let (rh, sh) = conn.split();
    Ok((BufReader::new(rh), sh))
}

/// Drives a worker for a one-shot or explicit-stop capture: optionally send
/// `Stop`, then read until the terminal `Done` message and return it.
pub fn drive_to_done(link: &mut WorkerLink, stop_first: bool) -> Result<ChildMsg> {
    if stop_first {
        write_msg(&mut link.writer, &ParentMsg::Stop).context("send stop")?;
    }
    while let Some(msg) = read_msg::<ChildMsg, _>(&mut link.reader)? {
        match msg {
            ChildMsg::Started { .. } | ChildMsg::Status { .. } => continue,
            done @ ChildMsg::Done { .. } => return Ok(done),
        }
    }
    Err(anyhow!("worker exited without a Done message"))
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
        assert_eq!(got, None, "worker observes clean EOF when parent/server drops");
    }
}
