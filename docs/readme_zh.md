<div align="center">
  <img src="logo.png" alt="OpenProcMon" width="140">
  <p><a href="../README.md">English</a> · <strong>中文</strong></p>
</div>

# OpenProcMon
一个开源的 Windows [Process Monitor](https://learn.microsoft.com/en-us/sysinternals/downloads/procmon) 实现：内核 miniFilter 驱动实时捕获进程、文件、注册表和网络活动，Rust SDK 与驱动通信并解析事件流，Rust/GPUI 桌面 GUI 负责展示。

> **SDK 与 GUI 的全新 Rust 重写版本。** 内核驱动保持不变，原始 C++ 实现保留在 [`cpp-backup/`](../cpp-backup/) 中供参考。Rust SDK 与原版 Process Monitor 驱动二进制兼容，并可读写 Procmon 的 `.PML` 日志。

![主窗口](snapshots/main.png)

## 目录

- [架构](#架构)
- [仓库结构](#仓库结构)
- [功能特性](#功能特性)
- [截图](#截图)
- [构建](#构建)
- [运行](#运行)
- [SDK 示例](#sdk-示例)
- [PML 日志](#pml-日志)
- [驱动兼容性](#驱动兼容性)
- [已知问题](#已知问题)
- [状态与路线图](#状态与路线图)
- [许可证](#许可证)

## 架构

```
┌──────────────────────────────────────────────┐
│  GUI            crates/gui  (Rust + GPUI)    │  事件表格 · 详情面板 ·
│                                              │  过滤/高亮 · 进程树
├──────────────────────────────────────────────┤
│  SDK            crates/sdk  (Rust)           │  驱动端口 · 事件解析 ·
│                                              │  进程跟踪 · PML 读写
├──────────────────────────────────────────────┤
│  内核驱动        kernel/     (C, miniFilter)  │  进程/文件/注册表回调
└──────────────────────────────────────────────┘
```

驱动与 SDK 通过 Filter Manager 通信端口交互；内核/用户态契约定义在
[`kernel/logsdk.h`](../kernel/logsdk.h) 中，Rust SDK 使用 `#[repr(C, packed)]`
结构体精确对齐它。

## 仓库结构

```text
openprocmon/
├── bin/          # 预编译二进制（如原版 Process Monitor 驱动 PROCMON24.SYS）
├── cpp-backup/   # 原始 C++ SDK + WTL GUI，保留供参考
├── crates/       # Rust 工作区
│   ├── sdk/      #   procmon-sdk — 驱动通信、事件解析、PML 读写、符号
│   ├── gui/      #   procmon-gui — GPUI 桌面应用（实时捕获 + .PML 查看）
│   └── example/  #   procmon-example — 控制台 SDK 演示（捕获 / 保存 / 回放）
├── docs/         # 设计文档、logo 和截图
└── kernel/       # miniFilter 驱动（C，使用 WDK 构建）
```

## 功能特性

- 实时监控 **进程、文件系统、注册表和网络** 活动。
- 按进程名、PID、操作、路径、结果或类别进行实时 **过滤** 和 **高亮**。
- **进程树**，以及针对进程、文件、注册表、网络和交叉引用的 **活动汇总**。
- 带每帧模块解析的 **调用栈** 视图。
- 读写 **Procmon 兼容的 `.PML`** 日志——用 OpenProcMon 捕获并在 Sysinternals Process Monitor 中打开，反之亦然。
- **功能完整的 Rust SDK**——以编程方式驱动一切：加载/连接驱动、选择监控内容、推送过滤器、消费解析后的事件流。GUI 只是其中一个消费者（见 [SDK 示例](#sdk-示例)）。
- **现代化、GPU 加速的 UI**（GPUI），设计简洁——支持浅色/深色主题和中英文本地化。

## 截图

**进程活动汇总** — 按进程统计事件数量并按类别细分。

![进程活动汇总](snapshots/active_process.png)

**设置** — 符号/dbghelp 路径、历史记录上限、高亮颜色、主题和语言。

![设置](snapshots/settings.png)

## 构建

### 前置要求

- 较新的 **Rust** 工具链（stable）——见 [rustup](https://rustup.rs/)。
- **Windows**（SDK 和 GUI 使用 Win32 API）。
- 仅构建内核驱动时需要：[Windows 驱动程序工具包（WDK）](https://learn.microsoft.com/en-us/windows-hardware/drivers/download-the-wdk)。

### Rust 工作区

```bash
# 构建全部（GUI、SDK、示例）
cargo build

# GUI 的 release 构建
cargo build -p procmon-gui --release
```

### 内核驱动

驱动使用 WDK 构建（见 `kernel/`）。构建完成后，需要对其进行测试签名，或在加载前启用测试签名 / 禁用驱动签名强制。

## 运行

```bash
# 用真实内核驱动运行 GUI（以管理员身份运行）
cargo run -p procmon-gui
```

当 `procmon.sys` 与可执行文件位于同一目录时，GUI 会按需加载并启动驱动；捕获真实系统活动需要管理员权限。

## SDK 示例

实时捕获和离线 `.PML` 读取都通过同一个 `EventSource` 流转，因此消费循环完全相同——只是创建 source 的方式不同。

**实时捕获** — 连接驱动（按需加载 `.sys`）：

```rust
use procmon_sdk::{
    Action, Column, DriverLoader, EventSource, FilterSet, MonitorFlags, Relation, Rule,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = EventSource::from_driver(
        DriverLoader::new("OpenProcmon24", "procmon.sys"),
        MonitorFlags::PROCESS | MonitorFlags::FILE | MonitorFlags::REGISTRY,
    )?;

    // Include 规则将视图限制为其匹配项：只显示 notepad.exe。
    source.set_filter(FilterSet::new(vec![Rule::new(
        Column::ProcessName,
        Relation::Is,
        "notepad.exe",
        Action::Include,
    )]));

    // `events()` 流式返回解析后的事件；字段惰性生成。
    for ev in source.events() {
        if !source.is_visible(&ev) {
            continue; // 被当前过滤器丢弃
        }
        println!(
            "{:>6}  {:<22}  {:<16}  {}",
            ev.pid(),
            ev.operation_name(),
            ev.result(),
            ev.path().unwrap_or_default(),
        );
    }
    Ok(())
}
```

**读取 `.PML`** — 无需驱动；循环完全一致：

```rust
use procmon_sdk::{Action, Column, EventSource, FilterSet, Relation, Rule};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = EventSource::from_pml("out.pml")?;

    // Exclude 规则隐藏其匹配项：丢弃临时文件噪声。
    source.set_filter(FilterSet::new(vec![Rule::new(
        Column::Path,
        Relation::EndsWith,
        ".tmp",
        Action::Exclude,
    )]));

    for ev in source.events() {
        if !source.is_visible(&ev) {
            continue;
        }
        println!(
            "{:>6}  {:<22}  {:<16}  {}",
            ev.pid(),
            ev.operation_name(),
            ev.result(),
            ev.path().unwrap_or_default(),
        );
    }
    Ok(())
}
```

运行内置的控制台演示：

```bash
# 实时捕获（以管理员身份运行）。可选的 .sys 路径用于按需加载驱动。
cargo run -p procmon-example -- [C:\path\to\procmon.sys]
```

## PML 日志

OpenProcMon 读写 Sysinternals Process Monitor 的 `.PML` 格式：

```bash
# 实时捕获，然后保存为 Procmon 兼容的 .PML
cargo run -p procmon-example -- --save out.pml [C:\path\to\procmon.sys]

# 回放 .PML（无需驱动）
cargo run -p procmon-example -- --pml out.pml
```

在 GUI 中，使用 **文件 ▸ 打开** 加载 `.PML`。

## 驱动兼容性

你不需要自己的代码签名证书：SDK 与原版 Process Monitor 驱动 100% 兼容，因此可以直接用原版 Procmon 驱动来驱动它。反过来，本驱动也可以替换原版，用于研究 Process Monitor 的工作原理，或作为你自己 EDR 类工具的起点。

## 已知问题

- **OpenProcMon 写出的 `.PML` 文件可能导致原版 Process Monitor 崩溃。**
  PML 写入器目前尚未与 Sysinternals Process Monitor 期望的格式完全字节兼容，
  因此用 OpenProcMon 捕获/保存的日志在原版 Procmon 中打开时可能导致其崩溃。
  在 OpenProcMon 中读取由 Procmon 生成的 `.PML` 文件，以及将 OpenProcMon 写出的
  `.PML` 文件在 OpenProcMon 中往返读取，均能正常工作。让写入器完全兼容的修复
  已在计划中。

## 状态与路线图

Rust 重写正在积极开发中。

- [x] Rust SDK：驱动端口、事件解析（进程/文件/注册表/网络）
- [x] 进程跟踪、镜像元数据与图标提取
- [x] PML 读写器（Procmon 兼容）
- [x] GUI：事件表格、详情面板、过滤/高亮、进程树、汇总
- [x] 带模块解析的调用栈
- [x] 从 GUI 保存当前捕获
- [ ] AI Mcp服务器和skills
- [ ] 性能优化

## 许可证

基于 [MIT 许可证](../LICENSE) 发布。
