//! The `procmon-cli mcp` server: an rmcp stdio server exposing capture +
//! PML-analysis tools, 1:1 with the CLI subcommands and returning the same core
//! JSON shapes. Stateful — running captures are held by `session_id` so analysis
//! tools can reference a just-finished capture without a file path.
//!
//! All protocol I/O is stdout (rmcp's transport); this module never prints to
//! stdout. Blocking core work runs on `spawn_blocking` so the async runtime
//! stays responsive during long captures / large-PML scans.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use procmon_core as core;
use procmon_sdk::PmlReader;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::schemars::{self, JsonSchema};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServiceExt};
use serde::Deserialize;

/// A capture session's output PML path and (while running) its backend.
struct Session {
    pml_path: String,
    backend: Option<Backend>,
}

/// Background capture implementation: in-process when the server is elevated, or
/// an elevated worker driven over a pipe when it is not.
enum Backend {
    InProcess(core::CaptureSession),
    #[cfg(windows)]
    Elevated(crate::orchestrate::WorkerLink),
}

#[derive(Clone)]
pub struct ProcmonServer {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    next_id: Arc<AtomicU64>,
    // Read by the `#[tool_handler]`-generated dispatch (tools/list, tools/call);
    // the dead-code lint doesn't see through the macro.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

/// Runs the MCP server over stdio until the client disconnects.
pub fn serve() -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let service = ProcmonServer::new().serve(rmcp::transport::stdio()).await?;
        service.waiting().await?;
        Ok::<(), anyhow::Error>(())
    })
}

// --- tool argument types (JSON Schema derived for the client) --------------

/// Selects the data source for an analysis tool: exactly one of a finished
/// capture `session_id` or a `.PML` file path.
#[derive(Deserialize, JsonSchema)]
struct Source {
    /// A `session_id` returned by `capture`/`start_capture` (after it stopped).
    #[serde(default)]
    session_id: Option<String>,
    /// Path to a `.PML` file to analyze.
    #[serde(default)]
    pml_path: Option<String>,
}

/// A filter expression (see `list_filter_columns` for the
/// columns / operators). Example: `Category == "File System" && Operation ==
/// WriteFile`. Empty / omitted matches everything.
type FilterExpr = Option<String>;

#[derive(Deserialize, JsonSchema)]
struct CaptureArgs {
    /// Target process names (empty = whole system).
    #[serde(default)]
    process_names: Vec<String>,
    #[serde(default)]
    pids: Vec<u32>,
    /// Follow child processes of the targets (default true).
    #[serde(default = "default_true")]
    include_children: bool,
    /// Optional command to launch before capturing.
    #[serde(default)]
    launch: Option<String>,
    /// Sources: any of process/file/registry/network (default all).
    #[serde(default)]
    monitors: Vec<String>,
    /// Optional capture-time filter expression.
    #[serde(default)]
    filter: FilterExpr,
    #[serde(default = "default_duration")]
    duration_seconds: u64,
    #[serde(default = "default_max_mb")]
    max_mb: usize,
    #[serde(default = "default_sample")]
    sample: usize,
}

#[derive(Deserialize, JsonSchema)]
struct StartArgs {
    #[serde(default)]
    process_names: Vec<String>,
    #[serde(default)]
    pids: Vec<u32>,
    #[serde(default = "default_true")]
    include_children: bool,
    #[serde(default)]
    launch: Option<String>,
    #[serde(default)]
    monitors: Vec<String>,
    #[serde(default)]
    filter: FilterExpr,
    #[serde(default = "default_max_mb")]
    max_mb: usize,
}

#[derive(Deserialize, JsonSchema)]
struct SessionId {
    session_id: String,
}

#[derive(Deserialize, JsonSchema)]
struct SummaryArgs {
    #[serde(flatten)]
    source: Source,
    #[serde(default = "default_top")]
    top: usize,
}

#[derive(Deserialize, JsonSchema)]
struct QueryArgs {
    #[serde(flatten)]
    source: Source,
    /// Filter expression: `Column OP value` clauses joined with && / || / ! and
    /// parens (OP: == != ~ !~ ^= $= < >, or `Column in (a, b)`); quote values with
    /// spaces. File "writes" are not just WriteFile — they span WriteFile,
    /// SetEndOfFileInformationFile / SetAllocationInformationFile (truncate/extend),
    /// SetRenameInformationFile (rename/move), SetDispositionInformationFile (mark
    /// for delete), DeleteFile. CreateFile is a file OPEN (how a process opens ANY
    /// file), not necessarily a creation — the OpenResult extension field says what
    /// happened (Created/Opened/Overwritten/…), so files actually created =
    /// 'Operation == CreateFile && OpenResult == Created'. Disposition is what was
    /// requested (Open/Create/OpenIf/…). See list_filter_columns for exact column /
    /// field / operation names and meanings.
    #[serde(default)]
    filter: FilterExpr,
    /// Group-by column(s); comma-separate for multi-column (e.g. ProcessName,Path).
    /// Returns distinct values + counts instead of raw rows — use it to summarize.
    /// Summary views: busiest processes = group_by=ProcessName (add ,Category for a
    /// breakdown); network endpoints = group_by=RemoteAddress (+ metric=NetBytes);
    /// who-touched-a-file = filter on Path, group_by=ProcessName.
    #[serde(default)]
    group_by: Option<String>,
    /// Numeric column / field to roll up per group — adds sum/avg/min/max + first/
    /// last time (e.g. group_by=RemoteAddress metric=NetBytes for bytes per endpoint).
    /// NetBytes is an accurate, summable network transfer size; file IO byte counts
    /// are NOT exposed (memory-mapped IO is invisible) so don't sum file bytes — use
    /// operation counts for file activity. Procmon records WHICH file/endpoint was
    /// touched, never the bytes written; for what was actually sent/stolen, look at
    /// Network activity. Only used with group_by.
    #[serde(default)]
    metric: Option<String>,
    /// Apply the default noise filter (default true).
    #[serde(default = "default_true")]
    exclude_noise: bool,
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    include_detail: bool,
}

#[derive(Deserialize, JsonSchema)]
struct TimelineArgs {
    #[serde(flatten)]
    source: Source,
    /// Process id whose timeline to build.
    pid: u32,
    /// Include reads / queries / closes too (default: only state-changing
    /// operations plus all network activity).
    #[serde(default)]
    include_reads: bool,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(Deserialize, JsonSchema)]
struct WindowArgs {
    #[serde(flatten)]
    source: Source,
    /// Center event seq (from query_events / get_event).
    seq: usize,
    /// Events to include before the center (default 25).
    #[serde(default = "default_window")]
    before: usize,
    /// Events to include after the center (default 25).
    #[serde(default = "default_window")]
    after: usize,
    /// Restrict to the center event's process (default true).
    #[serde(default = "default_true")]
    same_process: bool,
}

#[derive(Deserialize, JsonSchema)]
struct GetEventArgs {
    #[serde(flatten)]
    source: Source,
    seq: usize,
    #[serde(default)]
    parts: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
struct GetProcessArgs {
    #[serde(flatten)]
    source: Source,
    pid: u32,
}

#[derive(Deserialize, JsonSchema)]
struct SourceOnly {
    #[serde(flatten)]
    source: Source,
}

#[derive(Deserialize, JsonSchema)]
struct ListArgs {
    #[serde(flatten)]
    source: Source,
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(Deserialize, JsonSchema)]
struct ExportArgs {
    #[serde(flatten)]
    source: Source,
    /// Output format: `pml`, `csv`, or `xml`.
    format: String,
    out_path: String,
    #[serde(default)]
    filter: FilterExpr,
    #[serde(default)]
    include_stacks: bool,
}

fn default_true() -> bool {
    true
}
fn default_duration() -> u64 {
    10
}
fn default_max_mb() -> usize {
    512
}
fn default_sample() -> usize {
    100
}
fn default_top() -> usize {
    10
}
fn default_limit() -> usize {
    100
}
fn default_window() -> usize {
    25
}

// --- tools ------------------------------------------------------------------

#[tool_router]
impl ProcmonServer {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicU64::new(1)),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Capture process/file/registry/network activity to a PML for a fixed \
        duration, then return a session_id, summary and a sample of events. Targets are by \
        process name (+children) and/or pid; an optional command is launched first. \
        Live capture requires Administrator."
    )]
    async fn capture(
        &self,
        Parameters(a): Parameters<CaptureArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Validate the filter before any UAC prompt.
        parse_filter_opt(&a.filter)?;
        let elevated = crate::elevate::is_elevated();
        let out = temp_pml();
        let out2 = out.clone();
        let CaptureArgs {
            process_names,
            pids,
            include_children,
            launch,
            monitors,
            filter,
            duration_seconds,
            max_mb,
            sample,
        } = a;
        let max_bytes = max_mb * 1024 * 1024;
        let (pml_path, events_written, stopped_reason) =
            tokio::task::spawn_blocking(move || -> Result<(String, usize, String), String> {
                if elevated {
                    let spec = core::TargetSpec {
                        process_names: process_names.clone(),
                        pids: pids.clone(),
                        include_children,
                        launch: launch
                            .clone()
                            .map(|s| s.split_whitespace().map(str::to_string).collect()),
                        monitors: core::parse_monitors(&monitors),
                        filter: filter.as_deref().map(core::parse_filter).transpose()?,
                    };
                    let limits = core::CaptureLimits {
                        max_bytes,
                        duration: Some(std::time::Duration::from_secs(duration_seconds)),
                    };
                    let o = core::capture(crate::make_loader(), spec, limits, &out2)
                        .map_err(|e| crate::loader::describe_error(&e))?;
                    Ok((
                        o.pml_path,
                        o.events_written,
                        format!("{:?}", o.stopped_reason),
                    ))
                } else {
                    #[cfg(windows)]
                    {
                        let args = crate::build_worker_args(
                            &process_names,
                            &pids,
                            include_children,
                            launch.as_deref(),
                            &monitors,
                            duration_seconds,
                            max_mb,
                            std::path::Path::new(&out2),
                            filter.as_deref(),
                            false,
                        );
                        let mut link = crate::orchestrate::launch_worker(
                            &crate::orchestrate::pipe_name(0),
                            args,
                        )
                        .map_err(|e| e.to_string())?;
                        // Read the worker's terminal Done (or EOF on exit); the
                        // child handle is unreliable to wait on.
                        match link.read_done().map_err(|e| e.to_string())? {
                            Some((events, reason, pml_path)) => {
                                Ok((pml_path, events as usize, reason))
                            }
                            None => {
                                let reader = core::open_pml(&out2).map_err(|e| e.to_string())?;
                                let count = core::pml_info(&reader).event_count as usize;
                                Ok((out2.clone(), count, "Duration".to_string()))
                            }
                        }
                    }
                    #[cfg(not(windows))]
                    Err("self-elevation is only supported on Windows".into())
                }
            })
            .await
            .map_err(internal)?
            .map_err(internal)?;

        let id = self.store_finished(pml_path.clone());
        let pml = pml_path.clone();
        let body = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
            let reader = core::open_pml(&pml).map_err(|e| e.to_string())?;
            let summary = core::summary(&reader, 10);
            let events = core::query(
                &reader,
                None,
                &core::default_noise(),
                &[],
                None,
                0,
                sample,
                false,
            );
            Ok(serde_json::json!({
                "summary": summary,
                "sample_events": events.events,
            }))
        })
        .await
        .map_err(internal)?
        .map_err(internal)?;

        json(&serde_json::json!({
            "session_id": id,
            "pml_path": pml_path,
            "events_written": events_written,
            "stopped_reason": stopped_reason,
            "summary": body["summary"],
            "sample_events": body["sample_events"],
        }))
    }

    #[tool(
        description = "Start a background capture (no duration limit) and return its session_id. \
        Stop it with stop_capture, then analyze by session_id. Live capture requires Administrator."
    )]
    async fn start_capture(
        &self,
        Parameters(a): Parameters<StartArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Validate the filter before any UAC prompt.
        parse_filter_opt(&a.filter)?;
        let out = temp_pml();
        let elevated = crate::elevate::is_elevated();
        let out2 = out.clone();
        let StartArgs {
            process_names,
            pids,
            include_children,
            launch,
            monitors,
            filter,
            max_mb,
        } = a;
        let max_bytes = max_mb * 1024 * 1024;
        let (backend, pml) =
            tokio::task::spawn_blocking(move || -> Result<(Backend, String), String> {
                if elevated {
                    let spec = core::TargetSpec {
                        process_names: process_names.clone(),
                        pids: pids.clone(),
                        include_children,
                        launch: launch
                            .clone()
                            .map(|s| s.split_whitespace().map(str::to_string).collect()),
                        monitors: core::parse_monitors(&monitors),
                        filter: filter.as_deref().map(core::parse_filter).transpose()?,
                    };
                    let limits = core::CaptureLimits {
                        max_bytes,
                        duration: None,
                    };
                    let s = core::CaptureSession::start(crate::make_loader(), spec, limits, &out2)
                        .map_err(|e| crate::loader::describe_error(&e))?;
                    let p = s.pml_path().to_string_lossy().into_owned();
                    Ok((Backend::InProcess(s), p))
                } else {
                    #[cfg(windows)]
                    {
                        let args = crate::background_worker_args(
                            &process_names,
                            &pids,
                            include_children,
                            launch.as_deref(),
                            &monitors,
                            max_mb,
                            std::path::Path::new(&out2),
                            filter.as_deref(),
                        );
                        let mut link = crate::orchestrate::launch_worker(
                            &crate::orchestrate::pipe_name(0),
                            args,
                        )
                        .map_err(|e| e.to_string())?;
                        // The worker's first message (sent while alive) is the
                        // real PML path.
                        let p = link
                            .read_started()
                            .map_err(|e| e.to_string())?
                            .unwrap_or_else(|| out2.clone());
                        Ok((Backend::Elevated(link), p))
                    }
                    #[cfg(not(windows))]
                    Err("self-elevation is only supported on Windows".into())
                }
            })
            .await
            .map_err(internal)?
            .map_err(internal)?;

        let id = self.next_id();
        self.sessions.lock().unwrap().insert(
            id.clone(),
            Session {
                pml_path: pml.clone(),
                backend: Some(backend),
            },
        );
        json(&serde_json::json!({ "session_id": id, "pml_path": pml }))
    }

    #[tool(description = "Stop a running capture (finalizes its PML) and return the outcome.")]
    async fn stop_capture(
        &self,
        Parameters(a): Parameters<SessionId>,
    ) -> Result<CallToolResult, McpError> {
        let (backend, pml_path) = {
            let mut map = self.sessions.lock().unwrap();
            let s = map
                .get_mut(&a.session_id)
                .ok_or_else(|| McpError::invalid_params("unknown session_id", None))?;
            (s.backend.take(), s.pml_path.clone())
        };
        let Some(backend) = backend else {
            return json(&serde_json::json!({ "stopped": false, "note": "already stopped" }));
        };
        let body: Result<serde_json::Value, String> =
            tokio::task::spawn_blocking(move || match backend {
                Backend::InProcess(s) => {
                    let o = s.stop().map_err(|e| e.to_string())?;
                    Ok(serde_json::json!({
                        "events_written": o.events_written,
                        "stopped_reason": format!("{:?}", o.stopped_reason),
                        "pml_path": o.pml_path,
                    }))
                }
                #[cfg(windows)]
                Backend::Elevated(mut link) => {
                    // Signal the worker to finalize, then read its terminal Done
                    // (or EOF on exit). The child handle is unreliable to wait on.
                    link.send_stop().ok();
                    match link.read_done().map_err(|e| e.to_string())? {
                        Some((events, reason, path)) => Ok(serde_json::json!({
                            "events_written": events,
                            "stopped_reason": reason,
                            "pml_path": path,
                        })),
                        None => {
                            let reader = core::open_pml(&pml_path).map_err(|e| e.to_string())?;
                            let count = core::pml_info(&reader).event_count;
                            Ok(serde_json::json!({
                                "events_written": count,
                                "stopped_reason": "Manual",
                                "pml_path": pml_path,
                            }))
                        }
                    }
                }
            })
            .await
            .map_err(internal)?;
        let body = body.map_err(internal)?;
        json(&serde_json::json!({
            "stopped": true,
            "events_written": body["events_written"],
            "stopped_reason": body["stopped_reason"],
            "pml_path": body["pml_path"],
        }))
    }

    #[tool(description = "Whether a capture session is still running, and its output path.")]
    async fn capture_status(
        &self,
        Parameters(a): Parameters<SessionId>,
    ) -> Result<CallToolResult, McpError> {
        let map = self.sessions.lock().unwrap();
        let s = map
            .get(&a.session_id)
            .ok_or_else(|| McpError::invalid_params("unknown session_id", None))?;
        // A session counts as running until stop_capture takes its backend.
        let running = s.backend.is_some();
        json(&serde_json::json!({ "running": running, "pml_path": s.pml_path }))
    }

    #[tool(
        description = "Capture overview: total events, counts by category, top processes, rate."
    )]
    async fn summary(
        &self,
        Parameters(a): Parameters<SummaryArgs>,
    ) -> Result<CallToolResult, McpError> {
        let top = a.top;
        self.analyze(a.source, move |r| Ok(core::summary(r, top)))
            .await
    }

    #[tool(
        description = "Find or summarize events in a .PML — the universal analysis primitive. \
        `filter` is one expression string: `Column OP value` clauses joined with && / || / ! and \
        parens; quote values with spaces. Operators: == (is) != (is not) ~ (contains) !~ (excludes) \
        ^= (begins) $= (ends) < > (numeric), and `Column in (a, b, c)` (matches ANY of the values). \
        With group_by, returns the distinct values + counts of that column instead of raw rows \
        (summarize — don't page thousands); comma-separate for multi-column \
        (group_by=ProcessName,Path), and add metric=<numeric column> for sum/avg/min/max + \
        first/last time per group. Without group_by, a page of events, each with a seq for \
        get_event / event_window. Prefer group_by/metric over exporting CSV to count or total \
        things yourself. \
        Recipes: files X wrote = 'Category == \"File System\" && ProcessName == X && Operation in \
        (WriteFile, SetEndOfFileInformationFile, DeleteFile)' group_by=Path; registry persistence \
        = 'Category == Registry && Operation in (RegSetValue, RegCreateKey) && Path ~ Run' \
        group_by=Path; network endpoints = 'Category == Network && ProcessName == X' \
        group_by=RemoteAddress (+ metric=NetBytes for bytes per endpoint); failed ops = \
        'Result != SUCCESS'. Summaries (the GUI's summary views) are all just group_by: busiest \
        processes = group_by=ProcessName (add ,Category for a breakdown); file/registry summary = \
        'Category == \"File System\"' (or Registry) group_by=Path; network summary = 'Category == \
        Network' group_by=RemoteAddress metric=NetBytes; who-touched-a-file = 'Path ~ \"...\"' \
        group_by=ProcessName; operation/result mix = group_by=Operation or group_by=Result; \
        files actually created (vs merely opened) = 'Operation == CreateFile && OpenResult == \
        Created' group_by=Path. CreateFile is a file OPEN (not necessarily a creation — see the \
        OpenResult extension field); NetBytes is summable but file byte totals are not. Call \
        list_filter_columns for exact column / field / operation names + meanings."
    )]
    async fn query_events(
        &self,
        Parameters(a): Parameters<QueryArgs>,
    ) -> Result<CallToolResult, McpError> {
        let filter = parse_filter_opt(&a.filter)?;
        let group = parse_columns(a.group_by.as_deref())?;
        let metric =
            match a.metric.as_deref() {
                Some(c) => Some(core::parse_field(c).ok_or_else(|| {
                    McpError::invalid_params(format!("unknown column: {c}"), None)
                })?),
                None => None,
            };
        let (offset, detail, exclude_noise) = (a.offset, a.include_detail, a.exclude_noise);
        let limit = a.limit.min(MAX_QUERY_LIMIT);
        self.analyze(a.source, move |r| {
            let noise = if exclude_noise {
                core::default_noise()
            } else {
                Vec::new()
            };
            let mut res = core::query(
                r,
                filter.as_ref(),
                &noise,
                &group,
                metric,
                offset,
                limit,
                detail,
            );
            // Clip unbounded per-row strings so a detail-heavy or long-path page
            // can't trip the response-size guard; full values are in get_event.
            for e in &mut res.events {
                clip(&mut e.path, MAX_FIELD);
                if let Some(d) = e.detail.as_mut() {
                    clip(d, MAX_FIELD);
                }
            }
            for g in &mut res.groups {
                for v in &mut g.values {
                    clip(v, MAX_FIELD);
                }
            }
            // Return as many rows as fit the budget (marking truncated) rather
            // than letting an oversized page trip the all-or-nothing guard.
            fit_within_budget(&mut res);
            Ok(res)
        })
        .await
    }

    #[tool(description = "Full detail of one event by its seq (event/process/stack parts).")]
    async fn get_event(
        &self,
        Parameters(a): Parameters<GetEventArgs>,
    ) -> Result<CallToolResult, McpError> {
        let (seq, parts) = (a.seq, a.parts);
        self.analyze(a.source, move |r| {
            core::get_event(r, seq, &parts).ok_or_else(|| format!("no event with seq {seq}"))
        })
        .await
    }

    #[tool(
        description = "A process's activity as a time-ordered timeline. By default keeps only \
        state-changing operations (writes / deletes / creates, registry writes, process / image \
        load) plus all network activity — reads / queries / closes are folded away; set \
        include_reads=true for everything. Noise is excluded. A quick 'what did this PID do'."
    )]
    async fn process_timeline(
        &self,
        Parameters(a): Parameters<TimelineArgs>,
    ) -> Result<CallToolResult, McpError> {
        let (pid, include_reads) = (a.pid, a.include_reads);
        let limit = a.limit.min(MAX_QUERY_LIMIT);
        self.analyze(a.source, move |r| {
            let mut res = core::process_timeline(r, pid, include_reads, limit);
            for e in &mut res.events {
                clip(&mut e.path, MAX_FIELD);
                if let Some(d) = e.detail.as_mut() {
                    clip(d, MAX_FIELD);
                }
            }
            fit_within_budget(&mut res);
            Ok(res)
        })
        .await
    }

    #[tool(
        description = "Context around one event: the events just before and after seq, by \
        default within the same process. Use it to see what led up to / followed a specific \
        event (get a seq from query_events / get_event)."
    )]
    async fn event_window(
        &self,
        Parameters(a): Parameters<WindowArgs>,
    ) -> Result<CallToolResult, McpError> {
        let (seq, same) = (a.seq, a.same_process);
        let before = a.before.min(MAX_QUERY_LIMIT);
        let after = a.after.min(MAX_QUERY_LIMIT);
        self.analyze(a.source, move |r| {
            core::event_window(r, seq, before, after, same)
                .ok_or_else(|| format!("no event with seq {seq}"))
        })
        .await
    }

    #[tool(description = "Full detail (+ loaded modules) of one process by pid.")]
    async fn get_process(
        &self,
        Parameters(a): Parameters<GetProcessArgs>,
    ) -> Result<CallToolResult, McpError> {
        let pid = a.pid;
        self.analyze(a.source, move |r| {
            core::get_process(r, pid).ok_or_else(|| format!("no process with pid {pid}"))
        })
        .await
    }

    #[tool(
        description = "Processes in the capture (paginated): identity + a clipped command line. \
        For a quick overview prefer query_events with group_by=ProcessName; for one process's \
        full command line + modules use get_process(pid). Args: offset, limit (default 100)."
    )]
    async fn list_processes(
        &self,
        Parameters(a): Parameters<ListArgs>,
    ) -> Result<CallToolResult, McpError> {
        let (offset, limit) = (a.offset, a.limit);
        self.analyze(a.source, move |r| {
            let all = core::list_processes(r);
            let total = all.len();
            let truncated = offset.saturating_add(limit) < total;
            let mut page: Vec<core::ProcessNode> =
                all.into_iter().skip(offset).take(limit).collect();
            for n in &mut page {
                clip(&mut n.command_line, MAX_CMDLINE);
            }
            Ok(serde_json::json!({
                "total": total,
                "offset": offset,
                "returned": page.len(),
                "truncated": truncated,
                "processes": page,
            }))
        })
        .await
    }

    #[tool(
        description = "The parent->child process tree (pid + name structure). Use get_process(pid) \
        for a node's full command line + modules."
    )]
    async fn process_tree(
        &self,
        Parameters(a): Parameters<SourceOnly>,
    ) -> Result<CallToolResult, McpError> {
        self.analyze(a.source, move |r| {
            let tree = core::process_tree(r);
            Ok(serde_json::json!({
                "total_processes": count_tree(&tree),
                "tree": compact_tree(&tree),
            }))
        })
        .await
    }

    #[tool(description = "PML metadata: event count, computer name, OS build, process count.")]
    async fn pml_info(
        &self,
        Parameters(a): Parameters<SourceOnly>,
    ) -> Result<CallToolResult, McpError> {
        self.analyze(a.source, move |r| Ok(core::pml_info(r))).await
    }

    #[tool(
        description = "Export a (filtered) capture to PML / CSV / XML at out_path — for handing \
        the full data to another tool or archiving it, NOT for in-context analysis. For counts / \
        sums / top-N, use query_events with group_by (+ metric); don't dump CSV and compute \
        yourself."
    )]
    async fn export(
        &self,
        Parameters(a): Parameters<ExportArgs>,
    ) -> Result<CallToolResult, McpError> {
        let fmt = core::Format::parse(&a.format)
            .ok_or_else(|| McpError::invalid_params("format must be pml|csv|xml", None))?;
        let filter = parse_filter_opt(&a.filter)?;
        let (out, stacks) = (a.out_path, a.include_stacks);
        self.analyze(a.source, move |r| {
            core::export(r, fmt, filter.as_ref(), &[], stacks, &out)
                .map(|n| serde_json::json!({ "out": out, "events_written": n }))
        })
        .await
    }

    #[tool(
        description = "The filter vocabulary for query_events / capture filters: exact column \
        names (each with a description), structured extension fields (network endpoints — \
        RemoteAddress / RemotePort / NetBytes / …; file CreateFile — Disposition / OpenResult), \
        relations, and per-category operation names. \
        Call it instead of guessing names."
    )]
    async fn list_filter_columns(&self) -> Result<CallToolResult, McpError> {
        json(&core::filter_vocab())
    }

    #[tool(description = "Driver reachability + elevation + per-tool capability matrix.")]
    async fn driver_status(&self) -> Result<CallToolResult, McpError> {
        let running =
            tokio::task::spawn_blocking(|| procmon_sdk::MonitorController::connect().is_ok())
                .await
                .map_err(internal)?;
        json(&serde_json::json!({
            "elevated": crate::elevate::is_elevated(),
            "driver_running": running,
            "tools": crate::elevate::capability_matrix(),
            "note": "Live capture needs admin; when unelevated the capture tools auto-RunAs (UAC). PML analysis never needs elevation.",
        }))
    }
}

impl ProcmonServer {
    fn next_id(&self) -> String {
        format!("s{}", self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    fn store_finished(&self, pml_path: String) -> String {
        let id = self.next_id();
        self.sessions.lock().unwrap().insert(
            id.clone(),
            Session {
                pml_path,
                backend: None,
            },
        );
        id
    }

    /// Resolves a [`Source`] to a PML path (a finished session's output, or a
    /// direct file path) — exactly one must be given.
    fn resolve_source(&self, s: &Source) -> Result<String, McpError> {
        match (&s.session_id, &s.pml_path) {
            (Some(id), None) => {
                let map = self.sessions.lock().unwrap();
                let sess = map
                    .get(id)
                    .ok_or_else(|| McpError::invalid_params("unknown session_id", None))?;
                if sess.backend.is_some() {
                    return Err(McpError::invalid_params(
                        "session is still capturing; stop_capture first",
                        None,
                    ));
                }
                Ok(sess.pml_path.clone())
            }
            (None, Some(p)) => Ok(p.clone()),
            _ => Err(McpError::invalid_params(
                "provide exactly one of session_id or pml_path",
                None,
            )),
        }
    }

    /// Resolves the source, opens the PML, and runs a blocking analysis closure,
    /// returning its serde result as JSON content.
    async fn analyze<T, F>(&self, source: Source, f: F) -> Result<CallToolResult, McpError>
    where
        T: serde::Serialize + Send + 'static,
        F: FnOnce(&Arc<PmlReader>) -> Result<T, String> + Send + 'static,
    {
        let path = self.resolve_source(&source)?;
        let result = tokio::task::spawn_blocking(move || -> Result<T, String> {
            let reader = core::open_pml(&path).map_err(|e| e.to_string())?;
            f(&reader)
        })
        .await
        .map_err(internal)?
        .map_err(internal)?;
        json(&result)
    }
}

#[tool_handler]
impl rmcp::ServerHandler for ProcmonServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.protocol_version = ProtocolVersion::V_2025_06_18;
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info = Implementation::from_build_env();
        // No `instructions`: some clients (e.g. Codex) don't surface
        // InitializeResult.instructions to the model, so all guidance lives in the
        // tool descriptions / parameter schemas (which tools/list always delivers)
        // and in list_filter_columns' return value.
        info
    }
}

// --- helpers ----------------------------------------------------------------

/// Parses a comma-separated `group_by` spec into columns (`None`/empty = no
/// grouping), mapping an unknown column to `invalid_params`.
fn parse_columns(spec: Option<&str>) -> Result<Vec<core::Field>, McpError> {
    match spec {
        Some(s) => s
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|c| {
                core::parse_field(c)
                    .ok_or_else(|| McpError::invalid_params(format!("unknown column: {c}"), None))
            })
            .collect(),
        None => Ok(Vec::new()),
    }
}

/// Parses an optional filter expression (`None`/empty = match all) into an
/// [`core::Expr`], mapping a parse error to `invalid_params`.
fn parse_filter_opt(filter: &FilterExpr) -> Result<Option<core::Expr>, McpError> {
    match filter.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => core::parse_filter(s)
            .map(Some)
            .map_err(|e| McpError::invalid_params(e, None)),
        _ => Ok(None),
    }
}

/// Serializes `value` as pretty JSON in a single text content block.
/// A tool response above this many bytes would bloat the model's context window,
/// so [`json`] returns guidance instead of the payload; the list/tree tools
/// paginate or compact to stay well under it.
const MAX_RESPONSE_BYTES: usize = 48 * 1024;
/// Command lines are clipped to this in list views (a browser/Electron command
/// line is often 1–2 KB); the full line is available via `get_process`.
const MAX_CMDLINE: usize = 256;
/// Hard cap on `query_events` `limit` — the agent can page, but cannot request a
/// flood of rows in one call.
const MAX_QUERY_LIMIT: usize = 1000;
/// Per-field clip for an event's `path` / `detail` and a group's `value`: a
/// registry value or a long path can be KBs; the full value is in `get_event`.
const MAX_FIELD: usize = 512;

/// Truncates `s` in place to at most `max` bytes on a char boundary, marking it
/// with an ellipsis when clipped.
fn clip(s: &mut String, max: usize) {
    if s.len() <= max {
        return;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
    s.push('…');
}

/// Drops trailing rows from a `QueryResult` until it serializes within the
/// response budget, marking it `truncated`. This makes `query_events`
/// byte-aware: a detail-heavy or long-path page returns as many rows as fit
/// (plus `total_matched` / `truncated` so the agent can page or group) instead
/// of tripping the all-or-nothing [`json`] guard and returning nothing.
fn fit_within_budget(res: &mut core::QueryResult) {
    // group_by produces `groups`, a plain query produces `events`; only one is
    // ever populated, so trimming both vecs to the same target is safe.
    loop {
        let len = serde_json::to_string(res).map(|s| s.len()).unwrap_or(0);
        if len <= MAX_RESPONSE_BYTES {
            return;
        }
        let rows = res.events.len().max(res.groups.len());
        if rows <= 1 {
            // A single row over budget — let the json() guard handle that edge.
            return;
        }
        // Estimate the row count that fits (90% of budget for header/JSON
        // slack), always dropping at least one row so the loop makes progress.
        let keep = ((rows * MAX_RESPONSE_BYTES * 9) / (len * 10)).clamp(1, rows - 1);
        res.truncated = true;
        res.events.truncate(keep.min(res.events.len()));
        res.groups.truncate(keep.min(res.groups.len()));
    }
}

/// A compact process tree — pid / parent_pid / name / children only, so even a
/// few-hundred-process tree fits the response budget. Per-node detail (command
/// line, modules, user, …) is a `get_process(pid)` away.
fn compact_tree(nodes: &[core::ProcessNode]) -> Vec<serde_json::Value> {
    nodes
        .iter()
        .map(|n| {
            let mut o = serde_json::json!({
                "pid": n.pid,
                "parent_pid": n.parent_pid,
                "name": n.name,
            });
            if !n.children.is_empty() {
                o["children"] = serde_json::Value::Array(compact_tree(&n.children));
            }
            o
        })
        .collect()
}

fn count_tree(nodes: &[core::ProcessNode]) -> usize {
    nodes.iter().map(|n| 1 + count_tree(&n.children)).sum()
}

fn json<T: serde::Serialize>(value: &T) -> Result<CallToolResult, McpError> {
    // Compact (not pretty): the model reads this, so whitespace is pure overhead.
    let text = serde_json::to_string(value).map_err(internal)?;
    if text.len() > MAX_RESPONSE_BYTES {
        let msg = format!(
            "Response is too large for the model context ({} KB; cap {} KB). Narrow the \
             request instead of fetching everything:\n\
             • Processes overview → query_events with group_by=ProcessName.\n\
             • Events → add a filter and/or group_by (e.g. group_by=Path), or lower `limit`.\n\
             • One process's full detail → get_process(pid).\n\
             • The complete data → export(format=csv|xml|pml, out_path).",
            text.len() / 1024,
            MAX_RESPONSE_BYTES / 1024,
        );
        return Ok(CallToolResult::success(vec![Content::text(msg)]));
    }
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

/// Wraps any displayable error as an MCP internal error.
fn internal<E: std::fmt::Display>(e: E) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

/// A unique temp `.PML` path for a capture's output.
fn temp_pml() -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir()
        .join(format!("procmon-mcp-{}-{n}.pml", std::process::id()))
        .to_string_lossy()
        .into_owned()
}
