---
name: procmon
description: >
  Capture and analyze Windows process / file / registry / network activity with
  OpenProcMon (procmon-cli). Use when investigating what a program does — files
  it writes, registry keys it touches, network it makes, its process tree and
  call stacks — or when analyzing a Procmon-compatible .PML capture. Think
  "Process Monitor as Wireshark": capture writes a .PML; every analysis reads one.
---

# OpenProcMon (procmon-cli)

OpenProcMon is Process Monitor as Wireshark. A **capture** records process/file/
registry/network events to a Procmon-compatible **`.PML`** file; every analysis
command **reads a `.PML`** and prints JSON. The same `procmon-cli` binary also
serves these as MCP tools (`procmon-cli mcp`).

## Prerequisites

- 64-bit Windows. `procmon-cli` on PATH.
- **Live capture needs Administrator** (it loads a kernel driver). Analyzing an
  existing `.PML` needs no elevation. Check with `procmon-cli driver-status`.

## Workflow

1. **Capture** the target's behavior to a `.PML` (or open an existing one).
2. **Query** the `.PML` with filters to extract what you need.
3. **Drill in** to a specific event for its detail + call stack.

### 1. Capture

Capture a program and all its children for 10s, launching it first:

```bash
procmon-cli capture --name notepad.exe --launch "notepad.exe" --duration 10
```

- `--name X` (repeatable) targets a process by image name — matches **present and
  future** processes of that name. `--pid N` targets a specific instance.
- Child processes are followed by default; `--no-children` to disable.
- `--launch "<cmd>"` starts the program first so startup is captured.
- `--monitor process,file,registry,network` selects sources (default: all).
- `--duration <secs>` and `--max-mb <N>` bound the capture; `--out <path>` sets
  the `.PML` (default: a temp file). `--filter "<clause>"` narrows what's recorded
  (lossy — prefer narrowing at analysis time).
- Empty `--name`/`--pid` captures the **whole system**. For injection-suspected
  malware (the payload may run inside an existing process, not a child), capture
  system-wide and filter afterward.

`capture` prints the `pml_path`, a summary, and a sample of events. Re-analyze
that `.PML` with the commands below.

### 2. Query — the universal tool

`procmon-cli query --pml <file> [--filter ...]... [--group-by <col>] [--limit N]`

- **`--filter "Column relation value"`** — repeat for AND across columns. Use
  `procmon-cli vocab` for the **exact** column / relation / operation names — do
  not guess them.
- **`--group-by <Column>`** — return distinct values + counts (de-duplicated),
  instead of raw events. Use it to avoid flooding (e.g. distinct files, not 5000
  WriteFile events).
- Noise (NTFS metadata, monitoring tools, IRP/FastIO bookkeeping, the tool
  itself) is excluded by default; add `--no-noise` to see everything.

**Recipes** (the answers to common questions):

```bash
# What files did notepad.exe WRITE? → distinct file paths
procmon-cli query --pml cap.pml \
  --filter "Category is File System" --filter "ProcessName is notepad.exe" \
  --filter "Operation is WriteFile" --group-by Path

# Registry keys a process SET (persistence often lands under ...\Run)
procmon-cli query --pml cap.pml \
  --filter "Category is Registry" --filter "Operation is RegSetValue" \
  --filter "Path contains Run" --group-by Path

# Network endpoints a process talked to
procmon-cli query --pml cap.pml \
  --filter "Category is Network" --filter "ProcessName is app.exe" --group-by Path

# Operations that FAILED (probing / blocked)
procmon-cli query --pml cap.pml --filter "Result is not SUCCESS" --limit 50

# Which processes are busiest / by category
procmon-cli query --pml cap.pml --group-by ProcessName
procmon-cli summary --pml cap.pml
```

Repeated `--filter` flags AND together (every clause must match), so they must be
on **different** columns. To match any of several operations (WriteFile OR
DeleteFile), run one query per operation, or use the MCP `query_events` tool,
whose clause `values` is an array (OR within a clause).

### 3. Drill in

Each event row from `query` has a `seq`. Get its full detail + call stack:

```bash
procmon-cli get-event --pml cap.pml --seq 1234 --part event,process,stack
procmon-cli get-process --pml cap.pml --pid 4321   # identity + loaded modules
procmon-cli tree --pml cap.pml                       # parent→child process tree
```

### Export / metadata

```bash
procmon-cli export --pml cap.pml --format csv --out out.csv --filter "Category is Registry"
procmon-cli export --pml cap.pml --format xml --out out.xml --stacks
procmon-cli pml-info --pml cap.pml      # event count, computer, OS
```

## Limits to keep in mind

- **File *content* is not captured** (only path/offset/length). Registry value
  *data* IS captured (in an event's Detail). To see what was written into a
  dropped file, read the file separately.
- Analysis re-reads the `.PML` each call; that's fine for typical captures.
- Cross-event reasoning (e.g. dropped-then-executed) is yours to assemble from
  multiple queries — the tool returns the raw material.

## As an MCP server

`procmon-cli mcp` serves the same operations as MCP tools over stdio
(`capture`, `start_capture`/`stop_capture`, `query_events`, `get_event`,
`get_process`, `list_processes`, `process_tree`, `summary`, `export`,
`pml_info`, `list_filter_columns`, `driver_status`). Tools take a `source` of a
finished `session_id` or a `pml_path`. The server's `instructions` carry the same
recipes as this skill.
