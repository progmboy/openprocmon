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

/// A capture session: its output PML path, and the live handle while running.
struct Session {
    pml_path: String,
    running: Option<core::CaptureSession>,
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

/// A Wireshark-style filter expression (see `list_filter_columns` for the
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
    /// Optional capture-time filter expression (Wireshark-style).
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
    /// Filter expression (Wireshark-style). See list_filter_columns.
    #[serde(default)]
    filter: FilterExpr,
    #[serde(default)]
    group_by: Option<String>,
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
        let spec = core::TargetSpec {
            process_names: a.process_names,
            pids: a.pids,
            include_children: a.include_children,
            launch: a
                .launch
                .map(|s| s.split_whitespace().map(str::to_string).collect()),
            monitors: core::parse_monitors(&a.monitors),
            filter: parse_filter_opt(&a.filter)?,
        };
        let limits = core::CaptureLimits {
            max_bytes: a.max_mb * 1024 * 1024,
            duration: Some(std::time::Duration::from_secs(a.duration_seconds)),
        };
        let out = temp_pml();
        let out2 = out.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            core::capture(crate::make_loader(), spec, limits, &out2)
                .map_err(|e| crate::loader::describe_error(&e))
        })
        .await
        .map_err(internal)?
        .map_err(internal)?;

        let id = self.store_finished(outcome.pml_path.clone());
        let sample = a.sample;
        let pml = outcome.pml_path.clone();
        let body = tokio::task::spawn_blocking(move || -> Result<serde_json::Value, String> {
            let reader = core::open_pml(&pml).map_err(|e| e.to_string())?;
            let summary = core::summary(&reader, 10);
            let events = core::query(
                &reader,
                None,
                &core::default_noise(),
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
            "pml_path": outcome.pml_path,
            "events_written": outcome.events_written,
            "stopped_reason": outcome.stopped_reason,
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
        let spec = core::TargetSpec {
            process_names: a.process_names,
            pids: a.pids,
            include_children: a.include_children,
            launch: a
                .launch
                .map(|s| s.split_whitespace().map(str::to_string).collect()),
            monitors: core::parse_monitors(&a.monitors),
            filter: parse_filter_opt(&a.filter)?,
        };
        let limits = core::CaptureLimits {
            max_bytes: a.max_mb * 1024 * 1024,
            duration: None,
        };
        let out = temp_pml();
        let out2 = out.clone();
        let session = tokio::task::spawn_blocking(move || {
            core::CaptureSession::start(crate::make_loader(), spec, limits, &out2)
                .map_err(|e| crate::loader::describe_error(&e))
        })
        .await
        .map_err(internal)?
        .map_err(internal)?;

        let id = self.next_id();
        self.sessions.lock().unwrap().insert(
            id.clone(),
            Session {
                pml_path: out.clone(),
                running: Some(session),
            },
        );
        json(&serde_json::json!({ "session_id": id, "pml_path": out }))
    }

    #[tool(description = "Stop a running capture (finalizes its PML) and return the outcome.")]
    async fn stop_capture(
        &self,
        Parameters(a): Parameters<SessionId>,
    ) -> Result<CallToolResult, McpError> {
        let running = {
            let mut map = self.sessions.lock().unwrap();
            let s = map
                .get_mut(&a.session_id)
                .ok_or_else(|| McpError::invalid_params("unknown session_id", None))?;
            s.running.take()
        };
        let Some(session) = running else {
            return json(&serde_json::json!({ "stopped": false, "note": "already stopped" }));
        };
        let outcome = tokio::task::spawn_blocking(move || session.stop())
            .await
            .map_err(internal)?
            .map_err(internal)?;
        json(&serde_json::json!({
            "stopped": true,
            "events_written": outcome.events_written,
            "stopped_reason": outcome.stopped_reason,
            "pml_path": outcome.pml_path,
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
        let running = s.running.as_ref().is_some_and(|c| c.is_running());
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
        description = "Query events with a Wireshark-style filter expression. The `filter` is one \
        string of `Column OP value` clauses joined by && / || / ! and parens, e.g. \
        'Category == \"File System\" && Operation == WriteFile'. Operators: == != ~ (contains) \
        !~ (excludes) ^= (begins) $= (ends) < > and `Column in (a, b)` for OR over values. With \
        group_by, returns distinct values + counts of that column (e.g. files written: \
        'Category == \"File System\" && Operation == WriteFile' group_by=Path). Without group_by, \
        a page of events (each with a seq for get_event). exclude_noise (default true) drops \
        NTFS-metadata / monitoring-tool / bookkeeping noise. Call list_filter_columns for the \
        exact column / operator / operation names."
    )]
    async fn query_events(
        &self,
        Parameters(a): Parameters<QueryArgs>,
    ) -> Result<CallToolResult, McpError> {
        let filter = parse_filter_opt(&a.filter)?;
        let group =
            match a.group_by.as_deref() {
                Some(c) => Some(core::parse_column(c).ok_or_else(|| {
                    McpError::invalid_params(format!("unknown column: {c}"), None)
                })?),
                None => None,
            };
        let (offset, limit, detail, exclude_noise) =
            (a.offset, a.limit, a.include_detail, a.exclude_noise);
        self.analyze(a.source, move |r| {
            let noise = if exclude_noise {
                core::default_noise()
            } else {
                Vec::new()
            };
            Ok(core::query(
                r,
                filter.as_ref(),
                &noise,
                group,
                offset,
                limit,
                detail,
            ))
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
        description = "All processes seen in the capture (flat list with identity + command line)."
    )]
    async fn list_processes(
        &self,
        Parameters(a): Parameters<SourceOnly>,
    ) -> Result<CallToolResult, McpError> {
        self.analyze(a.source, move |r| Ok(core::list_processes(r)))
            .await
    }

    #[tool(description = "The parent->child process tree of the capture.")]
    async fn process_tree(
        &self,
        Parameters(a): Parameters<SourceOnly>,
    ) -> Result<CallToolResult, McpError> {
        self.analyze(a.source, move |r| Ok(core::process_tree(r)))
            .await
    }

    #[tool(description = "PML metadata: event count, computer name, OS build, process count.")]
    async fn pml_info(
        &self,
        Parameters(a): Parameters<SourceOnly>,
    ) -> Result<CallToolResult, McpError> {
        self.analyze(a.source, move |r| Ok(core::pml_info(r))).await
    }

    #[tool(description = "Export a (filtered) capture to PML / CSV / XML at out_path.")]
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
        description = "The filter vocabulary: exact column names, relations, and per-category \
        operation names to use in query_events / capture filters."
    )]
    async fn list_filter_columns(&self) -> Result<CallToolResult, McpError> {
        json(&core::filter_vocab())
    }

    #[tool(description = "Whether the driver is reachable (capture needs Administrator).")]
    async fn driver_status(&self) -> Result<CallToolResult, McpError> {
        let running =
            tokio::task::spawn_blocking(|| procmon_sdk::MonitorController::connect().is_ok())
                .await
                .map_err(internal)?;
        json(&serde_json::json!({
            "driver_running": running,
            "note": "Live capture requires Administrator; PML analysis does not.",
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
                running: None,
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
                if sess.running.as_ref().is_some_and(|c| c.is_running()) {
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
        info.instructions = Some(INSTRUCTIONS.to_string());
        info
    }
}

const INSTRUCTIONS: &str = r#"
OpenProcMon — Process Monitor as Wireshark. capture writes a .PML; every analysis tool reads
one. Typical flow: capture (or start_capture/stop_capture) -> query_events with a filter ->
get_event for a stack. Pass a `source` of either session_id (a finished capture) or pml_path.

query_events is the universal primitive. `filter` is a Wireshark-style expression string:
`Column OP value` clauses joined with && / || / ! and parentheses. Quote values with spaces or
special characters. Operators:
  ==  is            !=  is not          ~   contains       !~  excludes
  ^=  begins with   $=  ends with       <   less than      >   more than
  Column in (a, b, c)   matches ANY of the listed values (OR)
group_by returns distinct values + counts of a column (use it to avoid flooding). Recipes:
- What files did X write?  'Category == "File System" && ProcessName == X
                            && Operation in (WriteFile, SetEndOfFileInformationFile, DeleteFile)'
                            group_by=Path
- Registry persistence:    'Category == Registry && Operation in (RegSetValue, RegCreateKey)
                            && Path ~ Run'   group_by=Path
- Network endpoints of X:  'Category == Network && ProcessName == X'   group_by=Path
- Failed operations:       'Result != SUCCESS'

Call list_filter_columns for the exact column / operator / operation names — do not guess them.
Live capture (capture/start_capture) requires Administrator; PML analysis does not."#;

// --- helpers ----------------------------------------------------------------

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
fn json<T: serde::Serialize>(value: &T) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string_pretty(value).map_err(internal)?;
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
