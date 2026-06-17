//! `procmon-cli`: the command-line + MCP front-end for OpenProcMon.
//!
//! "Process Monitor as Wireshark": `capture` writes a Procmon-compatible `.PML`
//! (live, needs Administrator + driver); every other command reads a `.PML` and
//! prints JSON — the same shape the MCP tools return. The one filter vocabulary
//! (`vocab`) drives both the capture filter and the analysis queries.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use procmon_core as core;
use procmon_sdk::{DriverLoader, PmlReader};

mod ipc;
mod loader;
mod mcp;

#[derive(Parser)]
#[command(name = "procmon-cli", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Capture activity to a .PML (live; needs Administrator). Targets are by
    /// process name (+ children) and/or pid; an optional command is launched
    /// first. Prints the outcome plus a summary and a sample of events.
    Capture {
        /// Target process name (repeatable). Empty = capture the whole system.
        #[arg(long = "name")]
        names: Vec<String>,
        /// Target pid (repeatable).
        #[arg(long = "pid")]
        pids: Vec<u32>,
        /// Do NOT follow child processes of the targets.
        #[arg(long)]
        no_children: bool,
        /// Launch this command (whole string) before capturing.
        #[arg(long)]
        launch: Option<String>,
        /// Sources to monitor (comma-separated): process,file,registry,network.
        #[arg(
            long,
            value_delimiter = ',',
            default_value = "process,file,registry,network"
        )]
        monitor: Vec<String>,
        /// Capture for at most this many seconds.
        #[arg(long, default_value_t = 10)]
        duration: u64,
        /// Stop once this many MiB have been captured.
        #[arg(long = "max-mb", default_value_t = 512)]
        max_mb: usize,
        /// Capture-time filter expression, e.g. 'Operation == WriteFile'
        /// (Wireshark-style; && / || / ! / in (...)). See `vocab`.
        #[arg(long = "filter")]
        filter: Option<String>,
        /// Output .PML path (default: a temp file).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Number of sample events to include in the output.
        #[arg(long, default_value_t = 100)]
        sample: usize,
    },
    /// Overview of a capture: totals, by-category, top processes, rate.
    Summary {
        #[command(flatten)]
        src: PmlArg,
        #[arg(long, default_value_t = 10)]
        top: usize,
    },
    /// Query events: filter (cross-clause AND) + optional group-by aggregation.
    Query {
        #[command(flatten)]
        src: PmlArg,
        /// Filter expression, e.g. 'Category == "File System" && Operation == WriteFile'
        /// (Wireshark-style; && / || / ! / in (...)). See `vocab`.
        #[arg(long = "filter")]
        filter: Option<String>,
        /// Aggregate: distinct values + counts of this column.
        #[arg(long = "group-by")]
        group_by: Option<String>,
        /// Include the noise (NTFS metadata / monitoring tools / bookkeeping).
        #[arg(long)]
        no_noise: bool,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        /// Include the (expensive) Detail column in event rows.
        #[arg(long)]
        detail: bool,
    },
    /// Full detail of one event (event/process/stack) by its `seq`.
    GetEvent {
        #[command(flatten)]
        src: PmlArg,
        #[arg(long)]
        seq: usize,
        /// Parts to include (comma-separated): event,process,stack.
        #[arg(long, value_delimiter = ',', default_value = "event,process,stack")]
        part: Vec<String>,
    },
    /// Full detail (+ modules) of one process by pid.
    GetProcess {
        #[command(flatten)]
        src: PmlArg,
        #[arg(long)]
        pid: u32,
    },
    /// All processes seen in the capture (flat).
    Processes {
        #[command(flatten)]
        src: PmlArg,
    },
    /// The parent→child process tree.
    Tree {
        #[command(flatten)]
        src: PmlArg,
    },
    /// .PML metadata (event count, computer, OS, process count).
    PmlInfo {
        #[command(flatten)]
        src: PmlArg,
    },
    /// Export a (filtered) capture to PML / CSV / XML.
    Export {
        #[command(flatten)]
        src: PmlArg,
        #[arg(long)]
        format: String,
        #[arg(long)]
        out: PathBuf,
        /// Filter expression (Wireshark-style; see `vocab`).
        #[arg(long = "filter")]
        filter: Option<String>,
        /// Include call-stack frames (XML only).
        #[arg(long)]
        stacks: bool,
    },
    /// The filter vocabulary: valid column/relation/operation names.
    Vocab,
    /// Whether the driver is reachable / capture is possible.
    DriverStatus,
    /// Serve the Model Context Protocol over stdio (for AI agents).
    Mcp,
}

/// The `--pml <path>` source shared by the analysis commands.
#[derive(clap::Args)]
struct PmlArg {
    /// Path to a .PML file to analyze.
    #[arg(long)]
    pml: PathBuf,
}

impl PmlArg {
    fn open(&self) -> Result<Arc<PmlReader>> {
        core::open_pml(&self.pml).with_context(|| format!("open {}", self.pml.display()))
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Capture {
            names,
            pids,
            no_children,
            launch,
            monitor,
            duration,
            max_mb,
            filter,
            out,
            sample,
        } => cmd_capture(
            names,
            pids,
            no_children,
            launch,
            monitor,
            duration,
            max_mb,
            filter,
            out,
            sample,
        ),
        Command::Summary { src, top } => print(&core::summary(&src.open()?, top)),
        Command::Query {
            src,
            filter,
            group_by,
            no_noise,
            offset,
            limit,
            detail,
        } => {
            let reader = src.open()?;
            let expr = parse_filter_opt(&filter)?;
            let group = group_by.as_deref().map(parse_group).transpose()?;
            let noise = if no_noise {
                Vec::new()
            } else {
                core::default_noise()
            };
            print(&core::query(
                &reader,
                expr.as_ref(),
                &noise,
                group,
                offset,
                limit,
                detail,
            ))
        }
        Command::GetEvent { src, seq, part } => {
            let d = core::get_event(&src.open()?, seq, &part)
                .with_context(|| format!("no event with seq {seq}"))?;
            print(&d)
        }
        Command::GetProcess { src, pid } => {
            let p = core::get_process(&src.open()?, pid)
                .with_context(|| format!("no process with pid {pid}"))?;
            print(&p)
        }
        Command::Processes { src } => print(&core::list_processes(&src.open()?)),
        Command::Tree { src } => print(&core::process_tree(&src.open()?)),
        Command::PmlInfo { src } => print(&core::pml_info(&src.open()?)),
        Command::Export {
            src,
            format,
            out,
            filter,
            stacks,
        } => {
            let fmt = core::Format::parse(&format)
                .with_context(|| format!("unknown format {format:?} (pml|csv|xml)"))?;
            let expr = parse_filter_opt(&filter)?;
            let n = core::export(
                &src.open()?,
                fmt,
                expr.as_ref(),
                &[],
                stacks,
                out.to_str().context("non-UTF-8 output path")?,
            )
            .map_err(anyhow::Error::msg)?;
            print(&serde_json::json!({ "out": out, "events_written": n }))
        }
        Command::Vocab => print(&core::filter_vocab()),
        Command::DriverStatus => print(&driver_status()),
        Command::Mcp => mcp::serve(),
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_capture(
    names: Vec<String>,
    pids: Vec<u32>,
    no_children: bool,
    launch: Option<String>,
    monitor: Vec<String>,
    duration: u64,
    max_mb: usize,
    filter: Option<String>,
    out: Option<PathBuf>,
    sample: usize,
) -> Result<()> {
    let out_path = out.unwrap_or_else(|| {
        std::env::temp_dir().join(format!("procmon-capture-{}.pml", std::process::id()))
    });
    let spec = core::TargetSpec {
        process_names: names,
        pids,
        include_children: !no_children,
        launch: launch.map(|s| shell_words(&s)),
        monitors: core::parse_monitors(&monitor),
        filter: parse_filter_opt(&filter)?,
    };
    let limits = core::CaptureLimits {
        max_bytes: max_mb * 1024 * 1024,
        duration: Some(std::time::Duration::from_secs(duration)),
    };
    let outcome = core::capture(make_loader(), spec, limits, &out_path)
        .map_err(|e| anyhow::anyhow!(loader::describe_error(&e)))?;

    // Re-open the produced PML for a summary + sample.
    let reader = core::open_pml(&out_path)?;
    let noise = core::default_noise();
    let summary = core::summary(&reader, 10);
    let sample_events = core::query(&reader, None, &noise, None, 0, sample, false);
    print(&serde_json::json!({
        "pml_path": outcome.pml_path,
        "events_written": outcome.events_written,
        "stopped_reason": outcome.stopped_reason,
        "summary": summary,
        "sample_events": sample_events.events,
    }))
}

/// Driver reachability: try to connect to an already-running driver port.
fn driver_status() -> serde_json::Value {
    let running = procmon_sdk::MonitorController::connect().is_ok();
    serde_json::json!({
        "driver_running": running,
        "note": "Live capture requires Administrator; PML analysis does not.",
    })
}

/// Parses an optional `--filter` expression (`None`/empty = match all).
fn parse_filter_opt(filter: &Option<String>) -> Result<Option<core::Expr>> {
    match filter.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => core::parse_filter(s).map(Some).map_err(anyhow::Error::msg),
        _ => Ok(None),
    }
}

/// Parses a `--group-by` column name.
fn parse_group(name: &str) -> Result<procmon_sdk::Column> {
    core::parse_column(name).with_context(|| format!("unknown column {name:?}"))
}

/// Splits a launch command into argv on whitespace (quotes not handled — pass a
/// bare path; complex commands can be launched by the agent directly).
fn shell_words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_string).collect()
}

/// Serializes `value` as pretty JSON to stdout.
fn print<T: serde::Serialize>(value: &T) -> Result<()> {
    let mut s = serde_json::to_string_pretty(value)?;
    s.push('\n');
    print!("{s}");
    Ok(())
}

const DRIVER_NAME: &str = "OpenProcmon24";

#[cfg(feature = "embedded-driver")]
pub(crate) fn make_loader() -> DriverLoader {
    const DRIVER_IMAGE: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../bin/PROCMON24.SYS"
    ));
    DriverLoader::from_embedded(DRIVER_NAME, "PROCMON24.SYS", DRIVER_IMAGE)
}

#[cfg(not(feature = "embedded-driver"))]
pub(crate) fn make_loader() -> DriverLoader {
    let sys = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("procmon.sys")))
        .unwrap_or_else(|| PathBuf::from("procmon.sys"));
    DriverLoader::new(DRIVER_NAME, sys)
}
