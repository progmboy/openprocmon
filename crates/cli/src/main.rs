//! `procmon-cli`: the command-line + MCP front-end for OpenProcMon.
//!
//! A capture-then-analyze tool: `capture` writes a Procmon-compatible `.PML`
//! (live, needs Administrator + driver); every other command reads a `.PML` and
//! prints JSON — the same shape the MCP tools return. The one filter vocabulary
//! (`vocab`) drives both the capture filter and the analysis queries.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use procmon_core as core;
use procmon_sdk::{DriverLoader, PmlReader};

mod elevate;
mod ipc;
mod loader;
mod mcp;
mod orchestrate;
mod worker;

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
        /// (&& / || / ! / in (...)). See `vocab`.
        #[arg(long = "filter")]
        filter: Option<String>,
        /// Output .PML path (default: a temp file).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Number of sample events to include in the output.
        #[arg(long, default_value_t = 100)]
        sample: usize,
        /// Internal: when set, run as the elevated capture worker driven over
        /// this pipe by an unelevated parent. Not for direct use.
        #[arg(long = "control-pipe", hide = true)]
        control_pipe: Option<String>,
        /// Internal: parent pid for orphan-protection wait (worker mode only).
        #[arg(long = "parent-pid", hide = true)]
        parent_pid: Option<u32>,
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
        /// (&& / || / ! / in (...)). See `vocab`.
        #[arg(long = "filter")]
        filter: Option<String>,
        /// Aggregate: distinct values + counts. Comma-separate for multi-column
        /// (e.g. ProcessName,Path).
        #[arg(long = "group-by")]
        group_by: Option<String>,
        /// Numeric column to roll up per group (sum/avg/min/max + first/last time),
        /// e.g. NetBytes. Only used with --group-by.
        #[arg(long = "metric")]
        metric: Option<String>,
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
    /// Process timeline: a PID's state-changing activity (+ all network), time-ordered.
    Timeline {
        #[command(flatten)]
        src: PmlArg,
        #[arg(long)]
        pid: u32,
        /// Include reads / queries / closes too (default: only key operations).
        #[arg(long)]
        include_reads: bool,
        #[arg(long, default_value_t = 200)]
        limit: usize,
    },
    /// Event window: the events around a `seq` (same process unless --all-processes).
    Window {
        #[command(flatten)]
        src: PmlArg,
        #[arg(long)]
        seq: usize,
        #[arg(long, default_value_t = 25)]
        before: usize,
        #[arg(long, default_value_t = 25)]
        after: usize,
        /// Don't restrict to the center event's process.
        #[arg(long)]
        all_processes: bool,
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
        /// Filter expression (see `vocab`).
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
            control_pipe,
            parent_pid,
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
            control_pipe,
            parent_pid,
        ),
        Command::Summary { src, top } => print(&core::summary(&src.open()?, top)),
        Command::Query {
            src,
            filter,
            group_by,
            metric,
            no_noise,
            offset,
            limit,
            detail,
        } => {
            let reader = src.open()?;
            let expr = parse_filter_opt(&filter)?;
            let group = parse_groups(group_by.as_deref())?;
            let metric = metric.as_deref().map(parse_group).transpose()?;
            let noise = if no_noise {
                Vec::new()
            } else {
                core::default_noise()
            };
            print(&core::query(
                &reader,
                expr.as_ref(),
                &noise,
                &group,
                metric,
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
        Command::Timeline {
            src,
            pid,
            include_reads,
            limit,
        } => print(&core::process_timeline(
            &src.open()?,
            pid,
            include_reads,
            limit,
        )),
        Command::Window {
            src,
            seq,
            before,
            after,
            all_processes,
        } => {
            let w = core::event_window(&src.open()?, seq, before, after, !all_processes)
                .with_context(|| format!("no event with seq {seq}"))?;
            print(&w)
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
    control_pipe: Option<String>,
    parent_pid: Option<u32>,
) -> Result<()> {
    let out_path = out.unwrap_or_else(|| {
        std::env::temp_dir().join(format!("procmon-capture-{}.pml", std::process::id()))
    });
    let limits = core::CaptureLimits {
        max_bytes: max_mb * 1024 * 1024,
        duration: Some(std::time::Duration::from_secs(duration)),
    };

    // The parsed spec is built ONLY for the paths that capture in-process. The
    // orchestration path (c) forwards the RAW filter/launch/monitor strings to
    // the worker, which re-parses them — an Expr has no string form.
    let build_spec = |filter: &Option<String>| -> Result<core::TargetSpec> {
        Ok(core::TargetSpec {
            process_names: names.clone(),
            pids: pids.clone(),
            include_children: !no_children,
            launch: launch.clone().map(|s| shell_words(&s)),
            monitors: core::parse_monitors(&monitor),
            filter: parse_filter_opt(filter)?,
        })
    };

    // (a) Worker mode: an elevated child driven by an unelevated parent.
    if let Some(pipe) = control_pipe {
        return run_capture_worker(build_spec(&filter)?, limits, &out_path, &pipe, parent_pid);
    }

    // (b) Already elevated: capture in-process (current behavior).
    if elevate::is_elevated() {
        let outcome = core::capture(make_loader(), build_spec(&filter)?, limits, &out_path)
            .map_err(|e| anyhow::anyhow!(loader::describe_error(&e)))?;
        return print_capture_result(
            &outcome.pml_path,
            outcome.events_written,
            &outcome.stopped_reason,
            sample,
        );
    }

    // (c) Unelevated: validate the filter early (avoid a wasted UAC prompt),
    // then forward the RAW args to an elevated worker over a pipe.
    parse_filter_opt(&filter)?; // validation only; the worker re-parses.
    let args = build_worker_args(
        &names,
        &pids,
        !no_children,
        launch.as_deref(),
        &monitor,
        duration,
        max_mb,
        &out_path,
        filter.as_deref(),
        /*background=*/ false,
    );
    let outcome = orchestrate_one_shot(args, &out_path)?;
    print_capture_result(&outcome.0, outcome.1, &outcome.2, sample)
}

/// Worker mode: connect to the parent pipe, start the capture, run the control
/// loop (stop / EOF). Orphan protection: pipe EOF in run_worker + parent-pid wait.
fn run_capture_worker(
    spec: core::TargetSpec,
    limits: core::CaptureLimits,
    out_path: &std::path::Path,
    pipe: &str,
    parent_pid: Option<u32>,
) -> Result<()> {
    // parent_pid is reserved for an optional parent-liveness backup; pipe EOF
    // already covers parent death, so it is currently unused.
    let _ = parent_pid;
    let session = core::CaptureSession::start(make_loader(), spec, limits, out_path)
        .map_err(|e| anyhow::anyhow!(loader::describe_error(&e)))?;
    let (reader, mut writer) = orchestrate::connect_worker(pipe)?;

    worker::run_worker(Box::new(session), reader, &mut writer)
        .map_err(|e| anyhow::anyhow!("worker loop: {e}"))?;
    Ok(())
}

/// Unelevated one-shot: launch an elevated worker, then read its terminal `Done`
/// over the pipe (or a clean EOF when it exits). The worker finalizes the PML
/// before exiting; the result is the `Done` it reports, falling back to reading
/// the PML's event count if the `Done` was lost. The child handle is never
/// waited on (runas hProcess is unreliable).
fn orchestrate_one_shot(
    worker_argv: Vec<String>,
    out_path: &std::path::Path,
) -> Result<(String, usize, core::StoppedReason)> {
    #[cfg(not(windows))]
    {
        let _ = (worker_argv, out_path);
        anyhow::bail!("self-elevation is only supported on Windows");
    }
    #[cfg(windows)]
    {
        let mut link = orchestrate::launch_worker(&orchestrate::pipe_name(0), worker_argv)?;
        let done = link.read_done()?;
        match done {
            Some((events, reason, pml_path)) => {
                Ok((pml_path, events as usize, parse_stopped_reason(&reason)))
            }
            None => {
                // Worker exited without a Done; read the finalized PML.
                let reader = core::open_pml(out_path)?;
                let count = core::pml_info(&reader).event_count as usize;
                Ok((
                    out_path.to_string_lossy().into_owned(),
                    count,
                    core::StoppedReason::Duration,
                ))
            }
        }
    }
}

#[cfg(windows)]
fn parse_stopped_reason(s: &str) -> core::StoppedReason {
    match s {
        "Duration" => core::StoppedReason::Duration,
        "SizeLimit" => core::StoppedReason::SizeLimit,
        _ => core::StoppedReason::Manual,
    }
}

/// Builds the argv for an elevated `procmon-cli capture ... --control-pipe`
/// child from the RAW capture parameters. Forwards `filter`/`launch`/`monitor`
/// verbatim so the worker re-parses them identically — nothing is lost. Shared
/// by the CLI (`cmd_capture` path c) and MCP (`background_worker_args`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_worker_args(
    names: &[String],
    pids: &[u32],
    include_children: bool,
    launch: Option<&str>,
    monitor: &[String],
    duration: u64,
    max_mb: usize,
    out_path: &std::path::Path,
    filter: Option<&str>,
    background: bool,
) -> Vec<String> {
    let mut a = vec!["capture".to_string()];
    for n in names {
        a.push("--name".into());
        a.push(n.clone());
    }
    for p in pids {
        a.push("--pid".into());
        a.push(p.to_string());
    }
    if !include_children {
        a.push("--no-children".into());
    }
    if let Some(cmd) = launch {
        a.push("--launch".into());
        a.push(cmd.to_string());
    }
    if !monitor.is_empty() {
        a.push("--monitor".into());
        a.push(monitor.join(","));
    }
    if let Some(f) = filter {
        a.push("--filter".into());
        a.push(f.to_string());
    }
    // Background mode means "until stopped" — use a very large duration; the
    // parent stops it via the pipe.
    let secs = if background { u64::MAX / 2 } else { duration };
    a.push("--duration".into());
    a.push(secs.to_string());
    a.push("--max-mb".into());
    a.push(max_mb.to_string());
    a.push("--out".into());
    a.push(out_path.to_string_lossy().into_owned());
    a.push("--parent-pid".into());
    a.push(std::process::id().to_string());
    a
}

/// MCP wrapper: build worker argv for a background (`start_capture`) session.
#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn background_worker_args(
    names: &[String],
    pids: &[u32],
    include_children: bool,
    launch: Option<&str>,
    monitor: &[String],
    max_mb: usize,
    out_path: &std::path::Path,
    filter: Option<&str>,
) -> Vec<String> {
    build_worker_args(
        names,
        pids,
        include_children,
        launch,
        monitor,
        /*duration ignored in background*/ 0,
        max_mb,
        out_path,
        filter,
        true,
    )
}

/// Re-opens the produced PML and prints the standard capture result JSON.
fn print_capture_result(
    pml_path: &str,
    events_written: usize,
    stopped_reason: &core::StoppedReason,
    sample: usize,
) -> Result<()> {
    let reader = core::open_pml(pml_path)?;
    let noise = core::default_noise();
    let summary = core::summary(&reader, 10);
    let sample_events = core::query(&reader, None, &noise, &[], None, 0, sample, false);
    print(&serde_json::json!({
        "pml_path": pml_path,
        "events_written": events_written,
        "stopped_reason": stopped_reason,
        "summary": summary,
        "sample_events": sample_events.events,
    }))
}

/// Driver reachability + elevation + capability matrix.
fn driver_status() -> serde_json::Value {
    let running = procmon_sdk::MonitorController::connect().is_ok();
    serde_json::json!({
        "elevated": elevate::is_elevated(),
        "driver_running": running,
        "tools": elevate::capability_matrix(),
        "note": "Live capture needs admin; when unelevated `capture` auto-RunAs (UAC). PML analysis never needs elevation.",
    })
}

/// Parses an optional `--filter` expression (`None`/empty = match all).
fn parse_filter_opt(filter: &Option<String>) -> Result<Option<core::Expr>> {
    match filter.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => core::parse_filter(s).map(Some).map_err(anyhow::Error::msg),
        _ => Ok(None),
    }
}

/// Parses a `--group-by` / `--metric` field name (a Column or an extension field).
fn parse_group(name: &str) -> Result<core::Field> {
    core::parse_field(name).with_context(|| format!("unknown column {name:?}"))
}

/// Parses a `--group-by` spec into fields (comma-separated; `None`/empty = no
/// grouping).
fn parse_groups(spec: Option<&str>) -> Result<Vec<core::Field>> {
    match spec {
        Some(s) => s
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(parse_group)
            .collect(),
        None => Ok(Vec::new()),
    }
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
