//! PML writer — serializes processes + events back to a `.PML` file.
//!
//! The reference parser has no writer, so this is the inverse of [`super::reader`]
//! / [`super::detail`]. It builds each section into a buffer, then assembles
//! `header + events + strings + icon + hosts/ports + event-offsets + processes`
//! with computed absolute offsets (no seeking needed). Layout matches Procmon's so
//! our reader (and ideally Procmon) reads it back.
//!
//! Detail blobs are re-encoded for the path-bearing cases (File / Registry /
//! Process-create / Load-Image); other detail columns and Network detail are not
//! re-encoded yet (the round-trip target is the common fields + Path).

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::Result;
use crate::event::Event;
use crate::pml::model::{PmlEvent, PmlIcon, PmlModule, PmlProcess};
use crate::process::ProcessRecord;

const HEADER_SIZE: usize = 0x3a8;

/// Builds a PML byte image from added processes + events.
pub struct PmlWriter {
    is_64bit: bool,
    pub computer_name: String,
    pub system_root: String,
    pub windows_major: u32,
    pub windows_minor: u32,
    pub windows_build: u32,
    pub num_logical_processors: u32,
    pub ram_bytes: u64,
    processes: Vec<PmlProcess>,
    events: Vec<PmlEvent>,
    /// Icon array; index 0 is the empty "no icon" placeholder, matching the reader.
    icons: Vec<PmlIcon>,
    /// Process seq → index in `processes`, for de-duping `push_event` sources.
    proc_index: HashMap<u32, u32>,
}

impl PmlWriter {
    pub fn new(is_64bit: bool) -> Self {
        Self {
            is_64bit,
            computer_name: String::new(),
            system_root: "C:\\Windows".to_string(),
            windows_major: 10,
            windows_minor: 0,
            windows_build: 0,
            num_logical_processors: 1,
            ram_bytes: 0,
            processes: Vec::new(),
            events: Vec::new(),
            icons: vec![PmlIcon::default()], // index 0 = empty placeholder
            proc_index: HashMap::new(),
        }
    }

    pub fn add_process(&mut self, process: PmlProcess) {
        self.processes.push(process);
    }

    #[allow(dead_code)] // PML→PML round-trip test support
    pub(crate) fn add_event(&mut self, event: PmlEvent) {
        self.events.push(event);
    }

    /// Records a live captured [`Event`] into the log (Save-to-PML). The event's
    /// process is de-duped into the process table.
    ///
    /// The full detail blob is transcoded from the driver's `EventData` to PML
    /// form (paths resolved to DOS/hive, all other fields byte-exact — see
    /// [`crate::parse::transcode`]), so every Detail column survives the round
    /// trip, not just Path. The completion (POST) data is carried verbatim as the
    /// PML "extra" blob (e.g. `CreateFile`'s OpenResult). Network events (no driver
    /// `EventData`) are encoded from the decoded `NetworkEvent` into the PML network
    /// blob, with numeric endpoints (a live capture has no resolved name tables).
    pub fn push_event(&mut self, ev: &Event) {
        let process_index = self.intern_process(ev.process());
        let class = ev.class(); // unified EventClass — no mapping needed
        let stack = ev.call_stack().iter().map(|f| f.address()).collect();
        // Per-category serializer produces the PML operation code + detail blob
        // (file/reg/proc transcode the driver detail with paths→DOS; network
        // encodes its endpoints). The completion (POST) data, if any, is the extra
        // blob; network has none.
        let (operation, detail) = crate::parse::pml_serialize(ev);
        let raw_detail = detail.map(Arc::from);
        let raw_extra = ev.post_data().map(Arc::from);
        self.events.push(PmlEvent {
            process_index,
            tid: ev.thread_id(),
            class,
            operation,
            duration: ev.duration_ticks().unwrap_or(0).max(0) as u64,
            date_filetime: ev.time_raw() as u64,
            result: ev.status_raw() as u32,
            stack,
            category: std::borrow::Cow::Borrowed(""),
            path: Arc::from(ev.path().unwrap_or_default().as_str()),
            details: Vec::new(),
            op_name: None,
            raw_detail,
            raw_extra,
        });
    }

    /// Interns an event's process into the table, returning its index.
    fn intern_process(&mut self, proc: Option<&Arc<ProcessRecord>>) -> u32 {
        let Some(rec) = proc else { return 0 };
        let seq = rec.info.seq;
        if let Some(&i) = self.proc_index.get(&seq) {
            return i;
        }
        let index = self.processes.len() as u32;
        self.proc_index.insert(seq, index);
        let mut p = pml_process_from(rec, index);
        // The PE icon bytes (RT_ICON) are already in ICONIMAGE form — register them
        // into the icon table so the PML carries them for offline rendering.
        if let Some(meta) = rec.meta() {
            p.icon_small = self.intern_icon(meta.icon_small.as_deref(), 16);
            p.icon_big = self.intern_icon(meta.icon_large.as_deref(), 32);
        }
        self.processes.push(p);
        index
    }

    /// Adds an icon's `ICONIMAGE` bytes to the icon table, returning its index
    /// (0 = the empty placeholder when there is no icon).
    fn intern_icon(&mut self, bytes: Option<&[u8]>, dimension: u32) -> u32 {
        match bytes {
            Some(b) if !b.is_empty() => {
                let idx = self.icons.len() as u32;
                self.icons.push(PmlIcon { dimension, data: Arc::from(b) });
                idx
            }
            _ => 0,
        }
    }

    fn sizeof_pvoid(&self) -> usize {
        if self.is_64bit { 8 } else { 4 }
    }

    /// Serializes everything to an in-memory PML image.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let pv = self.sizeof_pvoid();

        // Intern every process/module string into the dedup table (index 0 = "").
        let mut strings = Interner::new();
        for p in &self.processes {
            intern_process(&mut strings, p);
        }

        // --- events section (records back-to-back, absolute offsets collected) ---
        let mut events_buf = WBuf::new();
        let mut event_offsets: Vec<u32> = Vec::with_capacity(self.events.len());
        let off_events = HEADER_SIZE;
        for e in &self.events {
            event_offsets.push((off_events + events_buf.len()) as u32);
            encode_event(&mut events_buf, e, pv);
        }

        let strings_buf = encode_strings(&strings);
        let icon_buf = encode_icons(&self.icons);
        let hosts_buf = {
            let mut b = WBuf::new();
            b.u32(0); // 0 hosts
            b.u32(0); // 0 ports
            b
        };

        // Absolute offsets of the trailing sections.
        let off_strings = off_events + events_buf.len();
        let off_icon = off_strings + strings_buf.len();
        let off_hosts = off_icon + icon_buf.len();
        let off_eoff = off_hosts + hosts_buf.len();
        let off_proc = off_eoff + event_offsets.len() * 5;

        let mut eoff_buf = WBuf::new();
        for &o in &event_offsets {
            eoff_buf.u32(o);
            eoff_buf.u8(0); // flags
        }
        let proc_buf = encode_processes(&self.processes, &strings, pv);

        // --- header ---
        let header = self.encode_header(HeaderOffsets {
            number_of_events: self.events.len() as u32,
            events_offset: off_events as u64,
            events_offsets_array_offset: off_eoff as u64,
            process_table_offset: off_proc as u64,
            strings_table_offset: off_strings as u64,
            icon_table_offset: off_icon as u64,
            hosts_and_ports_offset: off_hosts as u64,
        });

        // Assemble: header, events, strings, icon, hosts, event-offsets, processes.
        let mut out = Vec::with_capacity(off_proc + proc_buf.len());
        out.extend_from_slice(&header);
        out.extend_from_slice(&events_buf.d);
        out.extend_from_slice(&strings_buf.d);
        out.extend_from_slice(&icon_buf.d);
        out.extend_from_slice(&hosts_buf.d);
        out.extend_from_slice(&eoff_buf.d);
        out.extend_from_slice(&proc_buf.d);
        Ok(out)
    }

    /// Writes the PML image to `path`.
    pub fn write_to_path<P: AsRef<std::path::Path>>(&self, path: P) -> Result<()> {
        let bytes = self.to_bytes()?;
        std::fs::write(path, bytes).map_err(|e| crate::error::Error::Parse(format!("PML write: {e}")))
    }

    fn encode_header(&self, o: HeaderOffsets) -> Vec<u8> {
        let mut b = WBuf::new();
        b.bytes(b"PML_");
        b.u32(9); // version
        b.u32(self.is_64bit as u32);
        b.fixed_utf16(&self.computer_name, 0x20);
        b.fixed_utf16(&self.system_root, 0x208);
        b.u32(o.number_of_events);
        b.zeros(8); // unknown
        b.u64(o.events_offset);
        b.u64(o.events_offsets_array_offset);
        b.u64(o.process_table_offset);
        b.u64(o.strings_table_offset);
        b.u64(o.icon_table_offset);
        b.zeros(12); // unknown
        b.u32(self.windows_major);
        b.u32(self.windows_minor);
        b.u32(self.windows_build);
        b.u32(0); // build after decimal
        b.fixed_utf16("", 0x32); // service pack
        b.zeros(0xd6); // unknown
        b.u32(self.num_logical_processors);
        b.u64(self.ram_bytes);
        b.u64(HEADER_SIZE as u64); // header_size
        b.u64(o.hosts_and_ports_offset);
        debug_assert!(b.len() <= HEADER_SIZE, "PML header overflow: {}", b.len());
        b.zeros(HEADER_SIZE - b.len());
        b.d
    }
}

/// Builds a PML process-table entry from a live process record, mapping every
/// field the SDK record carries: SID/integrity RID are resolved to the names PML
/// stores (`DOMAIN\User`, `High`, …), the image path is DOS form, the version
/// strings come from the PE metadata, and loaded modules are copied across.
/// (`icon_small`/`icon_big` are set by [`PmlWriter::intern_process`].)
fn pml_process_from(rec: &ProcessRecord, index: u32) -> PmlProcess {
    let info = &rec.info;
    let image_dos = crate::path::nt_to_dos(&info.image_path);
    let name = image_dos.rsplit(['\\', '/']).next().unwrap_or("").to_string();
    let (hi, lo) = info.authentication_id;
    let user = info
        .user_sid
        .as_deref()
        .and_then(crate::sid::account_name)
        .unwrap_or_default();
    let integrity = info.integrity_rid.map(crate::sid::integrity_level).unwrap_or("");
    // Version metadata from the PE resources (absent fields → empty strings).
    let meta = rec.meta();
    let meta_str = |f: Option<&String>| f.map(|v| Arc::from(v.as_str())).unwrap_or_else(|| Arc::from(""));
    let company = meta_str(meta.and_then(|m| m.company.as_ref()));
    let version = meta_str(meta.and_then(|m| m.version.as_ref()));
    let description = meta_str(meta.and_then(|m| m.description.as_ref()));
    // Loaded modules (the SDK module record has only base/size/path; PML's
    // per-module version strings are left empty).
    let modules = rec
        .modules()
        .iter()
        .map(|m| PmlModule {
            base_address: m.base,
            size: m.size,
            image_path: Arc::from(crate::path::nt_to_dos(&m.path).as_str()),
            version: Arc::from(""),
            company: Arc::from(""),
            description: Arc::from(""),
            timestamp: 0,
        })
        .collect();
    PmlProcess {
        process_index: index,
        pid: info.pid,
        parent_pid: info.parent_pid,
        authentication_id: ((hi as u32 as u64) << 32) | lo as u64,
        session: info.session_id,
        start_time: info.create_time as u64,
        end_time: rec.exit_time().unwrap_or(0) as u64,
        virtualized: info.is_virtualized,
        is_64bit: !info.is_wow64,
        integrity: Arc::from(integrity),
        user: Arc::from(user.as_str()),
        process_name: Arc::from(name.as_str()),
        image_path: Arc::from(image_dos.as_str()),
        command_line: Arc::from(info.command_line.as_str()),
        company,
        version,
        description,
        icon_small: 0,
        icon_big: 0,
        modules,
    }
}

struct HeaderOffsets {
    number_of_events: u32,
    events_offset: u64,
    events_offsets_array_offset: u64,
    process_table_offset: u64,
    strings_table_offset: u64,
    icon_table_offset: u64,
    hosts_and_ports_offset: u64,
}

// ---------------------------------------------------------------------------
// String interning + tables.
// ---------------------------------------------------------------------------

struct Interner {
    map: HashMap<String, u32>,
    list: Vec<String>,
}

impl Interner {
    fn new() -> Self {
        let mut s = Self { map: HashMap::new(), list: Vec::new() };
        s.intern(""); // index 0 = empty string
        s
    }

    fn intern(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.map.get(s) {
            return i;
        }
        let i = self.list.len() as u32;
        self.list.push(s.to_string());
        self.map.insert(s.to_string(), i);
        i
    }

    fn index_of(&self, s: &str) -> u32 {
        self.map.get(s).copied().unwrap_or(0)
    }
}

fn intern_process(strings: &mut Interner, p: &PmlProcess) {
    for s in [
        &p.integrity,
        &p.user,
        &p.process_name,
        &p.image_path,
        &p.command_line,
        &p.company,
        &p.version,
        &p.description,
    ] {
        strings.intern(s);
    }
    for m in &p.modules {
        for s in [&m.image_path, &m.version, &m.company, &m.description] {
            strings.intern(s);
        }
    }
}

/// Strings array: count + relative-offset table + each `{u32 size, utf16+NUL}`.
fn encode_strings(strings: &Interner) -> WBuf {
    let n = strings.list.len();
    // Header part: count(4) + n*4 offset slots. Data follows.
    let data_start = 4 + n * 4;
    let mut data = WBuf::new();
    let mut offsets = Vec::with_capacity(n);
    for s in &strings.list {
        offsets.push((data_start + data.len()) as u32);
        let units: Vec<u16> = s.encode_utf16().collect();
        let size = (units.len() + 1) * 2; // include NUL terminator
        data.u32(size as u32);
        for u in units {
            data.u16(u);
        }
        data.u16(0); // NUL
    }
    let mut b = WBuf::new();
    b.u32(n as u32);
    for o in offsets {
        b.u32(o);
    }
    b.bytes(&data.d);
    b
}

/// Icon array: count + relative-offset table + each `Icon { u32 dimension, u32
/// size, ICONIMAGE[size] }` (inverse of the reader's `parse_icons`). Index 0 is
/// the empty placeholder.
fn encode_icons(icons: &[PmlIcon]) -> WBuf {
    let n = icons.len();
    let data_start = 4 + n * 4;
    let mut data = WBuf::new();
    let mut offsets = Vec::with_capacity(n);
    for ic in icons {
        offsets.push((data_start + data.len()) as u32);
        data.u32(ic.dimension);
        data.u32(ic.data.len() as u32);
        data.bytes(&ic.data);
    }
    let mut b = WBuf::new();
    b.u32(n as u32);
    for o in offsets {
        b.u32(o);
    }
    b.bytes(&data.d);
    b
}

/// Process array: count + index array + relative-offset array + Process structs.
fn encode_processes(processes: &[PmlProcess], strings: &Interner, pv: usize) -> WBuf {
    let n = processes.len();
    // Encode each process struct first to know its size, then lay out offsets.
    let structs: Vec<WBuf> = processes.iter().map(|p| encode_process(p, strings, pv)).collect();
    let data_start = 4 + n * 4 + n * 4; // count + index array + offset array
    let mut offsets = Vec::with_capacity(n);
    let mut running = data_start;
    for s in &structs {
        offsets.push(running as u32);
        running += s.len();
    }
    let mut b = WBuf::new();
    b.u32(n as u32);
    for p in processes {
        b.u32(p.process_index); // index array
    }
    for o in offsets {
        b.u32(o); // offset array
    }
    for s in &structs {
        b.bytes(&s.d);
    }
    b
}

fn encode_process(p: &PmlProcess, strings: &Interner, pv: usize) -> WBuf {
    let mut b = WBuf::new();
    b.u32(p.process_index);
    b.u32(p.pid);
    b.u32(p.parent_pid);
    b.zeros(4);
    b.u64(p.authentication_id);
    b.u32(p.session);
    b.zeros(4);
    b.u64(p.start_time);
    b.u64(p.end_time);
    b.u32(p.virtualized as u32);
    b.u32(p.is_64bit as u32);
    b.u32(strings.index_of(&p.integrity));
    b.u32(strings.index_of(&p.user));
    b.u32(strings.index_of(&p.process_name));
    b.u32(strings.index_of(&p.image_path));
    b.u32(strings.index_of(&p.command_line));
    b.u32(strings.index_of(&p.company));
    b.u32(strings.index_of(&p.version));
    b.u32(strings.index_of(&p.description));
    b.u32(p.icon_small); // icon index (small)
    b.u32(p.icon_big); // icon index (big)
    b.pvoid(0, pv); // unknown
    b.u32(p.modules.len() as u32);
    for m in &p.modules {
        b.pvoid(0, pv); // unknown
        b.pvoid(m.base_address, pv);
        b.u32(m.size);
        b.u32(strings.index_of(&m.image_path));
        b.u32(strings.index_of(&m.version));
        b.u32(strings.index_of(&m.company));
        b.u32(strings.index_of(&m.description));
        b.u32(m.timestamp);
        b.zeros(0x18);
    }
    b
}

// ---------------------------------------------------------------------------
// Event + detail encoding.
// ---------------------------------------------------------------------------

fn encode_event(b: &mut WBuf, e: &PmlEvent, pv: usize) {
    // The detail blob is the event's `raw_detail`: from `push_event` (live →
    // PML, per-category serialized) or from the reader (PML → PML, verbatim).
    // Events with no raw detail (bare from-scratch `PmlEvent`s) get an empty blob.
    let detail: Vec<u8> = e.raw_detail.as_deref().map(<[u8]>::to_vec).unwrap_or_default();
    let stack_bytes = e.stack.len() * pv;
    // Extra-detail is written right after the record; its field is the offset from
    // the event start (= common 52 + stack + details), which the reader resolves.
    let extra_offset = match &e.raw_extra {
        Some(_) => (52 + stack_bytes + detail.len()) as u32,
        None => 0,
    };

    // Common struct "<IIIHHIQQIHHII" (52 bytes).
    b.u32(e.process_index);
    b.u32(e.tid);
    b.u32(e.class.to_u32());
    b.u16(e.operation);
    b.u16(0);
    b.u32(0);
    b.u64(e.duration);
    b.u64(e.date_filetime);
    b.u32(e.result);
    b.u16(e.stack.len() as u16);
    b.u16(0);
    b.u32(detail.len() as u32);
    b.u32(extra_offset);
    for &frame in &e.stack {
        b.pvoid(frame, pv);
    }
    b.bytes(&detail);
    if let Some(raw) = &e.raw_extra {
        b.u16(raw.len() as u16);
        b.bytes(raw);
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pml::model::PmlProcess;
    use crate::pml::reader::PmlReader;
    use crate::EventClass;
    use std::sync::Arc;

    /// Common event fields + the call stack + the process table round-trip through
    /// the writer's structural encoding (detail blobs are covered by the
    /// `push_event_*` tests; from-scratch `PmlEvent`s carry no detail).
    #[test]
    fn round_trip_common_fields_and_process() {
        let mut w = PmlWriter::new(true);
        w.computer_name = "TESTPC".to_string();
        w.add_process(PmlProcess {
            process_index: 0,
            pid: 1234,
            parent_pid: 4,
            process_name: Arc::from("chrome.exe"),
            image_path: Arc::from("C:\\Program Files\\chrome.exe"),
            is_64bit: true,
            ..Default::default()
        });

        for i in 0..3u32 {
            w.add_event(PmlEvent {
                process_index: 0,
                tid: 10 + i,
                class: EventClass::File,
                operation: 23, // ReadFile
                duration: 100 + i as u64,
                date_filetime: 0x01D0_0000_0000_0000 + i as u64,
                result: 0,
                stack: vec![0xdead_beef, 0x1234_5678],
                ..Default::default()
            });
        }

        let bytes = w.to_bytes().expect("serialize");
        let tmp = std::env::temp_dir().join("openprocmon-roundtrip.pml");
        std::fs::write(&tmp, &bytes).expect("write temp");
        let r = PmlReader::open(&tmp).expect("read back");

        assert!(r.header().is_64bit);
        assert_eq!(r.len(), 3);
        for i in 0..3u32 {
            let e = r.event(i as usize).expect("event");
            assert_eq!(e.class, EventClass::File);
            assert_eq!(e.operation, 23);
            assert_eq!(e.tid, 10 + i);
            assert_eq!(e.duration, 100 + i as u64);
            assert_eq!(e.date_filetime, 0x01D0_0000_0000_0000 + i as u64);
            assert_eq!(e.stack, vec![0xdead_beef, 0x1234_5678]);
            let proc = r.process(e.process_index).expect("process");
            assert_eq!(proc.pid, 1234);
            assert_eq!(&*proc.process_name, "chrome.exe");
        }
    }

    /// A live (driver-form) registry event saved to PML keeps its resolved hive
    /// path: `Event::path` normalizes `\REGISTRY\MACHINE\...` to `HKLM\...`, the
    /// writer stores that, and the reader (PML mode) reads it back unchanged.
    #[test]
    fn push_event_registry_path_round_trip() {
        use crate::event::Event;
        use crate::kernel_types::synth_record;

        // LOG_REG_QUERYKEY: key_name_length(u16) + fill(u16) + length(u32) +
        // key_information_class(u32), then the key name (UTF-16).
        let key = "\\REGISTRY\\MACHINE\\SOFTWARE\\OpenProcMon\\Test";
        let units: Vec<u16> = key.encode_utf16().collect();
        let mut data = Vec::new();
        data.extend_from_slice(&(units.len() as u16).to_le_bytes()); // key_name_length
        data.extend_from_slice(&0u16.to_le_bytes()); // fill
        data.extend_from_slice(&0u32.to_le_bytes()); // length
        data.extend_from_slice(&0u32.to_le_bytes()); // key_information_class
        for u in &units {
            data.extend_from_slice(&u.to_le_bytes());
        }
        // monitor_type=2 (Reg), notify_type=3 (QueryKey).
        let pre = synth_record(2, 3, 0, &data).into_boxed_slice();
        let ev = Event::from_filter(pre, None, None).expect("event");
        let dos = ev.path().expect("hive path"); // e.g. HKLM\SOFTWARE\OpenProcMon\Test
        assert!(dos.starts_with("HKLM\\"), "expected hive form, got {dos}");

        let mut w = PmlWriter::new(true);
        w.push_event(&ev);
        let bytes = w.to_bytes().expect("serialize");
        let tmp = std::env::temp_dir().join("openprocmon-pushevent.pml");
        std::fs::write(&tmp, &bytes).expect("write temp");
        let r = PmlReader::open(&tmp).expect("read back");

        assert_eq!(r.len(), 1);
        let read = r.event(0).expect("event");
        assert_eq!(read.class, EventClass::Registry);
        assert_eq!(read.operation, 3);
        assert_eq!(&*read.path, dos.as_str(), "hive path preserved through PML");
    }

    /// The Detail column survives live→PML, not just Path: a RegSetValue's
    /// Type/Length/Data is transcoded byte-exact (value bytes trail the key name,
    /// which shrinks `\REGISTRY\...`→`HKLM\...`, so the splice must keep them).
    #[test]
    fn push_event_registry_detail_round_trip() {
        use crate::event::Event;
        use crate::kernel_types::{reg_notify, synth_record};
        use windows::Win32::System::Registry::REG_DWORD;

        // LOG_REG_SETVALUEKEY (16 bytes) + NT key name + 4-byte DWORD value (= 1).
        let key = "\\REGISTRY\\MACHINE\\SOFTWARE\\X";
        let name: Vec<u8> = key.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let units = (name.len() / 2) as u16;
        let mut d = Vec::new();
        d.extend_from_slice(&units.to_le_bytes()); // key_name_length
        d.extend_from_slice(&0u16.to_le_bytes()); // fill02
        d.extend_from_slice(&REG_DWORD.0.to_le_bytes()); // value_type
        d.extend_from_slice(&4u32.to_le_bytes()); // data_size
        d.extend_from_slice(&4u16.to_le_bytes()); // copy_size
        d.extend_from_slice(&0u16.to_le_bytes()); // fill0e
        d.extend_from_slice(&name);
        d.extend_from_slice(&1u32.to_le_bytes()); // the DWORD value
        let pre = synth_record(2, reg_notify::SETVALUEKEY, 0, &d).into_boxed_slice();
        let ev = Event::from_filter(pre, None, None).expect("event");
        assert_eq!(ev.detail(), "Type: REG_DWORD, Length: 4, Data: 1");

        let mut w = PmlWriter::new(true);
        w.push_event(&ev);
        let bytes = w.to_bytes().expect("serialize");
        let tmp = std::env::temp_dir().join("openprocmon-regdetail.pml");
        std::fs::write(&tmp, &bytes).expect("write temp");
        let r = PmlReader::open(&tmp).expect("read back");

        let read = r.event(0).expect("event");
        assert_eq!(&*read.path, "HKLM\\SOFTWARE\\X");
        let detail = read.details.iter().find(|(k, _)| k == "Detail").map(|(_, v)| v.as_str());
        assert_eq!(detail, Some("Type: REG_DWORD, Length: 4, Data: 1"));
    }

    /// File CreateFile: the trailing LOG_FILE_CREATE detail and the POST-derived
    /// OpenResult both survive live→PML (POST data carried as the PML extra blob).
    #[test]
    fn push_event_file_create_detail_round_trip() {
        use crate::event::Event;
        use crate::kernel_types::{file_opt, irp_mj, synth_record, FILE_NOTIFY_BASE, LogFileOptHead};
        use core::mem::size_of;
        use windows::Wdk::Storage::FileSystem::Minifilters::FLT_PARAMETERS;
        use windows::Win32::Storage::FileSystem::FILE_GENERIC_READ;

        let nt = "\\Device\\HarddiskVolume1\\Windows\\test.txt";
        let name: Vec<u8> = nt.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut d = vec![0u8; size_of::<LogFileOptHead>()];
        // SAFETY: FLT_PARAMETERS is POD for our purposes; zeroed is valid.
        let mut params: FLT_PARAMETERS = unsafe { core::mem::zeroed() };
        params.Create.Options = 1u32 << 24; // disposition byte = 1 => "Open"
        // SAFETY: read the union's bytes for serialization.
        let pb = unsafe {
            core::slice::from_raw_parts(&params as *const _ as *const u8, size_of::<FLT_PARAMETERS>())
        };
        d.extend_from_slice(pb);
        d.extend_from_slice(&((name.len() / 2) as u16).to_le_bytes()); // NameLength
        d.extend_from_slice(&0u16.to_le_bytes()); // Fill42
        d.extend_from_slice(&name);
        d.extend_from_slice(&FILE_GENERIC_READ.0.to_le_bytes()); // LOG_FILE_CREATE.DesiredAccess
        d.extend_from_slice(&0u32.to_le_bytes()); // UserTokenLength
        let _ = file_opt::name_offset(); // (offsets exercised by the transcoder)

        let op = FILE_NOTIFY_BASE + irp_mj::CREATE as u16;
        let pre = synth_record(3, op, 0, &d).into_boxed_slice();
        // POST carries IO_STATUS.Information = 1 => OpenResult "Opened".
        let post = synth_record(0, op, 0, &1u64.to_le_bytes()).into_boxed_slice();
        let ev = Event::from_filter(pre, Some(post), None).expect("event");
        // Offline there is no volume map, so the NT path passes through unchanged;
        // a live capture would resolve it to a drive letter. The round trip is what
        // matters here, plus the detail + POST-derived OpenResult below.
        let path = ev.path().expect("path");
        assert!(ev.detail().contains("Disposition: Open"));
        assert!(ev.detail().contains("OpenResult: Opened"));

        let mut w = PmlWriter::new(true);
        w.push_event(&ev);
        let bytes = w.to_bytes().expect("serialize");
        let tmp = std::env::temp_dir().join("openprocmon-filedetail.pml");
        std::fs::write(&tmp, &bytes).expect("write temp");
        let r = PmlReader::open(&tmp).expect("read back");

        let read = r.event(0).expect("event");
        assert_eq!(&*read.path, path.as_str(), "path preserved through PML");
        let detail = read.details.iter().find(|(k, _)| k == "Detail").map(|(_, v)| v.clone());
        let detail = detail.expect("detail column");
        assert!(detail.contains("Disposition: Open"), "detail: {detail}");
        assert!(detail.contains("OpenResult: Opened"), "OpenResult from extra: {detail}");
    }

    /// A live (ETW) network event saves to PML and reads back with its operation,
    /// endpoints and length intact (encoded via the shared network blob, no driver
    /// EventData; numeric endpoints since a live capture has no name tables).
    #[test]
    fn push_event_network_round_trip() {
        use crate::event::Event;
        use crate::network::{NetOp, NetworkEvent};

        let net = NetworkEvent {
            pid: 4321,
            is_tcp: true,
            op: NetOp::Send,
            local: "10.0.0.1:5000".parse().unwrap(),
            remote: "1.2.3.4:443".parse().unwrap(),
            local_name: None,
            remote_name: None,
            length: 1460,
            time: 0,
        };
        let ev = Event::from_network(Arc::new(net), crate::event::ProcessSource::Live(None));

        let mut w = PmlWriter::new(true);
        w.push_event(&ev);
        let bytes = w.to_bytes().expect("serialize");
        let tmp = std::env::temp_dir().join("openprocmon-net.pml");
        std::fs::write(&tmp, &bytes).expect("write temp");
        let r = PmlReader::open(&tmp).expect("read back");

        let read = r.event(0).expect("event");
        assert_eq!(read.class, EventClass::Network);
        assert_eq!(read.operation, NetOp::Send.to_pml());
        assert_eq!(&*read.path, "10.0.0.1:5000 -> 1.2.3.4:443");
        assert_eq!(read.operation_name(), "TCP Send");
        let len = read.details.iter().find(|(k, _)| k == "Length").map(|(_, v)| v.as_str());
        assert_eq!(len, Some("1460"));
    }

    /// Saving a live event writes every process field the SDK record carries —
    /// PE metadata (company/version/description), loaded modules, and icon bytes —
    /// not just identity. Guards against silently dropping SDK data (cf. icons).
    #[test]
    fn push_event_writes_full_process_metadata() {
        use crate::event::Event;
        use crate::kernel_types::synth_record;
        use crate::process::{Module, ProcessInfo, ProcessMeta, ProcessRecord};

        let rec = ProcessRecord::new(ProcessInfo {
            seq: 7,
            pid: 4242,
            image_path: "\\Device\\HarddiskVolume1\\Windows\\notepad.exe".into(),
            command_line: "notepad.exe".into(),
            ..Default::default()
        });
        rec.set_meta(std::sync::Arc::new(ProcessMeta {
            description: Some("Notepad".into()),
            company: Some("Contoso".into()),
            version: Some("10.0".into()),
            icon_small: Some(vec![0xAA; 16]),
            icon_large: Some(vec![0xBB; 32]),
        }));
        rec.add_module(Module {
            base: 0x1000,
            size: 0x2000,
            path: "\\Device\\HarddiskVolume1\\Windows\\System32\\ntdll.dll".into(),
        });

        // A registry event carrying this process.
        let key = "\\REGISTRY\\MACHINE\\X";
        let units: Vec<u16> = key.encode_utf16().collect();
        let mut d = Vec::new();
        d.extend_from_slice(&(units.len() as u16).to_le_bytes());
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        for u in &units {
            d.extend_from_slice(&u.to_le_bytes());
        }
        let pre = synth_record(2, 3, 0, &d).into_boxed_slice();
        let ev = Event::from_filter(pre, None, Some(rec)).expect("event");

        let mut w = PmlWriter::new(true);
        w.push_event(&ev);
        let bytes = w.to_bytes().expect("serialize");
        let tmp = std::env::temp_dir().join("openprocmon-procmeta.pml");
        std::fs::write(&tmp, &bytes).expect("write");
        let r = PmlReader::open(&tmp).expect("read");

        let e = r.event(0).expect("event");
        let p = r.process(e.process_index).expect("process");
        assert_eq!(p.pid, 4242);
        assert_eq!(&*p.company, "Contoso");
        assert_eq!(&*p.version, "10.0");
        assert_eq!(&*p.description, "Notepad");
        assert_eq!(p.modules.len(), 1, "module list should round-trip");
        assert!(p.modules[0].image_path.ends_with("ntdll.dll"));
        // Icons (ICONIMAGE bytes) round-trip via the icon table.
        let small = r.icon(p.icon_small).expect("small icon");
        assert_eq!(&*small.data, [0xAA; 16].as_slice());
        assert_eq!(small.dimension, 16);
        let big = r.icon(p.icon_big).expect("big icon");
        assert_eq!(&*big.data, [0xBB; 32].as_slice());
        assert_eq!(big.dimension, 32);
    }
}

#[derive(Default)]
struct WBuf {
    d: Vec<u8>,
}

impl WBuf {
    fn new() -> Self {
        Self { d: Vec::new() }
    }

    fn len(&self) -> usize {
        self.d.len()
    }

    fn u8(&mut self, v: u8) {
        self.d.push(v);
    }

    fn u16(&mut self, v: u16) {
        self.d.extend_from_slice(&v.to_le_bytes());
    }

    fn u32(&mut self, v: u32) {
        self.d.extend_from_slice(&v.to_le_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.d.extend_from_slice(&v.to_le_bytes());
    }

    fn pvoid(&mut self, v: u64, size: usize) {
        if size == 8 {
            self.u64(v);
        } else {
            self.u32(v as u32);
        }
    }

    fn zeros(&mut self, n: usize) {
        self.d.resize(self.d.len() + n, 0);
    }

    fn bytes(&mut self, b: &[u8]) {
        self.d.extend_from_slice(b);
    }

    /// A fixed-size UTF-16 field, NUL-terminated and zero-padded to `size` bytes.
    fn fixed_utf16(&mut self, s: &str, size: usize) {
        let max_units = size / 2;
        let mut written = 0;
        for u in s.encode_utf16().take(max_units.saturating_sub(1)) {
            self.u16(u);
            written += 1;
        }
        // NUL + pad to size.
        let remaining = size - written * 2;
        self.zeros(remaining);
    }
}
