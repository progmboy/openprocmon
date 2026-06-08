//! Per-event detail parsing (Path / Category / detail columns), dispatched by
//! [`EventClass`] + operation.
//!
//! The PML detail blob is the driver's `EventData` (same op-codes / struct layouts
//! / FLT_PARAMETERS as our driver-compatible SDK) — it differs only in PML's string
//! re-encoding, which the SDK detail views handle via `DetailMode::Pml`. So we feed
//! the blob through the SDK's `Event` for full Path + Detail-column fidelity, and
//! decode Network locally (its PML blob is a different shape). Reading is 64-bit
//! only — [`crate::PmlReader`] rejects 32-bit captures up front, so `parse_via_sdk`
//! (which relies on the host x64 `FLT_PARAMETERS` width) is always correct here.

// This module's PmlEvent decode path is now used only by the PML round-trip and
// new/old comparison tests (the reader's public output is the unified `Event`).
#![allow(dead_code)]

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use crate::event::Event;
use crate::kernel_types::synth_record;
use crate::EventClass;

/// Lookup tables the detail parser may need (the network host/port name tables).
pub(crate) struct Tables<'a> {
    pub hosts: &'a HashMap<[u8; 16], Arc<str>>,
    pub ports: &'a HashMap<(u16, bool), Arc<str>>,
}

/// The Path / Category / detail columns extracted from an event's detail blobs.
pub(crate) struct Parsed {
    pub category: Cow<'static, str>,
    pub path: Arc<str>,
    pub details: Vec<(Cow<'static, str>, String)>,
    pub op_name: Option<String>,
}

impl Default for Parsed {
    fn default() -> Self {
        Self {
            category: Cow::Borrowed(""),
            path: Arc::from(""),
            details: Vec::new(),
            op_name: None,
        }
    }
}

/// Parses an event's detail (+ extra-detail) blobs into Path / Category / Detail.
pub(crate) fn parse_event(
    class: EventClass,
    operation: u16,
    details: &[u8],
    extra: Option<&[u8]>,
    tables: &Tables,
) -> Parsed {
    if class == EventClass::Network {
        let mut p = Parsed::default();
        // Decode the PML blob into the shared NetworkEvent model and render it
        // through the same NetView the live ETW path uses.
        if let Some(net) =
            crate::parse::network::decode_pml(operation, details, tables.hosts, tables.ports)
        {
            use crate::parse::OperationView;
            let view = crate::parse::network::NetView::new(&net);
            if let Some(path) = view.path() {
                p.path = Arc::from(path.as_str());
            }
            p.details.push(("Length".into(), net.length.to_string()));
            p.op_name = Some(view.op_label().to_string());
        }
        return p;
    }

    if matches!(
        class,
        EventClass::File | EventClass::Registry | EventClass::Process
    ) {
        return parse_via_sdk(class, operation, details, extra);
    }

    Parsed::default()
}

/// Feeds the detail blob through the mode-aware SDK `Event` parsing, reusing its
/// full per-operation Path + Detail formatting (and sub-op names). 64-bit only.
fn parse_via_sdk(
    class: EventClass,
    operation: u16,
    details: &[u8],
    extra: Option<&[u8]>,
) -> Parsed {
    // EventClass::to_u32 == the driver MonitorType code (Process=1, Registry=2,
    // File=3, Profiling=4); only File/Registry/Process reach here.
    let monitor = class.to_u32() as u16;
    let pre = synth_record(monitor, operation, 0, details).into_boxed_slice();
    let post = extra.map(|e| synth_record(0, operation, 0, e).into_boxed_slice());

    let mut p = Parsed {
        category: category_for(class, operation),
        ..Default::default()
    };
    if let Some(ev) = Event::from_pml(pre, post) {
        if let Some(path) = ev.path() {
            p.path = Arc::from(path.as_str());
        }
        let detail = ev.detail();
        if !detail.is_empty() {
            p.details.push((Cow::Borrowed("Detail"), detail));
        }
        p.op_name = Some(ev.operation_name().to_string());
    }
    p
}

fn category_for(class: EventClass, op: u16) -> Cow<'static, str> {
    match class {
        EventClass::File => filesystem_category(op),
        EventClass::Registry => registry_category(op),
        _ => Cow::Borrowed(""),
    }
}

/// Procmon's Category column for a registry op (Write for mutating ops, else Read).
fn registry_category(op: u16) -> Cow<'static, str> {
    match op {
        1 | 4 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 16 => Cow::Borrowed("Write"),
        _ => Cow::Borrowed("Read"),
    }
}

fn filesystem_category(op: u16) -> Cow<'static, str> {
    match op {
        // Write*
        3 | 24 | 26 | 28 | 31 | 41 | 46 => Cow::Borrowed("Write"),
        // Read / Query*
        5 | 6 | 23 | 25 | 27 | 30 | 32 | 40 | 45 => Cow::Borrowed("Read"),
        _ => Cow::Borrowed(""),
    }
}
