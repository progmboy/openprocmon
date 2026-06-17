//! The elevated capture worker: drive a running capture, react to parent `Stop`
//! over the pipe, and — critically — self-stop gracefully on pipe EOF (parent
//! exited) so the PML is finalized rather than left truncated by a hard kill.

use std::io::{BufRead, Write};

use procmon_core::CaptureOutcome;

use crate::ipc::{read_msg, write_msg, ChildMsg, ParentMsg};

/// A running capture the worker can stop to obtain its outcome.
/// `core::CaptureSession` implements this; tests use a fake.
pub trait Capturer {
    fn pml_path(&self) -> String;
    /// Consumes the capturer, finalizing and returning the outcome.
    fn stop(self: Box<Self>) -> std::io::Result<CaptureOutcome>;
}

/// Runs the worker control loop until the parent sends `Stop` or the pipe hits
/// EOF (parent died). Sends `Started` first, then `Done` after finalizing.
/// Returns the outcome that was sent.
pub fn run_worker<R: BufRead, W: Write>(
    capturer: Box<dyn Capturer>,
    reader: &mut R,
    writer: &mut W,
) -> std::io::Result<CaptureOutcome> {
    write_msg(
        writer,
        &ChildMsg::Started {
            pml_path: capturer.pml_path(),
        },
    )?;

    // Block on parent messages. A clean EOF (None) means the parent exited —
    // finalize anyway. A ParentMsg::Stop also finalizes.
    loop {
        match read_msg::<ParentMsg, _>(reader)? {
            Some(ParentMsg::Stop) => break,
            None => break, // pipe EOF: parent gone -> graceful stop
        }
    }

    let outcome = capturer.stop()?;
    write_msg(
        writer,
        &ChildMsg::Done {
            events_written: outcome.events_written as u64,
            stopped_reason: format!("{:?}", outcome.stopped_reason),
            pml_path: outcome.pml_path.clone(),
        },
    )?;
    Ok(outcome)
}

impl Capturer for procmon_core::CaptureSession {
    fn pml_path(&self) -> String {
        self.pml_path().to_string_lossy().into_owned()
    }
    fn stop(self: Box<Self>) -> std::io::Result<CaptureOutcome> {
        (*self)
            .stop()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procmon_core::StoppedReason;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    struct FakeCapturer {
        stopped: Arc<AtomicBool>,
    }
    impl Capturer for FakeCapturer {
        fn pml_path(&self) -> String {
            "C:/tmp/fake.pml".into()
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

    #[test]
    fn explicit_stop_finalizes_and_sends_done() {
        let stopped = Arc::new(AtomicBool::new(false));
        let cap = Box::new(FakeCapturer {
            stopped: stopped.clone(),
        });
        // Parent sends a Stop line.
        let input = b"{\"type\":\"stop\"}\n".to_vec();
        let mut reader = std::io::BufReader::new(&input[..]);
        let mut out: Vec<u8> = Vec::new();
        let outcome = run_worker(cap, &mut reader, &mut out).unwrap();
        assert_eq!(outcome.events_written, 7);
        assert!(stopped.load(Ordering::SeqCst), "capturer was stopped");
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("\"type\":\"started\""));
        assert!(text.contains("\"type\":\"done\""));
    }

    #[test]
    fn pipe_eof_triggers_graceful_stop() {
        let stopped = Arc::new(AtomicBool::new(false));
        let cap = Box::new(FakeCapturer {
            stopped: stopped.clone(),
        });
        // Empty input == immediate EOF (parent exited before sending anything).
        let input: Vec<u8> = Vec::new();
        let mut reader = std::io::BufReader::new(&input[..]);
        let mut out: Vec<u8> = Vec::new();
        let outcome = run_worker(cap, &mut reader, &mut out).unwrap();
        assert!(stopped.load(Ordering::SeqCst), "EOF still finalizes the PML");
        assert_eq!(format!("{:?}", outcome.stopped_reason), "Manual");
    }
}
