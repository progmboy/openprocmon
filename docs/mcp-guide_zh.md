# OpenProcMon MCP 服务器 — 使用文档

> English: [mcp-guide.md](./mcp-guide.md)

`procmon-cli mcp` 把 OpenProcMon 暴露成一个**基于 stdio 的 MCP 服务器**,让 AI agent
(Claude Code、Claude Desktop、Codex、Cursor……)替你**捕获和分析** Windows 的
进程 / 文件 / 注册表 / 网络活动。

**模型**:一次*捕获*写出一个 Procmon 兼容的 `.PML`;所有*分析*工具都读它。你给工具传一个
`source`,要么是 `session_id`(已结束的捕获会话),要么是 `pml_path`(磁盘上任意 `.PML`,
**包括原版 Process Monitor 抓的**)。

**提权**:实时捕获(`capture` / `start_capture`)需要**管理员 + 内核驱动**;
**PML 分析两者都不需要** —— 让工具指向一个已有的 `.PML`,普通非提权客户端就能跑。

---

## 1. 编译二进制

```bash
cargo build -p procmon-cli --release
```

产物在 `target/release/procmon-cli`(Windows 是 `procmon-cli.exe`)。记下它的**绝对路径**
—— 除非二进制在 `PATH` 里,多数 MCP 客户端都要绝对路径。下文示例用
`C:\tools\openprocmon\procmon-cli.exe`,替换成你自己的路径即可。

服务器命令永远一样:**`procmon-cli mcp`**(stdio 传输)。

---

## 2. 接入你的客户端

### Claude Code

一条命令(user 作用域,所有项目都可用):

```bash
claude mcp add --transport stdio --scope user openprocmon -- C:\tools\openprocmon\procmon-cli.exe mcp
```

或在仓库根放一个项目级 `.mcp.json`:

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

在 Claude Code 里用 `/mcp` 确认 `openprocmon` 已连接。服务器自带 `instructions` 和
`list_filter_columns` 工具,agent 会自动学到过滤语法,无需额外提示设置。

### Claude Desktop

编辑 `claude_desktop_config.json`(Windows 在
`%APPDATA%\Claude\claude_desktop_config.json`):

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

然后重启 Claude Desktop。

### Codex(OpenAI Codex CLI)

加到 `~/.codex/config.toml`:

```toml
[mcp_servers.openprocmon]
command = "C:\\tools\\openprocmon\\procmon-cli.exe"
args = ["mcp"]
```

### 其他 MCP 客户端(Cursor、Windsurf、Cline、Continue……)

形态都一样 —— 一个 stdio 服务器,`command` + `args`:

```json
{ "command": "C:\\tools\\openprocmon\\procmon-cli.exe", "args": ["mcp"] }
```

### 捕获 vs 分析,以及提权

- **只做分析**(读 `.PML`):不需要管理员。正常启动客户端、传 `pml_path` 即可。
- **实时捕获**:*服务器进程*必须**提权**运行,且驱动已安装。要么从提权终端启动整个 MCP
  客户端;要么你自己在提权终端跑 `procmon-cli capture …`,再把产出的 `.PML` 通过 `pml_path`
  交给 agent(**推荐** —— agent 侧保持非特权)。

---

## 3. 工具清单

### 读工具(无副作用、无需提权)

每个读工具都接受 `source` = `{ "pml_path": "…" }` **或** `{ "session_id": "…" }`。

| 工具 | 返回什么 | 关键参数 |
|---|---|---|
| `pml_info` | 元数据:事件数、计算机名、OS 版本、进程数 | `source` |
| `summary` | 总数、按类别、Top-N 进程、速率火花线 | `source`、`top=10` |
| `list_processes` | 所有进程的扁平清单(身份 + 命令行) | `source` |
| `process_tree` | 父→子 进程树 | `source` |
| `get_process` | 某进程的身份 + **已加载模块** | `source`、`pid` |
| **`query_events`** | 万能查询 —— 事件分页,或 `group_by` 给去重值+计数 | `source`、`filter?`、`group_by?`、`exclude_noise=true`、`offset=0`、`limit=100`、`include_detail=false` |
| `get_event` | 单个事件完整细节(event / process / stack) | `source`、`seq`、`parts=["event","process","stack"]` |
| `list_filter_columns` | 精确的列名 / 操作符 / 各类别操作名 | — |
| `driver_status` | 驱动可达性 + 是否提权 + 各工具能力矩阵 | — |
| `capture_status` | 会话是否还在捕获 + 已写字节 | `session_id` |

### 写工具(有副作用;实时捕获需管理员 + 驱动)

| 工具 | 做什么 | 关键参数 |
|---|---|---|
| `capture` | 一次性:捕获 `duration_seconds` 秒、写 `.PML`、返回概览 | `process_names[]`、`pids[]`、`include_children=true`、`launch?`、`monitors[]`、`filter?`、`duration_seconds=10`、`max_mb=512`、`sample=100` |
| `start_capture` | 启动后台捕获会话(稍后停止) | 同 `capture`,去掉 `duration_seconds`/`sample` |
| `stop_capture` | 停止会话、定稿其 `.PML` | `session_id` |
| `export` | 把(可过滤的)捕获导出为 **PML / CSV / XML** | `source`、`format`、`out_path`、`filter?`、`include_stacks=false` |

`process_names` 为空的 `capture` 会捕获**整个系统**。捕获工具始终排除自身的驱动/IO 噪声。

---

## 4. 查询语言

`query_events` 是主力。`filter` 是一个表达式字符串,由 `Column OP value` 子句用
`&&` / `||` / `!` 和括号连接。含空格的值要加引号,如 `"File System"`。

**操作符**

| | | | |
|---|---|---|---|
| `==` 是 | `!=` 不是 | `~` 包含 | `!~` 不包含 |
| `^=` 以…开头 | `$=` 以…结尾 | `<` 小于 | `>` 大于 |
| `Column in (a, b, c)` —— 匹配列出值中**任意一个**(OR) | | | |

**`group_by`** 把"洪水"变成"摘要":不再返回一页原始事件,而是某一列的**去重值 + 计数**。
任何可能返回几千行的查询都用它(比如"写了哪些文件" → `group_by=Path`)。

**其他参数**:`exclude_noise`(默认 `true`,过滤 NTFS 元数据 / 监控工具 / 记账噪声,
设 `false` 看原始流)、`include_detail`(加上昂贵的 Detail 列)、`offset` / `limit`
(原始事件分页,每条带 `seq` 供 `get_event` 用)。

**永远先调 `list_filter_columns` 拿精确列名/操作名 —— 不要猜。**

### 配方

```text
X 写了哪些文件?
  Category == "File System" && ProcessName == X
  && Operation in (WriteFile, SetEndOfFileInformationFile, DeleteFile)
  group_by = Path

注册表持久化?
  Category == Registry && Operation in (RegSetValue, RegCreateKey) && Path ~ Run
  group_by = Path

X 的网络端点?
  Category == Network && ProcessName == X        group_by = Path

失败的操作?
  Result != SUCCESS
```

---

## 5. 实战示例 —— 分析一个恶意样本的 `.PML`

下面是对一个样本捕获(`LogfileSample.PML`,22.6 万事件,计算机名 `MALWARE`)的真实防御分析。
agent 全程只读 `.PML`,无需提权。

### 第 1 步 —— 定位:元数据 + 有哪些进程

```text
pml_info { pml_path: "LogfileSample.PML" }
→ 226,224 事件,计算机 "MALWARE",Win build 26100,339 进程
```

339 个进程太多,别整页打印 —— 按活跃度摘要:

```text
query_events { pml_path: "…", group_by: "ProcessName" }
→ svchost.exe 100714, decoded_assembly.exe 18786, msedge.exe 9831, …
  … 随机名 exe:BQZIL6PT3ZUBPA9013PEUPVV2R0.exe、ADI89CRSP7AT1ZIE5.exe
  … Maui.com、procdump.exe、powershell.exe、tasklist.exe、find.exe
```

`decoded_assembly.exe`(运行时解码出的 .NET 载荷)和那些随机名 exe 是可疑集合。

### 第 2 步 —— 还原执行链

```text
process_tree { pml_path: "…" }
```

看可疑分支,树长这样:

```text
decoded_assembly.exe        (C:\Users\wobol\OneDrive\Desktop\…)
├─ BQZIL…exe  →  …tmp /SL5=…  →  …exe /VERYSILENT   (Inno Setup 投递器,在 %TEMP%)
│     ├─ cmd /C tasklist /FI "IMAGENAME eq avgui.exe" | find "avgui.exe"   (查杀软:AVG)
│     ├─ cmd /C tasklist /FI "IMAGENAME eq opssvc.exe" | find "opssvc.exe" (查杀软:Quick Heal)
│     └─ Maui.com  rabbitweed.a3x   (被改名的 AutoIt3 解释器,跑 .a3x 脚本)
└─ ADI89…exe  →  (同样的 Inno Setup 套路)  →  Maui.com  diurnals.a3x
```

命令行给出的关键判断:`Maui.com` **不是**勒索软件,它执行 `*.a3x`(编译后的 AutoIt 脚本),
即一个**被改名的 AutoIt3.exe loader**。`powershell.exe → procdump.exe -ma -w
decoded_assembly.exe` 是分析员自己的脱壳步骤,不是恶意行为。

### 第 3 步 —— 载荷写了什么(释放)

```text
query_events {
  filter: "Category == \"File System\" && ProcessName == decoded_assembly.exe
           && Operation in (WriteFile, SetEndOfFileInformationFile, DeleteFile)",
  group_by: "Path"
}
→ %TEMP%\ADI89CRSP7AT1ZIE5.exe、%TEMP%\BQZIL6PT3ZUBPA9013PEUPVV2R0.exe
```

只写了两个文件、都是 `%TEMP%` 下的可执行文件 → 干净利落的**释放器**。

### 第 4 步 —— 它接触了什么?(打开 ≫ 读取)

一个关键分析要点:**接触 ≠ 读取**。很多 stealer 是*打开*文件探测是否存在(并通过
`CreateFileMapping` 映射读内容,这根本不出 `ReadFile`)。所以要看 `CreateFile`,不能只看
`ReadFile`:

```text
# 只看 ReadFile(严重低估):
query_events { filter: "… && Operation == ReadFile", group_by: "Path" }
→ Edge\User Data\Default\Login Data、Edge\…\Local State、Web Data、History …

# 看 CreateFile(真正的目标面):
query_events { filter: "… && Operation == CreateFile && Path ~ \"wobol\"", group_by: "Path" }
→ 加密钱包:Bitcoin\wallets、Ethereum、Ledger Live、Coinomi、Atomic、Jaxx、Binance …
  所有 Chromium 浏览器的 Local State:Chrome、Brave、Edge、CocCoc、Epic、360Browser …
  密码管理器:1Password、NordPass、Authy
  邮件/FTP:The Bat!、Mailbird、eM Client、FileZilla、SmartFTP
  VPN/远控:NordVPN、ProtonVPN、OpenVPN、AnyDesk、Telegram
  云凭据:.aws、.azure、gcloud
```

结论:一个**广谱信息窃取器** —— 它*打开*几十个凭据/钱包目标,*读取*本机实际存在的那些
(这里是 Edge 的保存密码 + 解密用的 `Local State` 密钥 + cookie + 信用卡)。

### 给你自己分析的要点

- `pml_info` → `group_by ProcessName` → `process_tree` 是快速定位三连。
- 用 `group_by` 避免洪水;只在要深挖某条操作的调用栈时才取原始事件 + `get_event`。
- 判断"碰了哪些敏感数据"要看 **`CreateFile`(+`CreateFileMapping`)**,不能只看 `ReadFile`。
- Procmon 只记*哪个*文件被碰过,从不记写入的字节 —— 要回答"外传了什么",转去看
  **`Category == Network`**。

---

## 6. 技巧与排错

- **大结果**会被客户端写进文件、需要分块读回 —— 多用 `group_by` 和精确 `filter` 让输出变小。
- **`exclude_noise=true`**(默认)隐藏 NTFS 元数据 / 监控工具 / System 记账;设 `false` 看全。
- 捕获遇到**驱动 / 提权问题**:调 `driver_status` —— 它给出可达性、是否提权、各工具能力矩阵。
- **同一套词汇**驱动捕获过滤和分析过滤;对应的 CLI 是 `procmon-cli vocab` / `procmon-cli --help`。
