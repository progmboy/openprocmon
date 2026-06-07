/* ============ OpenProcmon — detail panel (tabs) ============ */
const { useState: useStateD, useMemo: useMemoD } = React;

function KV({ k, v, mono = true, cls = "" }) {
  return React.createElement("div", { className: "kv" },
    React.createElement("div", { className: "k" }, k),
    React.createElement("div", { className: "v " + (mono ? "" : "plain ") + cls }, v)
  );
}
function Group({ title, children }) {
  return React.createElement("div", { className: "kv-group" },
    title && React.createElement("div", { className: "kv-title" }, title),
    children
  );
}
function cmdHighlight(cmd) {
  // highlight --args
  const parts = cmd.split(/(\s--[^\s]+)/g);
  return parts.map((p, i) =>
    p.trim().startsWith("--")
      ? React.createElement("span", { key: i, className: "arg" }, p)
      : React.createElement("span", { key: i }, p)
  );
}

// derive target-file metadata from a file path (version / company / signed)
function strHash(s) { let h = 0; for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0; return h; }
function targetFileInfo(e) {
  if (e.cat !== "file" || !e.path) return null;
  const leaf = e.path.split("\\").pop();
  if (!leaf || leaf.indexOf("*") >= 0 || leaf.indexOf(".") < 0) return null;
  const ext = (leaf.match(/\.([a-z0-9]+)$/i) || [, ""])[1].toLowerCase();
  const binary = /^(exe|dll|sys|drv|ocx|mui|node|ttf)$/.test(ext);
  let company = "Microsoft Corporation";
  if (/chrome/i.test(leaf)) company = "Google LLC";
  let version = "—";
  if (binary) {
    if (/chrome/i.test(leaf)) version = "124.0.6367.61";
    else if (/msedge/i.test(leaf)) version = "124.0.2478.51";
    else if (ext === "ttf") { version = "10.0.22621." + (1000 + strHash(leaf) % 800); company = "Microsoft Corporation"; }
    else version = "10.0.22621." + (2000 + strHash(leaf) % 800);
  }
  return { leaf, ext, binary, version, company, signed: binary };
}

// ---------- Event tab ----------
const CAT_ICON = { registry: "registry", file: "filesys", network: "network", process: "procthread", thread: "procthread", perf: "perf" };

function Group2({ icon, title, children }) {
  return React.createElement("div", { className: "kv-group" },
    React.createElement("div", { className: "kv-title" },
      icon && React.createElement(Icon, { name: icon, size: 13 }),
      React.createElement("span", null, title)),
    children);
}

function EventTab({ e }) {
  const m = PM.CAT_META[e.cat];
  const fi = targetFileInfo(e);
  return React.createElement("div", { className: "evt-tab" },
    // category header
    React.createElement("div", { className: "evt-cat-head" },
      React.createElement(Icon, { name: CAT_ICON[e.cat] || "info", size: 18, className: m.cls }),
      React.createElement("span", { className: "cat-name " + m.cls }, tr(m.label, m.en))),
    // operation
    React.createElement("div", { className: "evt-field" },
      React.createElement("div", { className: "evt-field-label" }, tr("操作", "Operation")),
      React.createElement("div", { className: "field-box" },
        React.createElement("span", { className: "fb-op " + m.cls }, e.op))),
    // time info
    React.createElement(Group2, { icon: "clock", title: tr("时间信息", "Time") },
      React.createElement(KV, { k: tr("日期", "Date"), v: e.date }),
      React.createElement(KV, { k: tr("时间戳", "Timestamp"), v: e.time }),
      React.createElement("div", { className: "kv" },
        React.createElement("div", { className: "k" }, tr("持续时间", "Duration")),
        React.createElement("div", { className: "v dur" }, e.duration + " s"))),
    // process info
    React.createElement(Group2, { icon: "cpu", title: tr("进程信息", "Process Info") },
      React.createElement("div", { className: "kv" },
        React.createElement("div", { className: "k" }, tr("进程 ID (PID)", "Process ID (PID)")),
        React.createElement("div", { className: "v tid" }, fmtId(e.proc.pid))),
      React.createElement("div", { className: "kv" },
        React.createElement("div", { className: "k" }, tr("线程 ID (TID)", "Thread ID (TID)")),
        React.createElement("div", { className: "v tid" }, fmtId(e.tid)))),
    // path
    e.path && React.createElement(Group2, { icon: "open", title: tr("路径", "Path") },
      React.createElement("div", { className: "codeblock path" }, e.path)),
    // result
    React.createElement("div", { className: "evt-field" },
      React.createElement("div", { className: "evt-field-label" }, tr("结果", "Result")),
      React.createElement("div", { className: "field-box" },
        React.createElement("span", { className: "fb-res " + e.result.cls }, e.result.text))),
    // target file
    fi && React.createElement(Group2, { title: tr("目标文件", "Target File") },
      React.createElement(KV, { k: tr("文件名", "File Name"), v: fi.leaf }),
      React.createElement(KV, { k: tr("版本", "Version"), v: fi.binary ? fi.version : tr("不适用", "n/a") }),
      React.createElement(KV, { k: tr("公司", "Company"), v: fi.company }),
      React.createElement("div", { className: "kv" },
        React.createElement("div", { className: "k" }, tr("已签名", "Signed")),
        React.createElement("div", { className: "v" },
          React.createElement("span", { className: "tag " + (fi.signed ? "green" : "gray") }, fi.signed ? tr("已签名", "Signed") : tr("无签名", "Unsigned"))))),
    // other details
    React.createElement("div", { className: "evt-field" },
      React.createElement("div", { className: "evt-field-label" }, tr("其他详情", "Other Details")),
      React.createElement("textarea", {
        className: "detail-textarea mono scroll", readOnly: true,
        rows: Math.min(9, Object.keys(e.detail.kv).length + 1),
        value: Object.entries(e.detail.kv).map(([k, v]) => k + ": " + (/offset|length/i.test(k) ? fmtOff(v) : v)).join("\n"),
        onFocus: ev => ev.target.select()
      }))
  );
}

// ---------- Process tab ----------
function ProcessTab({ p }) {
  const [q, setQ] = useStateD("");
  const mods = useMemoD(() =>
    p.modules.filter(m => !q || m.name.toLowerCase().includes(q.toLowerCase()) || m.path.toLowerCase().includes(q.toLowerCase())),
    [p, q]);
  const intTag = { System: "red", High: "amber", Medium: "blue", Low: "gray" }[p.integrity] || "gray";
  return React.createElement("div", null,
    React.createElement("div", { className: "kv-group" },
      React.createElement("div", { style: { display: "flex", gap: 14, alignItems: "center", padding: "6px 0 14px" } },
        React.createElement(AppIcon, { proc: p, size: "lg" }),
        React.createElement("div", { style: { minWidth: 0 } },
          React.createElement("div", { style: { fontSize: 15, fontWeight: 600, color: "var(--text)" } }, p.name),
          React.createElement("div", { style: { fontSize: 11.5, color: "var(--muted)", marginTop: 2 } }, p.company),
          React.createElement("div", { style: { fontSize: 11.5, color: "var(--text-2)", marginTop: 3, fontFamily: "var(--mono-font)" } },
            React.createElement("span", { style: { color: "var(--muted)", fontFamily: "var(--ui-font)" } }, tr("版本 ", "Version ")), p.version),
          React.createElement("div", { style: { marginTop: 7, display: "flex", gap: 6 } },
            React.createElement("span", { className: "tag " + (p.status === "running" ? "green" : "gray") }, p.status === "running" ? tr("运行中", "Running") : tr("已结束", "Exited")),
            React.createElement("span", { className: "tag " + intTag }, p.integrity)
          )
        )
      ),
      React.createElement(KV, { k: "PID", v: fmtId(p.pid) }),
      React.createElement(KV, { k: tr("架构", "Architecture"), v: p.arch + "-bit" }),
      React.createElement(KV, { k: tr("父进程 ID", "Parent PID"), v: fmtId(p.ppid) }),
      React.createElement(KV, { k: tr("是否虚拟化", "Virtualized"), v: p.virtualized ? tr("是", "Yes") : tr("否", "No") }),
      React.createElement(KV, { k: "Session ID", v: String(p.session) }),
      React.createElement(KV, { k: "Integrity", v: p.integrity }),
      React.createElement(KV, { k: tr("用户", "User"), v: p.user }),
      React.createElement(KV, { k: tr("启动时间", "Start Time"), v: p.start })
    ),
    React.createElement("div", { className: "kv-group" },
      React.createElement("div", { className: "kv block" },
        React.createElement("div", { className: "k" }, tr("路径", "Path")),
        React.createElement("div", { className: "codeblock" }, p.path)
      ),
      React.createElement("div", { className: "kv block", style: { marginTop: 10 } },
        React.createElement("div", { className: "k" }, tr("命令行", "Command Line")),
        React.createElement("div", { className: "codeblock" }, cmdHighlight(p.cmdline))
      )
    ),
    React.createElement("div", { className: "kv-group" },
      React.createElement("div", { className: "kv-title" }, tr("模块列表", "Modules") + " (" + p.modules.length + ")"),
      React.createElement("div", { className: "mod-search" },
        React.createElement(Icon, { name: "search", size: 13 }),
        React.createElement("input", { value: q, onChange: e => setQ(e.target.value), placeholder: tr("过滤模块…", "Filter modules…") })
      ),
      React.createElement("div", { className: "mod-list", style: { padding: 0, maxHeight: 220, overflow: "auto" } },
        mods.map((m, i) =>
          React.createElement("div", { key: i, className: "mod-row" },
            React.createElement("span", { className: "mname" }, m.name),
            React.createElement("span", { className: "mver" }, m.version),
            React.createElement("span", { className: "mpath" }, m.path)
          )
        ),
        mods.length === 0 && React.createElement("div", { style: { padding: 14, color: "var(--muted)", fontSize: 11.5 } }, tr("无匹配模块", "No matching modules"))
      )
    )
  );
}

// ---------- Stack tab ----------
function StackTab({ e }) {
  const frames = e.getStack();
  return React.createElement("div", null,
    React.createElement("div", { className: "stack-note" },
      React.createElement(Icon, { name: "layers", size: 14 }),
      tr("操作 ", "Operation "), React.createElement("b", { style: { color: "var(--text)", margin: "0 3px", fontFamily: "var(--mono-font)" } }, e.op),
      tr(" 的调用堆栈 · ", " call stack · "), frames.length, tr(" 帧", " frames")
    ),
    React.createElement("div", { style: { overflow: "auto" } },
      React.createElement("table", { className: "stack-table" },
        React.createElement("thead", null,
          React.createElement("tr", null,
            React.createElement("th", null, "Frame"),
            React.createElement("th", null, tr("模块", "Module")),
            React.createElement("th", null, tr("位置", "Location")),
            React.createElement("th", null, tr("地址", "Address")),
            React.createElement("th", null, tr("路径", "Path"))
          )
        ),
        React.createElement("tbody", null,
          frames.map((f, i) =>
            React.createElement("tr", { key: i, className: f.k ? "krow" : "urow" },
              React.createElement("td", { className: f.k ? "frame-k" : "frame-u" }, f.frame),
              React.createElement("td", { className: "stack-mod" }, f.mod),
              React.createElement("td", { className: "stack-loc" },
                React.createElement("span", { className: "fn" }, f.loc.split("+")[0]),
                React.createElement("span", { className: "off" }, "+" + f.loc.split("+")[1])),
              React.createElement("td", { className: "stack-addr" }, f.addr),
              React.createElement("td", { className: "stack-path", title: f.path,
                style: { overflow: "hidden", textOverflow: "ellipsis", maxWidth: 180 } }, f.path)
            )
          )
        )
      )
    ),
    React.createElement("div", { className: "stack-legend" },
      React.createElement("div", { className: "lg-title" }, tr("说明：", "Note:")),
      React.createElement("div", { className: "lg-row" },
        React.createElement("span", { className: "frame-k" }, "K"),
        React.createElement("span", null, tr(" = 内核模式", " = Kernel mode"))),
      React.createElement("div", { className: "lg-row" },
        React.createElement("span", { className: "frame-u" }, "U"),
        React.createElement("span", null, tr(" = 用户模式", " = User mode")))
    )
  );
}

function DetailPanel({ event, tab, setTab, onClose }) {
  if (!event) {
    return React.createElement("div", { className: "detail-panel" },
      React.createElement("div", { className: "detail-empty" },
        React.createElement(Icon, { name: "fileText", size: 46 }),
        React.createElement("div", { style: { fontSize: 13.5, color: "var(--text-2)", fontWeight: 500 } }, tr("未选择事件", "No event selected")),
        React.createElement("div", { className: "hint" }, tr("双击列表中的任意事件，在此查看事件、进程与调用堆栈的完整详情。", "Double-click any event to view its full event, process and call-stack details here."))
      )
    );
  }
  const p = event.proc;
  const stackCount = event.getStack().length;
  return React.createElement("div", { className: "detail-panel" },
    React.createElement("div", { className: "detail-head" },
      React.createElement(AppIcon, { proc: p }),
      React.createElement("div", { className: "meta" },
        React.createElement("div", { className: "pname" }, p.name,
          React.createElement("span", { style: { color: "var(--muted)", fontWeight: 400, fontFamily: "var(--mono-font)", fontSize: 11.5 } }, "PID " + p.pid)),
        React.createElement("div", { className: "psub" }, event.op + " · " + event.time)
      ),
      React.createElement("div", { className: "x", onClick: onClose, title: tr("关闭", "Close") }, React.createElement(Icon, { name: "x", size: 16 }))
    ),
    React.createElement("div", { className: "tabs" },
      React.createElement("div", { className: "tab" + (tab === "event" ? " active" : ""), onClick: () => setTab("event") },
        React.createElement(Icon, { name: "info", size: 14 }), tr("事件详情", "Event")),
      React.createElement("div", { className: "tab" + (tab === "process" ? " active" : ""), onClick: () => setTab("process") },
        React.createElement(Icon, { name: "cpu", size: 14 }), tr("进程详情", "Process")),
      React.createElement("div", { className: "tab" + (tab === "stack" ? " active" : ""), onClick: () => setTab("stack") },
        React.createElement(Icon, { name: "layers", size: 14 }), tr("调用栈", "Stack"),
        React.createElement("span", { className: "badge" }, stackCount))
    ),
    React.createElement("div", { className: "detail-content scroll" },
      tab === "event" && React.createElement(EventTab, { e: event }),
      tab === "process" && React.createElement(ProcessTab, { p: p }),
      tab === "stack" && React.createElement(StackTab, { e: event })
    )
  );
}

window.DetailPanel = DetailPanel;
