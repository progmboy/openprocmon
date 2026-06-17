//! The elevated capture worker: drive a running capture and finalize the PML
//! gracefully when ANY of three things happens — the capture self-stops at its
//! duration/size limit (one-shot), the parent sends `Stop` (background
//! stop_capture), or the pipe hits EOF because the parent died (orphan
//! protection). A hard kill would truncate the not-yet-written PML, so every
//! path goes through `capturer.stop()` to write it out.

use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use procmon_core::CaptureOutcome;

use crate::ipc::{read_msg, write_msg, ChildMsg, ParentMsg};

/// A running capture the worker can poll and stop.
/// `core::CaptureSession` implements this; tests use a fake.
pub trait Capturer {
    fn pml_path(&self) -> String;
    /// Whether the capture is still running (false once it self-stops at its
    /// duration/size limit).
    fn is_running(&self) -> bool;
    /// Consumes the capturer, finalizing and returning the outcome.
    fn stop(self: Box<Self>) -> std::io::Result<CaptureOutcome>;
}

/// Runs the worker until the capture self-stops OR the parent signals (a `Stop`
/// message or a clean pipe EOF), then finalizes and reports `Done`. Sends
/// `Started` first. The pipe reader runs on a detached thread so the poll loop
/// can also notice a self-stop; `Done` is best-effort (the pipe may already be
/// broken if the parent died).
pub fn run_worker<R, W>(
    capturer: Box<dyn Capturer>,
    reader: R,
    writer: &mut W,
) -> std::io::Result<CaptureOutcome>
where
    R: BufRead + Send + 'static,
    W: Write,
{
    write_msg(
        writer,
        &ChildMsg::Started {
            pml_path: capturer.pml_path(),
        },
    )?;

    // A detached reader thread sets `signalled` when the parent sends `Stop` or
    // the pipe hits EOF (parent died). Detached, not joined: on a self-stop the
    // parent never sends anything, so the thread would block forever — letting
    // it die with the process avoids a second deadlock.
    let signalled = Arc::new(AtomicBool::new(false));
    let flag = signalled.clone();
    std::thread::spawn(move || {
        let mut reader = reader;
        // Either a `Stop` or a clean EOF (None) means "finalize now".
        let _ = read_msg::<ParentMsg, _>(&mut reader);
        flag.store(true, Ordering::SeqCst);
    });

    // Wait for the capture to self-stop (duration/size) or the parent to signal.
    while capturer.is_running() && !signalled.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(100));
    }

    let outcome = capturer.stop()?;
    // Best-effort: if the parent died the pipe write fails, but the PML is saved.
    let _ = write_msg(
        writer,
        &ChildMsg::Done {
            events_written: outcome.events_written as u64,
            stopped_reason: format!("{:?}", outcome.stopped_reason),
            pml_path: outcome.pml_path.clone(),
        },
    );
    Ok(outcome)
}

impl Capturer for procmon_core::CaptureSession {
    fn pml_path(&self) -> String {
        self.pml_path().to_string_lossy().into_owned()
    }
    fn is_running(&self) -> bool {
        procmon_core::CaptureSession::is_running(self)
    }
    fn stop(self: Box<Self>) -> std::io::Result<CaptureOutcome> {
        (*self)
            .stop()
            .map_err(|e| std::io::Error::other(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procmon_core::StoppedReason;
    use std::io::Cursor;

    struct FakeCapturer {
        running: Arc<AtomicBool>,
        stopped: Arc<AtomicBool>,
    }
    impl Capturer for FakeCapturer {
        fn pml_path(&self) -> String {
            "C:/tmp/fake.pml".into()
        }
        fn is_running(&self) -> bool {
            self.running.load(Ordering::SeqCst)
        }
        fn stop(self: Box<Self>) -> std::io::Result<CaptureOutcome> {
            self.stopped.store(true, Ordering::SeqCst);
            Ok(CaptureOutcome {
                pml_path: "C:/tmp/fake.pml".into(),
                events_written: 7,
                stopped_reason: StoppedReason::Manual,
            })
        }
    }

    fn fake(running: bool) -> (Box<FakeCapturer>, Arc<AtomicBool>) {
        let stopped = Arc::new(AtomicBool::new(false));
        let cap = Box::new(FakeCapturer {
            running: Arc::new(AtomicBool::new(running)),
            stopped: stopped.clone(),
        });
        (cap, stopped)
    }

    #[test]
    fn explicit_stop_finalizes_and_sends_done() {
        let (cap, stopped) = fake(true);
        // Parent sends a Stop line; capture is "running" until the signal.
        let reader = Cursor::new(b"{\"type\":\"stop\"}\n".to_vec());
        let mut out: Vec<u8> = Vec::new();
        let outcome = run_worker(cap, reader, &mut out).unwrap();
        assert_eq!(outcome.events_written, 7);
        assert!(stopped.load(Ordering::SeqCst), "capturer was stopped");
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("\"type\":\"started\""));
        assert!(text.contains("\"type\":\"done\""));
    }

    #[test]
    fn pipe_eof_triggers_graceful_stop() {
        let (cap, stopped) = fake(true);
        // Empty input == immediate EOF (parent exited before sending anything).
        let reader = Cursor::new(Vec::new());
        let mut out: Vec<u8> = Vec::new();
        let outcome = run_worker(cap, reader, &mut out).unwrap();
        assert!(
            stopped.load(Ordering::SeqCst),
            "EOF still finalizes the PML"
        );
        assert_eq!(format!("{:?}", outcome.stopped_reason), "Manual");
    }

    #[test]
    fn self_stop_finalizes_without_a_parent_signal() {
        // One-shot duration case: the capture has already self-stopped; the
        // parent never sends Stop. The worker must finalize anyway, not hang.
        let (cap, stopped) = fake(false);
        let reader = Cursor::new(Vec::new());
        let mut out: Vec<u8> = Vec::new();
        run_worker(cap, reader, &mut out).unwrap();
        assert!(stopped.load(Ordering::SeqCst), "self-stop still finalizes");
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("\"type\":\"done\""));
    }
}
