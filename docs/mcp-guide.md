# OpenProcMon MCP Server — Usage Guide

> 中文版：[mcp-guide_zh.md](./mcp-guide_zh.md)

`procmon-cli mcp` exposes OpenProcMon as an **MCP server over stdio**, so an AI
agent (Claude Code, Claude Desktop, Codex, Cursor, …) can capture and analyze
Windows process / file / registry / network activity for you — **in plain
English**. You ask questions; the agent picks the tools.

**Model:** a *capture* writes a Procmon-compatible `.PML`; every *analysis* tool
reads one. The agent works against either a finished capture or any `.PML` on disk
(including ones produced by the real Process Monitor).

**Elevation:** live capture needs **Administrator + the kernel driver**.
**Analyzing an existing `.PML` needs neither** — it works from a normal,
unelevated client.

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

> The blocks below are configuration you copy-paste into your client's config —
> the only place you touch any JSON/TOML. Everything *after* setup is plain
> English.

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
connected.

### Claude Desktop

Edit `claude_desktop_config.json`
(`%APPDATA%\Claude\claude_desktop_config.json` on Windows), then restart:

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

### Codex (OpenAI Codex CLI)

Add to `~/.codex/config.toml`:

```toml
[mcp_servers.openprocmon]
command = "C:\\tools\\openprocmon\\procmon-cli.exe"
args = ["mcp"]
```

### Other MCP clients (Cursor, Windsurf, Cline, Continue, …)

Same shape — a stdio server with `command` + `args`:

```json
{ "command": "C:\\tools\\openprocmon\\procmon-cli.exe", "args": ["mcp"] }
```

### Capture vs. analyze, and elevation

- **Analysis only** (read a `.PML`): no Administrator needed — just run your
  client normally and tell the agent which `.PML` to open.
- **Live capture**: the *server process* must run **elevated** and the driver must
  be installed. Either launch the whole client from an elevated shell, or run
  `procmon-cli capture …` yourself from an elevated terminal and then hand the
  resulting `.PML` to the agent (recommended — keeps the agent side unprivileged).

---

## 3. What the agent can do (the tools)

You don't call these by hand — the agent does. This is just so you know what to
ask for.

**Analysis (read-only, no elevation):**

- **`query_events`** — the workhorse. Find events, or summarize a column into
  distinct values + counts (e.g. "what files were written").
- **`process_tree`** — the parent→child spawn tree (structure + names).
- **`list_processes`** — processes with identity + a clipped command line
  (paginated; the full command line is in `get_process`).
- **`get_process`** — one process's identity + loaded modules.
- **`get_event`** — one event's full detail, including its call stack.
- **`summary`** / **`pml_info`** — quick overview / metadata (event count,
  computer, OS, process count).
- **`list_filter_columns`** — the exact filter vocabulary (the agent uses this so
  it doesn't guess).

**Capture (needs Administrator + driver):**

- **`capture`** — one-shot: monitor target processes for a few seconds, write a
  `.PML`, return an overview.
- **`start_capture`** / **`stop_capture`** — a background session you start and
  stop.
- **`export`** — write a (filtered) capture out as PML / CSV / XML.
- **`driver_status`** / **`capture_status`** — is the driver/elevation ready; is a
  session still running.

---

## 4. How questions become filters (optional reference)

Over MCP you just ask in plain English and the agent figures out the filter. You
**don't** need this section to use the tool. It's here for the curious, and
because the same syntax drives the `procmon-cli` command line.

Under the hood the agent builds a filter expression — `Column OP value` clauses
joined with `&&` / `||` / `!` — for example:

| Your question | The filter the agent builds |
|---|---|
| "What files did `app.exe` write?" | `Category == "File System" && ProcessName == app.exe && Operation in (WriteFile, SetEndOfFileInformationFile, DeleteFile)`, grouped by Path |
| "Any registry persistence?" | `Category == Registry && Operation in (RegSetValue, RegCreateKey) && Path ~ Run`, grouped by Path |
| "What did it connect to?" | `Category == Network && ProcessName == app.exe`, grouped by Path |
| "Show me the failures." | `Result != SUCCESS` |

By default the agent hides bookkeeping noise (NTFS metadata, the monitoring tools
themselves) — if you want *everything*, just say so ("include the noise").

---

## 5. Worked example — triaging a malware `.PML`

> **Try it yourself.** The sample below is **`Logfile.pml`** from
> [BitravenS/Cloudflare-malware](https://github.com/BitravenS/Cloudflare-malware).
> Download that `.PML`, save it locally, and ask the agent to analyze it

A real triage conversation:

> **You —** *Open `Logfile.pml` and tell me what's in it.*
>
> **Agent —** 226k events, computer name **`MALWARE`**, 339 processes. The
> busiest / oddest are `svchost.exe`, **`decoded_assembly.exe`** (a runtime-decoded
> .NET payload), `msedge.exe`, plus random-named executables (`BQZIL…exe`,
> `ADI89…exe`) and `Maui.com`.

> **You —** *Which ones look suspicious, and how were they launched?*
>
> **Agent —** `decoded_assembly.exe` (on the Desktop) drops two random-named **Inno
> Setup** installers into `%TEMP%`; each one checks for **AVG / Quick Heal**
> antivirus, then runs **`Maui.com`** — a *renamed AutoIt interpreter* — on a
> `.a3x` script. (The `procdump … decoded_assembly.exe` you'll also notice is the
> analyst's own unpacking step, not the malware.)

> **You —** *What files did `decoded_assembly.exe` write?*
>
> **Agent —** Exactly two — both executables dropped into `%TEMP%` (the two
> installers above). A clean **dropper**.

> **You —** *What sensitive files did it touch? Don't only count reads — opens
> count too.*
>
> **Agent —** Looking at file **opens** (not just reads), it probes dozens of
> credential / wallet targets: crypto wallets (Bitcoin, Ethereum, Ledger, Coinomi,
> Atomic, Jaxx…), **every Chromium browser's `Local State`**, password managers
> (1Password, NordPass, Authy), email/FTP clients (The Bat!, Mailbird, FileZilla…),
> VPNs, AnyDesk, Telegram, and cloud creds (`.aws`, `.azure`, `gcloud`) — and it
> **reads** the ones that actually exist on this machine (Edge's saved passwords +
> the `Local State` key that decrypts them + cookies + cards). A **broad-spectrum
> infostealer**.

Every line above was a natural-language question; the agent translated each into
the right query under the hood.

**One thing worth telling the agent explicitly:** *"check opens (`CreateFile`),
not just reads."* Stealers often *open* a file to probe it and read its content via
memory-mapping, which never shows up as a read — so a reads-only view badly
undercounts what was touched. And remember Procmon records *which* file was
touched, never the bytes; for "what did it actually steal/send", ask about the
**network** activity instead.

---

## 6. Tips & troubleshooting

- **Responses are size-capped server-side.** Every tool result is bounded (~48 KB);
  if a request would exceed that, the server returns a short "narrow it" hint
  (use `group_by` / a filter / `get_process`) instead of a wall of data — so even
  a huge capture can't blow up your context. `list_processes` is paginated and
  `process_tree` returns just the pid/name structure for the same reason.
- **Ask for summaries, not dumps.** "Group the writes by path", "which processes
  are busiest" — the agent returns counts instead of thousands of raw rows.
- **Capture not working?** Ask the agent to check `driver_status` — it reports
  driver reachability, whether you're elevated, and what each tool can do.
- **Same vocabulary on the CLI.** Everything here also works from the terminal:
  `procmon-cli --help`, `procmon-cli vocab`, `procmon-cli query --pml … --filter …`.
