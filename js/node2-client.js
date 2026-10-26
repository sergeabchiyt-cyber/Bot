/* Node 2's sole typed, read-only HTTP/WebSocket client. */
(function (root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) module.exports = api;
  if (root) root.Node2Client = api;
})(typeof window !== "undefined" ? window : globalThis, function () {
  "use strict";

  const defaults = Object.freeze({
    NODE1_BASE_URL: "https://engine-southeastasia-sng-main.onrender.com",
    NODE3_BASE_URL: "https://strategy-api-vp-levels.onrender.com",
  });

  function baseConfig(runtime) {
    const input = runtime && typeof runtime === "object" ? runtime : {};
    function origin(key) {
      const value = String(input[key] || defaults[key]).trim();
      if (!value) return "";
      try {
        const url = new URL(value);
        if (!/^https?:$/.test(url.protocol) || url.username || url.password || url.search || url.hash || (url.pathname && url.pathname !== "/")) return "";
        return `${url.protocol}//${url.host}`;
      } catch { return ""; }
    }
    return Object.freeze({ node1: origin("NODE1_BASE_URL"), node3: origin("NODE3_BASE_URL") });
  }

  const isObject = (v) => !!v && typeof v === "object" && !Array.isArray(v);
  const requiredArray = (v, name) => {
    if (!Array.isArray(v)) throw new TypeError(`${name} must be an array`);
    return v;
  };
  const requiredObject = (v, name) => {
    if (!isObject(v)) throw new TypeError(`${name} must be an object`);
    return v;
  };
  const optionalNumber = (value) => value == null || value === "" || !Number.isFinite(Number(value)) ? null : Number(value);
  const optionalText = (value) => value == null ? null : String(value);

  function parseHealth(value) {
    const v = requiredObject(value, "health");
    return { status: optionalText(v.status), freshness_state: optionalText(v.freshness_state),
      tradeable: v.tradeable === true, node1_connected: v.node1_connected === true,
      vp_ready: v.vp_ready === true, contract_violations: optionalNumber(v.contract_violations) ?? 0,
      intents_pending: optionalNumber(v.intents_pending) ?? 0, node4_connected: v.node4_connected === true,
      node4_token_configured: v.node4_token_configured === true, uptime_secs: optionalNumber(v.uptime_secs),
      contract_version: optionalText(v.contract_version) };
  }
  function stripSensitive(value) {
    if (Array.isArray(value)) return value.map(stripSensitive);
    if (!isObject(value)) return value;
    const clean = {};
    for (const [key, item] of Object.entries(value)) {
      if (/token|secret|password|credential|authorization/i.test(key)) continue;
      clean[key] = stripSensitive(item);
    }
    return clean;
  }
  function parseLevels(value) {
    const rows = Array.isArray(value) ? value : requiredArray(value && value.levels, "levels");
    return rows.map((row) => {
      requiredObject(row, "level row");
      if (!row.window || !["PW", "PS", "CW"].includes(row.window)) throw new TypeError("level row has invalid window");
      // `vah` and `val` deliberately remain absent/null for PS (D5).
      return { ...stripSensitive(row), poc: optionalNumber(row.poc), vah: row.window === "PS" ? null : optionalNumber(row.vah),
        val: row.window === "PS" ? null : optionalNumber(row.val) };
    });
  }
  function parseDrawings(value) {
    const v = requiredObject(value, "drawings");
    return requiredArray(v.items, "drawings.items").map((item) => {
      requiredObject(item, "drawing item");
      if (!item.id || !item.kind) throw new TypeError("drawing needs id and kind");
      return { ...stripSensitive(item), rows: Array.isArray(item.rows) ? item.rows.map(stripSensitive) : [] };
    });
  }
  function parseScanning(value) {
    const v = requiredObject(value, "scanning");
    return { ...stripSensitive(v), setups: requiredArray(v.setups, "scanning.setups").map(stripSensitive) };
  }
  function parseSignals(value) {
    const v = requiredObject(value, "signals");
    const clean = stripSensitive(v);
    return { ...clean, pending: requiredArray(clean.pending || [], "signals.pending").map(stripSensitive), recent: requiredArray(clean.recent || [], "signals.recent").map(stripSensitive),
      execution_reports: requiredArray(clean.execution_reports || [], "signals.execution_reports").map(stripSensitive) };
  }
  function parseDiagnostics(value) { return stripSensitive(requiredObject(value, "diagnostics")); }
  function parseCandles(value) {
    const rows = Array.isArray(value) ? value : requiredArray(value && value.candles, "candles");
    return rows.map((bar) => {
      requiredObject(bar, "candle");
      for (const field of ["time", "open", "high", "low", "close"]) {
        if (optionalNumber(bar[field]) === null) throw new TypeError(`candle.${field} must be numeric`);
      }
      return { ...stripSensitive(bar), time: Number(bar.time), open: Number(bar.open), high: Number(bar.high), low: Number(bar.low),
        close: Number(bar.close), volume: optionalNumber(bar.volume), closed: bar.closed !== false };
    });
  }
  function parseTickVolume(value) { return stripSensitive(requiredObject(value, "tick volume")); }
  function parseStatus(value) { return stripSensitive(requiredObject(value, "status")); }
  function parseCalendar(value) {
    if (Array.isArray(value)) return value.map(stripSensitive);
    const v = stripSensitive(requiredObject(value, "calendar"));
    return Array.isArray(v.events) ? v.events : Array.isArray(v.items) ? v.items : [];
  }
  function parseVp(value) { return requiredObject(value, "vp"); }

  const parsers = Object.freeze({ health: parseHealth, levels: parseLevels, drawings: parseDrawings,
    scanning: parseScanning, signals: parseSignals, diagnostics: parseDiagnostics,
    candles: parseCandles, tickVolume: parseTickVolume, calendar: parseCalendar, vp: parseVp });

  function createClient(runtime, transport) {
    const bases = baseConfig(runtime);
    const request = transport || (typeof fetch === "function" ? fetch.bind(globalThis) : null);
    async function get(service, path, query, parser) {
      const base = service === "node1" ? bases.node1 : bases.node3;
      if (!base) throw new Error(`${service === "node1" ? "Node 1" : "Node 3"} base URL is not configured`);
      const url = new URL(path, `${base}/`);
      if (query) for (const [key, value] of Object.entries(query)) if (value != null && value !== "") url.searchParams.set(key, String(value));
      if (!request) throw new Error("fetch is not supported by this browser");
      const response = await request(url.toString(), { method: "GET", cache: "no-store", headers: { Accept: "application/json" } });
      if (!response.ok) {
        let detail = "";
        try { detail = await response.text(); } catch { /* no body */ }
        throw new Error(`${path} returned HTTP ${response.status}${detail ? `: ${detail}` : ""}`);
      }
      const data = await response.json();
      return parser(data);
    }
    const node1 = {
      candles: (interval) => {
        if (!["1m", "15m", "1h"].includes(interval)) throw new TypeError("unsupported candle interval");
        return get("node1", "/candles", { interval }, parseCandles);
      },
      tickVolume: () => get("node1", "/tick-volume", null, parseTickVolume),
      calendar: () => get("node1", "/calendar", null, parseCalendar),
      status: () => get("node1", "/status", null, parseStatus),
    };
    const node3 = {
      health: () => get("node3", "/health", null, parseHealth),
      levels: () => get("node3", "/levels", null, parseLevels),
      drawings: () => get("node3", "/drawings", null, parseDrawings),
      vp: (params) => get("node3", "/vp", params, parseVp),
      scanning: () => get("node3", "/scanning", null, parseScanning),
      signals: () => get("node3", "/signals", null, parseSignals),
      diagnostics: () => get("node3", "/diagnostics", null, parseDiagnostics),
    };
    function websocketUrl(service, path = "/ws") {
      const base = service === "node1" ? bases.node1 : bases.node3;
      if (!base) return "";
      return base.replace(/^https:/, "wss:").replace(/^http:/, "ws:") + path;
    }
    return Object.freeze({ bases, node1: Object.freeze(node1), node3: Object.freeze(node3), websocketUrl, get });
  }

  return Object.freeze({ defaults, baseConfig, createClient, parsers,
    parseHealth, parseLevels, parseDrawings, parseScanning, parseSignals, parseDiagnostics,
    parseCandles, parseTickVolume, parseCalendar, parseVp });
});
