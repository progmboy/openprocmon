//! Export a (filtered) capture to PML / CSV / XML — the three formats the GUI's
//! Save dialog writes, so files are interchangeable. CSV/XML are ported from the
//! GUI's `export_csv`/`export_xml` (`crates/gui/src/model/sdk_source.rs`) on PML
//! events; PML uses [`procmon_sdk::PmlReader::write_subset`] (raw, byte-faithful).

use std::sync::Arc;

use procmon_sdk::{Event, PmlReader, Result};

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
            export_xml(&matched, include_stacks, path)
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

/// Procmon-style CSV: UTF-8 BOM, fully-quoted, CRLF rows (matches the GUI).
fn export_csv(events: &[Event], path: &str) -> std::result::Result<usize, String> {
    let mut buf: Vec<u8> = vec![0xEF, 0xBB, 0xBF];
    let mut n = 0;
    {
        let mut w = csv::WriterBuilder::new()
            .quote_style(csv::QuoteStyle::Always)
            .terminator(csv::Terminator::CRLF)
            .from_writer(&mut buf);
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
        for ev in events {
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
        w.flush().map_err(|e| e.to_string())?;
    }
    std::fs::write(path, buf).map_err(|e| e.to_string())?;
    Ok(n)
}

/// Procmon-style XML: a process list (one entry per distinct pid, first-seen
/// order) + an event list, with optional `module+offset` stacks. Ported from the
/// GUI's `export_xml` (symbol resolution deferred — frames are module+offset).
fn export_xml(
    events: &[Event],
    include_stacks: bool,
    path: &str,
) -> std::result::Result<usize, String> {
    use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event as Xml};
    use quick_xml::writer::Writer;

    // 1-based ProcessIndex per distinct pid, in first-seen order.
    let mut order: Vec<u32> = Vec::new();
    let mut index_of: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    let mut sample: std::collections::HashMap<u32, &Event> = std::collections::HashMap::new();
    for ev in events {
        let pid = ev.pid();
        index_of.entry(pid).or_insert_with(|| {
            order.push(pid);
            sample.insert(pid, ev);
            order.len()
        });
    }

    let mut w = Writer::new(Vec::<u8>::new());
    let start = |w: &mut Writer<Vec<u8>>, n: &str| -> std::result::Result<(), String> {
        w.write_event(Xml::Start(BytesStart::new(n.to_string())))
            .map_err(|e| e.to_string())
    };
    let end = |w: &mut Writer<Vec<u8>>, n: &str| -> std::result::Result<(), String> {
        w.write_event(Xml::End(BytesEnd::new(n.to_string())))
            .map_err(|e| e.to_string())
    };
    let leaf = |w: &mut Writer<Vec<u8>>, n: &str, text: &str| -> std::result::Result<(), String> {
        w.create_element(n.to_string())
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
            let modules = ev.modules();
            let mods: Vec<procmon_sdk::SymModule> = modules
                .iter()
                .map(|m| procmon_sdk::SymModule {
                    base: m.base,
                    size: m.size as u64,
                    path: &m.path,
                })
                .collect();
            start(&mut w, "stack")?;
            for (depth, frame) in ev.call_stack().iter().enumerate() {
                let addr = frame.address();
                let (_, location, fpath) = procmon_sdk::resolve_frame(addr, &mods, &[]);
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

    let mut out: Vec<u8> = vec![0xEF, 0xBB, 0xBF];
    out.extend_from_slice(&w.into_inner());
    std::fs::write(path, out).map_err(|e| e.to_string())?;
    Ok(events.len())
}

