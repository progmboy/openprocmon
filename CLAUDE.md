# OpenProcMon - CLAUDE.md

## Project Overview

OpenProcMon is an open-source Process Monitor implementation for real-time monitoring of process, file and registry activity on Windows. Layered architecture: a kernel miniFilter driver captures events -> the SDK layer talks to the driver over a Filter Manager port -> the GUI layer presents the event data.

The SDK and GUI are a completed ground-up Rust rewrite of the original C++ implementation, which is kept under `cpp-backup/` for reference. The kernel driver is unchanged.

## Architecture

```
┌─────────────────────────────────────┐
│  GUI Layer                          │
│  ├── Rust GPUI GUI (crates/gui/)    │
│  └── C++ WTL GUI (cpp-backup/gui/)  │  [reference]
├─────────────────────────────────────┤
│  SDK Layer                          │
│  ├── Rust SDK (crates/sdk/)         │
│  └── C++ SDK (cpp-backup/sdk)       │  [reference]
├─────────────────────────────────────┤
│  Kernel Driver (kernel/)            │
│  └── miniFilter driver (complete)   │
└─────────────────────────────────────┘
```

## Directory Structure

```
openprocmon/
├── Cargo.toml              # Rust workspace root (members = ["crates/*"])
├── Cargo.lock
├── bin/                    # Prebuilt binaries (stock Process Monitor driver PROCMON24.SYS)
├── kernel/                 # Kernel driver (miniFilter, complete)
│   ├── logsdk.h           # ★ Core: kernel/user-mode interface (structs the Rust SDK mirrors)
│   ├── procmon.c          # Driver entry
│   ├── process.c/h        # Process monitoring callbacks
│   ├── file.c/h           # File-operation monitoring
│   └── reg.c/h            # Registry-operation monitoring
├── crates/                 # ★ Rust workspace
│   ├── sdk/               #   procmon-sdk: driver comms + event parsing + PML read/write
│   │   ├── benches/       #     baseline.rs: CPU+memory bench (see BASELINE.md)
│   │   └── src/           #     monitor/pipeline/parse/event/filter/pml/...
│   ├── gui/               #   procmon-gui: gpui-component GUI on top of the SDK
│   │   ├── locales/       #     en/zh strings (rust-i18n)
│   │   ├── themes/        #     procmon.json (light/dark ThemeConfig + palette)
│   │   └── src/           #     app/model/components/dialogs
│   └── example/           #   procmon-example: console SDK demo (capture/save/replay)
├── docs/
│   └── design/             # GUI design mockups (React/Figma prototypes, reference only)
│       └── gui-design-v1/
└── cpp-backup/             # Original C++ implementation (kept for reference)
    ├── CMakeLists.txt
    ├── cmake/             # CMake build modules
    ├── procmon.sln
    ├── gui/               # C++ WTL GUI (reference)
    │   ├── MainFrm.h      #   Main window
    │   ├── View.h         #   Event list view
    │   ├── dataview.cpp/h #   Data-view management
    │   ├── filterdlg.cpp/h#   Filter dialog
    │   ├── filtermgr.cpp/h#   Filter management
    │   └── propdlg.cpp/h  #   Properties dialog
    └── sdk/procmonsdk/    # C++ SDK (reference)
        ├── sdk.hpp        #   SDK entry point
        ├── kernelsdk.hpp  #   Kernel struct definitions (references logsdk.h)
        ├── monctl.cxx/hpp #   Monitor controller: driver connect, start/stop
        ├── eventmgr.cxx/hpp #  Event manager: queueing and dispatch
        ├── event.cxx/hpp  #   Event object
        ├── eventview.cxx/hpp # Event view interface
        ├── eventfactory.cxx/hpp # Event parsing factory
        ├── drvload.cxx/hpp#   Driver load/unload
        ├── procmgr.cxx/hpp#   Process management
        ├── process.cxx/hpp#   Process info
        ├── fileopt.cxx/hpp#   File-operation parsing
        ├── regopt.cxx/hpp #   Registry-operation parsing
        ├── procopt.cxx/hpp#   Process-operation parsing
        ├── buffer.cxx/hpp #   Thread-safe message buffer
        ├── thread.cxx/hpp #   Thread wrapper
        ├── strmaps.cxx/hpp#   String maps (enum value -> display name)
        ├── utils.cxx/hpp  #   Utility functions
        └── logger.cxx/hpp #   Logging framework
```

## Key Kernel Interface (logsdk.h)

The Rust SDK mirrors these kernel structures exactly (`#pragma pack(1)`):

### Communication constants
- Port: `\\ProcessMonitor24Port`
- Control codes: `CTLCODE_MONITOR=0`, `CTLCODE_THREADPOFILING=1`
- Monitor flags: `CTL_MONITOR_PROC_ON=0x01`, `CTL_MONITOR_FILE_ON=0x02`, `CTL_MONITOR_REG_ON=0x04`

### Core structures
- `LOG_ENTRY` (0x34 bytes, packed) — common header of every event: ProcessSeq, ThreadId, MonitorType, NotifyType, Time, Status, DataLength, …
- `PROCMON_MESSAGE_HEADER` — Filter Manager message header (pack(4))
- `FLTMSG_CONTROL_FLAGS` / `FLTMSG_CONTROL_THREADPROFILING` — control messages

### Event type enums
- `LOG_MONITOR_TYPE`: Process(1), Reg(2), File(3), Profiling(4)
- `LOG_PROCESS_NOTIFY_TYPE`: Init(0), Create(1), Exit(2), ThreadCreate(3), ThreadExit(4), ImageLoad(5), Start(7), Performance(8)
- `LOG_REG_NOTIFY_TYPE`: OpenKeyEx(0) ~ QueryKeySecurity(16)
- File operations: IRP MajorFunction + 20

### Event data structures
- Process: `LOG_PROCESSCREATE_INFO`, `LOG_PROCESSSTART_INFO`, `LOG_PROCESSBASIC_INFO`, `LOG_LOADIMAGE_INFO`
- File: `LOG_FILE_OPT`, `LOG_FILE_CREATE`
- Registry: `LOG_REG_CREATEOPENKEY`, `LOG_REG_SETVALUEKEY`, `LOG_REG_QUERYKEY`, `LOG_REG_ENUMERATEKEY`, …
- Performance: `LOG_THREAD_PROFILING_INFO`, `LOG_PROCESS_PROFILING_INFO`

### Event data access pattern
```c
// Event data follows LOG_ENTRY + FrameChain
EventData = (LOG_ENTRY + 1) + nFrameChainCounts * sizeof(PVOID)
EntrySize = DataLength + (sizeof(PVOID) * nFrameChainCounts) + sizeof(LOG_ENTRY)
```

## C++ SDK Core Components (reference implementation)

### CMonitorController (monctl.hpp)
The SDK's central controller:
- `Connect()` — connects to the kernel driver via FilterConnectCommunicationPort
- `SetMonitor(proc, file, reg)` — sets the monitor flags
- `Start()` / `Stop()` — starts/stops monitoring
- Internal receive thread `CRecvThread` and processing thread `COPtThread`

### CEventMgr (eventmgr.hpp)
Event manager; receives and dispatches events through a thread-safe queue

### CEventView (eventview.hpp)
Event view interface; parses LOG_ENTRY and exposes friendly accessors

### CDrvLoader (drvload.hpp)
Driver loader; installs/starts/stops the kernel driver via the SCM (Service Control Manager)

## Rust SDK Implementation Notes

### Key Windows API dependencies
- `FilterConnectCommunicationPort` — connect to the driver's communication port
- `FilterGetMessage` — receive kernel messages
- `FilterSendMessage` — send control messages
- Service Control Manager APIs — driver install/start/stop

### Rust crates in use
- `windows` crate — Windows API bindings
- `windows-sys` — lightweight Windows FFI (GUI-side helpers)

### Struct alignment notes
- `LOG_ENTRY` and the control messages use `#[repr(C, packed)]`
- `PROCMON_MESSAGE_HEADER` uses `#[repr(C, packed(4))]`
- Pointer-sized fields (`SIZE_T`, `PVOID`) depend on the target (x64 only)

### Performance
- Event ingestion is zero-copy: batches are `Arc`-shared and each `Event` holds a `Record` (buffer + offset) — see `crates/sdk/benches/BASELINE.md` for the tracked CPU/memory numbers and how to re-run the bench (`cargo bench -p procmon-sdk --bench baseline`)
- Filter evaluation is allocation-free (in-place ASCII case folding, per-evaluation column memo, numeric fast path)

## Rust GUI Notes

### Stack
- **gpui** — Zed's UI framework (git)
- **gpui-component** — component library for gpui (git)
- rust-i18n for en/zh localization; theme in `crates/gui/themes/procmon.json`

### Structure
- `app.rs` — `AppState` (shared state) + `AppView` (window root, drain task)
- `model/` — `EventBuffer` (retained ring buffer + filtered view), `CapturedEvent` (lazy display columns), sources (live SDK / PML)
- `components/` — menu bar, toolbar, monitor bar, event table (virtualized DataTable), detail panel, status bar
- `dialogs/` — filter/highlight, save, settings, process tree, analytics summaries
- The GUI consumes the SDK event stream over a channel, drained on a frame timer; filtering/search re-evaluate the retained buffer

## Build & Run

### C++ build (reference, cpp-backup/)
```bash
# Requires the WDK (Windows Driver Kit) and WTL
cd cpp-backup
cmake -B build
cmake --build build
```

### Rust build (workspace)
```bash
# At the repo root
cargo build                    # build all crates
cargo run -p procmon-gui       # run the GUI
cargo run -p procmon-example   # run the SDK console demo
```

### Driver installation (requires Administrator)
The driver is installed via the SCM; run the GUI elevated. With the default `embedded-driver` feature the GUI carries the driver image and drops it to `System32\Drivers` on demand.

## Conventions

- Rust code follows standard Rust style (rustfmt); CI enforces `cargo clippy --workspace --all-targets -- -D warnings`
- Kernel struct mappings must use `#[repr(C, packed)]` to match the wire layout exactly
- SDK error handling uses the `Result<T, E>` pattern
- GUI components follow gpui-component's `Render` / `RenderOnce` trait patterns
- Windows API calls live in `unsafe` blocks with safety comments
- Commit messages in English
