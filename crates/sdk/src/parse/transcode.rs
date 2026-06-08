//! Shared helper for serializing a live (driver-form) detail blob into PML form.
//!
//! The driver's `EventData` buffer and Procmon's PML detail blob are structurally
//! identical (same structs, op-codes, `FLT_PARAMETERS`) — they differ only in the
//! path strings: the driver stores NT device paths (`\Device\HarddiskVolume1\…`)
//! and `\REGISTRY\…` keys, whereas PML stores the resolved DOS (`C:\…`) and hive
//! (`HKLM\…`) forms. So each category's `pml_detail` (in `parse::{file,reg,proc}`)
//! copies the live blob verbatim and uses [`splice`] to relocate **only** the
//! embedded path string(s): each [`PathEdit`] patches a path's `u16` length field
//! (written as UTF-16, i.e. string-info with the ASCII bit clear, which our reader
//! and Procmon both accept) and replaces its data span. Every other detail field
//! — access masks, value data, info classes, the trailing `LOG_FILE_CREATE`, SIDs,
//! command line — is preserved byte-exact.

use crate::parse::{read_detail_str, DetailMode};

/// One path string to relocate: where its `u16` length field lives, where its
/// UTF-16 data starts, how many units it currently occupies, and the converted
/// (DOS/hive) replacement text.
pub(crate) struct PathEdit {
    pub len_field_off: usize,
    pub data_off: usize,
    pub raw_units: usize,
    pub text: String,
}

/// Rebuilds `blob`, replacing each path's length field and UTF-16 data span with
/// its converted form. Edits act on original offsets and may change byte length;
/// they are applied left-to-right (sorted, non-overlapping) so later offsets stay
/// valid against the original blob.
pub(crate) fn splice(blob: &[u8], edits: Vec<PathEdit>) -> Vec<u8> {
    // Each path contributes two non-overlapping edits: its length field and data.
    let mut ops: Vec<(usize, usize, Vec<u8>)> = Vec::with_capacity(edits.len() * 2);
    for e in &edits {
        let units: Vec<u16> = e.text.encode_utf16().collect();
        let count = (units.len() as u16) & 0x7fff; // ASCII bit clear => UTF-16
        ops.push((e.len_field_off, 2, count.to_le_bytes().to_vec()));
        let mut bytes = Vec::with_capacity(units.len() * 2);
        for u in units {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        ops.push((e.data_off, e.raw_units * 2, bytes));
    }
    ops.sort_by_key(|(off, _, _)| *off);

    let mut out = Vec::with_capacity(blob.len());
    let mut cursor = 0;
    for (off, len, bytes) in ops {
        // Skip malformed/overlapping edits rather than panic on a bad blob.
        if off < cursor || off > blob.len() {
            continue;
        }
        out.extend_from_slice(&blob[cursor..off]);
        out.extend_from_slice(&bytes);
        cursor = (off + len).min(blob.len());
    }
    out.extend_from_slice(&blob[cursor..]);
    out
}

/// Reads the live UTF-16 string at `off` (unit count `raw`) for conversion.
pub(crate) fn live_str(data: &[u8], off: usize, raw: u16) -> String {
    read_detail_str(data, off, raw, DetailMode::Live).0
}

/// Reads a little-endian `u16` at `off`, or 0 if out of bounds.
pub(crate) fn u16_at(data: &[u8], off: usize) -> u16 {
    data.get(off..off + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .unwrap_or(0)
}
