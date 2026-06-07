/* ============ OpenProcmon — unified Settings dialog ============ */
const { useState: useSt } = React;

// highlight color palette (key -> swatch color)
const HL_COLORS = [
  { key: "amber", color: "#f0c36b", zh: "琥珀", en: "Amber" },
  { key: "blue", color: "#4f8cf7", zh: "蓝色", en: "Blue" },
  { key: "green", color: "#6ee59a", zh: "绿色", en: "Green" },
  { key: "red", color: "#f0816b", zh: "红色", en: "Red" },
  { key: "purple", color: "#b794f6", zh: "紫色", en: "Purple" },
  { key: "cyan", color: "#34d3c0", zh: "青色", en: "Cyan" },
];
const HL_MAP = HL_COLORS.reduce((m, c) => (m[c.key] = c.color, m), {});

// ---- small reusable controls ----
function Switch({ on, onClick, disabled }) {
  return React.createElement("button", {
    type: "button", className: "switch" + (on ? " on" : "") + (disabled ? " disabled" : ""),
    onClick: disabled ? undefined : onClick, "aria-pressed": on,
  }, React.createElement("span", { className: "switch-knob" }));
}

function Seg({ options, value, onChange }) {
  return React.createElement("div", { className: "seg-ctl" },
    options.map(o => React.createElement("button", {
      key: o.v, type: "button",
      className: "seg-btn" + (value === o.v ? " active" : ""),
      onClick: () => onChange(o.v),
    }, o.label)));
}

function SetRow({ title, desc, control, full }) {
  return React.createElement("div", { className: "set-row" + (full ? " full" : "") },
    React.createElement("div", { className: "set-row-text" },
      React.createElement("div", { className: "set-row-title" }, title),
      desc && React.createElement("div", { className: "set-row-desc" }, desc)),
    React.createElement("div", { className: "set-row-ctl" }, control));
}

// ---- category panels ----
function AppearancePanel({ d, set }) {
  return React.createElement("div", { className: "set-panel" },
    React.createElement("div", { className: "set-section-title" }, tr("外观", "Appearance")),
    React.createElement(SetRow, {
      title: tr("主题", "Theme"),
      desc: tr("界面明暗配色", "Light or dark interface"),
      control: React.createElement(Seg, {
        options: [{ v: "light", label: tr("浅色", "Light") }, { v: "dark", label: tr("深色", "Dark") }],
        value: d.theme, onChange: v => set({ theme: v }),
      }),
    }),
    React.createElement(SetRow, {
      title: tr("界面语言", "Language"),
      desc: tr("菜单与界面文字语言", "Menu and UI text language"),
      control: React.createElement(Seg, {
        options: [{ v: "zh", label: "中文" }, { v: "en", label: "English" }],
        value: d.lang, onChange: v => set({ lang: v }),
      }),
    }),
    React.createElement(SetRow, {
      title: tr("高亮颜色", "Highlight Color"),
      desc: tr("事件列表中高亮行的颜色", "Color used for highlighted event rows"),
      full: true,
      control: React.createElement("div", { className: "swatch-row" },
        HL_COLORS.map(c => React.createElement("button", {
          key: c.key, type: "button",
          className: "swatch" + (d.highlightColor === c.key ? " sel" : ""),
          style: { background: c.color }, title: tr(c.zh, c.en),
          onClick: () => set({ highlightColor: c.key }),
        }, d.highlightColor === c.key && React.createElement(Icon, { name: "check", size: 14, style: { color: "#111" } })))),
    }),
    React.createElement("div", { className: "set-preview", style: { "--hl-prev": HL_MAP[d.highlightColor] } },
      React.createElement("div", { className: "set-preview-row hl" }, "chrome.exe   ReadFile   C:\\Windows\\System32\\…"),
      React.createElement("div", { className: "set-preview-row" }, "svchost.exe  RegQueryKey  HKLM\\Software\\…")));
}

function SymbolsPanel({ d, set }) {
  return React.createElement("div", { className: "set-panel" },
    React.createElement("div", { className: "set-section-title" }, tr("符号设置", "Symbol Configuration")),
    React.createElement("p", { className: "set-lead" },
      tr("符号用于将调用堆栈中的地址解析为函数名。", "Symbols resolve call-stack addresses into function names.")),
    React.createElement("label", { className: "fld-label" }, tr("符号路径 (_NT_SYMBOL_PATH)", "Symbol path (_NT_SYMBOL_PATH)")),
    React.createElement("input", { className: "fld mono full", value: d.symbols.sym, spellCheck: false,
      onChange: e => set({ symbols: { ...d.symbols, sym: e.target.value } }) }),
    React.createElement("label", { className: "fld-label" }, tr("DbgHelp.dll 路径", "DbgHelp.dll path")),
    React.createElement("input", { className: "fld mono full", value: d.symbols.dbghelp, spellCheck: false,
      onChange: e => set({ symbols: { ...d.symbols, dbghelp: e.target.value } }) }));
}

function HistoryPanel({ d, set }) {
  const h = d.history;
  return React.createElement("div", { className: "set-panel" },
    React.createElement("div", { className: "set-section-title" }, tr("历史深度", "History Depth")),
    React.createElement(SetRow, {
      title: tr("开启 RingBuffer", "Enable Ring Buffer"),
      desc: tr("循环缓冲，达到限制后自动丢弃最旧事件", "Circular buffer — drop oldest events when a limit is hit"),
      control: React.createElement(Switch, { on: h.ring, onClick: () => set({ history: { ...h, ring: !h.ring } }) }),
    }),
    React.createElement("div", { className: "limit-card" + (h.ring ? "" : " off") },
      React.createElement("div", { className: "limit-line" },
        React.createElement("span", { className: "ll-k" }, tr("限制", "Limit")),
        React.createElement("input", { type: "number", min: 1, className: "fld num", value: h.mb,
          onChange: e => set({ history: { ...h, mb: Math.max(1, +e.target.value || 0) } }) }),
        React.createElement("span", { className: "ll-u" }, "MB")),
      React.createElement("div", { className: "limit-line" },
        React.createElement("span", { className: "ll-k" }, tr("限制", "Limit")),
        React.createElement("input", { type: "number", min: 1, className: "fld num", value: h.min,
          onChange: e => set({ history: { ...h, min: Math.max(1, +e.target.value || 0) } }) }),
        React.createElement("span", { className: "ll-u" }, tr("分钟", "minutes")))));
}

function ProfilingPanel({ d, set }) {
  const p = d.profiling;
  return React.createElement("div", { className: "set-panel" },
    React.createElement("div", { className: "set-section-title" }, tr("Profiling 事件", "Profiling Events")),
    React.createElement("p", { className: "set-lead" },
      tr("Process Monitor 可生成线程 profiling 事件，按固定间隔捕获所有正在执行线程的状态。",
         "Process Monitor can generate thread profiling events that capture the state of all executing threads at a regular interval.")),
    React.createElement(SetRow, {
      title: tr("启用线程 Profiling 事件", "Enable thread profiling events"),
      control: React.createElement(Switch, { on: p.enabled, onClick: () => set({ profiling: { ...p, enabled: !p.enabled } }) }),
    }),
    React.createElement(SetRow, {
      title: tr("采样间隔", "Sampling interval"),
      desc: tr("两次线程状态采样之间的时间", "Time between thread-state samples"),
      control: React.createElement("div", { style: { opacity: p.enabled ? 1 : 0.4, pointerEvents: p.enabled ? "auto" : "none" } },
        React.createElement(Seg, {
          options: [{ v: "1s", label: tr("每 1 秒", "Every 1s") }, { v: "100ms", label: tr("每 100 毫秒", "Every 100ms") }],
          value: p.interval, onChange: v => set({ profiling: { ...p, interval: v } }),
        })),
    }));
}

function BootPanel({ d, set }) {
  return React.createElement("div", { className: "set-panel" },
    React.createElement("div", { className: "set-section-title" }, tr("开机捕获", "Boot Logging")),
    React.createElement("p", { className: "set-lead" },
      tr("在系统启动早期即开始记录事件，用于诊断开机阶段的活动。下次重启时生效。",
         "Begin logging events early in the boot process to diagnose start-up activity. Takes effect on next reboot.")),
    React.createElement(SetRow, {
      title: tr("启用开机捕获", "Enable boot logging"),
      desc: tr("重启后自动记录启动事件", "Capture start-up events automatically after reboot"),
      control: React.createElement(Switch, { on: d.bootCapture, onClick: () => set({ bootCapture: !d.bootCapture }) }),
    }),
    d.bootCapture && React.createElement("div", { className: "set-note warn" },
      React.createElement(Icon, { name: "info", size: 14 }),
      tr("开机捕获将在下次系统重启时启动。", "Boot logging will start on the next system reboot.")));
}

function DisplayPanel({ d, set }) {
  return React.createElement("div", { className: "set-panel" },
    React.createElement("div", { className: "set-section-title" }, tr("显示格式", "Display Format")),
    React.createElement(SetRow, {
      title: tr("以十六进制显示 FileOffset 和 Length", "Show FileOffset and Length in hexadecimal"),
      desc: tr("文件读写事件中的偏移与长度", "Offset and length in file read/write events"),
      control: React.createElement(Switch, { on: d.hexFileOffset, onClick: () => set({ hexFileOffset: !d.hexFileOffset }) }),
    }),
    React.createElement(SetRow, {
      title: tr("以十六进制显示线程和进程 ID", "Show Thread and Process ID in hexadecimal"),
      desc: tr("事件列表与详情中的 PID / TID", "PID / TID in the event list and details"),
      control: React.createElement(Switch, { on: d.hexThreadProcId, onClick: () => set({ hexThreadProcId: !d.hexThreadProcId }) }),
    }),
    React.createElement("div", { className: "set-preview mono" },
      React.createElement("div", { className: "set-preview-row" }, "PID  " + (d.hexThreadProcId ? "0x400" : "1024") + "   TID  " + (d.hexThreadProcId ? "0x1546" : "5446")),
      React.createElement("div", { className: "set-preview-row" }, "Offset  " + (d.hexFileOffset ? "0x8000" : "32768") + "   Length  " + (d.hexFileOffset ? "0x1000" : "4096"))));
}

const SET_CATS = [
  { key: "appearance", icon: "palette", zh: "外观", en: "Appearance", panel: AppearancePanel },
  { key: "symbols", icon: "layers", zh: "符号设置", en: "Symbols", panel: SymbolsPanel },
  { key: "history", icon: "clock", zh: "历史深度", en: "History Depth", panel: HistoryPanel },
  { key: "profiling", icon: "perf", zh: "Profiling 事件", en: "Profiling", panel: ProfilingPanel },
  { key: "boot", icon: "power", zh: "开机捕获", en: "Boot Logging", panel: BootPanel },
  { key: "display", icon: "hash", zh: "显示格式", en: "Display Format", panel: DisplayPanel },
];

function SettingsDialog({ initial, onApply, onClose }) {
  const [cat, setCat] = useSt("appearance");
  const [draft, setDraft] = useSt(initial);
  const set = (patch) => setDraft(d => ({ ...d, ...patch }));
  const Active = SET_CATS.find(c => c.key === cat).panel;
  return React.createElement("div", { className: "overlay", onMouseDown: e => { if (e.target === e.currentTarget) onClose(); } },
    React.createElement("div", { className: "dialog settings-dialog", onMouseDown: e => e.stopPropagation() },
      React.createElement("div", { className: "dialog-head" },
        React.createElement(Icon, { name: "settings", size: 17, style: { color: "var(--accent)" } }),
        React.createElement("span", { className: "title" }, tr("设置", "Settings")),
        React.createElement("div", { className: "x", onClick: onClose }, React.createElement(Icon, { name: "x", size: 16 }))),
      React.createElement("div", { className: "settings-body" },
        React.createElement("div", { className: "settings-nav" },
          SET_CATS.map(c => React.createElement("button", {
            key: c.key, type: "button", className: "snav" + (cat === c.key ? " active" : ""),
            onClick: () => setCat(c.key),
          }, React.createElement(Icon, { name: c.icon, size: 16 }), React.createElement("span", null, tr(c.zh, c.en))))),
        React.createElement("div", { className: "settings-content scroll" },
          React.createElement(Active, { d: draft, set }))),
      React.createElement("div", { className: "dialog-foot" },
        React.createElement("button", { className: "btn", onClick: onClose }, tr("取消", "Cancel")),
        React.createElement("button", { className: "btn primary", onClick: () => onApply(draft) }, tr("应用", "Apply")))
    ));
}

Object.assign(window, { SettingsDialog, HL_MAP });

// ============ Save To File dialog ============
function Radio({ on, onClick, disabled, children }) {
  return React.createElement("label", {
    className: "rc-row" + (on ? " on" : "") + (disabled ? " disabled" : ""),
    onClick: disabled ? undefined : onClick,
  },
    React.createElement("span", { className: "rc-radio" }, React.createElement("span", { className: "rc-radio-dot" })),
    React.createElement("span", { className: "rc-label" }, children));
}
function Check({ on, onClick, disabled, children }) {
  return React.createElement("label", {
    className: "rc-row" + (on ? " on" : "") + (disabled ? " disabled" : ""),
    onClick: disabled ? undefined : onClick,
  },
    React.createElement("span", { className: "rc-check" }, on && React.createElement(Icon, { name: "check", size: 12 })),
    React.createElement("span", { className: "rc-label" }, children));
}

function SaveDialog({ defaults, counts, onSave, onClose }) {
  const [scope, setScope] = useSt(defaults.scope || "filtered");
  const [profiling, setProfiling] = useSt(defaults.profiling !== false);
  const [format, setFormat] = useSt(defaults.format || "pml");
  const [stacks, setStacks] = useSt(false);
  const [symbols, setSymbols] = useSt(false);
  const [path, setPath] = useSt(defaults.path || "D:\\tools\\ProcessMonitor\\Logfile.PML");
  const xml = format === "xml";

  const setFmt = (f) => {
    setFormat(f);
    const ext = f === "pml" ? "PML" : f === "csv" ? "CSV" : "XML";
    setPath(p => p.replace(/\.(PML|CSV|XML)$/i, "." + ext));
    if (f !== "xml") { setStacks(false); setSymbols(false); }
  };

  const count = scope === "all" ? counts.total : scope === "highlighted" ? counts.highlighted : counts.filtered;

  return React.createElement("div", { className: "overlay", onMouseDown: e => { if (e.target === e.currentTarget) onClose(); } },
    React.createElement("div", { className: "dialog save-dialog", onMouseDown: e => e.stopPropagation() },
      React.createElement("div", { className: "dialog-head" },
        React.createElement(Icon, { name: "save", size: 17, style: { color: "var(--accent)" } }),
        React.createElement("span", { className: "title" }, tr("保存到文件", "Save To File")),
        React.createElement("div", { className: "x", onClick: onClose }, React.createElement(Icon, { name: "x", size: 16 }))),
      React.createElement("div", { className: "dialog-body save-body" },
        // events to save
        React.createElement("div", { className: "save-group-label" }, tr("要保存的事件：", "Events to save:")),
        React.createElement("div", { className: "rc-list" },
          React.createElement(Radio, { on: scope === "all", onClick: () => setScope("all") },
            tr("全部事件", "All events"), React.createElement("span", { className: "rc-count" }, counts.total.toLocaleString())),
          React.createElement(Radio, { on: scope === "filtered", onClick: () => setScope("filtered") },
            tr("使用当前过滤器显示的事件", "Events displayed using current filter"), React.createElement("span", { className: "rc-count" }, counts.filtered.toLocaleString())),
          React.createElement("div", { className: "rc-indent" },
            React.createElement(Check, { on: profiling, onClick: () => setProfiling(v => !v), disabled: scope !== "filtered" },
              tr("同时包含 profiling 事件", "Also include profiling events"))),
          React.createElement(Radio, { on: scope === "highlighted", onClick: () => setScope("highlighted") },
            tr("高亮的事件", "Highlighted events"), React.createElement("span", { className: "rc-count" }, counts.highlighted.toLocaleString()))),
        // format
        React.createElement("div", { className: "save-group-label", style: { marginTop: 18 } }, tr("格式：", "Format:")),
        React.createElement("div", { className: "rc-list" },
          React.createElement(Radio, { on: format === "pml", onClick: () => setFmt("pml") },
            tr("原生 Process Monitor 格式 (PML)", "Native Process Monitor Format (PML)")),
          React.createElement(Radio, { on: format === "csv", onClick: () => setFmt("csv") },
            tr("逗号分隔值 (CSV)", "Comma-Separated Values (CSV)")),
          React.createElement(Radio, { on: format === "xml", onClick: () => setFmt("xml") },
            tr("可扩展标记语言 (XML)", "Extensible Markup Language (XML)")),
          React.createElement("div", { className: "rc-indent" },
            React.createElement(Check, { on: stacks, onClick: () => setStacks(v => !v), disabled: !xml },
              tr("包含堆栈跟踪（会增大文件体积）", "Include stack traces (will increase file size)")),
            React.createElement(Check, { on: symbols, onClick: () => setSymbols(v => !v), disabled: !xml || !stacks },
              tr("解析堆栈符号（会比较慢）", "Resolve stack symbols (will be slow)")))),
        // path
        React.createElement("div", { className: "save-path-row" },
          React.createElement("span", { className: "save-path-k" }, tr("路径：", "Path:")),
          React.createElement("input", { className: "fld mono", style: { flex: 1 }, value: path, spellCheck: false, onChange: e => setPath(e.target.value) }),
          React.createElement("button", { className: "btn ellipsis-btn", title: tr("浏览…", "Browse…"),
            onClick: () => { const leaf = path.split("\\").pop(); setPath("C:\\Users\\Admin\\Desktop\\" + leaf); } }, "…"))),
      React.createElement("div", { className: "dialog-foot" },
        React.createElement("div", { className: "save-foot-info" }, tr("将保存 ", "Will save ") + count.toLocaleString() + tr(" 个事件", " events")),
        React.createElement("button", { className: "btn", onClick: onClose }, tr("取消", "Cancel")),
        React.createElement("button", { className: "btn primary", onClick: () => onSave({ scope, profiling, format, stacks, symbols, path, count }) }, tr("确定", "OK")))
    ));
}

Object.assign(window, { SaveDialog });
