//! Memory-mapped, lazy PML reader. Mirrors `stream_logs_format.py`'s
//! `PMLStreamReader`: `open` parses the header + string/process/host/port tables +
//! the event-offset array; [`PmlReader::event`] parses one event on demand from
//! the mmap (random access), so huge logs cost only what's actually read.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use memmap2::Mmap;

use crate::error::{Error, Result};
use crate::pml::detail::{self, Tables};
use crate::pml::model::{PmlEvent, PmlIcon, PmlModule, PmlProcess};
use crate::EventClass;

const HEADER_SIZE: usize = 0x3a8;

/// Parsed PML header fields we keep (subset of the on-disk header).
#[derive(Clone, Debug)]
pub struct Header {
    pub is_64bit: bool,
    pub computer_name: String,
    pub system_root: String,
    pub number_of_events: u32,
    pub windows_major: u32,
    pub windows_minor: u32,
    pub windows_build: u32,
    pub num_logical_processors: u32,
    pub ram_bytes: u64,
    events_offsets_array_offset: u64,
    process_table_offset: u64,
    strings_table_offset: u64,
    icon_table_offset: u64,
    hosts_and_ports_offset: u64,
}

/// A read-only view over a `.PML` file.
pub struct PmlReader {
    _mmap: Mmap,
    header: Header,
    sizeof_pvoid: usize,
    event_offsets: Vec<u32>,
    processes: HashMap<u32, PmlProcess>,
    icons: Vec<PmlIcon>,
    hosts: HashMap<[u8; 16], Arc<str>>,
    ports: HashMap<(u16, bool), Arc<str>>,
}

impl PmlReader {
    /// Opens and indexes a PML file (header + tables + event offsets).
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(|e| Error::Parse(format!("PML open: {e}")))?;
        // SAFETY: the file is opened read-only; we never mutate the mapping and the
        // mapping is owned by `PmlReader` for as long as any borrow of it lives.
        let mmap =
            unsafe { Mmap::map(&file) }.map_err(|e| Error::Parse(format!("PML mmap: {e}")))?;
        Self::from_mmap(mmap)
    }

    fn from_mmap(mmap: Mmap) -> Result<Self> {
        let data = &mmap[..];
        let header = parse_header(data)?;
        // 32-bit PML detail parsing is not implemented yet (the SDK detail views
        // assume the host x64 FLT_PARAMETERS / pointer width). Reject up front
        // rather than return half-parsed events.
        if !header.is_64bit {
            return Err(Error::Parse(
                "PML: 32-bit captures are not supported yet".into(),
            ));
        }
        let sizeof_pvoid = 8;

        let strings = parse_strings(data, header.strings_table_offset as usize)?;
        let processes = parse_processes(
            data,
            header.process_table_offset as usize,
            sizeof_pvoid,
            &strings,
        )?;
        let event_offsets = parse_event_offsets(
            data,
            header.events_offsets_array_offset as usize,
            header.number_of_events as usize,
        )?;
        let (hosts, ports) = parse_hosts_ports(data, header.hosts_and_ports_offset as usize)?;
        let icons = parse_icons(data, header.icon_table_offset as usize)?;

        let _ = &strings; // consumed by process parsing above
        Ok(Self {
            _mmap: mmap,
            header,
            sizeof_pvoid,
            event_offsets,
            processes,
            icons,
            hosts,
            ports,
        })
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    /// Number of events in the log.
    pub fn len(&self) -> usize {
        self.event_offsets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.event_offsets.is_empty()
    }

    /// Sequentially iterates all events as unified [`Event`]s (the PML stream for
    /// [`crate::EventSource`]). Events that fail to synthesize are skipped.
    pub fn events(self: &Arc<Self>) -> impl Iterator<Item = crate::event::Event> + '_ {
        let reader = Arc::clone(self);
        (0..self.len()).filter_map(move |i| reader.event_as_event(i).ok())
    }

    /// All known processes (unordered).
    pub fn processes(&self) -> impl Iterator<Item = &PmlProcess> {
        self.processes.values()
    }

    /// The process referenced by an event's `process_index`, if present.
    pub fn process(&self, process_index: u32) -> Option<&PmlProcess> {
        self.processes.get(&process_index)
    }

    /// The captured icon at `index` (see [`PmlProcess::icon_small`] /
    /// [`PmlProcess::icon_big`]), or `None` if the index is out of range. The
    /// icon's `data` is a Windows `ICONIMAGE` resource for `CreateIconFromResourceEx`.
    /// Index 0 is the empty "no icon" placeholder, reported as `None`.
    pub fn icon(&self, index: u32) -> Option<&PmlIcon> {
        self.icons
            .get(index as usize)
            .filter(|i| !i.data.is_empty())
    }

    /// All captured icons, indexed by the process icon fields.
    pub fn icons(&self) -> &[PmlIcon] {
        &self.icons
    }

    /// Parses the event at index `i` (0-based) from the mmap. Zero-copy: the detail
    /// bytes are borrowed for parsing and only the decoded fields are kept. Use
    /// [`event_with_raw`](Self::event_with_raw) for a byte-exact PML→PML round-trip.
    #[allow(dead_code)] // internal decode used by round-trip / comparison tests
    pub(crate) fn event(&self, i: usize) -> Result<PmlEvent> {
        self.event_inner(i, false)
    }

    /// Synthesizes the event at index `i` as a unified [`Event`] (kernel-record
    /// form, `mode = Pml`), with process info resolved via `ProcessSource::Pml`.
    /// The PML detail blob is the driver's `EventData`, so it parses through the
    /// exact same path as a live record. Network events are handled in a later
    /// step (here they synthesize with monitor type `Other`).
    pub fn event_as_event(self: &Arc<Self>, i: usize) -> Result<crate::event::Event> {
        let off = *self
            .event_offsets
            .get(i)
            .ok_or_else(|| Error::Parse(format!("PML: event index {i} out of range")))?
            as usize;
        let data = &self._mmap[..];
        let mut c = Cur::at(data, off)?;
        let process_index = c.u32()?;
        let tid = c.u32()?;
        let class_val = c.u32()?;
        let operation = c.u16()?;
        let _ = c.u16()?;
        let _ = c.u32()?;
        let duration = c.u64()?;
        let date_filetime = c.u64()?;
        let result = c.u32()?;
        let stack_depth = c.u16()? as usize;
        let _ = c.u16()?;
        let details_size = c.u32()? as usize;
        let extra_off = c.u32()? as usize;
        let class = EventClass::from_u32(class_val);
        let mut stack = Vec::with_capacity(stack_depth);
        for _ in 0..stack_depth {
            stack.push(c.pvoid(self.sizeof_pvoid)?);
        }
        let details = c.take(details_size)?;
        // Extra/completion blob (e.g. CreateFile's OpenResult) — the POST record's data.
        let extra: Option<&[u8]> = if extra_off > 0 {
            let mut ec = Cur::at(data, off + extra_off)?;
            let size = ec.u16()? as usize;
            Some(ec.take(size)?)
        } else {
            None
        };

        // Network: decode the PML blob into the shared NetworkEvent (same model the
        // live ETW path uses); pid/time come from the event's process/timestamp.
        if class == EventClass::Network {
            let mut net =
                crate::parse::network::decode_pml(operation, details, &self.hosts, &self.ports)
                    .ok_or_else(|| Error::Parse("PML: malformed network blob".into()))?;
            net.pid = self.process(process_index).map(|p| p.pid).unwrap_or(0);
            net.time = date_filetime as i64;
            return Ok(crate::event::Event::from_network(
                std::sync::Arc::new(net),
                crate::event::ProcessSource::Pml(Arc::clone(self), process_index),
            ));
        }

        // class -> LOG_MONITOR_TYPE (Process=1, Reg=2, File=3, Profiling=4).
        let monitor = match class {
            EventClass::Process => 1u16,
            EventClass::Registry => 2,
            EventClass::File => 3,
            EventClass::Profiling => 4,
            _ => 0,
        };
        let pre = crate::kernel_types::synth_record_full(
            monitor,
            operation,
            result as i32,
            tid,
            date_filetime as i64,
            &stack,
            details,
        );
        // PML stores duration directly; a sentinel value (where start + duration
        // would overflow) is treated as "no duration".
        let duration_ticks = (duration != 0 && date_filetime.checked_add(duration).is_some())
            .then_some(duration as i64);
        // A POST record only carries the completion (extra) data, if present.
        let post = extra.map(|e| {
            crate::kernel_types::synth_record_full(
                0,
                operation,
                result as i32,
                tid,
                date_filetime as i64,
                &[],
                e,
            )
        });
        crate::event::Event::from_pml_with(
            pre,
            post,
            crate::event::ProcessSource::Pml(Arc::clone(self), process_index),
            duration_ticks,
        )
        .ok_or_else(|| Error::Parse("PML: synthesized record too short".into()))
    }

    /// Like [`event`](Self::event) but also keeps the raw detail/extra blobs (one
    /// heap copy each) so a [`PmlWriter`] can write them back byte-for-byte. The PML
    /// detail blob is the driver's `EventData` with paths already in DOS form, so
    /// round-trip is just copying these bytes — no field-by-field re-encoding.
    #[allow(dead_code)] // internal decode used by round-trip / comparison tests
    pub(crate) fn event_with_raw(&self, i: usize) -> Result<PmlEvent> {
        self.event_inner(i, true)
    }

    #[allow(dead_code)]
    fn event_inner(&self, i: usize, keep_raw: bool) -> Result<PmlEvent> {
        let off = *self
            .event_offsets
            .get(i)
            .ok_or_else(|| Error::Parse(format!("PML: event index {i} out of range")))?
            as usize;
        let data = &self._mmap[..];
        let mut c = Cur::at(data, off)?;

        // Common event struct: "<IIIHHIQQIHHII" (52 bytes).
        let process_index = c.u32()?;
        let tid = c.u32()?;
        let class_val = c.u32()?;
        let operation = c.u16()?;
        let _ = c.u16()?;
        let _ = c.u32()?;
        let duration = c.u64()?;
        let date_filetime = c.u64()?;
        let result = c.u32()?;
        let stack_depth = c.u16()? as usize;
        let _ = c.u16()?;
        let details_size = c.u32()? as usize;
        let extra_details_offset = c.u32()? as usize;

        let class = EventClass::from_u32(class_val);

        // Stack frames (return addresses), pointer-sized.
        let mut stack = Vec::with_capacity(stack_depth);
        for _ in 0..stack_depth {
            stack.push(c.pvoid(self.sizeof_pvoid)?);
        }

        let details = c.take(details_size)?;

        // Extra detail (optional): the field is the offset from the event start.
        let extra = if extra_details_offset > 0 {
            let base = off + extra_details_offset;
            let mut ec = Cur::at(data, base)?;
            let size = ec.u16()? as usize;
            Some(ec.take(size)?)
        } else {
            None
        };

        let tables = Tables {
            hosts: &self.hosts,
            ports: &self.ports,
        };
        let parsed = detail::parse_event(class, operation, details, extra, &tables);

        // Only copy the raw blobs (mmap → heap) when round-trip needs them; the
        // default read path stays zero-copy.
        let raw_detail = keep_raw.then(|| Arc::<[u8]>::from(details));
        let raw_extra = if keep_raw {
            extra.map(Arc::<[u8]>::from)
        } else {
            None
        };

        Ok(PmlEvent {
            process_index,
            tid,
            class,
            operation,
            duration,
            date_filetime,
            result,
            stack,
            category: parsed.category,
            path: parsed.path,
            details: parsed.details,
            op_name: parsed.op_name,
            raw_detail,
            raw_extra,
        })
    }
}

// ---------------------------------------------------------------------------
// A bounds-checked little-endian cursor over a byte slice.
// ---------------------------------------------------------------------------

struct Cur<'a> {
    d: &'a [u8],
    pos: usize,
}

impl<'a> Cur<'a> {
    fn at(d: &'a [u8], pos: usize) -> Result<Self> {
        if pos > d.len() {
            return Err(Error::Parse("PML: offset past end of file".into()));
        }
        Ok(Self { d, pos })
    }

    fn need(&self, n: usize) -> Result<()> {
        if self.pos + n > self.d.len() {
            Err(Error::Parse("PML: unexpected end of data".into()))
        } else {
            Ok(())
        }
    }

    fn skip(&mut self, n: usize) -> Result<()> {
        self.need(n)?;
        self.pos += n;
        Ok(())
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        self.need(n)?;
        let s = &self.d[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes(b.try_into().unwrap()))
    }

    fn pvoid(&mut self, size: usize) -> Result<u64> {
        if size == 8 {
            self.u64()
        } else {
            Ok(self.u32()? as u64)
        }
    }

    /// Reads `size` bytes of UTF-16LE, decoding up to the first NUL.
    fn utf16(&mut self, size: usize) -> Result<String> {
        Ok(crate::parse::decode_utf16(self.take(size)?))
    }
}

// ---------------------------------------------------------------------------
// Table parsers (run once at open).
// ---------------------------------------------------------------------------

fn parse_header(data: &[u8]) -> Result<Header> {
    if data.len() < HEADER_SIZE {
        return Err(Error::Parse("PML: file smaller than header".into()));
    }
    let mut c = Cur::at(data, 0)?;
    if c.take(4)? != b"PML_" {
        return Err(Error::Parse("PML: bad signature".into()));
    }
    let version = c.u32()?;
    if version != 9 {
        return Err(Error::Parse(format!("PML: unsupported version {version}")));
    }
    let is_64bit = c.u32()? != 0;
    let computer_name = c.utf16(0x20)?;
    let system_root = c.utf16(0x208)?;
    let number_of_events = c.u32()?;
    c.skip(8)?;
    let _events_offset = c.u64()?;
    let events_offsets_array_offset = c.u64()?;
    let process_table_offset = c.u64()?;
    let strings_table_offset = c.u64()?;
    let icon_table_offset = c.u64()?;
    c.skip(12)?;
    let windows_major = c.u32()?;
    let windows_minor = c.u32()?;
    let windows_build = c.u32()?;
    let _windows_build_dec = c.u32()?;
    let _service_pack = c.utf16(0x32)?;
    c.skip(0xd6)?;
    let num_logical_processors = c.u32()?;
    let ram_bytes = c.u64()?;
    let header_size = c.u64()?;
    let hosts_and_ports_offset = c.u64()?;

    if events_offsets_array_offset == 0
        || process_table_offset == 0
        || strings_table_offset == 0
        || icon_table_offset == 0
    {
        return Err(Error::Parse(
            "PML: not closed cleanly (corrupt offsets)".into(),
        ));
    }
    if header_size as usize != HEADER_SIZE || hosts_and_ports_offset == 0 {
        return Err(Error::Parse("PML: corrupt header".into()));
    }

    Ok(Header {
        is_64bit,
        computer_name,
        system_root,
        number_of_events,
        windows_major,
        windows_minor,
        windows_build,
        num_logical_processors,
        ram_bytes,
        events_offsets_array_offset,
        process_table_offset,
        strings_table_offset,
        icon_table_offset,
        hosts_and_ports_offset,
    })
}

/// Strings array: count + relative-offset table + each `String { u32 size, utf16 }`.
fn parse_strings(data: &[u8], base: usize) -> Result<Vec<Arc<str>>> {
    let mut c = Cur::at(data, base)?;
    let n = c.u32()? as usize;
    let mut offsets = Vec::with_capacity(n);
    for _ in 0..n {
        offsets.push(c.u32()? as usize);
    }
    let mut strings = Vec::with_capacity(n);
    for off in offsets {
        let mut sc = Cur::at(data, base + off)?;
        let size = sc.u32()? as usize;
        strings.push(Arc::from(sc.utf16(size)?.as_str()));
    }
    Ok(strings)
}

fn str_at(strings: &[Arc<str>], ix: u32) -> Arc<str> {
    strings
        .get(ix as usize)
        .cloned()
        .unwrap_or_else(|| Arc::from(""))
}

/// Process array: count + (skipped) index array + relative-offset table + Process[].
fn parse_processes(
    data: &[u8],
    base: usize,
    sizeof_pvoid: usize,
    strings: &[Arc<str>],
) -> Result<HashMap<u32, PmlProcess>> {
    let mut c = Cur::at(data, base)?;
    let n = c.u32()? as usize;
    c.skip(n * 4)?; // process-index array (re-derivable from each Process)
    let mut offsets = Vec::with_capacity(n);
    for _ in 0..n {
        offsets.push(c.u32()? as usize);
    }
    let mut map = HashMap::with_capacity(n);
    for off in offsets {
        let mut pc = Cur::at(data, base + off)?;
        let proc = read_process(&mut pc, sizeof_pvoid, strings)?;
        map.insert(proc.process_index, proc);
    }
    Ok(map)
}

fn read_process(c: &mut Cur, sizeof_pvoid: usize, strings: &[Arc<str>]) -> Result<PmlProcess> {
    let process_index = c.u32()?;
    let pid = c.u32()?;
    let parent_pid = c.u32()?;
    c.skip(4)?;
    let authentication_id = c.u64()?;
    let session = c.u32()?;
    c.skip(4)?;
    let start_time = c.u64()?;
    let end_time = c.u64()?;
    let virtualized = c.u32()? != 0;
    let is_64bit = c.u32()? != 0;
    let integrity = str_at(strings, c.u32()?);
    let user = str_at(strings, c.u32()?);
    let process_name = str_at(strings, c.u32()?);
    let image_path = str_at(strings, c.u32()?);
    let command_line = str_at(strings, c.u32()?);
    let company = str_at(strings, c.u32()?);
    let version = str_at(strings, c.u32()?);
    let description = str_at(strings, c.u32()?);
    let icon_small = c.u32()?;
    let icon_big = c.u32()?;
    c.pvoid(sizeof_pvoid)?; // unknown
    let num_modules = c.u32()? as usize;
    let mut modules = Vec::with_capacity(num_modules);
    for _ in 0..num_modules {
        modules.push(read_module(c, sizeof_pvoid, strings)?);
    }
    Ok(PmlProcess {
        process_index,
        pid,
        parent_pid,
        authentication_id,
        session,
        start_time,
        end_time,
        virtualized,
        is_64bit,
        integrity,
        user,
        process_name,
        image_path,
        command_line,
        company,
        version,
        description,
        icon_small,
        icon_big,
        modules,
    })
}

fn read_module(c: &mut Cur, sizeof_pvoid: usize, strings: &[Arc<str>]) -> Result<PmlModule> {
    c.pvoid(sizeof_pvoid)?; // unknown
    let base_address = c.pvoid(sizeof_pvoid)?;
    let size = c.u32()?;
    let image_path = str_at(strings, c.u32()?);
    let version = str_at(strings, c.u32()?);
    let company = str_at(strings, c.u32()?);
    let description = str_at(strings, c.u32()?);
    let timestamp = c.u32()?;
    c.skip(0x18)?;
    Ok(PmlModule {
        base_address,
        size,
        image_path,
        version,
        company,
        description,
        timestamp,
    })
}

/// Icon array: count + relative-offset table + each `Icon { u32 dimension, u32
/// size, ICONIMAGE[size] }`. The ICONIMAGE bytes are kept verbatim (self-contained
/// — they render without the original executable, which may be on another machine).
fn parse_icons(data: &[u8], base: usize) -> Result<Vec<PmlIcon>> {
    let mut c = Cur::at(data, base)?;
    let n = c.u32()? as usize;
    let mut offsets = Vec::with_capacity(n);
    for _ in 0..n {
        offsets.push(c.u32()? as usize);
    }
    let mut icons = Vec::with_capacity(n);
    for off in offsets {
        // Index 0 is an empty placeholder ("no icon"); it parses as dimension 0 /
        // size 0 / empty data, which `icon()` treats as None.
        let mut ic = Cur::at(data, base + off)?;
        let dimension = ic.u32()?;
        let size = ic.u32()? as usize;
        let bytes = ic.take(size)?;
        icons.push(PmlIcon {
            dimension,
            data: Arc::<[u8]>::from(bytes),
        });
    }
    Ok(icons)
}

/// Event-offset array: `number_of_events` entries of `u32 offset + u8 flags`.
fn parse_event_offsets(data: &[u8], base: usize, count: usize) -> Result<Vec<u32>> {
    let mut c = Cur::at(data, base)?;
    let mut offsets = Vec::with_capacity(count);
    for _ in 0..count {
        offsets.push(c.u32()?);
        c.skip(1)?;
    }
    Ok(offsets)
}

type HostsPorts = (HashMap<[u8; 16], Arc<str>>, HashMap<(u16, bool), Arc<str>>);

fn parse_hosts_ports(data: &[u8], base: usize) -> Result<HostsPorts> {
    let mut c = Cur::at(data, base)?;
    let nh = c.u32()? as usize;
    let mut hosts = HashMap::with_capacity(nh);
    for _ in 0..nh {
        let mut ip = [0u8; 16];
        ip.copy_from_slice(c.take(16)?);
        let len = c.u32()? as usize;
        let name = c.utf16(len)?;
        hosts.insert(ip, Arc::from(name.as_str()));
    }
    let np = c.u32()? as usize;
    let mut ports = HashMap::with_capacity(np);
    for _ in 0..np {
        let port = c.u16()?;
        let is_tcp = c.u16()? != 0;
        let len = c.u32()? as usize;
        let name = c.utf16(len)?;
        ports.insert((port, is_tcp), Arc::from(name.as_str()));
    }
    Ok((hosts, ports))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/resources")
            .join(name)
    }

    /// A process-unique temp path (tests run in parallel and the reader mmaps the
    /// file, so a shared name races: a write fails while another test holds the map).
    fn unique_temp(name: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("openprocmon-{}-{n}-{name}.pml", std::process::id()))
    }

    /// The fixtures are zlib-compressed; decompress to a temp file (the reader
    /// mmaps a file) and open that.
    fn open_resource(name: &str) -> PmlReader {
        use std::io::Read;
        let raw = std::fs::read(resource(name)).expect("read fixture");
        let mut buf = Vec::new();
        flate2::read::ZlibDecoder::new(&raw[..])
            .read_to_end(&mut buf)
            .expect("zlib decompress");
        let tmp = unique_temp(name);
        std::fs::write(&tmp, &buf).expect("write temp pml");
        PmlReader::open(tmp).expect("open PML")
    }

    #[test]
    fn event_as_event_network_matches() {
        let mut checked = 0;
        for name in [
            "CompressedLogFileUTC64ProcessPML",
            "CompressedLogFileUTC64RegistryPML",
            "CompressedLogFileUTC64FilesystemPML",
            "CompressedLogfileTests64bitUTCPML",
        ] {
            let reader = std::sync::Arc::new(open_resource(name));
            for i in 0..reader.len() {
                let old = reader.event(i).expect("old");
                if old.class != crate::EventClass::Network {
                    continue;
                }
                let ev = reader.event_as_event(i).expect("new");
                assert_eq!(
                    ev.path().unwrap_or_default(),
                    old.path.to_string(),
                    "net path @ {name}:{i}"
                );
                checked += 1;
            }
        }
        eprintln!("verified {checked} PML network events");
    }

    #[test]
    fn events_resolve_varied_pids() {
        // A file-system capture (no network events) — exercises the non-network
        // pid path, which a network-bearing fixture would mask.
        let reader = std::sync::Arc::new(open_resource("CompressedLogFileUTC64FilesystemPML"));
        let mut pids = std::collections::HashSet::new();
        for ev in reader.events() {
            pids.insert(ev.pid());
        }
        assert!(
            pids.len() > 1,
            "expected multiple distinct pids, got {pids:?}"
        );
    }

    #[test]
    fn event_as_event_matches_pmlevent_columns() {
        use crate::filter::{Column, FilterFields};
        let reader = std::sync::Arc::new(open_resource("CompressedLogFileUTC64FilesystemPML"));
        assert!(!reader.is_empty());
        for i in 0..reader.len() {
            let old = reader.event(i).expect("old");
            let ev = reader.event_as_event(i).expect("new");
            assert_eq!(ev.operation_name(), old.operation_name(), "op @ {i}");
            assert_eq!(
                ev.path().unwrap_or_default(),
                old.path.to_string(),
                "path @ {i}"
            );
            assert_eq!(
                ev.result().into_owned(),
                old.result_name().into_owned(),
                "result @ {i}"
            );
            assert_eq!(ev.thread_id(), old.tid, "tid @ {i}");
            assert_eq!(
                ev.filter_field(Column::ProcessName).as_deref(),
                reader
                    .process(old.process_index)
                    .map(|p| p.process_name.as_ref()),
                "pname @ {i}"
            );
        }
    }

    fn check(name: &str, want_64bit: bool) {
        let r = open_resource(name);
        assert_eq!(r.header().is_64bit, want_64bit, "{name} bitness");
        assert_eq!(
            r.len(),
            r.header().number_of_events as usize,
            "{name} event count"
        );
        assert!(!r.is_empty(), "{name} has events");
        assert!(r.processes().count() > 0, "{name} has processes");
        // First and last events parse and reference a known process.
        let first = r.event(0).expect("event 0");
        assert!(
            r.process(first.process_index).is_some(),
            "{name} event0 process"
        );
        let last = r.event(r.len() - 1).expect("last event");
        assert!(
            r.process(last.process_index).is_some(),
            "{name} last process"
        );
        // Every event must parse without error (exercises all offsets/detail sizes),
        // and most filesystem events should yield a non-empty Path (detail parsing).
        let mut with_path = 0usize;
        for i in 0..r.len() {
            let ev = r
                .event(i)
                .unwrap_or_else(|e| panic!("{name} event {i}: {e:?}"));
            if !ev.path.is_empty() {
                with_path += 1;
            }
        }
        assert!(
            with_path * 2 > r.len(),
            "{name}: expected most events to have a path, got {with_path}/{}",
            r.len()
        );
    }

    #[test]
    fn reads_64bit_filesystem_pml() {
        check("CompressedLogFileUTC64FilesystemPML", true);
    }

    #[test]
    fn rejects_32bit_pml() {
        // 32-bit detail parsing is not implemented yet; opening must fail cleanly
        // rather than return half-parsed events.
        use std::io::Read;
        let raw = std::fs::read(resource("CompressedLogFileUTC32FilesystemPML")).expect("fixture");
        let mut buf = Vec::new();
        flate2::read::ZlibDecoder::new(&raw[..])
            .read_to_end(&mut buf)
            .expect("unzip");
        let tmp = unique_temp("reject32");
        std::fs::write(&tmp, &buf).expect("write");
        let result = PmlReader::open(&tmp);
        assert!(result.is_err(), "32-bit PML must be rejected");
        let err = result.err().unwrap();
        assert!(
            format!("{err:?}").contains("32-bit"),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn reads_64bit_registry_pml() {
        // Registry events carry the key path in their detail blob.
        check("CompressedLogFileUTC64RegistryPML", true);
    }

    #[test]
    fn lossless_round_trip_real_pml() {
        // Read a real PML, write it back through PmlWriter (raw detail preserved via
        // event_with_raw), read again, and assert every event is identical —
        // including the raw detail/extra bytes (byte-level detail fidelity).
        let r = open_resource("CompressedLogFileUTC64FilesystemPML");
        let mut w = crate::pml::PmlWriter::new(r.header().is_64bit);
        for p in r.processes() {
            w.add_process(p.clone());
        }
        let originals: Vec<_> = (0..r.len())
            .map(|i| r.event_with_raw(i).expect("event"))
            .collect();
        for e in &originals {
            w.add_event(e.clone());
        }
        let bytes = w.to_bytes().expect("serialize");
        let tmp = unique_temp("lossless");
        std::fs::write(&tmp, &bytes).expect("write");
        let r2 = PmlReader::open(&tmp).expect("reopen");

        assert_eq!(r2.len(), originals.len());
        for (i, a) in originals.iter().enumerate() {
            let b = r2.event_with_raw(i).expect("event2");
            assert_eq!(a.class, b.class, "event {i} class");
            assert_eq!(a.operation, b.operation, "event {i} op");
            assert_eq!(a.tid, b.tid, "event {i} tid");
            assert_eq!(a.date_filetime, b.date_filetime, "event {i} time");
            assert_eq!(a.result, b.result, "event {i} result");
            assert_eq!(a.duration, b.duration, "event {i} duration");
            assert_eq!(a.path, b.path, "event {i} path");
            assert_eq!(a.stack, b.stack, "event {i} stack");
            assert_eq!(a.process_index, b.process_index, "event {i} process");
            assert_eq!(a.raw_detail, b.raw_detail, "event {i} detail bytes");
            assert_eq!(a.raw_extra, b.raw_extra, "event {i} extra bytes");
        }
    }

    #[test]
    fn sdk_detail_columns_populated_64bit() {
        // 64-bit PML reuses the SDK's per-operation parsing (DetailMode::Pml), so
        // the rich Detail column is populated (e.g. registry "Length/Class").
        let detail = |e: &PmlEvent| {
            e.details
                .iter()
                .find(|(k, _)| k == "Detail")
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let count_detail = |r: &PmlReader| {
            (0..r.len())
                .filter(|&i| !detail(&r.event(i).unwrap()).is_empty())
                .count()
        };
        let f = open_resource("CompressedLogFileUTC64FilesystemPML");
        assert!(count_detail(&f) > 0, "no file events got a Detail column");
        let r = open_resource("CompressedLogFileUTC64RegistryPML");
        assert!(
            count_detail(&r) > 0,
            "no registry events got a Detail column"
        );
        // RegQueryKey's detail is the information class, proving SDK reuse works.
        let e0 = r.event(0).unwrap();
        assert_eq!(e0.operation_name(), "RegQueryKey");
        assert!(
            detail(&e0).contains("Class:"),
            "reg detail = {:?}",
            detail(&e0)
        );
    }

    #[test]
    fn parses_embedded_process_icons() {
        // PML embeds process icons (ICONIMAGE) so they render without the original
        // exe. Verify the icon table parses and processes resolve to valid icons.
        let r = open_resource("CompressedLogFileUTC64ProcessPML");
        // Non-placeholder icons look like an ICONIMAGE (start with a 40-byte
        // BITMAPINFOHEADER) and are 16 or 32 px. Index 0 is the empty placeholder.
        let real: Vec<_> = r.icons().iter().filter(|i| !i.data.is_empty()).collect();
        assert!(!real.is_empty(), "process PML should embed icons");
        for icon in &real {
            let bi_size = u32::from_le_bytes(icon.data[0..4].try_into().unwrap());
            assert_eq!(bi_size, 40, "ICONIMAGE should start with BITMAPINFOHEADER");
            assert!(
                icon.dimension == 16 || icon.dimension == 32,
                "dim {}",
                icon.dimension
            );
        }
        // At least one process resolves to a real icon via its index.
        let resolved = r
            .processes()
            .filter_map(|p| r.icon(p.icon_big).or_else(|| r.icon(p.icon_small)))
            .count();
        assert!(resolved > 0, "no process resolved an icon");
    }

    #[test]
    fn reads_64bit_process_pml() {
        // Process events: just verify they all parse and reference a process
        // (many process ops have no path, so don't assert on path coverage here).
        let r = open_resource("CompressedLogFileUTC64ProcessPML");
        assert!(!r.is_empty());
        for i in 0..r.len() {
            let ev = r
                .event(i)
                .unwrap_or_else(|e| panic!("process event {i}: {e:?}"));
            assert!(r.process(ev.process_index).is_some());
        }
    }
}
