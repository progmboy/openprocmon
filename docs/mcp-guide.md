# OpenProcMon MCP Server — Usage Guide

> 中文版：[mcp-guide_zh.md](./mcp-guide_zh.md)

`procmon-cli mcp` exposes OpenProcMon as an **MCP server over stdio**, so an AI
agent (Claude Code, Claude Desktop, Codex, Cursor, …) can capture and analyze
Windows process / file / registry / network activity for you.

**Model:** a *capture* writes a Procmon-compatible `.PML`; every *analysis* tool
reads one. You pass a `source` of either a `session_id` (a finished capture) or a
`pml_path` (any `.PML` on disk — including ones produced by the real Process
Monitor).

**Elevation:** live capture (`capture` / `start_capture`) needs **Administrator +
the kernel driver**. **PML analysis needs neither** — pointing the tools at an
existing `.PML` works from a normal, unelevated client.

---

## 1. Build the binary

```bash
cargo build -p procmon-cli --release
```

The binary is at `target/release/procmon-cli` (`procmon-cli.exe` on Windows).
Note its **absolute path** — most MCP clients need it unless the binary is on
`PATH`. Examples below use `C:\tools\openprocmon\procmon-cli.exe`; substitute your
own path.

The server command is always the same: **`procmon-cli mcp`** (stdio transport).

---

## 2. Connect your client

### Claude Code

One command (user scope, available in every project):

```bash
claude mcp add --transport stdio --scope user openprocmon -- C:\tools\openprocmon\procmon-cli.exe mcp
```

Or commit a project-scoped `.mcp.json` to the repo root:

```json
{
  "mcpServers": {
    "openprocmon": {
      "command": "C:\\tools\\openprocmon\\procmon-cli.exe",
      "args": ["mcp"]
    }
  }
}
```

Verify with `/mcp` inside Claude Code — it should list `openprocmon` as
connected. The server ships `instructions` and a `list_filter_columns` tool, so
the agent learns the filter vocabulary automatically (no prompt setup needed).

### Claude Desktop

Edit `claude_desktop_config.json`
(`%APPDATA%\Claude\claude_desktop_config.json` on Windows):

```json
{
  "mcpServers": {
    "openprocmon": {
      "command": "C:\\tools\\openprocmon\\procmon-cli.exe",
      "args": ["mcp"]
    }
  }
}
```

Restart Claude Desktop.

### Codex (OpenAI Codex CLI)

Add to `~/.codex/config.toml`:

```toml
[mcp_servers.openprocmon]
command = "C:\\tools\\openprocmon\\procmon-cli.exe"
args = ["mcp"]
```

### Other MCP clients (Cursor, Windsurf, Cline, Continue, …)

They all use the same shape — a stdio server with `command` + `args`:

```json
{ "command": "C:\\tools\\openprocmon\\procmon-cli.exe", "args": ["mcp"] }
```

### Capture vs. analyze, and elevation

- **Analysis only** (read a `.PML`): no Administrator needed. Run the client
  normally and pass `pml_path`.
- **Live capture**: the *server process* must run **elevated** and the driver
  must be installed. Either launch the whole MCP client from an elevated shell,
  or run `procmon-cli capture …` yourself from an elevated terminal and then hand
  the resulting `.PML` to the agent via `pml_path` (recommended — keeps the agent
  side unprivileged).

---

## 3. The tools

### Read tools (no side effects, no elevation)

Every read tool takes a `source` = `{ "pml_path": "…" }` **or**
`{ "session_id": "…" }`.

| Tool | What it returns | Key args |
|---|---|---|
| `pml_info` | metadata: event count, computer name, OS build, process count | `source` |
| `summary` | totals, by-category, top-N processes, rate sparkline | `source`, `top=10` |
| `list_processes` | flat list of every process (identity + command line) | `source` |
| `process_tree` | parent→child process tree | `source` |
| `get_process` | one process's identity + **loaded modules** | `source`, `pid` |
| **`query_events`** | the universal query — events page, or `group_by` distinct values + counts | `source`, `filter?`, `group_by?`, `exclude_noise=true`, `offset=0`, `limit=100`, `include_detail=false` |
| `get_event` | full detail of one event (event / process / stack) | `source`, `seq`, `parts=["event","process","stack"]` |
| `list_filter_columns` | exact column / operator / per-category operation names | — |
| `driver_status` | driver reachability + elevation + per-tool capability matrix | — |
| `capture_status` | whether a session is still capturing + bytes written | `session_id` |

### Write tools (side effects; live capture needs Administrator + driver)

| Tool | What it does | Key args |
|---|---|---|
| `capture` | one-shot: capture for `duration_seconds`, write a `.PML`, return an overview | `process_names[]`, `pids[]`, `include_children=true`, `launch?`, `monitors[]`, `filter?`, `duration_seconds=10`, `max_mb=512`, `sample=100` |
| `start_capture` | start a background capture session (stop it later) | same as `capture` minus `duration_seconds`/`sample` |
| `stop_capture` | stop a running session, finalize its `.PML` | `session_id` |
| `export` | export a (filtered) capture to **PML / CSV / XML** | `source`, `format`, `out_path`, `filter?`, `include_stacks=false` |

A `capture` with empty `process_names` captures the **whole system**. The capture
tool always excludes its own driver/IO noise.

---

## 4. The query language

`query_events` is the workhorse. The `filter` is one expression string of
`Column OP value` clauses joined with `&&` / `||` / `!` and parentheses. Quote
values that contain spaces, e.g. `"File System"`.

**Operators**

| | | | |
|---|---|---|---|
| `==` is | `!=` is not | `~` contains | `!~` excludes |
| `^=` begins with | `$=` ends with | `<` less than | `>` more than |
| `Column in (a, b, c)` — matches ANY of the listed values (OR) | | | |

**`group_by`** turns a flood into a summary: instead of a page of raw events you
get **distinct values + counts** of one column. Use it whenever a query could
return thousands of rows (e.g. "what files were written" → `group_by=Path`).

**Other args:** `exclude_noise` (default `true`, drops NTFS-metadata /
monitoring-tool / bookkeeping noise — set `false` for the raw stream),
`include_detail` (adds the expensive Detail column), `offset` / `limit` for
paging raw events (each carries a `seq` for `get_event`).

**Always call `list_filter_columns` for the exact names — don't guess them.**

### Recipes

```text
What files did X write?
  Category == "File System" && ProcessName == X
  && Operation in (WriteFile, SetEndOfFileInformationFile, DeleteFile)
  group_by = Path

Registry persistence?
  Category == Registry && Operation in (RegSetValue, RegCreateKey) && Path ~ Run
  group_by = Path

Network endpoints of X?
  Category == Network && ProcessName == X        group_by = Path

Failed operations?
  Result != SUCCESS
```

---

## 5. Worked example — triaging a malware `.PML`

This walks an actual defensive triage of a sample capture (`LogfileSample.PML`,
226k events, computer name `MALWARE`). The agent only reads the `.PML` — no
elevation.

### Step 1 — Orient: metadata + which processes exist

```text
pml_info { pml_path: "LogfileSample.PML" }
→ 226,224 events, computer "MALWARE", Win build 26100, 339 processes
```

339 processes is too many to read raw; summarize by activity instead of dumping:

```text
query_events { pml_path: "…", group_by: "ProcessName" }
→ svchost.exe 100714, decoded_assembly.exe 18786, msedge.exe 9831, …
  … random-named exes: BQZIL6PT3ZUBPA9013PEUPVV2R0.exe, ADI89CRSP7AT1ZIE5.exe
  … Maui.com, procdump.exe, powershell.exe, tasklist.exe, find.exe
```

`decoded_assembly.exe` (a runtime-decoded .NET payload) and the random-named
executables stand out as the suspicious set.

### Step 2 — Reconstruct the execution chain

```text
process_tree { pml_path: "…" }
```

The tree (read the suspicious branches) reveals:

```text
decoded_assembly.exe        (C:\Users\wobol\OneDrive\Desktop\…)
├─ BQZIL…exe  →  …tmp /SL5=…  →  …exe /VERYSILENT   (Inno Setup droppers, in %TEMP%)
│     ├─ cmd /C tasklist /FI "IMAGENAME eq avgui.exe" | find "avgui.exe"   (AV check: AVG)
│     ├─ cmd /C tasklist /FI "IMAGENAME eq opssvc.exe" | find "opssvc.exe" (AV check: Quick Heal)
│     └─ Maui.com  rabbitweed.a3x   (a renamed AutoIt3 interpreter running a .a3x script)
└─ ADI89…exe  →  (same Inno Setup pattern)  →  Maui.com  diurnals.a3x
```

Key insight from the command lines: `Maui.com` is **not** ransomware — it runs
`*.a3x` (compiled AutoIt scripts), i.e. a **renamed AutoIt3.exe loader**.
`powershell.exe → procdump.exe -ma -w decoded_assembly.exe` is the analyst's own
unpacking step, not the malware.

### Step 3 — What did the payload write (drops)?

```text
query_events {
  filter: "Category == \"File System\" && ProcessName == decoded_assembly.exe
           && Operation in (WriteFile, SetEndOfFileInformationFile, DeleteFile)",
  group_by: "Path"
}
→ %TEMP%\ADI89CRSP7AT1ZIE5.exe, %TEMP%\BQZIL6PT3ZUBPA9013PEUPVV2R0.exe
```

Only two files written, both executables in `%TEMP%` → a clean **dropper**.

### Step 4 — What did it touch? (opens ≫ reads)

A key analyst point: **accessed ≠ read**. Many stealers *open* a file to probe
existence (and read content via `CreateFileMapping`, which never shows
`ReadFile`). So look at `CreateFile`, not only `ReadFile`:

```text
# Only-read view (undercounts):
query_events { filter: "… && Operation == ReadFile", group_by: "Path" }
→ Edge\User Data\Default\Login Data, Edge\…\Local State, Web Data, History …

# Open view (the real target surface):
query_events { filter: "… && Operation == CreateFile && Path ~ \"wobol\"", group_by: "Path" }
→ crypto wallets: Bitcoin\wallets, Ethereum, Ledger Live, Coinomi, Atomic, Jaxx, Binance, …
  every Chromium browser's Local State: Chrome, Brave, Edge, CocCoc, Epic, 360Browser, …
  password managers: 1Password, NordPass, Authy
  email/FTP: The Bat!, Mailbird, eM Client, FileZilla, SmartFTP
  VPN/remote: NordVPN, ProtonVPN, OpenVPN, AnyDesk, Telegram
  cloud creds: .aws, .azure, gcloud
```

The verdict: a **broad-spectrum infostealer** — it *opens* dozens of credential /
wallet targets and *reads* the ones that exist on the box (here, Edge's saved
passwords + the `Local State` key that decrypts them + cookies + cards).

### Takeaways for your own triage

- `pml_info` → `group_by ProcessName` → `process_tree` is the fast orientation
  loop.
- Use `group_by` to avoid flooding; reach for raw events + `get_event` only to
  drill into one operation's stack.
- To judge "what sensitive data did it touch", look at **`CreateFile`
  (+`CreateFileMapping`)**, not just `ReadFile`.
- Procmon records *which* file was touched, never the bytes written — for "what
  did it exfiltrate", pivot to **`Category == Network`**.

---

## 6. Tips & troubleshooting

- **Large results** are written to a file by the client and must be read back in
  chunks — prefer `group_by` and tight `filter`s to keep outputs small.
- **`exclude_noise=true`** (default) hides NTFS-metadata / monitoring-tool /
  System bookkeeping; set `false` to see everything.
- **Driver / elevation problems** with capture: call `driver_status` — it reports
  reachability, elevation, and a per-tool capability matrix.
- **The same vocabulary** drives capture-time filters and analysis filters; the
  matching CLI is `procmon-cli vocab` / `procmon-cli --help`.
