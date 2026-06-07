/* ============ OpenProcmon — main app ============ */
const { useState, useEffect, useRef, useMemo, useCallback } = React;

// ---------- menu definitions ----------
function fmtK(n) {
  if (n < 1000) return String(n);
  if (n < 10000) return (n / 1000).toFixed(1).replace(/\.0$/, "") + "k";
  if (n < 1000000) return Math.round(n / 1000) + "k";
  return (n / 1000000).toFixed(1) + "M";
}
function useMenus(ctx) {
  const c = ctx;
  return [
    { key: "file", label: tr("文件", "File"), items: [
      { icon: "open", label: tr("打开…", "Open…"), sc: "Ctrl+O", fn: () => c.toast(tr("打开 .PML 捕获文件", "Open .PML capture file"), "open") },
      { icon: "save", label: tr("保存…", "Save…"), sc: "Ctrl+S", fn: () => c.openDialog("save") },
      { icon: "saveAs", label: tr("另存为…", "Save As…"), fn: () => c.openDialog("save") },
      { sep: true },
      { icon: "download", label: tr("导入设置…", "Import Settings…"), fn: () => c.toast(tr("已导入配置", "Settings imported"), "open") },
      { icon: "upload", label: tr("导出设置…", "Export Settings…"), fn: () => c.toast(tr("已导出配置", "Settings exported"), "save") },
      { sep: true },
      { icon: "logout", label: tr("退出", "Exit"), fn: () => c.toast(tr("已请求退出（原型）", "Exit requested (prototype)"), "info") },
    ]},
    { key: "edit", label: tr("编辑", "Edit"), items: [
      { icon: "copy", label: tr("复制", "Copy"), sc: "Ctrl+C", fn: () => c.copySelected() },
      { icon: "search", label: tr("查找…", "Find…"), sc: "Ctrl+F", fn: () => c.focusSearch() },
      { sep: true },
      { icon: "trash", label: tr("清空显示", "Clear Display"), sc: "Ctrl+X", fn: () => c.clear() },
    ]},
    { key: "event", label: tr("事件", "Event"), items: [
      { icon: "filter", label: tr("过滤器…", "Filter…"), sc: "Ctrl+L", fn: () => c.openDialog("filter") },
      { icon: "ban", label: tr("清除过滤器", "Clear Filter"), fn: () => c.resetFilters() },
      { sep: true },
      { icon: "highlight", label: tr("高亮…", "Highlight…"), fn: () => c.openDialog("highlight") },
      { icon: "ban", label: tr("清除高亮", "Clear Highlight"), fn: () => c.clearHighlights() },
      { sep: true },
      { icon: "scroll", label: tr("自动滚动", "Auto Scroll"), check: c.autoscroll, fn: () => c.setAutoscroll(v => !v) },
      { sep: true },
      { icon: "bookmark", label: tr("书签", "Bookmark"), sc: "Ctrl+B", check: c.selected != null && c.isBookmarked(c.selected), fn: () => c.toggleBookmark() },
      { sep: true },
      { icon: "globe", label: tr("网络搜索", "Web Search"), fn: () => c.webSearch() },
    ]},
    { key: "tools", label: tr("工具", "Tools"), items: [
      { icon: "tree", label: tr("进程树…", "Process Tree…"), fn: () => c.openDialog("tree") },
      { icon: "perf", label: tr("系统活动概要…", "System Activity Summary…"), fn: () => c.openDialog("perf") },
      { sep: true },
      { icon: "cpu", label: tr("进程活动摘要…", "Process Activity Summary…"), fn: () => c.openDialog("sum-process") },
      { icon: "filesys", label: tr("文件摘要…", "File Summary…"), fn: () => c.openDialog("sum-file") },
      { icon: "registry", label: tr("注册表摘要…", "Registry Summary…"), fn: () => c.openDialog("sum-registry") },
      { icon: "network", label: tr("网络摘要…", "Network Summary…"), fn: () => c.openDialog("sum-network") },
      { icon: "crosshair", label: tr("交叉引用摘要…", "Cross Reference Summary…"), fn: () => c.openDialog("sum-xref") },
    ]},
    { key: "options", label: tr("选项", "Options"), items: [
      { icon: "settings", label: tr("设置…", "Settings…"), sc: "Ctrl+,", fn: () => c.openDialog("settings") },
      { sep: true },
      { icon: "pin", label: tr("始终置顶", "Always on Top"), check: c.alwaysOnTop, fn: () => c.toggleAlwaysOnTop() },
    ]},
    { key: "help", label: tr("帮助", "Help"), items: [
      { icon: "help", label: tr("帮助主题", "Help Topics"), sc: "F1", fn: () => c.toast(tr("帮助主题（原型）", "Help Topics (prototype)"), "info") },
      { icon: "refresh", label: tr("检查更新…", "Check for Updates…"), fn: () => c.toast(tr("已是最新版本 v1.0", "You're on the latest, v1.0"), "refresh") },
      { sep: true },
      { icon: "info", label: tr("关于 OpenProcmon", "About OpenProcmon"), fn: () => c.openDialog("about") },
    ]},
  ];
}

// recursive dropdown (supports nested submenus)
function MenuDropdown({ items, closeAll, depth }) {
  const [sub, setSub] = useState(null);
  return React.createElement("div", { className: "menu-dropdown" + (depth ? " submenu" : ""), onClick: e => e.stopPropagation() },
    items.map((it, i) => it.sep
      ? React.createElement("div", { key: i, className: "menu-sep" })
      : it.submenu
        ? React.createElement("div", { key: i, className: "menu-row has-sub", onMouseEnter: () => setSub(i), onMouseLeave: () => setSub(null) },
            React.createElement("span", { className: "mrow-icon" }, it.icon && React.createElement(Icon, { name: it.icon, size: 14 })),
            React.createElement("span", { className: "mrow-label" }, it.label),
            React.createElement("span", { className: "sub-caret" }, "›"),
            sub === i && React.createElement(MenuDropdown, { items: it.submenu, closeAll, depth: (depth || 0) + 1 }))
        : React.createElement("div", { key: i, className: "menu-row" + (it.check !== undefined ? " check" : "") + (it.check ? " checked" : ""),
            onClick: () => { closeAll(); it.fn && it.fn(); } },
            React.createElement("span", { className: "mrow-icon" }, it.icon && React.createElement(Icon, { name: it.icon, size: 14 })),
            React.createElement("span", { className: "mrow-label" }, it.label),
            it.sc && React.createElement("span", { className: "shortcut" }, it.sc))
    )
  );
}

function MenuBar({ ctx }) {
  const menus = useMenus(ctx);
  const [open, setOpen] = useState(null);
  useEffect(() => {
    if (!open) return;
    const h = () => setOpen(null);
    window.addEventListener("click", h);
    return () => window.removeEventListener("click", h);
  }, [open]);
  return React.createElement("div", { className: "menubar" },
    React.createElement("div", { className: "brand" },
      React.createElement("span", { className: "brand-logo" }, React.createElement(Icon, { name: "perf", size: 11, style: { color: "#fff" } })),
      "OpenProcmon"),
    menus.map(m =>
      React.createElement("div", { key: m.key, className: "menu-item" + (open === m.key ? " open" : ""),
        onClick: e => { e.stopPropagation(); setOpen(open === m.key ? null : m.key); },
        onMouseEnter: () => { if (open) setOpen(m.key); } },
        m.label,
        open === m.key && React.createElement(MenuDropdown, { items: m.items, closeAll: () => setOpen(null), depth: 0 })
      )
    )
  );
}

// ---------- toolbar ----------
function Toolbar({ ctx }) {
  return React.createElement("div", { className: "toolbar" },
    React.createElement("button", { className: "tbtn", title: tr("打开捕获", "Open capture"), onClick: () => ctx.toast(tr("打开 .PML 捕获文件", "Open .PML capture file"), "open") }, React.createElement(Icon, { name: "open" })),
    React.createElement("button", { className: "tbtn", title: tr("保存", "Save"), onClick: () => ctx.openDialog("save") }, React.createElement(Icon, { name: "save" })),
    React.createElement("div", { className: "tbar-sep" }),
    React.createElement("button", { className: "tbtn labeled" + (ctx.capturing ? " capturing" : ""), title: tr("捕获开关 (Ctrl+E)", "Capture toggle (Ctrl+E)"), onClick: ctx.toggleCapture },
      React.createElement(IconFill, { name: ctx.capturing ? "pause" : "play", size: 16 }),
      React.createElement("span", null, ctx.capturing ? tr("暂停", "Pause") : tr("捕获", "Capture"))),
    React.createElement("button", { className: "tbtn" + (ctx.autoscroll ? " active" : ""), title: tr("自动滚动 (Ctrl+A)", "Auto scroll (Ctrl+A)"), onClick: () => ctx.setAutoscroll(v => !v) },
      React.createElement(Icon, { name: "scroll" })),
    React.createElement("button", { className: "tbtn danger", title: tr("清空显示 (Ctrl+X)", "Clear display (Ctrl+X)"), onClick: ctx.clear }, React.createElement(Icon, { name: "trash" })),
    React.createElement("div", { className: "tbar-sep" }),
    React.createElement("button", { className: "tbtn" + (ctx.filters.length ? " active" : ""), title: tr("过滤器… (Ctrl+L)", "Filter… (Ctrl+L)"), onClick: () => ctx.openDialog("filter") },
      React.createElement(IconFill, { name: "filter", size: 16 })),
    React.createElement("button", { className: "tbtn" + (ctx.highlights.length ? " active" : ""), title: tr("高亮…", "Highlight…"), onClick: () => ctx.openDialog("highlight") },
      React.createElement(Icon, { name: "highlight" })),
    React.createElement("button", { className: "tbtn", title: tr("包含窗口对应的进程", "Include process from window"), onClick: ctx.includeFromWindow },
      React.createElement(Icon, { name: "crosshair" })),
    React.createElement("button", { className: "tbtn", title: tr("进程树…", "Process tree…"), onClick: () => ctx.openDialog("tree") },
      React.createElement(Icon, { name: "tree" })),
    React.createElement("button", { className: "tbtn", title: tr("跳到选中项", "Jump to selected"), onClick: ctx.jumpToSelected },
      React.createElement(Icon, { name: "jump" })),
    React.createElement("div", { className: "search-wrap" },
      React.createElement(Icon, { name: "search", size: 15 }),
      React.createElement("input", { ref: ctx.searchRef, value: ctx.search, placeholder: tr("搜索进程名、操作、路径…", "Search process, operation, path…"),
        onChange: e => ctx.setSearch(e.target.value) }),
      ctx.search && React.createElement("span", { className: "search-clear", onClick: () => ctx.setSearch("") }, React.createElement(Icon, { name: "x", size: 14 }))
    ),
    React.createElement("div", { className: "tbar-sep" }),
    React.createElement("div", { className: "lang-toggle", title: tr("界面语言", "Interface language") },
      React.createElement("button", { className: ctx.lang === "zh" ? "active" : "", onClick: () => ctx.setLang("zh") }, "中"),
      React.createElement("button", { className: ctx.lang === "en" ? "active" : "", onClick: () => ctx.setLang("en") }, "EN")
    ),
    React.createElement("button", { className: "tbtn", title: tr("切换主题", "Toggle theme"), onClick: ctx.toggleTheme },
      React.createElement(Icon, { name: ctx.theme === "dark" ? "sun" : "moon" }))
  );
}

// ---------- monitor bar ----------
const MONS = [
  { key: "registry", zh: "注册表", en: "Registry", icon: "registry" },
  { key: "file", zh: "文件系统", en: "File System", icon: "filesys" },
  { key: "network", zh: "网络", en: "Network", icon: "network" },
  { key: "process", zh: "进程/线程", en: "Process/Thread", icon: "procthread" },
  { key: "perf", zh: "性能", en: "Profiling", icon: "perf" },
];
function MonitorBar({ ctx }) {
  return React.createElement("div", { className: "monbar" },
    React.createElement("span", { className: "label" }, tr("监控:", "Monitor:")),
    MONS.map(m => React.createElement("button", { key: m.key, className: "mtoggle" + (ctx.monitors[m.key] ? " on" : ""),
      onClick: () => ctx.toggleMonitor(m.key),
      title: (ctx.monitors[m.key] ? tr("关闭", "Disable") : tr("开启", "Enable")) + tr(m.zh + "监控", " " + m.en + " monitoring") },
      React.createElement("span", { className: "dot" }),
      React.createElement(Icon, { name: m.icon, size: 14 }),
      tr(m.zh, m.en))),
    React.createElement("div", { className: "grow" }),
    React.createElement("span", { className: "mon-stat" }, tr("显示 " + ctx.visibleCount + " / " + ctx.totalCount + " 事件", "Showing " + ctx.visibleCount + " / " + ctx.totalCount + " events"))
  );
}

// ---------- event table ----------
// path renderer: single uniform color (set via .col-path); normalize the network arrow
function PathCell({ path, cat }) {
  if (!path) return "—";
  if (cat === "network" || /->/.test(path)) return path.replace(/\s*->\s*/g, " → ");
  return path;
}
function EventTable({ ctx }) {
  const scRef = ctx.scrollRef;
  const COLS = [
    { key: "idx", label: "#", min: 44, align: "right" },
    { key: "time", label: tr("时间", "Time"), min: 100 },
    { key: "proc", label: tr("进程名称", "Process Name"), min: 120 },
    { key: "pid", label: "PID", min: 56 },
    { key: "op", label: tr("操作", "Operation"), min: 96 },
    { key: "path", label: tr("路径", "Path"), min: 140 },
    { key: "result", label: tr("结果", "Result"), min: 96 },
    { key: "detail", label: tr("详情", "Detail"), min: 90 },
  ];
  const total = COLS.reduce((s, c) => s + (ctx.colWidths[c.key] || 100), 0);

  function startResize(e, key, min) {
    e.preventDefault(); e.stopPropagation();
    const startX = e.clientX, startW = ctx.colWidths[key];
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    const move = ev => ctx.setColWidth(key, Math.max(min, startW + (ev.clientX - startX)));
    const up = () => {
      document.removeEventListener("mousemove", move);
      document.removeEventListener("mouseup", up);
      document.body.style.cursor = ""; document.body.style.userSelect = "";
    };
    document.addEventListener("mousemove", move);
    document.addEventListener("mouseup", up);
  }

  return React.createElement("div", { className: "evt-table scroll", ref: scRef },
    React.createElement("table", { className: "evt-grid", style: { width: total, minWidth: total } },
      React.createElement("colgroup", null,
        COLS.map(c => React.createElement("col", { key: c.key, style: { width: ctx.colWidths[c.key] } }))
      ),
      React.createElement("thead", null,
        React.createElement("tr", null,
          COLS.map((c, i) =>
            React.createElement("th", { key: c.key, style: c.align === "right" ? { textAlign: "right" } : null },
              React.createElement("span", { className: "th-in" }, c.label),
              React.createElement("div", { className: "col-resizer", onMouseDown: e => startResize(e, c.key, c.min),
                onDoubleClick: e => { e.stopPropagation(); ctx.setColWidth(c.key, { idx: 56, time: 146, proc: 196, pid: 76, op: 162, path: 360, result: 148, detail: 224 }[c.key]); },
                title: tr("拖动调整列宽（双击复位）", "Drag to resize (double-click to reset)") })
            ))
        )
      ),
      React.createElement("tbody", null,
        ctx.rows.length === 0
          ? React.createElement("tr", null, React.createElement("td", { colSpan: 8 },
              React.createElement("div", { className: "empty-rows" },
                ctx.cleared ? tr("显示已清空 — 点击「捕获」继续记录事件。", "Display cleared — click Capture to resume recording.") : tr("没有匹配当前过滤条件的事件。", "No events match the current filters."))))
          : ctx.rows.map(e => {
              const m = PM.CAT_META[e.cat];
              const hl = ctx.highlights.includes(e.proc.name);
              return React.createElement("tr", { key: e.idx,
                className: (ctx.selected === e.idx ? "selected " : "") + (hl ? "highlighted " : "") + (ctx.isBookmarked(e.idx) ? "bookmarked" : ""),
                onClick: () => ctx.setSelected(e.idx),
                onDoubleClick: () => ctx.openDetail(e),
                onContextMenu: ev => { ev.preventDefault(); ctx.setSelected(e.idx); ctx.openContext(ev.clientX, ev.clientY, e); } },
                React.createElement("td", { className: "col-idx" },
                  ctx.isBookmarked(e.idx) && React.createElement("span", { className: "bm-dot", title: tr("已加书签", "Bookmarked") }),
                  e.idx),
                React.createElement("td", { className: "col-time" }, e.time),
                React.createElement("td", { className: "col-proc" },
                  React.createElement("span", { className: "pcell" }, React.createElement(AppIcon, { proc: e.proc }), e.proc.name)),
                React.createElement("td", { className: "col-pid" }, fmtId(e.proc.pid)),
                React.createElement("td", { className: "col-op " + m.cls }, e.op),
                React.createElement("td", { className: "col-path", title: e.path }, React.createElement(PathCell, { path: e.path, cat: e.cat })),
                React.createElement("td", { className: "col-result " + e.result.cls },
                  React.createElement("span", { className: "res-cell" },
                    React.createElement("span", { className: "res-dot" }), e.result.text)),
                React.createElement("td", { className: "col-detail", title: e.detail.summary }, e.detail.summary)
              );
            })
      )
    )
  );
}

// ---------- highlight dialog ----------
function HighlightDialog({ highlights, onChange, onClose }) {
  const [val, setVal] = useState(PM.PROC_LIST_FOR_FILTER[0] || "");
  return React.createElement("div", { className: "overlay", onMouseDown: e => { if (e.target === e.currentTarget) onClose(); } },
    React.createElement("div", { className: "dialog mini-dialog", onMouseDown: e => e.stopPropagation() },
      React.createElement("div", { className: "dialog-head" },
        React.createElement(Icon, { name: "highlight", size: 18, style: { color: "var(--accent)" } }),
        React.createElement("span", { className: "title" }, tr("高亮", "Highlight")),
        React.createElement("div", { className: "x", onClick: onClose }, React.createElement(Icon, { name: "x", size: 16 }))),
      React.createElement("div", { className: "dialog-body" },
        React.createElement("span", { className: "field-label" }, tr("高亮指定进程的所有事件", "Highlight all events from a process")),
        React.createElement("div", { style: { display: "flex", gap: 8 } },
          React.createElement("select", { className: "fld", style: { flex: 1 }, value: val, onChange: e => setVal(e.target.value) },
            PM.PROC_LIST_FOR_FILTER.map(p => React.createElement("option", { key: p, value: p }, p))),
          React.createElement("button", { className: "btn primary", onClick: () => { if (!highlights.includes(val)) onChange([...highlights, val]); } }, tr("添加", "Add"))),
        React.createElement("div", { style: { marginTop: 16 } },
          React.createElement("span", { className: "field-label" }, tr("当前高亮规则", "Active highlight rules")),
          highlights.length === 0
            ? React.createElement("div", { style: { color: "var(--muted)", fontSize: 12 } }, tr("暂无 — 添加后对应进程的事件行将以琥珀色标记。", "None — added processes get an amber row marker."))
            : React.createElement("div", { style: { display: "flex", flexWrap: "wrap", gap: 8 } },
                highlights.map(h => React.createElement("span", { key: h, className: "tag amber", style: { cursor: "pointer", paddingRight: 6 },
                  onClick: () => onChange(highlights.filter(x => x !== h)) }, h,
                  React.createElement(Icon, { name: "x", size: 12, style: { marginLeft: 4, verticalAlign: "-2px" } })))))
      ),
      React.createElement("div", { className: "dialog-foot" },
        React.createElement("button", { className: "btn primary", onClick: onClose }, tr("完成", "Done")))
    )
  );
}

// ---------- About dialog ----------
function AboutDialog({ onClose }) {
  return React.createElement("div", { className: "overlay", onMouseDown: e => { if (e.target === e.currentTarget) onClose(); } },
    React.createElement("div", { className: "dialog mini-dialog", style: { width: 400 }, onMouseDown: e => e.stopPropagation() },
      React.createElement("div", { className: "dialog-body", style: { textAlign: "center", padding: "30px 26px" } },
        React.createElement("div", { className: "brand-logo", style: { width: 56, height: 56, borderRadius: 14, margin: "0 auto 16px" } },
          React.createElement(Icon, { name: "perf", size: 30, style: { color: "#fff" } })),
        React.createElement("div", { style: { fontSize: 19, fontWeight: 700, color: "var(--text)" } }, "OpenProcmon"),
        React.createElement("div", { style: { color: "var(--muted)", fontSize: 12, marginTop: 4 } }, tr("开源进程监视器 · v1.0.0", "Open-source Process Monitor · v1.0.0")),
        React.createElement("div", { style: { color: "var(--text-2)", fontSize: 12, marginTop: 16, lineHeight: 1.7 } },
          tr("实时监控文件系统、注册表、网络与进程/线程活动。", "Real-time monitoring of file system, registry, network and process/thread activity.")),
        React.createElement("div", { style: { color: "var(--faint)", fontSize: 11, marginTop: 18 } }, "© 2026 OpenProcmon Project")),
      React.createElement("div", { className: "dialog-foot" },
        React.createElement("button", { className: "btn primary", onClick: onClose }, tr("确定", "OK")))
    ));
}

// ---------- toasts ----------
function Toasts({ items }) {
  return React.createElement("div", { className: "toast-wrap" },
    items.map(t => React.createElement("div", { className: "toast", key: t.id },
      React.createElement(Icon, { name: t.icon || "info", size: 15 }), t.msg)));
}

// ================= APP =================
function clockStepper(startEvent) {
  // parse "HH:MM:SS.fffffff"
  const [hms, frac] = startEvent.time.split(".");
  const [h, m, s] = hms.split(":").map(Number);
  let st = { h, m, s, frac: parseInt(frac, 10) };
  return function next() {
    st.frac += 90000 + Math.floor(Math.random() * 240000);
    if (st.frac >= 10000000) { st.frac -= 10000000; st.s++; if (st.s >= 60) { st.s = 0; st.m++; if (st.m >= 60) { st.m = 0; st.h = (st.h + 1) % 24; } } }
    const f = String(st.frac).padStart(7, "0");
    return `${String(st.h).padStart(2, "0")}:${String(st.m).padStart(2, "0")}:${String(st.s).padStart(2, "0")}.${f}`;
  };
}

function App() {
  const [theme, setTheme] = useState(() => localStorage.getItem("opm-theme") || "dark");
  const [lang, setLangState] = useState(() => window.__OPM_LANG || "zh");
  window.__OPM_LANG = lang; // set synchronously so all children translate in this render
  const [capturing, setCapturing] = useState(false);
  const [autoscroll, setAutoscroll] = useState(true);
  const [monitors, setMonitors] = useState({ registry: true, file: true, network: true, process: true, perf: false });
  const [search, setSearch] = useState("");
  const [filters, setFilters] = useState([]);
  const [highlights, setHighlights] = useState([]);
  const [selected, setSelected] = useState(null);
  const [bookmarks, setBookmarks] = useState([]);
  const [detailEvent, setDetailEvent] = useState(null);
  const [detailTab, setDetailTab] = useState("event");
  const [dialog, setDialog] = useState(null); // filter|tree|perf|highlight
  const [context, setContext] = useState(null); // {x,y,event}
  const [toasts, setToasts] = useState([]);
  const [liveEvents, setLiveEvents] = useState(() => PM.EVENTS.slice());
  const [cleared, setCleared] = useState(false);
  const [alwaysOnTop, setAlwaysOnTop] = useState(false);
  const [historyCfg, setHistoryCfg] = useState({ ring: true, mb: 512, min: 60 });
  const [symbols, setSymbols] = useState({ sym: "srv*C:\\Symbols*https://msdl.microsoft.com/download/symbols", dbghelp: "C:\\Program Files\\Windows Kits\\10\\Debuggers\\x64\\dbghelp.dll" });
  const [highlightColor, setHighlightColor] = useState(() => localStorage.getItem("opm-hlcolor") || "amber");
  const [profiling, setProfiling] = useState({ enabled: false, interval: "1s" });
  const [bootCapture, setBootCapture] = useState(false);
  const [hexFileOffset, setHexFileOffset] = useState(false);
  const [hexThreadProcId, setHexThreadProcId] = useState(false);
  // sync display formatters synchronously so all children format correctly this render
  window.__OPM_HEX_IDS = hexThreadProcId;
  window.__OPM_HEX_OFFSET = hexFileOffset;
  const DEFAULT_COL_W = { idx: 56, time: 146, proc: 196, pid: 76, op: 162, path: 360, result: 148, detail: 224 };
  const [colWidths, setColWidths] = useState(() => {
    try { return { ...DEFAULT_COL_W, ...JSON.parse(localStorage.getItem("opm-colw") || "{}") }; } catch (e) { return DEFAULT_COL_W; }
  });
  const setColWidth = useCallback((key, w) => setColWidths(c => { const n = { ...c, [key]: w }; localStorage.setItem("opm-colw", JSON.stringify(n)); return n; }), []);

  const scrollRef = useRef(null);
  const searchRef = useRef(null);
  const nextIdx = useRef(PM.EVENTS.length + 1);
  const stepper = useRef(clockStepper(PM.EVENTS[PM.EVENTS.length - 1]));

  useEffect(() => { document.documentElement.setAttribute("data-theme", theme); localStorage.setItem("opm-theme", theme); }, [theme]);
  useEffect(() => {
    document.documentElement.style.setProperty("--hl-color", (window.HL_MAP && window.HL_MAP[highlightColor]) || "#f0c36b");
    localStorage.setItem("opm-hlcolor", highlightColor);
  }, [highlightColor]);
  const setLang = useCallback((l) => { window.__OPM_LANG = l; localStorage.setItem("opm-lang", l); setLangState(l); }, []);
  const toggleLang = useCallback(() => setLang(window.__OPM_LANG === "en" ? "zh" : "en"), [setLang]);
  useEffect(() => { document.documentElement.setAttribute("lang", lang === "en" ? "en" : "zh-CN"); }, [lang]);
  useEffect(() => { setMonitors(m => m.perf === profiling.enabled ? m : { ...m, perf: profiling.enabled }); }, [profiling.enabled]);

  const toast = useCallback((msg, icon) => {
    const id = Math.random();
    setToasts(t => [...t, { id, msg, icon }]);
    setTimeout(() => setToasts(t => t.filter(x => x.id !== id)), 2400);
  }, []);

  // live capture simulation
  useEffect(() => {
    if (!capturing) return;
    const iv = setInterval(() => {
      const base = PM.EVENTS[Math.floor(Math.random() * PM.EVENTS.length)];
      const e = Object.assign(Object.create(Object.getPrototypeOf(base)), base, { idx: nextIdx.current++, time: stepper.current(), stack: null });
      e.getStack = function () { if (!e.stack) e.stack = base.getStack(); return e.stack; };
      setLiveEvents(prev => { const n = prev.concat(e); const cap = historyCfg.ring ? Math.max(500, Math.min(50000, historyCfg.mb * 10)) : 200000; return n.length > cap ? n.slice(n.length - cap) : n; });
    }, 650);
    return () => clearInterval(iv);
  }, [capturing, historyCfg]);

  // filtering
  const matchRule = useCallback((e, r) => {
    const field = { proc: e.proc.name, pid: String(e.proc.pid), op: e.op, path: e.path, result: e.result.text, cat: PM.CAT_META[e.cat].label }[r.col] || "";
    const a = field.toLowerCase(), b = String(r.val).toLowerCase();
    switch (r.rel) {
      case "is": return a === b;
      case "isnot": return a !== b;
      case "begins": return a.startsWith(b);
      case "ends": return a.endsWith(b);
      case "contains": return a.includes(b);
      case "excludes": return !a.includes(b);
      default: return false;
    }
  }, []);

  const rows = useMemo(() => {
    if (cleared) return [];
    const active = filters.filter(f => f.on);
    const inc = active.filter(f => f.act === "include");
    const exc = active.filter(f => f.act === "exclude");
    const q = search.trim().toLowerCase();
    return liveEvents.filter(e => {
      const monKey = (e.cat === "thread") ? "process" : e.cat;
      if (!monitors[monKey]) return false;
      if (exc.some(r => matchRule(e, r))) return false;
      if (inc.length && !inc.some(r => matchRule(e, r))) return false;
      if (q) {
        const hay = (e.proc.name + " " + e.op + " " + e.path + " " + e.result.text + " " + e.proc.pid).toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });
  }, [liveEvents, monitors, filters, search, cleared, matchRule]);

  // per-monitor category counts (over all captured events, not filtered)
  const catCounts = useMemo(() => {
    const c = { registry: 0, file: 0, network: 0, process: 0, perf: 0 };
    liveEvents.forEach(e => {
      const k = (e.cat === "thread") ? "process" : e.cat;
      if (c[k] !== undefined) c[k]++;
    });
    return c;
  }, [liveEvents]);

  // autoscroll on new rows
  useEffect(() => {
    if (autoscroll && capturing && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [rows.length, autoscroll, capturing]);

  // actions
  const openDetail = useCallback((e) => { setDetailEvent(e); setSelected(e.idx); }, []);
  const closeDetail = useCallback(() => setDetailEvent(null), []);
  const clear = useCallback(() => { setCleared(true); setLiveEvents([]); setSelected(null); toast(tr("已清空显示", "Display cleared"), "trash"); }, [toast]);
  const copySelected = useCallback(() => {
    const e = liveEvents.find(x => x.idx === selected);
    if (!e) { toast(tr("未选择事件", "No event selected"), "info"); return; }
    const line = [e.time, e.proc.name, e.proc.pid, e.op, e.path, e.result.text].join("\t");
    if (navigator.clipboard) navigator.clipboard.writeText(line).catch(() => {});
    toast(tr("已复制事件行", "Event row copied"), "copy");
  }, [liveEvents, selected, toast]);
  const toggleMonitor = useCallback((k) => setMonitors(m => ({ ...m, [k]: !m[k] })), []);
  const toggleCapture = useCallback(() => {
    setCapturing(c => {
      if (!c && cleared) setCleared(false);
      toast(!c ? tr("开始捕获事件", "Capturing events") : tr("已暂停捕获", "Capture paused"), !c ? "play" : "pause");
      return !c;
    });
  }, [cleared, toast]);
  const focusSearch = useCallback(() => { searchRef.current && searchRef.current.focus(); }, []);
  const jumpToSelected = useCallback(() => {
    if (selected == null) { toast(tr("未选择事件", "No event selected"), "info"); return; }
    const el = scrollRef.current && scrollRef.current.querySelector("tr.selected");
    if (el && scrollRef.current) {
      const top = el.offsetTop - scrollRef.current.clientHeight / 2;
      scrollRef.current.scrollTop = Math.max(0, top);
    }
    toast(tr("已跳转到选中事件", "Jumped to selected event"), "jump");
  }, [selected, toast]);
  const includeFromWindow = useCallback(() => {
    const target = "chrome.exe";
    setFilters(f => f.concat([{ id: Date.now(), on: true, col: "proc", rel: "is", val: target, act: "include" }]));
    toast(tr("已包含窗口进程: ", "Included window process: ") + target, "crosshair");
  }, [toast]);
  const resetFilters = useCallback(() => { setFilters([]); toast(tr("已清除全部过滤规则", "All filters cleared"), "filter"); }, [toast]);
  const clearHighlights = useCallback(() => { setHighlights([]); toast(tr("已清除全部高亮", "All highlights cleared"), "highlight"); }, [toast]);
  const applyTheme = useCallback((m) => setTheme(m), []);
  const doSave = useCallback((opts) => {
    setDialog(null);
    const fmt = opts.format.toUpperCase();
    toast(tr("已保存 ", "Saved ") + opts.count.toLocaleString() + tr(" 个事件为 " + fmt, " events as " + fmt), "save");
  }, [toast]);
  const applySettings = useCallback((s) => {
    setTheme(s.theme);
    if (s.lang !== window.__OPM_LANG) { window.__OPM_LANG = s.lang; localStorage.setItem("opm-lang", s.lang); setLangState(s.lang); }
    setHighlightColor(s.highlightColor);
    setSymbols(s.symbols);
    setHistoryCfg(s.history);
    setProfiling(s.profiling);
    setBootCapture(s.bootCapture);
    setHexFileOffset(s.hexFileOffset);
    setHexThreadProcId(s.hexThreadProcId);
    setDialog(null);
    toast(tr("设置已应用", "Settings applied"), "settings");
  }, [toast]);
  const toggleAlwaysOnTop = useCallback(() => setAlwaysOnTop(v => { toast(!v ? tr("已开启始终置顶", "Always on top enabled") : tr("已关闭始终置顶", "Always on top disabled"), "info"); return !v; }), [toast]);
  const webSearch = useCallback(() => {
    const e = liveEvents.find(x => x.idx === selected);
    const q = e ? (e.proc.name + " " + e.op) : "Process Monitor";
    window.open("https://www.google.com/search?q=" + encodeURIComponent(q), "_blank");
    toast(tr("已在浏览器中搜索", "Searching in browser"), "search");
  }, [liveEvents, selected, toast]);
  const isBookmarked = useCallback((idx) => bookmarks.includes(idx), [bookmarks]);
  const toggleBookmark = useCallback(() => {
    if (selected == null) { toast(tr("未选择事件", "No event selected"), "info"); return; }
    setBookmarks(b => b.includes(selected) ? b.filter(x => x !== selected) : b.concat([selected]));
    toast(bookmarks.includes(selected) ? tr("已移除书签", "Bookmark removed") : tr("已添加书签", "Bookmark added"), "props");
  }, [selected, bookmarks, toast]);

  const openContext = useCallback((x, y, e) => setContext({ x, y, event: e }), []);
  const onContextAction = useCallback((action) => {
    const e = context && context.event;
    if (!e) return;
    if (action === "props") { openDetail(e); setDetailTab("event"); }
    else if (action === "stack") { openDetail(e); setDetailTab("stack"); }
    else if (action === "include") { setFilters(f => f.concat([{ id: Date.now(), on: true, col: "proc", rel: "is", val: e.proc.name, act: "include" }])); toast(tr("已包含 ", "Included ") + e.proc.name, "plus"); }
    else if (action === "exclude") { setFilters(f => f.concat([{ id: Date.now(), on: true, col: "proc", rel: "is", val: e.proc.name, act: "exclude" }])); toast(tr("已排除 ", "Excluded ") + e.proc.name, "minus"); }
    else if (action === "highlight") { setHighlights(h => h.includes(e.proc.name) ? h : h.concat([e.proc.name])); toast(tr("已高亮 ", "Highlighted ") + e.proc.name, "highlight"); }
    else if (action === "copy") { copySelected(); }
    else if (action === "bookmark") { setBookmarks(b => b.includes(e.idx) ? b.filter(x => x !== e.idx) : b.concat([e.idx])); toast(bookmarks.includes(e.idx) ? tr("已移除书签", "Bookmark removed") : tr("已添加书签", "Bookmark added"), "props"); }
    else if (action === "jump") { setSelected(e.idx); jumpToSelected(); }
    else if (action === "websearch") { window.open("https://www.google.com/search?q=" + encodeURIComponent(e.op + " " + e.proc.name), "_blank"); }
  }, [context, openDetail, copySelected, jumpToSelected, toast, bookmarks]);

  // keyboard
  useEffect(() => {
    const h = (ev) => {
      if (ev.target.tagName === "INPUT" || ev.target.tagName === "SELECT") { if (ev.key === "Escape") ev.target.blur(); return; }
      if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "f") { ev.preventDefault(); focusSearch(); }
      else if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "l") { ev.preventDefault(); setDialog("filter"); }
      else if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "e") { ev.preventDefault(); toggleCapture(); }
      else if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "s") { ev.preventDefault(); setDialog("save"); }
      else if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "c") { copySelected(); }
      else if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === "b") { ev.preventDefault(); toggleBookmark(); }
      else if (ev.key === "Escape") { setDialog(null); setContext(null); setDetailEvent(null); }
      else if (ev.key === "Enter" && selected != null) { const e = rows.find(x => x.idx === selected); if (e) openDetail(e); }
      else if ((ev.key === "ArrowDown" || ev.key === "ArrowUp") && rows.length) {
        ev.preventDefault();
        const i = rows.findIndex(x => x.idx === selected);
        let ni = ev.key === "ArrowDown" ? i + 1 : i - 1;
        if (i === -1) ni = 0;
        ni = Math.max(0, Math.min(rows.length - 1, ni));
        const e = rows[ni]; setSelected(e.idx);
        if (detailEvent) setDetailEvent(e);
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [rows, selected, detailEvent, copySelected, focusSearch, toggleCapture, openDetail, toggleBookmark]);

  const ctx = {
    theme, lang, capturing, autoscroll, monitors, search, filters, highlights, selected, detailEvent, cleared, catCounts, bookmarks, alwaysOnTop,
    setSearch, setAutoscroll, setSelected, toggleMonitor, toggleCapture, setLang, toggleLang,
    toggleTheme: () => setTheme(t => t === "dark" ? "light" : "dark"), applyTheme, toggleAlwaysOnTop,
    openDialog: setDialog, toast, clear, copySelected, focusSearch, jumpToSelected, includeFromWindow,
    resetFilters, clearHighlights, webSearch, isBookmarked, toggleBookmark,
    openDetail, closeDetail, openContext,
    searchRef, scrollRef, rows, colWidths, setColWidth,
    visibleCount: rows.length, totalCount: liveEvents.length,
  };

  return React.createElement("div", { className: "app" + (alwaysOnTop ? " pinned" : "") },
    React.createElement(MenuBar, { ctx }),
    React.createElement(Toolbar, { ctx }),
    React.createElement(MonitorBar, { ctx }),
    React.createElement("div", { className: "workspace" },
      React.createElement("div", { className: "table-area" }, React.createElement(EventTable, { ctx })),
      detailEvent && React.createElement(DetailPanel, { event: detailEvent, tab: detailTab, setTab: setDetailTab, onClose: closeDetail })
    ),
    React.createElement("div", { className: "statusbar" },
      React.createElement("div", { className: "seg" },
        React.createElement("span", { className: "status-dot " + (capturing ? "run" : "pause") }),
        capturing ? tr("正在捕获", "Capturing") : tr("已暂停", "Paused")),
      React.createElement("div", { className: "seg" }, tr("显示: ", "Showing: "), React.createElement("b", null, rows.length)),
      React.createElement("div", { className: "seg" }, tr("事件总数: ", "Total events: "), React.createElement("b", null, liveEvents.length)),
      filters.length > 0 && React.createElement("div", { className: "seg" }, React.createElement(Icon, { name: "filter", size: 12, style: { marginRight: 4 } }), filters.filter(f => f.on).length, tr(" 条过滤规则", " filters")),
      highlights.length > 0 && React.createElement("div", { className: "seg" }, React.createElement(Icon, { name: "highlight", size: 12, style: { marginRight: 4 } }), highlights.length, tr(" 条高亮", " highlights")),
      bookmarks.length > 0 && React.createElement("div", { className: "seg" }, React.createElement("span", { className: "bm-dot", style: { position: "static", display: "inline-block", marginRight: 5 } }), bookmarks.length, tr(" 个书签", " bookmarks")),
      React.createElement("div", { className: "grow" }),
      React.createElement("div", { className: "seg last" }, tr(autoscroll ? "自动滚动: 开" : "自动滚动: 关", autoscroll ? "Auto scroll: On" : "Auto scroll: Off"))
    ),
    // dialogs
    dialog === "filter" && React.createElement(FilterDialog, { initial: filters, onClose: () => setDialog(null),
      onApply: (rules) => { setFilters(rules); setDialog(null); toast(tr("过滤器已应用 · ", "Filter applied · ") + rules.filter(r => r.on).length + tr(" 条规则生效", " active rules"), "filter"); } }),
    dialog === "tree" && React.createElement(ProcessTreeDialog, { onClose: () => setDialog(null),
      onJump: (name) => { setFilters(f => f.concat([{ id: Date.now(), on: true, col: "proc", rel: "is", val: name, act: "include" }])); setDialog(null); toast(tr("已在事件中筛选 ", "Filtered events to ") + name, "tree"); } }),
    dialog === "perf" && React.createElement(PerfDialog, { onClose: () => setDialog(null) }),
    dialog && dialog.indexOf("sum-") === 0 && React.createElement(SummaryDialog, { kind: dialog.slice(4), events: liveEvents, onClose: () => setDialog(null) }),
    dialog === "highlight" && React.createElement(HighlightDialog, { highlights, onChange: setHighlights, onClose: () => setDialog(null) }),
    dialog === "about" && React.createElement(AboutDialog, { onClose: () => setDialog(null) }),
    dialog === "settings" && React.createElement(SettingsDialog, {
      initial: { theme, lang, highlightColor, symbols, history: historyCfg, profiling, bootCapture, hexFileOffset, hexThreadProcId },
      onApply: applySettings, onClose: () => setDialog(null) }),
    dialog === "save" && React.createElement(SaveDialog, {
      defaults: { scope: filters.length ? "filtered" : "all", format: "pml", profiling: profiling.enabled, path: "D:\\tools\\ProcessMonitor\\Logfile.PML" },
      counts: {
        total: liveEvents.length,
        filtered: rows.length,
        highlighted: liveEvents.filter(e => highlights.includes(e.proc.name)).length,
      },
      onSave: doSave, onClose: () => setDialog(null) }),
    context && React.createElement(ContextMenu, { x: context.x, y: context.y, event: context.event, onAction: onContextAction, onClose: () => setContext(null) }),
    React.createElement(Toasts, { items: toasts })
  );
}

ReactDOM.createRoot(document.getElementById("root")).render(React.createElement(App));
