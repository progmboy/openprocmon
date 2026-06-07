# OpenProcMon - CLAUDE.md

## Project Overview

OpenProcMon 是一个开源的 Process Monitor 实现，用于实时监控 Windows 系统的进程、文件和注册表操作。项目采用分层架构：内核驱动（miniFilter）捕获事件 -> SDK 层通过 Filter Manager Port 与驱动通信 -> GUI 层展示事件数据。

**当前任务目标：**
1. 将 C++ SDK（`cpp-backup/sdk/procmonsdk/`）使用 Rust 重写（目标目录 `crates/sdk/`）
2. 将 C++ WTL GUI 使用 Rust + gpui-component 重写（目标目录 `crates/gui/`），并接入 Rust SDK

## Architecture

```
┌─────────────────────────────────────┐
│  GUI Layer                          │
│  ├── C++ WTL GUI (cpp-backup/gui/)   │
│  └── Rust GPUI GUI (crates/gui/)[WIP]│
├─────────────────────────────────────┤
│  SDK Layer                          │
│  ├── C++ SDK (cpp-backup/sdk) [参考] │
│  └── Rust SDK (crates/sdk/)    [WIP]│
├─────────────────────────────────────┤
│  Kernel Driver (kernel/)            │
│  └── miniFilter driver (已完成)      │
└─────────────────────────────────────┘
```

## Directory Structure

```
openprocmon/
├── Cargo.toml              # Rust workspace 根 (members = ["crates/*"])
├── Cargo.lock
├── kernel/                 # 内核驱动 (miniFilter, 已完成)
│   ├── logsdk.h           # ★ 核心：内核-用户态接口定义（Rust SDK 需对齐的结构体）
│   ├── procmon.c          # 驱动入口
│   ├── process.c/h        # 进程监控回调
│   ├── file.c/h           # 文件操作监控
│   └── reg.c/h            # 注册表操作监控
├── crates/                 # ★ Rust 工作区 (待实现)
│   ├── sdk/               #   procmon-sdk：驱动通信 + 事件解析
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── gui/               #   procmon-gui：gpui-component GUI，接入 SDK
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   └── example/           #   procmon-example：SDK 用法示例 (console)
│       ├── Cargo.toml
│       └── src/main.rs
├── docs/
│   └── design/             # GUI 设计稿 (React/Figma 原型，仅供参考)
│       ├── gui-design-v1/
│       └── gui-design-v2/
└── cpp-backup/             # 原始 C++ 参考实现（驱动除外，保留供对照）
    ├── CMakeLists.txt
    ├── cmake/             # CMake 构建模块
    ├── procmon.sln
    ├── gui/               # C++ WTL GUI (参考)
    │   ├── MainFrm.h      #   主窗口
    │   ├── View.h         #   事件列表视图
    │   ├── dataview.cpp/h #   数据视图管理
    │   ├── filterdlg.cpp/h#   过滤器对话框
    │   ├── filtermgr.cpp/h#   过滤器管理
    │   └── propdlg.cpp/h  #   属性对话框
    └── sdk/procmonsdk/    # C++ SDK (参考)
        ├── sdk.hpp        #   SDK 总入口
        ├── kernelsdk.hpp  #   内核结构体定义 (引用 logsdk.h)
        ├── monctl.cxx/hpp #   监控控制器：连接驱动、启停监控
        ├── eventmgr.cxx/hpp #  事件管理器：事件队列与分发
        ├── event.cxx/hpp  #   事件对象
        ├── eventview.cxx/hpp # 事件视图接口
        ├── eventfactory.cxx/hpp # 事件解析工厂
        ├── drvload.cxx/hpp#   驱动加载/卸载
        ├── procmgr.cxx/hpp#   进程管理
        ├── process.cxx/hpp#   进程信息
        ├── fileopt.cxx/hpp#   文件操作解析
        ├── regopt.cxx/hpp #   注册表操作解析
        ├── procopt.cxx/hpp#   进程操作解析
        ├── buffer.cxx/hpp #   线程安全消息缓冲
        ├── thread.cxx/hpp #   线程封装
        ├── strmaps.cxx/hpp#   字符串映射（枚举值->显示名）
        ├── utils.cxx/hpp  #   工具函数
        └── logger.cxx/hpp #   日志框架
```

> 注：`crates/{sdk,gui}` 的源码为占位骨架——原 Rust 实现在一次目录整理中丢失，需参考
> `cpp-backup/` 与 `docs/design/` 从头重写。

## Key Kernel Interface (logsdk.h)

Rust SDK 必须精确对齐以下内核结构体（`#pragma pack(1)`）：

### 通信常量
- Port: `\\OpenProcessMonitor24Port`
- 控制码: `CTLCODE_MONITOR=0`, `CTLCODE_THREADPOFILING=1`
- 监控标志: `CTL_MONITOR_PROC_ON=0x01`, `CTL_MONITOR_FILE_ON=0x02`, `CTL_MONITOR_REG_ON=0x04`

### 核心结构体
- `LOG_ENTRY` (0x34 bytes, packed) — 所有事件的通用头部，包含 ProcessSeq, ThreadId, MonitorType, NotifyType, Time, Status, DataLength 等
- `PROCMON_MESSAGE_HEADER` — Filter Manager 消息头 (pack(4))
- `FLTMSG_CONTROL_FLAGS` / `FLTMSG_CONTROL_THREADPROFILING` — 控制消息

### 事件类型枚举
- `LOG_MONITOR_TYPE`: Process(1), Reg(2), File(3), Profiling(4)
- `LOG_PROCESS_NOTIFY_TYPE`: Init(0), Create(1), Exit(2), ThreadCreate(3), ThreadExit(4), ImageLoad(5), Start(7), Performance(8)
- `LOG_REG_NOTIFY_TYPE`: OpenKeyEx(0) ~ QueryKeySecurity(16)
- 文件操作: IRP MajorFunction + 20

### 事件数据结构
- 进程: `LOG_PROCESSCREATE_INFO`, `LOG_PROCESSSTART_INFO`, `LOG_PROCESSBASIC_INFO`, `LOG_LOADIMAGE_INFO`
- 文件: `LOG_FILE_OPT`, `LOG_FILE_CREATE`
- 注册表: `LOG_REG_CREATEOPENKEY`, `LOG_REG_SETVALUEKEY`, `LOG_REG_QUERYKEY`, `LOG_REG_ENUMERATEKEY` 等
- 性能: `LOG_THREAD_PROFILING_INFO`, `LOG_PROCESS_PROFILING_INFO`

### 事件数据访问模式
```c
// 事件数据紧跟在 LOG_ENTRY + FrameChain 之后
EventData = (LOG_ENTRY + 1) + nFrameChainCounts * sizeof(PVOID)
EntrySize = DataLength + (sizeof(PVOID) * nFrameChainCounts) + sizeof(LOG_ENTRY)
```

## C++ SDK Core Components (参考实现)

### CMonitorController (monctl.hpp)
SDK 的核心控制器，负责：
- `Connect()` — 通过 FilterConnectCommunicationPort 连接内核驱动
- `SetMonitor(proc, file, reg)` — 设置监控标志
- `Start()` / `Stop()` — 启停监控
- 内部有接收线程 `CRecvThread` 和处理线程 `COPtThread`

### CEventMgr (eventmgr.hpp)
事件管理器，使用线程安全队列接收和分发事件

### CEventView (eventview.hpp)
事件视图接口，解析 LOG_ENTRY 并提供友好的访问方法

### CDrvLoader (drvload.hpp)
驱动加载器，通过 SCM (Service Control Manager) 安装/启动/停止内核驱动

## Rust SDK Implementation Notes

### 关键 Windows API 依赖
- `FilterConnectCommunicationPort` — 连接驱动通信端口
- `FilterGetMessage` — 接收内核消息
- `FilterSendMessage` — 发送控制消息
- Service Control Manager APIs — 驱动安装/启停

### 推荐 Rust crate
- `windows` crate — Windows API 绑定
- `windows-sys` — 轻量级 Windows FFI

### 结构体对齐注意
- `LOG_ENTRY` 和控制消息使用 `#[repr(C, packed)]`
- `PROCMON_MESSAGE_HEADER` 使用 `#[repr(C, packed(4))]`
- 指针大小字段 (`SIZE_T`, `PVOID`) 取决于目标平台 (x64)

## Rust GUI Notes

### 技术栈
- **gpui** `0.2.2` — Zed 编辑器的 UI 框架
- **gpui-component** `0.5.1` — gpui 的组件库
- 当前状态：`crates/gui` 为占位骨架，需从头重写（原 demo 源码已丢失）

### 重建参考
- UI 设计稿：`docs/design/gui-design-v1`、`gui-design-v2`（React/Figma 原型）
- 交互参考：`cpp-backup/gui/`（C++ WTL 实现）

### 预期组件结构（重建目标）
- `AppView` — 主应用，包含过滤/搜索/快捷键
- `ProcessTable` — 事件列表表格，按类别着色
- `DetailPanel` — 事件详情面板
- `MenuBar` / `Toolbar` / `StatusBar` — 菜单/工具栏/状态栏
- 数据模型与 SDK 事件类型对齐，从 SDK 事件流实时接收，提供监控启停/过滤器配置

## Build & Run

### C++ 构建 (参考, cpp-backup/)
```bash
# 需要 WDK (Windows Driver Kit) 和 WTL
cd cpp-backup
cmake -B build
cmake --build build
```

### Rust 构建 (workspace)
```bash
# 在仓库根目录
cargo build                    # 构建所有 crate
cargo run -p procmon-gui       # 运行 GUI
cargo run -p procmon-example   # 运行 SDK 示例
```

### 驱动安装 (需要管理员权限)
驱动通过 SCM 安装，GUI 需要以管理员身份运行。

## Conventions

- Rust 代码遵循标准 Rust 风格 (rustfmt)
- 内核结构体映射必须使用 `#[repr(C, packed)]` 确保内存布局一致
- SDK 错误处理使用 `Result<T, E>` 模式
- GUI 组件遵循 gpui-component 的 `Render` / `RenderOnce` trait 模式
- Windows API 调用使用 `unsafe` 块并添加安全性注释
