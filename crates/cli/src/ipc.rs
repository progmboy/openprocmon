//! Line-delimited JSON control protocol between the unelevated parent (pipe
//! server) and the elevated capture worker (pipe client). One JSON object per
//! line; `\n`-terminated. Both directions share this module so encoder and
//! decoder cannot drift.

use std::io::{BufRead, Write};

use serde::{Deserialize, Serialize};

/// Parent -> child (worker) commands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParentMsg {
    /// Ask the worker to stop capturing, finalize the PML, and exit.
    Stop,
}

/// Child (worker) -> parent events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChildMsg {
    /// Sent once the worker has connected and started capturing.
    Started { pml_path: String },
    /// Periodic progress (best-effort).
    Status { events_written: u64 },
    /// Terminal: capture finalized. The worker exits right after sending this.
    Done {
        events_written: u64,
        stopped_reason: String,
        pml_path: String,
    },
}

/// Writes one message as a single `\n`-terminated JSON line and flushes.
pub fn write_msg<T: Serialize, W: Write>(w: &mut W, msg: &T) -> std::io::Result<()> {
    let mut line = serde_json::to_string(msg)?;
    line.push('\n');
    w.write_all(line.as_bytes())?;
    w.flush()
}

/// Reads one `\n`-terminated JSON line into `T`. Returns `Ok(None)` on clean EOF
/// (peer closed the pipe — e.g. the parent exited).
pub fn read_msg<T: for<'de> Deserialize<'de>, R: BufRead>(r: &mut R) -> std::io::Result<Option<T>> {
    let mut line = String::new();
    let n = r.read_line(&mut line)?;
    if n == 0 {
        return Ok(None);
    }
    let msg = serde_json::from_str(line.trim_end())?;
    Ok(Some(msg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_msg_roundtrips_through_a_pipe_buffer() {
        let mut buf: Vec<u8> = Vec::new();
        write_msg(&mut buf, &ParentMsg::Stop).unwrap();
        assert_eq!(buf, b"{\"type\":\"stop\"}\n");

        let mut cursor = std::io::BufReader::new(&buf[..]);
        let got: Option<ParentMsg> = read_msg(&mut cursor).unwrap();
        assert_eq!(got, Some(ParentMsg::Stop));
    }

    #[test]
    fn child_msgs_roundtrip_and_eof_is_none() {
        let mut buf: Vec<u8> = Vec::new();
        write_msg(
            &mut buf,
            &ChildMsg::Started {
                pml_path: "C:/tmp/a.pml".into(),
            },
        )
        .unwrap();
        write_msg(
            &mut buf,
            &ChildMsg::Done {
                events_written: 42,
                stopped_reason: "Manual".into(),
                pml_path: "C:/tmp/a.pml".into(),
            },
        )
        .unwrap();

        let mut r = std::io::BufReader::new(&buf[..]);
        let a: Option<ChildMsg> = read_msg(&mut r).unwrap();
        assert!(matches!(a, Some(ChildMsg::Started { .. })));
        let b: Option<ChildMsg> = read_msg(&mut r).unwrap();
        assert!(matches!(b, Some(ChildMsg::Done { events_written: 42, .. })));
        // No more lines -> clean EOF.
        let c: Option<ChildMsg> = read_msg(&mut r).unwrap();
        assert_eq!(c, None);
    }
}
