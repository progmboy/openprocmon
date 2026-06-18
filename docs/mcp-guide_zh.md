# OpenProcMon MCP 服务器 — 使用文档

> English: [mcp-guide.md](./mcp-guide.md)

`procmon-cli mcp` 把 OpenProcMon 暴露成一个**基于 stdio 的 MCP 服务器**,让 AI agent
(Claude Code、Claude Desktop、Codex、Cursor……)替你**捕获和分析** Windows 的
进程 / 文件 / 注册表 / 网络活动 —— 而且是**用自然语言**。你提问,agent 自己选工具。

**模型**:一次*捕获*写出一个 Procmon 兼容的 `.PML`;所有*分析*工具都读它。agent 既可以分析
刚结束的捕获,也可以分析磁盘上任意 `.PML`(**包括原版 Process Monitor 抓的**)。

**提权**:实时捕获需要**管理员 + 内核驱动**;**分析一个已有的 `.PML` 两者都不需要** ——
普通非提权客户端就能跑。

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

> 下面这些是你复制粘贴进客户端配置的**配置块** —— 也是你唯一需要碰 JSON/TOML 的地方。
> 配置完之后,全程都是自然语言。

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

在 Claude Code 里用 `/mcp` 确认 `openprocmon` 已连接。

### Claude Desktop

编辑 `claude_desktop_config.json`(Windows 在
`%APPDATA%\Claude\claude_desktop_config.json`),然后重启:

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

- **只做分析**(读 `.PML`):不需要管理员 —— 正常启动客户端,告诉 agent 打开哪个 `.PML` 即可。
- **实时捕获**:*服务器进程*必须**提权**运行,且驱动已安装。要么从提权终端启动整个客户端;
  要么你自己在提权终端跑 `procmon-cli capture …`,再把产出的 `.PML` 交给 agent
  (**推荐** —— agent 侧保持非特权)。

---

## 3. agent 能做什么(工具)

这些不用你手动调,agent 会调。列在这里只是让你知道能让它干什么。

**分析(只读,无需提权):**

- **`query_events`** —— 主力。查事件,或把某一列汇总成去重值+计数(比如"写了哪些文件")。
- **`process_tree`** / **`list_processes`** —— 父→子 进程树 / 扁平清单。
- **`get_process`** —— 某进程的身份 + 已加载模块。
- **`get_event`** —— 单个事件的完整细节,含调用栈。
- **`summary`** / **`pml_info`** —— 快速概览 / 元数据(事件数、计算机、OS、进程数)。
- **`list_filter_columns`** —— 精确的过滤词汇(agent 用它来避免猜错列名)。

**捕获(需管理员 + 驱动):**

- **`capture`** —— 一次性:监控目标进程几秒、写 `.PML`、返回概览。
- **`start_capture`** / **`stop_capture`** —— 你启动、再停止的后台会话。
- **`export`** —— 把(可过滤的)捕获导出为 PML / CSV / XML。
- **`driver_status`** / **`capture_status`** —— 驱动/提权是否就绪;会话是否还在跑。

---

## 4. 问题怎么变成过滤器(可选参考)

通过 MCP 你只管用自然语言问,agent 自己拼过滤器。**这一节不看也能用**,放在这里只是给好奇的人,
以及因为同一套语法也驱动 `procmon-cli` 命令行。

底层 agent 会拼一个过滤表达式 —— `Column OP value` 子句用 `&&` / `||` / `!` 连接 —— 例如:

| 你的问题 | agent 拼的过滤器 |
|---|---|
| "`app.exe` 写了哪些文件?" | `Category == "File System" && ProcessName == app.exe && Operation in (WriteFile, SetEndOfFileInformationFile, DeleteFile)`,按 Path 分组 |
| "有没有注册表持久化?" | `Category == Registry && Operation in (RegSetValue, RegCreateKey) && Path ~ Run`,按 Path 分组 |
| "它连了哪些外网?" | `Category == Network && ProcessName == app.exe`,按 Path 分组 |
| "把失败的列出来。" | `Result != SUCCESS` |

默认 agent 会隐藏记账噪声(NTFS 元数据、监控工具自身);想看*全部*,直接说一声("把噪声也带上")。

---

## 5. 实战示例 —— 分析一个恶意样本的 `.PML`

> **自己动手试。** 下面用的样本是
> [BitravenS/Cloudflare-malware](https://github.com/BitravenS/Cloudflare-malware)
> 仓库里的 **`Logfile.pml`**。下载这个 `.PML`、存到本地,让 agent 打开它即可 —— 分析不需要
> 管理员、不需要驱动,人人可复现。

整个分析全程都是**自然语言提问** —— 你完全不用写过滤器或工具调用。一段真实对话:

> **你 ——** *打开 `Logfile.pml`,告诉我里面有什么。*
>
> **Agent ——** 22.6 万事件,计算机名 **`MALWARE`**,339 个进程。最活跃/最反常的是
> `svchost.exe`、**`decoded_assembly.exe`**(运行时解码出的 .NET 载荷)、`msedge.exe`,
> 还有随机名 exe(`BQZIL…exe`、`ADI89…exe`)和 `Maui.com`。

> **你 ——** *哪些可疑?它们是怎么被启动的?*
>
> **Agent ——** `decoded_assembly.exe`(在桌面)往 `%TEMP%` 释放了两个随机名的 **Inno
> Setup** 安装器;每个都先查 **AVG / Quick Heal** 杀软,再用 **`Maui.com`**
> —— 一个*被改名的 AutoIt 解释器* —— 去跑 `.a3x` 脚本。(你还会看到
> `procdump … decoded_assembly.exe`,那是分析员自己的脱壳步骤,不是恶意行为。)

> **你 ——** *`decoded_assembly.exe` 写了哪些文件?*
>
> **Agent ——** 正好两个 —— 都是丢进 `%TEMP%` 的可执行文件(上面那两个安装器)。干净的**释放器**。

> **你 ——** *它接触了哪些敏感文件?别只看读取 —— 打开也算。*
>
> **Agent ——** 看文件**打开**(不只读取),它探测了几十个凭据/钱包目标:加密钱包
> (Bitcoin、Ethereum、Ledger、Coinomi、Atomic、Jaxx……)、**所有 Chromium 浏览器的
> `Local State`**、密码管理器(1Password、NordPass、Authy)、邮件/FTP 客户端(The Bat!、
> Mailbird、FileZilla……)、VPN、AnyDesk、Telegram,以及云凭据(`.aws`、`.azure`、
> `gcloud`)—— 并**读取**了本机实际存在的那些(Edge 的保存密码 + 解密它们的 `Local State`
> 密钥 + cookie + 信用卡)。一个**广谱信息窃取器**。

上面每一句都是自然语言提问,agent 在底层把它翻成了对应的查询。

**有一点值得明确告诉 agent:** *"看打开(`CreateFile`),不要只看读取。"* stealer 常常是
*打开*文件探测、再用内存映射读内容 —— 这根本不出现"读取"事件,所以只看读取会严重低估它碰过什么。
另外记住:Procmon 只记*哪个*文件被碰过、从不记写入的字节;要回答"它到底偷/传了什么",改去问
**网络**活动。

---

## 6. 技巧与排错

- **要摘要、别要倾倒。** "把写入按路径分组""哪些进程最忙" —— agent 会返回计数而不是几千行原始
  事件。大的原始结果反正会被截断到文件里。
- **捕获不工作?** 让 agent 查 `driver_status` —— 它报告驱动可达性、你是否提权、各工具能干啥。
- **CLI 同一套词汇。** 这里的一切在终端也能用:`procmon-cli --help`、`procmon-cli vocab`、
  `procmon-cli query --pml … --filter …`。
