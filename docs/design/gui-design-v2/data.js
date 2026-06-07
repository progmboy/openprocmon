/* ============ OpenProcmon — data model ============ */
(function () {
  "use strict";

  // ---- icon palette helper (deterministic color per process name) ----
  const ICON_COLORS = {
    "chrome.exe":   ["#4587f4", "C"],
    "msedge.exe":   ["#2c8ad6", "e"],
    "explorer.exe": ["#f0a93b", "E"],
    "svchost.exe":  ["#6a7b8c", "S"],
    "System":       ["#3d8bff", "W"],
    "Registry":     ["#9a6ad6", "R"],
    "notepad.exe":  ["#5aa9e6", "N"],
    "cmd.exe":      ["#2b2b2b", ">"],
    "powershell.exe":["#1d3f7a", "PS"],
    "lsass.exe":    ["#c0563f", "L"],
    "spoolsv.exe":  ["#56a36b", "P"],
    "code.exe":     ["#2f7bc4", "</>"],
    "Taskmgr.exe":  ["#3aa45b", "T"],
    "dwm.exe":      ["#7a5bd6", "D"],
  };
  function iconFor(name) { return ICON_COLORS[name] || ["#6a7b8c", name.slice(0,1).toUpperCase()]; }

  // ---- module catalogs ----
  const COMMON_MODULES = [
    ["ntdll.dll", "10.0.22621.2506", "C:\\Windows\\System32\\ntdll.dll"],
    ["kernel32.dll", "10.0.22621.2506", "C:\\Windows\\System32\\kernel32.dll"],
    ["KernelBase.dll", "10.0.22621.2506", "C:\\Windows\\System32\\KernelBase.dll"],
    ["user32.dll", "10.0.22621.2428", "C:\\Windows\\System32\\user32.dll"],
    ["gdi32.dll", "10.0.22621.2134", "C:\\Windows\\System32\\gdi32.dll"],
    ["advapi32.dll", "10.0.22621.2506", "C:\\Windows\\System32\\advapi32.dll"],
    ["msvcrt.dll", "7.0.22621.2506", "C:\\Windows\\System32\\msvcrt.dll"],
    ["ole32.dll", "10.0.22621.2506", "C:\\Windows\\System32\\ole32.dll"],
    ["combase.dll", "10.0.22621.2715", "C:\\Windows\\System32\\combase.dll"],
    ["rpcrt4.dll", "10.0.22621.2506", "C:\\Windows\\System32\\rpcrt4.dll"],
    ["shell32.dll", "10.0.22621.2715", "C:\\Windows\\System32\\shell32.dll"],
    ["ws2_32.dll", "10.0.22621.1", "C:\\Windows\\System32\\ws2_32.dll"],
    ["crypt32.dll", "10.0.22621.2506", "C:\\Windows\\System32\\crypt32.dll"],
    ["bcrypt.dll", "10.0.22621.1", "C:\\Windows\\System32\\bcrypt.dll"],
  ];
  const APP_MODULES = {
    "chrome.exe": [
      ["chrome.dll", "124.0.6367.61", "C:\\Program Files\\Google\\Chrome\\Application\\124.0.6367.61\\chrome.dll"],
      ["chrome_elf.dll", "124.0.6367.61", "C:\\Program Files\\Google\\Chrome\\Application\\chrome_elf.dll"],
      ["v8_context_snapshot.bin", "124.0.6367.61", "C:\\Program Files\\Google\\Chrome\\Application\\124.0.6367.61\\v8_context_snapshot.bin"],
    ],
    "msedge.exe": [
      ["msedge.dll", "124.0.2478.51", "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\124.0.2478.51\\msedge.dll"],
      ["msedge_elf.dll", "124.0.2478.51", "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge_elf.dll"],
    ],
    "explorer.exe": [
      ["windows.storage.dll", "10.0.22621.2715", "C:\\Windows\\System32\\windows.storage.dll"],
      ["explorerframe.dll", "10.0.22621.2506", "C:\\Windows\\System32\\ExplorerFrame.dll"],
    ],
  };
  function modulesFor(name) {
    const list = (APP_MODULES[name] || []).concat(COMMON_MODULES);
    return list.map(m => ({ name: m[0], version: m[1], path: m[2] }));
  }

  // ---- process catalog ----
  function P(o) { return o; }
  const PROCESSES = [
    P({ name:"chrome.exe", pid:8456, ppid:2340, version:"124.0.6367.61", company:"Google LLC",
        path:"C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
        cmdline:'"C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe" --type=renderer --enable-features=NetworkServiceInProcess2 --lang=zh-CN --num-raster-threads=4 --enable-gpu-rasterization --renderer-client-id=7 --mojo-platform-channel-handle=4628',
        arch:64, virtualized:false, session:1, integrity:"Medium", user:"DESKTOP-7K2M\\Admin",
        start:"2026-05-31 13:42:08.337", status:"running" }),
    P({ name:"msedge.exe", pid:7234, ppid:2340, version:"124.0.2478.51", company:"Microsoft Corporation",
        path:"C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
        cmdline:'"C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe" --profile-directory=Default --single-argument',
        arch:64, virtualized:false, session:1, integrity:"Medium", user:"DESKTOP-7K2M\\Admin",
        start:"2026-05-31 13:40:55.912", status:"running" }),
    P({ name:"explorer.exe", pid:2340, ppid:1180, version:"10.0.22621.2715", company:"Microsoft Corporation",
        path:"C:\\Windows\\explorer.exe",
        cmdline:'C:\\Windows\\Explorer.EXE',
        arch:64, virtualized:false, session:1, integrity:"Medium", user:"DESKTOP-7K2M\\Admin",
        start:"2026-05-31 13:38:02.104", status:"running" }),
    P({ name:"svchost.exe", pid:1024, ppid:780, version:"10.0.22621.2506", company:"Microsoft Corporation",
        path:"C:\\Windows\\System32\\svchost.exe",
        cmdline:'C:\\Windows\\system32\\svchost.exe -k NetworkService -p -s Dnscache',
        arch:64, virtualized:false, session:0, integrity:"System", user:"NT AUTHORITY\\SYSTEM",
        start:"2026-05-31 13:37:41.220", status:"running" }),
    P({ name:"System", pid:4, ppid:0, version:"10.0.22621.2506", company:"Microsoft Corporation",
        path:"C:\\Windows\\System32\\ntoskrnl.exe",
        cmdline:'(系统内核进程)',
        arch:64, virtualized:false, session:0, integrity:"System", user:"NT AUTHORITY\\SYSTEM",
        start:"2026-05-31 13:37:18.000", status:"running" }),
    P({ name:"Registry", pid:88, ppid:4, version:"10.0.22621.2506", company:"Microsoft Corporation",
        path:"Registry",
        cmdline:'(注册表进程)',
        arch:64, virtualized:false, session:0, integrity:"System", user:"NT AUTHORITY\\SYSTEM",
        start:"2026-05-31 13:37:18.010", status:"running" }),
    P({ name:"notepad.exe", pid:5672, ppid:2340, version:"11.2402.22.0", company:"Microsoft Corporation",
        path:"C:\\Windows\\System32\\notepad.exe",
        cmdline:'"C:\\Windows\\System32\\notepad.exe" C:\\Users\\Admin\\Documents\\notes.txt',
        arch:64, virtualized:false, session:1, integrity:"Medium", user:"DESKTOP-7K2M\\Admin",
        start:"2026-05-31 13:44:30.551", status:"running" }),
    P({ name:"powershell.exe", pid:6088, ppid:3120, version:"10.0.22621.2506", company:"Microsoft Corporation",
        path:"C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
        cmdline:'powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "Get-Process | Sort-Object CPU -Descending | Select-Object -First 10"',
        arch:64, virtualized:false, session:1, integrity:"High", user:"DESKTOP-7K2M\\Admin",
        start:"2026-05-31 13:45:01.778", status:"running" }),
    P({ name:"lsass.exe", pid:780, ppid:680, version:"10.0.22621.2506", company:"Microsoft Corporation",
        path:"C:\\Windows\\System32\\lsass.exe",
        cmdline:'C:\\Windows\\system32\\lsass.exe',
        arch:64, virtualized:false, session:0, integrity:"System", user:"NT AUTHORITY\\SYSTEM",
        start:"2026-05-31 13:37:20.330", status:"running" }),
    P({ name:"spoolsv.exe", pid:1456, ppid:780, version:"10.0.22621.2134", company:"Microsoft Corporation",
        path:"C:\\Windows\\System32\\spoolsv.exe",
        cmdline:'C:\\Windows\\System32\\spoolsv.exe',
        arch:64, virtualized:false, session:0, integrity:"System", user:"NT AUTHORITY\\SYSTEM",
        start:"2026-05-31 13:37:45.901", status:"running" }),
    P({ name:"cmd.exe", pid:3120, ppid:2340, version:"10.0.22621.2506", company:"Microsoft Corporation",
        path:"C:\\Windows\\System32\\cmd.exe",
        cmdline:'"C:\\Windows\\System32\\cmd.exe"',
        arch:64, virtualized:false, session:1, integrity:"High", user:"DESKTOP-7K2M\\Admin",
        start:"2026-05-31 13:44:58.012", status:"running" }),
  ];
  const PMAP = {}; PROCESSES.forEach(p => { p.icon = iconFor(p.name); p.modules = modulesFor(p.name); PMAP[p.pid] = p; });

  // process tree relationships (for tree dialog)
  const TREE_EXTRA = {
    1180: { name:"userinit.exe", pid:1180, desc:"用户初始化进程", user:"DESKTOP-7K2M\\Admin" },
    680:  { name:"wininit.exe", pid:680, desc:"Windows 启动应用程序", user:"NT AUTHORITY\\SYSTEM" },
    0:    { name:"(系统空闲进程)", pid:0, desc:"System Idle", user:"" },
  };

  // ---- operation catalog ----
  // cat: registry|file|network|process|thread|perf
  const OPS = {
    registry: ["RegQueryValue","RegSetValue","RegOpenKey","RegCloseKey","RegEnumKey","RegEnumValue","RegQueryKey","RegCreateKey","RegDeleteValue"],
    file: ["CreateFile","ReadFile","WriteFile","CloseFile","QueryDirectory","QueryInformationFile","ReadFileMapped","FileSystemControl","IRP_MJ_CREATE","SetEndOfFileInformation","LockFile"],
    network: ["TCP Send","TCP Receive","TCP Connect","TCP Disconnect","UDP Send","UDP Receive","TCP Reconnect"],
    process: ["Process Create","Process Start","Process Exit","Load Image"],
    thread: ["Thread Create","Thread Exit"],
    perf: ["Process Profiling"],
  };

  const REG_PATHS = [
    "HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced",
    "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
    "HKLM\\SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters",
    "HKCU\\Software\\Google\\Chrome\\BLBeacon",
    "HKLM\\SOFTWARE\\Microsoft\\Cryptography\\Defaults\\Provider",
    "HKCU\\Software\\Microsoft\\Notepad",
    "HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager",
    "HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows\\System",
    "HKCU\\Software\\Classes\\Local Settings\\Software\\Microsoft\\Windows\\Shell\\Bags",
  ];
  const FILE_PATHS = [
    "C:\\Users\\Admin\\Documents\\file.txt",
    "C:\\Users\\Admin\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache\\Cache_Data\\f_00a31b",
    "C:\\Users\\Admin\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cookies",
    "C:\\Users\\Admin\\Desktop\\*.*",
    "C:\\Users\\Admin\\Downloads\\document.pdf",
    "C:\\Windows\\System32\\drivers\\etc\\hosts",
    "\\Device\\HarddiskVolume3\\Windows\\System32\\config\\SOFTWARE",
    "C:\\Program Files\\Google\\Chrome\\Application\\124.0.6367.61\\chrome.dll",
    "C:\\Users\\Admin\\AppData\\Roaming\\Microsoft\\Windows\\Recent\\notes.lnk",
    "C:\\Windows\\Fonts\\segoeui.ttf",
    "C:\\ProgramData\\Microsoft\\Windows Defender\\Scans\\History\\Store",
  ];
  const NET_PATHS = [
    "DESKTOP-7K2M:54321 -> 142.250.72.14:443",
    "DESKTOP-7K2M:54330 -> 13.107.42.16:443",
    "192.168.1.100:54350 -> 192.168.1.1:53",
    "DESKTOP-7K2M:54360 -> 20.190.159.4:443",
    "192.168.1.100:54370 -> 142.250.72.14:80",
  ];

  const RESULTS = {
    ok: { text:"SUCCESS", cls:"res-SUCCESS" },
    nf: { text:"NAME NOT FOUND", cls:"res-error" },
    bo: { text:"BUFFER OVERFLOW", cls:"res-warn" },
    ad: { text:"ACCESS DENIED", cls:"res-error" },
    rp: { text:"REPARSE", cls:"res-info" },
    np: { text:"NO MORE ENTRIES", cls:"res-warn" },
    pending: { text:"...", cls:"res-info" },
  };

  // RNG (seeded for stable output)
  let seed = 1337;
  function rnd() { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed / 0x7fffffff; }
  function pick(a) { return a[Math.floor(rnd() * a.length)]; }
  function chance(p) { return rnd() < p; }

  function categoryOf(op) {
    for (const c in OPS) if (OPS[c].includes(op)) return c;
    return "file";
  }
  function pathForCat(cat) {
    if (cat === "registry") return pick(REG_PATHS);
    if (cat === "network") return pick(NET_PATHS);
    if (cat === "process") return pick(PROCESSES).path;
    if (cat === "thread") return "";
    if (cat === "perf") return "";
    return pick(FILE_PATHS);
  }

  // detail (其他详情) per category — key/value pairs
  function detailFor(op, cat, path) {
    if (cat === "registry") {
      if (op.startsWith("RegQuery") || op === "RegEnumValue")
        return { summary:`Type: REG_SZ, Length: ${16+Math.floor(rnd()*48)}`, kv:{ "Type":"REG_SZ", "Length":`${16+Math.floor(rnd()*48)}`, "Data":pick(["zh-CN","1","Enabled","C:\\Windows","0x00000001"]) } };
      if (op === "RegSetValue")
        return { summary:`Type: REG_DWORD, Data: ${Math.floor(rnd()*8)}`, kv:{ "Type":"REG_DWORD", "Length":"4", "Data":`${Math.floor(rnd()*8)}` } };
      if (op === "RegOpenKey" || op === "RegCreateKey")
        return { summary:`Desired Access: Read/Query Value`, kv:{ "Desired Access":"Read, Query Value", "Disposition":"REG_OPENED_EXISTING_KEY" } };
      return { summary:`Query: HandleTags`, kv:{ "Query":"HandleTags", "HandleTags":"0x0" } };
    }
    if (cat === "file") {
      if (op === "CreateFile" || op === "IRP_MJ_CREATE")
        return { summary:`Desired Access: Generic Read, Disposition: Open`, kv:{ "Desired Access":"Generic Read, Synchronize", "Disposition":"Open", "Options":"Synchronous IO Non-Alert, Non-Directory File", "Attributes":"N", "ShareMode":"Read, Write, Delete", "AllocationSize":"n/a", "OpenResult":"Opened" } };
      if (op === "ReadFile" || op === "ReadFileMapped")
        return { summary:`Offset: ${Math.floor(rnd()*65536)}, Length: ${4096*(1+Math.floor(rnd()*16))}`, kv:{ "Offset":`${Math.floor(rnd()*655360)}`, "Length":`${4096*(1+Math.floor(rnd()*16))}`, "Priority":"Normal", "I/O Flags":"Non-cached, Paging I/O" } };
      if (op === "WriteFile")
        return { summary:`Offset: ${Math.floor(rnd()*65536)}, Length: ${512*(1+Math.floor(rnd()*8))}`, kv:{ "Offset":`${Math.floor(rnd()*65536)}`, "Length":`${512*(1+Math.floor(rnd()*8))}`, "Priority":"Normal" } };
      if (op === "QueryDirectory")
        return { summary:`Filter: *.*, 1: desktop.ini`, kv:{ "Filter":"*.*", "1":"desktop.ini", "2":"file.txt", "3":"notes.txt" } };
      return { summary:`Class: FileStandardInformation`, kv:{ "Class":"FileStandardInformation", "AllocationSize":`${4096*(1+Math.floor(rnd()*40))}`, "EndOfFile":`${1000+Math.floor(rnd()*60000)}` } };
    }
    if (cat === "network") {
      const len = 256 + Math.floor(rnd()*1200);
      return { summary:`Length: ${len}, seqnum: ${Math.floor(rnd()*99999)}`, kv:{ "Length":`${len}`, "seqnum":`${Math.floor(rnd()*99999)}`, "connid":`0`, "startime":`${Math.floor(rnd()*9999)}`, "endtime":`${Math.floor(rnd()*9999)}` } };
    }
    if (cat === "process") {
      if (op === "Load Image")
        return { summary:`Image Base: 0x7ff8${Math.floor(rnd()*0xffffff).toString(16)}, Size: ${0x20000+Math.floor(rnd()*0x80000)}`, kv:{ "Image Base":`0x7ff8${Math.floor(rnd()*0xffffff).toString(16)}`, "Image Size":`0x${(0x20000+Math.floor(rnd()*0x80000)).toString(16)}` } };
      return { summary:`PID: ${1000+Math.floor(rnd()*8000)}, Command line`, kv:{ "PID":`${1000+Math.floor(rnd()*8000)}`, "Command line":path, "Status":"0x0" } };
    }
    if (cat === "thread") {
      const tid = 1000 + Math.floor(rnd()*9000);
      return { summary:`Thread ID: ${tid}`, kv:{ "Thread ID":`${tid}`, "User Time":`0.00000${Math.floor(rnd()*9)}`, "Kernel Time":`0.00000${Math.floor(rnd()*9)}` } };
    }
    return { summary:`CPU: ${(rnd()*4).toFixed(1)}%`, kv:{ "CPU":`${(rnd()*4).toFixed(1)}%`, "Private Bytes":`${(40+rnd()*400).toFixed(0)} MB` } };
  }

  // call stack generator
  const KMODS = [
    ["ntoskrnl.exe","C:\\Windows\\System32\\ntoskrnl.exe", ["NtReadFile","NtWriteFile","NtQueryValueKey","IofCallDriver","NtCreateFile","ObReferenceObjectByHandle","KiSystemServiceCopyEnd"]],
    ["FLTMGR.SYS","C:\\Windows\\System32\\drivers\\FLTMGR.SYS", ["FltpDispatch","FltpCreate","FltpPerformPreCallbacks"]],
    ["ntfs.sys","C:\\Windows\\System32\\drivers\\ntfs.sys", ["NtfsFsdRead","NtfsCommonRead","NtfsCommonCreate"]],
    ["tcpip.sys","C:\\Windows\\System32\\drivers\\tcpip.sys", ["TcpSegmentTcbSend","TcpCreateAndConnectTcbComplete"]],
  ];
  const UMODS = [
    ["ntdll.dll","C:\\Windows\\System32\\ntdll.dll", ["NtReadFile","NtWriteFile","ZwQueryValueKey","RtlUserThreadStart","LdrInitializeThunk","NtCreateFile"]],
    ["KernelBase.dll","C:\\Windows\\System32\\KernelBase.dll", ["ReadFile","WriteFile","CreateFileW","RegQueryValueExW"]],
    ["kernel32.dll","C:\\Windows\\System32\\kernel32.dll", ["ReadFileImplementation","BaseThreadInitThunk","CreateFileWImplementation"]],
  ];
  function genStack(cat) {
    const frames = [];
    const kn = 3 + Math.floor(rnd()*3);
    for (let i = 0; i < kn; i++) {
      const m = pick(KMODS);
      frames.push({ k:true, mod:m[0], func:pick(m[2]), off:Math.floor(rnd()*0x400), addr:randAddr("fffff807"), path:m[1] });
    }
    const un = 4 + Math.floor(rnd()*4);
    for (let i = 0; i < un; i++) {
      const m = pick(UMODS);
      frames.push({ k:false, mod:m[0], func:pick(m[2]), off:Math.floor(rnd()*0x600), addr:randAddr("00007ff8"), path:m[1] });
    }
    return frames.map((f, i) => ({
      frame: (f.k ? "K" : "U") + i,
      mod: f.mod,
      loc: `${f.func}+0x${f.off.toString(16).toUpperCase()}`,
      addr: f.addr,
      path: f.path,
      k: f.k,
    }));
  }
  function randAddr(prefix) {
    let s = prefix;
    for (let i = 0; i < 8; i++) s += "0123456789abcdef"[Math.floor(rnd()*16)];
    return "0x" + s;
  }

  // ---- generate the event stream ----
  const ACTIVE = [PMAP[8456], PMAP[2340], PMAP[1024], PMAP[4], PMAP[5672], PMAP[7234], PMAP[88], PMAP[780], PMAP[6088], PMAP[1456], PMAP[3120]];
  const CAT_WEIGHTS = [["file",0.40],["registry",0.30],["network",0.13],["process",0.06],["thread",0.07],["perf",0.04]];
  function weightedCat() {
    const r = rnd(); let acc = 0;
    for (const [c, w] of CAT_WEIGHTS) { acc += w; if (r <= acc) return c; }
    return "file";
  }

  const EVENTS = [];
  let t = { h:13, m:45, s:23, frac:1234567 };
  function stepTime() {
    t.frac += 80000 + Math.floor(rnd()*260000);
    if (t.frac >= 10000000) { t.frac -= 10000000; t.s++; if (t.s >= 60) { t.s = 0; t.m++; } }
    const f = String(t.frac).padStart(7, "0");
    return `${String(t.h).padStart(2,"0")}:${String(t.m).padStart(2,"0")}:${String(t.s).padStart(2,"0")}.${f}`;
  }

  const N = 240;
  for (let i = 0; i < N; i++) {
    const cat = weightedCat();
    let proc;
    if (cat === "registry") proc = pick([PMAP[8456], PMAP[5672], PMAP[1024], PMAP[2340], PMAP[7234]]);
    else if (cat === "network") proc = pick([PMAP[8456], PMAP[7234], PMAP[1024]]);
    else if (cat === "thread" || cat === "process") proc = pick([PMAP[4], PMAP[8456], PMAP[2340], PMAP[6088]]);
    else if (cat === "perf") proc = pick(ACTIVE);
    else proc = pick([PMAP[8456], PMAP[2340], PMAP[7234], PMAP[5672], PMAP[1024], PMAP[4]]);

    const op = pick(OPS[cat]);
    const path = pathForCat(cat);
    let result = RESULTS.ok;
    if (cat === "file" && chance(0.12)) result = RESULTS.nf;
    else if (cat === "registry" && chance(0.10)) result = chance(0.5) ? RESULTS.nf : RESULTS.bo;
    else if (cat === "file" && chance(0.04)) result = RESULTS.ad;
    else if (op === "QueryDirectory" && chance(0.3)) result = RESULTS.np;

    const detail = detailFor(op, cat, path);
    const dur = (rnd() * 0.0009).toFixed(7);
    const tid = 1000 + Math.floor(rnd() * 9000);

    EVENTS.push({
      idx: i + 1,
      time: stepTime(),
      date: "2026-05-31",
      proc, cat, op, path, result,
      tid,
      duration: dur,
      detail,
      stack: null, // lazy
    });
  }
  // attach lazy stack getter
  EVENTS.forEach(e => { e.getStack = function () { if (!e.stack) e.stack = genStack(e.cat); return e.stack; }; });

  // ---- category meta ----
  const CAT_META = {
    registry: { label:"注册表", en:"Registry", cls:"op-registry", color:"var(--op-registry)", zh:"注册表" },
    file:     { label:"文件系统", en:"File System", cls:"op-file", color:"var(--op-file)", zh:"文件系统" },
    network:  { label:"网络", en:"Network", cls:"op-network", color:"var(--op-network)", zh:"网络" },
    process:  { label:"进程", en:"Process", cls:"op-process", color:"var(--op-process)", zh:"进程" },
    thread:   { label:"线程", en:"Thread", cls:"op-thread", color:"var(--op-thread)", zh:"线程" },
    perf:     { label:"性能", en:"Profiling", cls:"op-perf", color:"var(--op-perf)", zh:"性能分析" },
  };

  window.PM = {
    PROCESSES, PMAP, TREE_EXTRA, EVENTS, OPS, CAT_META, iconFor, categoryOf,
    PROC_LIST_FOR_FILTER: Array.from(new Set(EVENTS.map(e => e.proc.name))).sort(),
    OP_LIST_FOR_FILTER: Array.from(new Set(EVENTS.map(e => e.op))).sort(),
  };
})();
