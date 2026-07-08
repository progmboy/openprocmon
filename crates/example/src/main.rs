//! Console demonstration of `procmon-sdk`.
//!
//! ```text
//! # Live capture (run elevated). Optional .sys path loads the driver on demand.
//! cargo run -p procmon-example -- [C:\path\to\procmon.sys]
//!
//! # Live capture, then save the first events to a Procmon-compatible .PML:
//! cargo run -p procmon-example -- --save out.pml [C:\path\to\procmon.sys]
//!
//! # Read back a .PML (no driver needed) and print its events:
//! cargo run -p procmon-example -- --pml out.pml
//!
//! # Advanced output: low-level IRP_MJ_*/FASTIO_* operation names, no filtering:
//! cargo run -p procmon-example -- --pml out.pml --advanced
//! ```
//!
//! Both live capture and PML reading flow through one [`EventSource`]; the consume
//! loop is identical. `--save` (write) uses the `PmlWriter` and requires live
//! capture. Press Ctrl-C to exit live capture.

use clap::Parser;
use procmon_sdk::{
    default_display_filter, DriverLoader, EventSource, FilterSet, MonitorFlags, PmlWriter,
};
use std::error::Error;
use std::path::PathBuf;

/// Events captured before `--save` writes the file (live capture is unbounded).
const SAVE_LIMIT: usize = 5000;

/// Console demonstration of procmon-sdk: live capture, save to .PML, or read .PML.
#[derive(Parser)]
#[command(name = "procmon-example", version, about)]
struct Cli {
    /// Read a .PML file and print its events (no driver needed).
    #[arg(long, value_name = "FILE")]
    pml: Option<PathBuf>,

    /// Capture live events, then save them to a Procmon-compatible .PML.
    #[arg(long, value_name = "FILE")]
    save: Option<PathBuf>,

    /// Advanced output (cf. Procmon's Filter ▸ Enable Advanced Output): show the
    /// low-level IRP_MJ_*/FASTIO_* operation names and apply no filter (every event).
    /// Without it, the demo uses the friendly names and the default display filter.
    #[arg(long)]
    advanced: bool,

    /// Path to the driver .sys to load on demand (omit if it is already running).
    #[arg(value_name = "SYS")]
    sys: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    // Advanced output shows every event (no filter); the default view applies
    // Procmon's default display filter. The operation-name column follows suit via
    // `operation_name_advanced(cli.advanced)` below.
    let filter = if cli.advanced {
        FilterSet::default()
    } else {
        // Procmon's default display filter (the single source of truth lives in
        // `procmon_sdk::default_display_filter`, shared with the GUI and CLI).
        FilterSet::new(default_display_filter())
    };

    // One unified entry point for both live capture and offline PML.
    let source = match &cli.pml {
        Some(path) => EventSource::from_pml(path)?,
        None => {
            let sys = cli
                .sys
                .clone()
                .unwrap_or_else(|| PathBuf::from("procmon.sys"));
            EventSource::from_driver(
                DriverLoader::new("OpenProcmon24", sys),
                MonitorFlags::PROCESS | MonitorFlags::FILE | MonitorFlags::REGISTRY,
            )?
        }
    };
    source.set_filter(filter);

    // --save: capture live events and serialize them to a .PML (live only).
    if let Some(out) = &cli.save {
        if source.as_pml().is_some() {
            return Err("--save requires live capture (do not combine with --pml)".into());
        }
        let mut writer = PmlWriter::new(cfg!(target_pointer_width = "64"));
        // Host metadata + (at finish) the System process with kernel modules make
        // the PML self-describing, so Procmon resolves kernel stack frames.
        writer.stamp_host();
        println!(
            "Capturing up to {SAVE_LIMIT} events -> {} ...",
            out.display()
        );
        for ev in source.events().take(SAVE_LIMIT) {
            if source.is_visible(&ev) {
                writer.push_event(&ev);
            }
        }
        // Carry the full process table: pre-existing processes emit no events
        // (their INIT seeds are dropped in the correlator), so event interning
        // alone would orphan their children in the saved PML.
        if let Some(ctrl) = source.as_driver() {
            writer.stamp_processes(ctrl.processes());
        }
        writer.finish_live_to_path(out)?;
        println!("Saved {}.", out.display());
        return Ok(());
    }

    // Unified consume loop: stream events (live) or walk them (PML) the same way.
    println!("   PID  Operation               Result            Path");
    for ev in source.events() {
        if !source.is_visible(&ev) {
            continue;
        }
        println!(
            "{:>6}  {:<22}  {:<16}  {}",
            ev.pid(),
            ev.operation_name_advanced(cli.advanced),
            ev.result(),
            ev.path().unwrap_or_default(),
        );
    }

    Ok(())
}
