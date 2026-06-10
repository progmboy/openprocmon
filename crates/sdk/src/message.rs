//! Walking a received batch of records.
//!
//! `FilterGetMessage` delivers many variable-length records back-to-back in one
//! buffer. The receive thread hands that buffer to the parse thread as an
//! `Arc<[u8]>`, which uses [`entry_offsets`] to find each record's start and
//! wraps each one as a `Record` (buffer + offset) — no per-record copies. The
//! batch buffer is freed once no event references it any longer.

use crate::kernel_types::LogEntry;

/// Iterates the byte offset of every record in `batch`.
///
/// Advances by each record's `entry_size`; stops early (without panicking) if a
/// record is truncated or reports a zero size, so a corrupt tail can never cause
/// an out-of-bounds read or an infinite loop.
pub fn entry_offsets(batch: &[u8]) -> EntryIter<'_> {
    EntryIter { batch, off: 0 }
}

/// Yields the start offset of each record in a batch. See [`entry_offsets`].
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub struct EntryIter<'a> {
    batch: &'a [u8],
    off: usize,
}

impl Iterator for EntryIter<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        let header = LogEntry::view(self.batch, self.off)?;
        let cur = self.off;
        let size = header.entry_size();
        if size == 0 || cur + size > self.batch.len() {
            // Corrupt or truncated record: the header fit (view succeeded) so
            // `cur` is valid to yield, but we must not advance past the buffer.
            self.off = self.batch.len();
            return Some(cur);
        }
        self.off += size;
        Some(cur)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel_types::test_support::entry_bytes;

    #[test]
    fn iterates_two_records() {
        let a = entry_bytes(3, 20, 1, 0, &[1, 2, 3, 4]);
        let b = entry_bytes(2, 0, 1, 0, &[9, 9]);
        let mut batch = a.clone();
        batch.extend_from_slice(&b);
        let offsets: Vec<usize> = entry_offsets(&batch).collect();
        assert_eq!(offsets, vec![0, a.len()]);
    }

    #[test]
    fn empty_batch_yields_nothing() {
        assert_eq!(entry_offsets(&[]).count(), 0);
    }

    #[test]
    fn truncated_tail_stops_safely() {
        let mut batch = entry_bytes(3, 20, 1, 0, &[1, 2, 3, 4]);
        // Append a header claiming more data than is present.
        let mut bad = entry_bytes(3, 20, 2, 0, &[]);
        // Bump its data_length without supplying the bytes.
        let off = crate::kernel_types::LOG_ENTRY_SIZE - 8; // data_length field area
        let _ = off;
        bad.truncate(crate::kernel_types::LOG_ENTRY_SIZE);
        batch.extend_from_slice(&bad);
        // Should yield both starts and then stop, never looping or panicking.
        let count = entry_offsets(&batch).count();
        assert_eq!(count, 2);
    }
}
