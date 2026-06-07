/* ============ OpenProcmon — dialogs ============ */
const { useState: useS, useEffect: useE, useRef: useR, useMemo: useM } = React;

const FILTER_COLUMNS = [
  { v: "proc", zh: "进程名称", en: "Process Name" }, { v: "pid", zh: "PID", en: "PID" }, { v: "op", zh: "操作", en: "Operation" },
  { v: "path", zh: "路径", en: "Path" }, { v: "result", zh: "结果", en: "Result" }, { v: "cat", zh: "类别", en: "Category" },
];
const FILTER_RELS = [
  { v: "is", zh: "是", en: "is" }, { v: "isnot", zh: "不是", en: "is not" }, { v: "begins", zh: "开头是", en: "begins with" },
  { v: "ends", zh: "结尾是", en: "ends with" }, { v: "contains", zh: "包含", en: "contains" }, { v: "excludes", zh: "不包含", en: "excludes" },
];

// ---------------- Processing overlay (Loading) ----------------
function ProcessingOverlay({ onDone }) {
  const [pct, setPct] = useS(0);
  const [step, setStep] = useS("正在解析过滤规则…");
  const [done, setDone] = useS(false);
  useE(() => {
    const steps = [
      [12, tr("正在解析过滤规则…", "Parsing filter rules…")], [30, tr("正在编译规则树…", "Compiling rule tree…")], [52, tr("正在重新评估事件缓冲区…", "Re-evaluating event buffer…")],
      [74, tr("正在应用包含/排除规则…", "Applying include/exclude rules…")], [92, tr("正在重建事件视图…", "Rebuilding event view…")], [100, tr("完成", "Done")],
    ];
    let i = 0, cur = 0;
    const tick = setInterval(() => {
      const target = steps[Math.min(i, steps.length - 1)][0];
      cur += Math.max(1, (target - cur) * 0.28);
      if (cur >= target - 0.5) { cur = target; setStep(steps[Math.min(i, steps.length - 1)][1]); i++; }
      setPct(Math.round(cur));
      if (cur >= 100) {
        clearInterval(tick);
        setDone(true);
        setTimeout(onDone, 620);
      }
    }, 60);
    return () => clearInterval(tick);
  }, []);
  const R = 38, C = 2 * Math.PI * R, off = C * (1 - pct / 100);
  return React.createElement("div", { className: "proc-overlay" },
    React.createElement("div", { className: "proc-card" },
      done
        ? React.createElement("div", { className: "proc-done-icon" }, React.createElement(Icon, { name: "check", size: 30 }))
        : React.createElement("div", { className: "ring" },
            React.createElement("svg", null,
              React.createElement("circle", { className: "track", cx: 42, cy: 42, r: R, fill: "none", strokeWidth: 7 }),
              React.createElement("circle", { className: "prog", cx: 42, cy: 42, r: R, fill: "none", strokeWidth: 7,
                strokeDasharray: C, strokeDashoffset: off })
            ),
            React.createElement("div", { className: "pct" }, pct + "%")
          ),
      React.createElement("div", { className: "ptitle" }, done ? tr("过滤器已应用", "Filter applied") : tr("正在应用过滤器", "Applying filter")),
      React.createElement("div", { className: "pstep" }, step),
      React.createElement("div", { className: "proc-bar" }, React.createElement("i", { style: { width: pct + "%" } }))
    )
  );
}

// ---------------- Filter dialog ----------------
function FilterDialog({ initial, onApply, onClose }) {
  const [rules, setRules] = useS(initial && initial.length ? initial : [
    { id: 1, on: true, col: "proc", rel: "is", val: "Procmon.exe", act: "exclude" },
    { id: 2, on: true, col: "proc", rel: "is", val: "System", act: "exclude" },
    { id: 3, on: true, col: "op", rel: "begins", val: "Reg", act: "include" },
  ]);
  const [draft, setDraft] = useS({ col: "proc", rel: "is", val: "", act: "include" });
  const [processing, setProcessing] = useS(false);
  const nid = useR(100);

  function add() {
    if (!draft.val.trim()) return;
    setRules(r => [...r, { id: nid.current++, on: true, ...draft, val: draft.val.trim() }]);
    setDraft(d => ({ ...d, val: "" }));
  }
  function valSuggest() {
    if (draft.col === "proc") return PM.PROC_LIST_FOR_FILTER;
    if (draft.col === "op") return PM.OP_LIST_FOR_FILTER;
    if (draft.col === "cat") return Object.values(PM.CAT_META).map(c => c.label);
    if (draft.col === "result") return ["SUCCESS", "NAME NOT FOUND", "BUFFER OVERFLOW", "ACCESS DENIED"];
    return [];
  }
  const colLabel = v => { const c = FILTER_COLUMNS.find(c => c.v === v) || {}; return tr(c.zh, c.en); };
  const relLabel = v => { const c = FILTER_RELS.find(c => c.v === v) || {}; return tr(c.zh, c.en); };

  return React.createElement("div", { className: "overlay", onMouseDown: e => { if (e.target === e.currentTarget && !processing) onClose(); } },
    React.createElement("div", { className: "dialog filter-dialog", onMouseDown: e => e.stopPropagation() },
      React.createElement("div", { className: "dialog-head" },
        React.createElement(Icon, { name: "filter", size: 18, style: { color: "var(--accent)" } }),
        React.createElement("span", { className: "title" }, tr("过滤器", "Filter")),
        React.createElement("span", { className: "sub" }, tr("符合规则的事件将被显示。", "Events matching the rules are displayed.")),
        React.createElement("div", { className: "x", onClick: onClose }, React.createElement(Icon, { name: "x", size: 16 }))
      ),
      React.createElement("div", { className: "dialog-body scroll", style: { minWidth: 0 } },
        React.createElement("div", { className: "filter-builder" },
          React.createElement("div", { className: "filter-row-add" },
            React.createElement("select", { className: "fld", value: draft.col, onChange: e => setDraft({ ...draft, col: e.target.value }) },
              FILTER_COLUMNS.map(c => React.createElement("option", { key: c.v, value: c.v }, tr(c.zh, c.en)))),
            React.createElement("select", { className: "fld", value: draft.rel, onChange: e => setDraft({ ...draft, rel: e.target.value }) },
              FILTER_RELS.map(c => React.createElement("option", { key: c.v, value: c.v }, tr(c.zh, c.en)))),
            React.createElement("input", { className: "fld", list: "filter-vals", value: draft.val, placeholder: tr("值…", "Value…"),
              onChange: e => setDraft({ ...draft, val: e.target.value }), onKeyDown: e => { if (e.key === "Enter") add(); } }),
            React.createElement("datalist", { id: "filter-vals" }, valSuggest().map((s, i) => React.createElement("option", { key: i, value: s }))),
            React.createElement("select", { className: "fld", value: draft.act, onChange: e => setDraft({ ...draft, act: e.target.value }) },
              React.createElement("option", { value: "include" }, tr("包含", "Include")),
              React.createElement("option", { value: "exclude" }, tr("排除", "Exclude"))),
            React.createElement("button", { className: "btn primary", onClick: add, style: { height: 34 } }, tr("添加", "Add"))
          ),
          React.createElement("div", { className: "filter-list" },
            React.createElement("div", { className: "filter-list-head" },
              React.createElement("span", null, ""), React.createElement("span", null, tr("列", "Column")),
              React.createElement("span", null, tr("关系", "Relation")), React.createElement("span", null, tr("值", "Value")),
              React.createElement("span", null, tr("动作", "Action")), React.createElement("span", null, "")),
            rules.length === 0
              ? React.createElement("div", { className: "filter-empty" }, tr("暂无过滤规则 — 默认显示全部事件。", "No filter rules — all events shown by default."))
              : rules.map(r =>
                  React.createElement("div", { className: "filter-item", key: r.id, style: { opacity: r.on ? 1 : 0.45 } },
                    React.createElement("input", { type: "checkbox", className: "chk", checked: r.on, onChange: () => setRules(rs => rs.map(x => x.id === r.id ? { ...x, on: !x.on } : x)) }),
                    React.createElement("span", { className: "col" }, colLabel(r.col)),
                    React.createElement("span", { className: "reln" }, relLabel(r.rel)),
                    React.createElement("span", { className: "col" }, r.val),
                    React.createElement("span", { className: "act " + (r.act === "include" ? "inc" : "exc") }, r.act === "include" ? tr("包含", "Include") : tr("排除", "Exclude")),
                    React.createElement("div", { className: "del", onClick: () => setRules(rs => rs.filter(x => x.id !== r.id)), title: tr("删除", "Delete") },
                      React.createElement(Icon, { name: "trash", size: 14 }))
                  )
                )
          )
        )
      ),
      React.createElement("div", { className: "dialog-foot" },
        React.createElement("button", { className: "btn ghost", onClick: () => setRules([]), style: { marginRight: "auto" } }, tr("重置", "Reset")),
        React.createElement("button", { className: "btn", onClick: onClose }, tr("取消", "Cancel")),
        React.createElement("button", { className: "btn primary", onClick: () => setProcessing(true) },
          React.createElement(Icon, { name: "check", size: 15, style: { marginRight: 6, verticalAlign: "-3px" } }), tr("应用", "Apply"))
      )
    ),
    processing && React.createElement(ProcessingOverlay, { onDone: () => { setProcessing(false); onApply(rules); } })
  );
}

// ---------------- Process Tree ----------------
function buildTree() {
  const nodes = {};
  PM.PROCESSES.forEach(p => { nodes[p.pid] = { pid: p.pid, name: p.name, desc: p.company, user: p.user, proc: p, children: [] }; });
  Object.values(PM.TREE_EXTRA).forEach(e => { if (!nodes[e.pid]) nodes[e.pid] = { pid: e.pid, name: e.name, desc: e.desc, user: e.user, children: [] }; });
  // explicit parent overrides for a clean tree
  const PARENT = { 0: null, 4: 0, 88: 4, 680: 4, 780: 680, 1024: 780, 1456: 780, 1180: 680, 2340: 1180, 8456: 2340, 7234: 2340, 5672: 2340, 3120: 2340, 6088: 3120 };
  const roots = [];
  Object.keys(nodes).forEach(pidStr => {
    const pid = +pidStr;
    const par = PARENT[pid];
    if (par === null || par === undefined || !nodes[par]) roots.push(nodes[pid]);
    else nodes[par].children.push(nodes[pid]);
  });
  return roots.sort((a, b) => a.pid - b.pid);
}
function TreeNode({ node, depth, sel, setSel, expanded, toggle }) {
  const open = expanded[node.pid] !== false;
  const hasKids = node.children.length > 0;
  return React.createElement(React.Fragment, null,
    React.createElement("div", { className: "tree-node-row" + (sel === node.pid ? " sel" : ""),
      style: { paddingLeft: 14 + depth * 20 }, onClick: () => setSel(node.pid) },
      React.createElement("div", { className: "tree-caret" + (open ? " open" : "") + (hasKids ? "" : " leaf"),
        onClick: e => { e.stopPropagation(); toggle(node.pid); } }, React.createElement(Icon, { name: "chevron", size: 13 })),
      node.proc ? React.createElement(AppIcon, { proc: node.proc }) :
        React.createElement("span", { className: "appicon", style: { background: "var(--faint)" } }, node.name.slice(0, 1).toUpperCase()),
      React.createElement("span", { className: "tname" }, node.name),
      React.createElement("span", { className: "tpid" }, node.pid),
      React.createElement("span", { className: "tdesc" }, node.desc || node.user || "")
    ),
    hasKids && open && node.children.map(c =>
      React.createElement(TreeNode, { key: c.pid, node: c, depth: depth + 1, sel, setSel, expanded, toggle }))
  );
}
function ProcessTreeDialog({ onClose, onJump }) {
  const roots = useM(buildTree, []);
  const [sel, setSel] = useS(8456);
  const [expanded, setExpanded] = useS({});
  const toggle = pid => setExpanded(e => ({ ...e, [pid]: e[pid] === false }));
  const selNode = useM(() => { let f = null; const walk = n => { if (n.pid === sel) f = n; n.children.forEach(walk); }; roots.forEach(walk); return f; }, [sel, roots]);
  return React.createElement("div", { className: "overlay", onMouseDown: e => { if (e.target === e.currentTarget) onClose(); } },
    React.createElement("div", { className: "dialog tree-dialog", onMouseDown: e => e.stopPropagation() },
      React.createElement("div", { className: "dialog-head" },
        React.createElement(Icon, { name: "tree", size: 18, style: { color: "var(--accent)" } }),
        React.createElement("span", { className: "title" }, tr("进程树", "Process Tree")),
        React.createElement("span", { className: "sub" }, tr("当前捕获会话中出现的进程层级", "Process hierarchy seen in this capture session")),
        React.createElement("div", { className: "x", onClick: onClose }, React.createElement(Icon, { name: "x", size: 16 }))
      ),
      React.createElement("div", { className: "dialog-body scroll", style: { flex: 1 } },
        roots.map(r => React.createElement(TreeNode, { key: r.pid, node: r, depth: 0, sel, setSel, expanded, toggle }))
      ),
      React.createElement("div", { className: "dialog-foot", style: { justifyContent: "space-between" } },
        React.createElement("div", { style: { color: "var(--muted)", fontSize: 12 } },
          selNode ? React.createElement("span", null, tr("已选择 ", "Selected "),
            React.createElement("b", { style: { color: "var(--text)", fontFamily: "var(--mono-font)" } }, selNode.name + " (" + selNode.pid + ")")) : ""),
        React.createElement("div", { style: { display: "flex", gap: 9 } },
          React.createElement("button", { className: "btn", onClick: onClose }, tr("关闭", "Close")),
          React.createElement("button", { className: "btn primary", onClick: () => { if (selNode) onJump(selNode.name); } }, tr("在事件中查看", "View in Events"))
        )
      )
    )
  );
}

// ---------------- Performance / activity summary ----------------
function Sparkline({ data, color }) {
  const w = 100, h = 60, max = Math.max(...data, 1);
  const pts = data.map((d, i) => [i / (data.length - 1) * w, h - (d / max) * (h - 6) - 3]);
  const line = pts.map((p, i) => (i ? "L" : "M") + p[0].toFixed(1) + " " + p[1].toFixed(1)).join(" ");
  const area = line + ` L${w} ${h} L0 ${h} Z`;
  const gid = "g" + color.replace(/[^a-z]/gi, "");
  return React.createElement("svg", { className: "spark", viewBox: `0 0 ${w} ${h}`, preserveAspectRatio: "none" },
    React.createElement("defs", null, React.createElement("linearGradient", { id: gid, x1: 0, y1: 0, x2: 0, y2: 1 },
      React.createElement("stop", { offset: "0%", stopColor: color, stopOpacity: 0.28 }),
      React.createElement("stop", { offset: "100%", stopColor: color, stopOpacity: 0 }))),
    React.createElement("path", { d: area, fill: `url(#${gid})` }),
    React.createElement("path", { d: line, fill: "none", stroke: color, strokeWidth: 1.8, strokeLinejoin: "round" })
  );
}
function PerfDialog({ onClose }) {
  const stats = useM(() => {
    const evts = PM.EVENTS;
    const catCount = {}; Object.keys(PM.CAT_META).forEach(c => catCount[c] = 0);
    const procCount = {};
    evts.forEach(e => { catCount[e.cat]++; procCount[e.proc.name] = (procCount[e.proc.name] || 0) + 1; });
    const bins = 24; const series = new Array(bins).fill(0);
    evts.forEach((e, i) => { series[Math.floor(i / evts.length * bins)]++; });
    // add organic variation so the rate line reads as activity, not a flat block
    const wave = series.map((v, i) => Math.max(1, Math.round(v * (0.55 + 0.45 * Math.sin(i * 0.9) + 0.25 * Math.sin(i * 2.3 + 1)))));
    const topProc = Object.entries(procCount).sort((a, b) => b[1] - a[1]).slice(0, 6);
    return { catCount, series, wave, topProc, total: evts.length };
  }, []);
  const catRows = Object.entries(stats.catCount).sort((a, b) => b[1] - a[1]);
  const maxCat = Math.max(...catRows.map(c => c[1]), 1);
  const maxProc = Math.max(...stats.topProc.map(p => p[1]), 1);
  return React.createElement("div", { className: "overlay", onMouseDown: e => { if (e.target === e.currentTarget) onClose(); } },
    React.createElement("div", { className: "dialog perf-dialog", onMouseDown: e => e.stopPropagation() },
      React.createElement("div", { className: "dialog-head" },
        React.createElement(Icon, { name: "perf", size: 18, style: { color: "var(--accent)" } }),
        React.createElement("span", { className: "title" }, tr("系统活动概要", "System Activity Summary")),
        React.createElement("span", { className: "sub" }, tr("本次捕获的事件统计", "Event statistics for this capture")),
        React.createElement("div", { className: "x", onClick: onClose }, React.createElement(Icon, { name: "x", size: 16 }))
      ),
      React.createElement("div", { className: "dialog-body scroll" },
        React.createElement("div", { className: "perf-grid" },
          React.createElement("div", { className: "perf-card" },
            React.createElement("div", { className: "ph" },
              React.createElement("span", { className: "pt" }, tr("事件速率", "Event Rate")),
              React.createElement("span", { className: "pv", style: { color: "var(--accent)" } }, stats.total + tr(" 事件", " events"))),
            React.createElement(Sparkline, { data: stats.wave, color: "#4f8cf7" })),
          React.createElement("div", { className: "perf-card" },
            React.createElement("div", { className: "ph" },
              React.createElement("span", { className: "pt" }, tr("网络吞吐", "Network Throughput")),
              React.createElement("span", { className: "pv", style: { color: "var(--op-network)" } }, stats.catCount.network + tr(" 包", " pkts"))),
            React.createElement(Sparkline, { data: stats.series.map((v, i) => Math.round(v * (0.3 + Math.sin(i) * 0.2 + 0.3))), color: "#34d3c0" })),
          React.createElement("div", { className: "perf-card" },
            React.createElement("div", { className: "ph" }, React.createElement("span", { className: "pt" }, tr("按类别分布", "By Category"))),
            React.createElement("div", { className: "cat-bars" },
              catRows.map(([c, n]) => React.createElement("div", { className: "cat-bar-row", key: c },
                React.createElement("span", { className: "cn" },
                  React.createElement("span", { className: "sw", style: { background: PM.CAT_META[c].color } }), tr(PM.CAT_META[c].label, PM.CAT_META[c].en)),
                React.createElement("div", { className: "cat-bar-track" }, React.createElement("i", { style: { width: (n / maxCat * 100) + "%", background: PM.CAT_META[c].color } })),
                React.createElement("span", { className: "cv" }, n))))),
          React.createElement("div", { className: "perf-card" },
            React.createElement("div", { className: "ph" }, React.createElement("span", { className: "pt" }, tr("最活跃进程", "Most Active Processes"))),
            React.createElement("div", null,
              stats.topProc.map(([name, n]) => {
                const proc = PM.PROCESSES.find(p => p.name === name) || { icon: ["#6a7b8c", name[0]] };
                return React.createElement("div", { className: "top-proc-row", key: name },
                  React.createElement(AppIcon, { proc }),
                  React.createElement("div", { className: "tpn" }, React.createElement("span", null, name)),
                  React.createElement("div", { className: "cat-bar-track", style: { gridColumn: "auto" } }, React.createElement("i", { style: { width: (n / maxProc * 100) + "%", height: "100%", display: "block", background: "var(--accent)", borderRadius: 4 } })),
                  React.createElement("span", { className: "tpc" }, n));
              }))
          )
        )
      ),
      React.createElement("div", { className: "dialog-foot" },
        React.createElement("button", { className: "btn primary", onClick: onClose }, tr("关闭", "Close")))
    )
  );
}

// ---------------- Context menu ----------------
function ContextMenu({ x, y, event, onAction, onClose }) {
  const ref = useR(null);
  const [pos, setPos] = useS({ x, y });
  useE(() => {
    const el = ref.current; if (!el) return;
    const r = el.getBoundingClientRect();
    let nx = x, ny = y;
    if (x + r.width > window.innerWidth - 8) nx = window.innerWidth - r.width - 8;
    if (y + r.height > window.innerHeight - 8) ny = window.innerHeight - r.height - 8;
    setPos({ x: nx, y: ny });
  }, []);
  useE(() => {
    const h = () => onClose();
    window.addEventListener("click", h);
    window.addEventListener("contextmenu", h);
    return () => { window.removeEventListener("click", h); window.removeEventListener("contextmenu", h); };
  }, []);
  const pn = event.proc.name;
  const Row = (icon, label, action, extra) => React.createElement("div", { className: "ctx-row", onClick: () => { onAction(action); onClose(); } },
    icon && React.createElement(Icon, { name: icon, size: 15 }), React.createElement("span", null, label), extra);
  return React.createElement("div", { className: "ctx-menu", ref, style: { left: pos.x, top: pos.y },
    onClick: e => e.stopPropagation(), onContextMenu: e => { e.preventDefault(); e.stopPropagation(); } },
    Row("props", tr("属性…", "Properties…"), "props", React.createElement("span", { className: "shortcut" }, "Enter")),
    Row("layers", tr("查看调用堆栈", "View Call Stack"), "stack"),
    React.createElement("div", { className: "ctx-sep" }),
    React.createElement("div", { className: "ctx-row", onClick: () => { onAction("include"); onClose(); } },
      React.createElement(Icon, { name: "plus", size: 15 }), React.createElement("span", null, tr("包含 ", "Include "), React.createElement("span", { className: "accent" }, "'" + pn + "'"))),
    React.createElement("div", { className: "ctx-row", onClick: () => { onAction("exclude"); onClose(); } },
      React.createElement(Icon, { name: "minus", size: 15 }), React.createElement("span", null, tr("排除 ", "Exclude "), React.createElement("span", { className: "accent" }, "'" + pn + "'"))),
    React.createElement("div", { className: "ctx-row", onClick: () => { onAction("highlight"); onClose(); } },
      React.createElement(Icon, { name: "highlight", size: 15 }), React.createElement("span", null, tr("高亮 ", "Highlight "), React.createElement("span", { className: "accent" }, "'" + pn + "'"))),
    React.createElement("div", { className: "ctx-sep" }),
    Row("copy", tr("复制行", "Copy Row"), "copy", React.createElement("span", { className: "shortcut" }, "Ctrl+C")),
    Row("props", tr("书签", "Bookmark"), "bookmark", React.createElement("span", { className: "shortcut" }, "Ctrl+B")),
    Row("jump", tr("跳转到…", "Jump To…"), "jump"),
    Row("search", tr("在线搜索此操作", "Search Online"), "websearch")
  );
}

// ---------------- Summary dialogs (Tools menu) ----------------
function buildSummary(kind, events) {
  const num = {};
  if (kind === "process") {
    const m = {};
    events.forEach(e => {
      const k = e.proc.pid;
      m[k] = m[k] || { proc: e.proc, file: 0, registry: 0, network: 0, total: 0 };
      if (e.cat === "file") m[k].file++; else if (e.cat === "registry") m[k].registry++; else if (e.cat === "network") m[k].network++;
      m[k].total++;
    });
    return Object.values(m).sort((a, b) => b.total - a.total);
  }
  if (kind === "file") {
    const m = {};
    events.filter(e => e.cat === "file" && e.path).forEach(e => {
      const k = e.path;
      m[k] = m[k] || { path: k, total: 0, read: 0, write: 0, procs: new Set() };
      m[k].total++; m[k].procs.add(e.proc.name);
      if (/Read/i.test(e.op)) m[k].read++; if (/Write/i.test(e.op)) m[k].write++;
    });
    return Object.values(m).map(r => ({ ...r, procCount: r.procs.size })).sort((a, b) => b.total - a.total);
  }
  if (kind === "registry") {
    const m = {};
    events.filter(e => e.cat === "registry" && e.path).forEach(e => {
      const k = e.path;
      m[k] = m[k] || { path: k, total: 0, open: 0, query: 0, set: 0 };
      m[k].total++;
      if (/Open|Create/i.test(e.op)) m[k].open++; else if (/Query|Enum/i.test(e.op)) m[k].query++; else if (/Set|Delete/i.test(e.op)) m[k].set++;
    });
    return Object.values(m).sort((a, b) => b.total - a.total);
  }
  if (kind === "network") {
    const m = {};
    events.filter(e => e.cat === "network" && e.path).forEach(e => {
      const k = e.path;
      m[k] = m[k] || { path: k, total: 0, send: 0, recv: 0, procs: new Set() };
      m[k].total++; m[k].procs.add(e.proc.name);
      if (/Send/i.test(e.op)) m[k].send++; else if (/Receive/i.test(e.op)) m[k].recv++;
    });
    return Object.values(m).map(r => ({ ...r, procCount: r.procs.size })).sort((a, b) => b.total - a.total);
  }
  // cross reference: paths touched by >1 distinct process
  const m = {};
  events.filter(e => e.path).forEach(e => {
    const k = e.path;
    m[k] = m[k] || { path: k, total: 0, procs: new Set(), cats: new Set() };
    m[k].total++; m[k].procs.add(e.proc.name); m[k].cats.add(e.cat);
  });
  return Object.values(m).map(r => ({ ...r, procCount: r.procs.size, procList: [...r.procs] }))
    .filter(r => r.procCount > 1).sort((a, b) => b.procCount - a.procCount || b.total - a.total);
}

const SUMMARY_META = {
  process: { title: ["进程活动摘要", "Process Activity Summary"], icon: "cpu" },
  file: { title: ["文件摘要", "File Summary"], icon: "filesys" },
  registry: { title: ["注册表摘要", "Registry Summary"], icon: "registry" },
  network: { title: ["网络摘要", "Network Summary"], icon: "network" },
  xref: { title: ["交叉引用摘要", "Cross Reference Summary"], icon: "crosshair" },
};

function SummaryDialog({ kind, events, onClose }) {
  const [q, setQ] = useS("");
  const meta = SUMMARY_META[kind];
  const rows = useM(() => buildSummary(kind, events), [kind, events]);
  const filtered = useM(() => {
    if (!q) return rows;
    const s = q.toLowerCase();
    return rows.filter(r => (r.path || (r.proc && r.proc.name) || "").toLowerCase().includes(s));
  }, [rows, q]);

  // column defs per kind: [labelZh, labelEn, render(r), className]
  let cols;
  if (kind === "process") cols = [
    { z: "进程", e: "Process", grow: true, r: r => React.createElement("span", { className: "sum-name" }, React.createElement(AppIcon, { proc: r.proc }), r.proc.name) },
    { z: "PID", e: "PID", num: true, r: r => r.proc.pid },
    { z: "文件", e: "File", num: true, c: "c-file", r: r => r.file },
    { z: "注册表", e: "Registry", num: true, c: "c-reg", r: r => r.registry },
    { z: "网络", e: "Network", num: true, c: "c-net", r: r => r.network },
    { z: "总计", e: "Total", num: true, bold: true, r: r => r.total },
  ];
  else if (kind === "file") cols = [
    { z: "路径", e: "Path", grow: true, path: true, r: r => r.path },
    { z: "读", e: "Reads", num: true, r: r => r.read },
    { z: "写", e: "Writes", num: true, r: r => r.write },
    { z: "进程数", e: "Procs", num: true, r: r => r.procCount },
    { z: "总次数", e: "Total", num: true, bold: true, r: r => r.total },
  ];
  else if (kind === "registry") cols = [
    { z: "路径", e: "Path", grow: true, path: true, r: r => r.path },
    { z: "打开", e: "Opens", num: true, r: r => r.open },
    { z: "查询", e: "Queries", num: true, r: r => r.query },
    { z: "写入", e: "Sets", num: true, r: r => r.set },
    { z: "总次数", e: "Total", num: true, bold: true, r: r => r.total },
  ];
  else if (kind === "network") cols = [
    { z: "连接", e: "Connection", grow: true, path: true, r: r => r.path },
    { z: "发送", e: "Sends", num: true, c: "c-net", r: r => r.send },
    { z: "接收", e: "Receives", num: true, c: "c-net", r: r => r.recv },
    { z: "进程数", e: "Procs", num: true, r: r => r.procCount },
    { z: "总次数", e: "Total", num: true, bold: true, r: r => r.total },
  ];
  else cols = [
    { z: "路径", e: "Path", grow: true, path: true, r: r => r.path },
    { z: "进程数", e: "Processes", num: true, bold: true, r: r => r.procCount },
    { z: "访问次数", e: "Accesses", num: true, r: r => r.total },
    { z: "进程列表", e: "Process List", grow: true, r: r => React.createElement("span", { className: "sum-proclist" }, r.procList.join(", ")) },
  ];

  return React.createElement("div", { className: "overlay", onMouseDown: e => { if (e.target === e.currentTarget) onClose(); } },
    React.createElement("div", { className: "dialog summary-dialog", onMouseDown: e => e.stopPropagation() },
      React.createElement("div", { className: "dialog-head" },
        React.createElement(Icon, { name: meta.icon, size: 18, style: { color: "var(--accent)" } }),
        React.createElement("span", { className: "title" }, tr(meta.title[0], meta.title[1])),
        React.createElement("span", { className: "sub" }, tr(rows.length + " 项 · 共 " + events.length + " 事件", rows.length + " items · " + events.length + " events")),
        React.createElement("div", { className: "x", onClick: onClose }, React.createElement(Icon, { name: "x", size: 16 }))
      ),
      React.createElement("div", { className: "dialog-body", style: { padding: 0, display: "flex", flexDirection: "column", minHeight: 0 } },
        React.createElement("div", { className: "sum-toolbar" },
          React.createElement("div", { className: "mod-search", style: { margin: 0, flex: 1 } },
            React.createElement(Icon, { name: "search", size: 13 }),
            React.createElement("input", { value: q, onChange: e => setQ(e.target.value), placeholder: tr("过滤…", "Filter…") }))
        ),
        React.createElement("div", { className: "sum-table-wrap scroll" },
          React.createElement("table", { className: "sum-table" },
            React.createElement("thead", null,
              React.createElement("tr", null,
                cols.map((c, i) => React.createElement("th", { key: i, className: (c.num ? "num" : "") + (c.grow ? " grow" : "") }, tr(c.z, c.e))))),
            React.createElement("tbody", null,
              filtered.slice(0, 200).map((r, i) =>
                React.createElement("tr", { key: i },
                  cols.map((c, j) => React.createElement("td", { key: j,
                    className: (c.num ? "num " : "") + (c.path ? "sum-path " : "") + (c.bold ? "bold " : "") + (c.c || ""),
                    title: c.path ? r.path : undefined }, c.r(r)))))
            )
          ),
          filtered.length === 0 && React.createElement("div", { style: { padding: 30, textAlign: "center", color: "var(--muted)", fontSize: 12 } }, tr("无数据", "No data"))
        )
      ),
      React.createElement("div", { className: "dialog-foot" },
        React.createElement("div", { style: { marginRight: "auto", color: "var(--muted)", fontSize: 11.5 } },
          tr("显示前 ", "Showing top ") + Math.min(200, filtered.length) + tr(" 项", " items")),
        React.createElement("button", { className: "btn primary", onClick: onClose }, tr("关闭", "Close")))
    )
  );
}

Object.assign(window, { FilterDialog, ProcessTreeDialog, PerfDialog, ContextMenu, SummaryDialog });
