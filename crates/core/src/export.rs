//! Export a (filtered) capture to PML / CSV / XML — the three formats the GUI's
//! Save dialog writes, so files are interchangeable. The CSV/XML encoders here
//! are the single implementation (the GUI calls them on its selected rows);
//! PML uses [`procmon_sdk::PmlReader::write_subset`] (raw, byte-faithful).

use std::sync::Arc;

use procmon_sdk::{Event, PmlReader, Result, SymModule};

use crate::query::Expr;

/// Output format for [`export`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Pml,
    Csv,
    Xml,
}

impl Format {
    /// Parses a format name (`pml`/`csv`/`xml`, case-insensitive).
    pub fn parse(s: &str) -> Option<Format> {
        match s.to_ascii_lowercase().as_str() {
            "pml" => Some(Format::Pml),
            "csv" => Some(Format::Csv),
            "xml" => Some(Format::Xml),
            _ => None,
        }
    }
}

/// Stack-frame resolution context for [`export_xml`]: prebuilt [`SymModule`]
/// views of the System (PID 4) kernel-driver modules (so kernel frames resolve
/// without rebuilding the list per event) and an optional dbghelp symbol
/// resolver (frames fall back to `module + offset` without it).
#[derive(Default)]
pub struct StackSymbolizer<'a> {
    kernel: Vec<SymModule<'a>>,
    symbols: Option<&'a procmon_sdk::SymbolResolver>,
}

impl<'a> StackSymbolizer<'a> {
    /// Builds the symbolizer from borrowed [`SymModule`] views — the GUI hands
    /// in views over its display rows, the CLI over core `ModuleRow`s; neither
    /// copies its module strings.
    pub fn new(
        kernel_mods: impl IntoIterator<Item = SymModule<'a>>,
        symbols: Option<&'a procmon_sdk::SymbolResolver>,
    ) -> Self {
        Self {
            kernel: kernel_mods.into_iter().collect(),
            symbols,
        }
    }

    /// One frame's `(location, module path)`: a dbghelp symbol when available,
    /// the `module + 0xoffset` fallback otherwise. Kernel frames resolve
    /// against the prebuilt kernel list, user frames against `proc_mods`.
    fn frame_location<'p>(
        &'p self,
        addr: u64,
        proc_mods: &'p [SymModule<'p>],
    ) -> (String, &'p str) {
        let f = procmon_sdk::resolve_frame_full(addr, proc_mods, &self.kernel);
        let location = self
            .symbols
            .and_then(|r| r.resolve(addr, if f.kernel { &self.kernel } else { proc_mods }))
            .map(|s| s.to_string())
            .unwrap_or(f.location);
        (location, f.path)
    }
}

/// Exports the events matching `filter` (and not `noise`) to `path` in
/// `format`. `include_stacks` adds call-stack frames to XML. Returns the number
/// of events written.
pub fn export(
    reader: &Arc<PmlReader>,
    format: Format,
    filter: Option<&Expr>,
    noise: &[crate::query::Clause],
    include_stacks: bool,
    path: &str,
) -> std::result::Result<usize, String> {
    let passes = |ev: &Event| crate::analyze::event_passes(ev, filter, noise);
    match format {
        Format::Pml => export_pml(reader, &passes, path).map_err(|e| e.to_string()),
        Format::Csv => {
            let matched: Vec<Event> = reader.events().filter(&passes).collect();
            export_csv(&matched, path)
        }
        Format::Xml => {
            let matched: Vec<Event> = reader.events().filter(&passes).collect();
            // Kernel frames resolve against the capture's System (PID 4) modules.
            let kernel_mods = if include_stacks {
                crate::analyze::get_process(reader, 4)
                    .map(|p| p.modules)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let sym = StackSymbolizer::new(crate::record::sym_views(&kernel_mods), None);
            export_xml(&matched, include_stacks, &sym, path)
        }
    }
}

/// Writes the matching events as a Procmon-compatible PML subset (raw bytes
/// preserved). `passes` is re-evaluated per index via `event_as_event`.
fn export_pml(
    reader: &Arc<PmlReader>,
    passes: &impl Fn(&Event) -> bool,
    path: &str,
) -> Result<usize> {
    reader.write_subset(path, |i| {
        reader
            .event_as_event(i)
            .map(|ev| passes(&ev))
            .unwrap_or(false)
    })
}

/// Procmon-style CSV: UTF-8 BOM, fully-quoted, CRLF rows. Streams through a
/// `BufWriter` — rows are encoded straight to the file, never buffered whole.
/// `events` is any slice of owned or borrowed events (`Vec<Event>` from the
/// CLI's reader scan, `Vec<&Event>` from the GUI's selected rows).
pub fn export_csv<E: std::borrow::Borrow<Event>>(
    events: &[E],
    path: &str,
) -> std::result::Result<usize, String> {
    use std::io::Write;
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut out = std::io::BufWriter::new(file);
    out.write_all(&[0xEF, 0xBB, 0xBF]) // UTF-8 BOM
        .map_err(|e| e.to_string())?;
    let mut w = csv::WriterBuilder::new()
        .quote_style(csv::QuoteStyle::Always)
        .terminator(csv::Terminator::CRLF)
        .from_writer(out);
    w.write_record([
        "Time of Day",
        "Process Name",
        "PID",
        "Operation",
        "Path",
        "Result",
        "Detail",
        "User",
    ])
    .map_err(|e| e.to_string())?;
    let mut n = 0;
    for ev in events {
        let ev = ev.borrow();
        w.write_record([
            ev.time_of_day().as_str(),
            ev.process_name().unwrap_or(""),
            ev.pid().to_string().as_str(),
            ev.operation_name(),
            ev.path().unwrap_or_default().as_str(),
            ev.result().as_ref(),
            ev.detail().as_str(),
            ev.user().unwrap_or_default().as_str(),
        ])
        .map_err(|e| e.to_string())?;
        n += 1;
    }
    // Flush the CSV buffer, then the BufWriter (surfacing IO errors that a
    // plain drop would swallow).
    let out = w.into_inner().map_err(|e| e.to_string())?;
    out.into_inner().map_err(|e| e.to_string())?;
    Ok(n)
}

/// Procmon-style XML: a process list (one entry per distinct pid, first-seen
/// order) + an event list, with optional stacks. Frames prefer a dbghelp symbol
/// (when the symbolizer has one) and fall back to `module + offset`; kernel
/// frames resolve against the symbolizer's prebuilt kernel views. Streams
/// through a `BufWriter`. `events` is any slice of owned or borrowed events.
pub fn export_xml<E: std::borrow::Borrow<Event>>(
    events: &[E],
    include_stacks: bool,
    sym: &StackSymbolizer<'_>,
    path: &str,
) -> std::result::Result<usize, String> {
    use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event as Xml};
    use quick_xml::writer::Writer;
    use std::io::Write;
    type Xw = Writer<std::io::BufWriter<std::fs::File>>;

    // 1-based ProcessIndex per distinct pid, in first-seen order.
    let mut order: Vec<u32> = Vec::new();
    let mut index_of: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    let mut sample: std::collections::HashMap<u32, &Event> = std::collections::HashMap::new();
    for ev in events {
        let ev = ev.borrow();
        let pid = ev.pid();
        index_of.entry(pid).or_insert_with(|| {
            order.push(pid);
            sample.insert(pid, ev);
            order.len()
        });
    }

    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut out = std::io::BufWriter::new(file);
    out.write_all(&[0xEF, 0xBB, 0xBF]) // UTF-8 BOM
        .map_err(|e| e.to_string())?;
    let mut w = Writer::new(out);
    let start = |w: &mut Xw, n: &str| -> std::result::Result<(), String> {
        w.write_event(Xml::Start(BytesStart::new(n)))
            .map_err(|e| e.to_string())
    };
    let end = |w: &mut Xw, n: &str| -> std::result::Result<(), String> {
        w.write_event(Xml::End(BytesEnd::new(n)))
            .map_err(|e| e.to_string())
    };
    let leaf = |w: &mut Xw, n: &str, text: &str| -> std::result::Result<(), String> {
        w.create_element(n)
            .write_text_content(BytesText::new(text))
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    let bit = |on: bool| if on { "1" } else { "0" };

    w.write_event(Xml::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .map_err(|e| e.to_string())?;
    start(&mut w, "procmon")?;
    start(&mut w, "processlist")?;
    for &pid in &order {
        let ev = sample[&pid];
        let parent = ev.parent_pid().unwrap_or(0);
        let parent_index = index_of.get(&parent).copied().unwrap_or(0);
        start(&mut w, "process")?;
        leaf(&mut w, "ProcessIndex", &index_of[&pid].to_string())?;
        leaf(&mut w, "ProcessId", &pid.to_string())?;
        leaf(&mut w, "ParentProcessId", &parent.to_string())?;
        leaf(&mut w, "ParentProcessIndex", &parent_index.to_string())?;
        leaf(
            &mut w,
            "AuthenticationId",
            &ev.auth_id().unwrap_or_default(),
        )?;
        leaf(&mut w, "CreateTime", &ev.process_create_time().to_string())?;
        leaf(
            &mut w,
            "FinishTime",
            &ev.process_exit_time().unwrap_or(0).to_string(),
        )?;
        leaf(
            &mut w,
            "IsVirtualized",
            bit(ev.is_virtualized() == Some(true)),
        )?;
        leaf(&mut w, "Is64bit", bit(ev.is_wow64() != Some(true)))?;
        leaf(&mut w, "Integrity", ev.integrity().unwrap_or(""))?;
        leaf(&mut w, "Owner", &ev.user().unwrap_or_default())?;
        leaf(&mut w, "ProcessName", ev.process_name().unwrap_or(""))?;
        leaf(&mut w, "ImagePath", ev.image_path().unwrap_or(""))?;
        leaf(&mut w, "CommandLine", ev.command_line().unwrap_or(""))?;
        leaf(&mut w, "CompanyName", ev.company().unwrap_or(""))?;
        leaf(&mut w, "Version", ev.version().unwrap_or(""))?;
        leaf(&mut w, "Description", ev.description().unwrap_or(""))?;
        start(&mut w, "modulelist")?;
        for m in ev.modules() {
            start(&mut w, "module")?;
            leaf(&mut w, "Timestamp", "0")?;
            leaf(&mut w, "BaseAddress", &format!("0x{:x}", m.base))?;
            leaf(&mut w, "Size", &m.size.to_string())?;
            leaf(&mut w, "Path", &m.path)?;
            leaf(&mut w, "Version", "")?;
            leaf(&mut w, "Company", "")?;
            leaf(&mut w, "Description", "")?;
            end(&mut w, "module")?;
        }
        end(&mut w, "modulelist")?;
        end(&mut w, "process")?;
    }
    end(&mut w, "processlist")?;

    start(&mut w, "eventlist")?;
    for ev in events {
        let ev = ev.borrow();
        let pidx = index_of.get(&ev.pid()).copied().unwrap_or(0);
        start(&mut w, "event")?;
        leaf(&mut w, "ProcessIndex", &pidx.to_string())?;
        leaf(&mut w, "Time_of_Day", &ev.time_of_day())?;
        leaf(&mut w, "Process_Name", ev.process_name().unwrap_or(""))?;
        leaf(&mut w, "PID", &ev.pid().to_string())?;
        leaf(&mut w, "Operation", ev.operation_name())?;
        leaf(&mut w, "Path", &ev.path().unwrap_or_default())?;
        leaf(&mut w, "Result", &ev.result())?;
        leaf(&mut w, "Detail", &ev.detail())?;
        leaf(&mut w, "User", &ev.user().unwrap_or_default())?;
        if include_stacks {
            // Per-event views over the process's own modules; the kernel list
            // is prebuilt once inside the symbolizer.
            let modules = ev.modules();
            let proc_views: Vec<SymModule> = modules
                .iter()
                .map(|m| SymModule {
                    base: m.base,
                    size: m.size as u64,
                    path: &m.path,
                })
                .collect();
            start(&mut w, "stack")?;
            for (depth, frame) in ev.call_stack().iter().enumerate() {
                let addr = frame.address();
                let (location, fpath) = sym.frame_location(addr, &proc_views);
                start(&mut w, "frame")?;
                leaf(&mut w, "depth", &depth.to_string())?;
                leaf(&mut w, "address", &format!("0x{addr:x}"))?;
                leaf(&mut w, "path", fpath)?;
                leaf(&mut w, "location", &location)?;
                end(&mut w, "frame")?;
            }
            end(&mut w, "stack")?;
        }
        end(&mut w, "event")?;
    }
    end(&mut w, "eventlist")?;
    end(&mut w, "procmon")?;

    // Flush the BufWriter explicitly (surfacing IO errors a drop would swallow).
    w.into_inner().into_inner().map_err(|e| e.to_string())?;
    Ok(events.len())
}
