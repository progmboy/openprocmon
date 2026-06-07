/* ============ OpenProcmon — i18n (bilingual 中/EN) ============ */
/* Technical fields (process names, paths, operations, results) stay English.
   Only UI chrome is translated. tr(zh, en) picks by the active language. */
(function () {
  window.__OPM_LANG = localStorage.getItem("opm-lang") || "zh";
  window.tr = function (zh, en) {
    return window.__OPM_LANG === "en" ? (en === undefined ? zh : en) : zh;
  };
  // display formatters — toggled by Settings (read synchronously during render)
  window.__OPM_HEX_IDS = false;     // hex display for Thread/Process IDs
  window.__OPM_HEX_OFFSET = false;  // hex display for FileOffset/Length
  window.fmtId = function (n) {
    const v = parseInt(n, 10);
    if (!window.__OPM_HEX_IDS || isNaN(v)) return String(n);
    return "0x" + v.toString(16).toUpperCase();
  };
  window.fmtOff = function (n) {
    const v = typeof n === "number" ? n : parseInt(String(n).replace(/[^0-9]/g, ""), 10);
    if (!window.__OPM_HEX_OFFSET || isNaN(v)) return String(n);
    return "0x" + v.toString(16).toUpperCase();
  };
})();
