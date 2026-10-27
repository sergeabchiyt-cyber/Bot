/* ============================================================
 * XAUUSD Terminal — the one browser-safe base-URL configuration.
 *
 * Node 2 talks to exactly two read-only services. Paths and typed parsers
 * live in js/client.js; this file contains only origins and public settings.
 *
 * Install an override before this script when a deployment moves:
 *   window.XAUUSD_CONFIG = { NODE1_HOST: "https://...", NODE3_HOST: "https://..." };
 *
 * `MARKET_HOST` is accepted as a legacy alias for NODE1_HOST. A `NODE4_*`
 * key is intentionally ignored: Node 2 has no Node 4 client or credential.
 * ============================================================ */
(function (root, factory) {
  "use strict";

  const config = factory(root);
  if (typeof module === "object" && module.exports) module.exports = config;
  if (root) {
    root.App = root.App || {};
    root.App.config = config;
  }
})(typeof window !== "undefined" ? window : null, function (root) {
  "use strict";

  const runtime = root && root.XAUUSD_CONFIG && typeof root.XAUUSD_CONFIG === "object"
    ? root.XAUUSD_CONFIG
    : {};
  const own = (object, key) => Object.prototype.hasOwnProperty.call(object, key);

  const DEFAULTS = {
    NODE1_HOST: "https://engine-southeastasia-sng-main.onrender.com",
    NODE3_HOST: "https://strategy-api-vp-levels.onrender.com",
  };

  /** Accept an absolute origin or a same-origin proxy prefix only. */
  function normaliseBase(value) {
    if (value === null || value === false) return "";
    const raw = String(value == null ? "" : value).trim();
    if (!raw) return "";

    if (raw.startsWith("/")) {
      if (raw.includes("?") || raw.includes("#") || raw.includes("@") || !/^\/[A-Za-z0-9._~/:-]*$/.test(raw)) return "";
      return raw.replace(/\/+$/, "");
    }

    const candidate = /^(https?|wss?):\/\//i.test(raw) ? raw : `https://${raw}`;
    try {
      const url = new URL(candidate);
      if (!["http:", "https:", "ws:", "wss:"].includes(url.protocol)) return "";
      if (url.username || url.password || url.search || url.hash ||
          (url.pathname && url.pathname !== "/")) return "";
      const protocol = url.protocol === "ws:" ? "http:"
        : url.protocol === "wss:" ? "https:" : url.protocol;
      return `${protocol}//${url.host}`;
    } catch {
      return "";
    }
  }

  const NODE1_HOST = normaliseBase(
    own(runtime, "NODE1_HOST") ? runtime.NODE1_HOST
      : own(runtime, "MARKET_HOST") ? runtime.MARKET_HOST
        : DEFAULTS.NODE1_HOST
  );
  const NODE3_HOST = normaliseBase(
    own(runtime, "NODE3_HOST") ? runtime.NODE3_HOST : DEFAULTS.NODE3_HOST
  );

  const node1Topics = ["candle", "tick_volume", "calendar", "status"];
  const node3Topics = ["levels", "drawing", "vp", "scanning", "signals", "diagnostics"];

  return {
    // One place for every base URL. The client turns these into fixed paths.
    baseUrls: { node1: NODE1_HOST, node3: NODE3_HOST },
    services: {
      node1: {
        label: "Node 1",
        base: NODE1_HOST,
        contract: "2.0",
        topics: node1Topics.slice(),
      },
      node3: {
        label: "Node 3",
        base: NODE3_HOST,
        contract: "2.0",
        topics: node3Topics.slice(),
      },
    },

    // Compatibility aliases. No endpoint map exists: paths are typed methods.
    NODE1_HOST,
    NODE3_HOST,
    MARKET_HOST: NODE1_HOST,

    stream: {
      reconnectMinMs: 1000,
      reconnectMaxMs: 30000,
      heartbeatEveryMs: 30000,
      heartbeatTimeoutMs: 75000,
      staleAfterMs: 90000,
      pollFallbackMs: 5000,
      watchdogEveryMs: 5000,
    },
    requestTimeoutMs: 12000,
    refreshMs: 30000,
    calendarRefreshMs: 5 * 60 * 1000,
    calendarKeepPastMs: 6 * 60 * 60 * 1000,
    calendar: {
      currency: "USD",
      refreshMs: 5 * 60 * 1000,
      keepPastMs: 6 * 60 * 60 * 1000,
      impactRank: { high: 3, medium: 2, low: 1, holiday: 0 },
    },
    chart: {
      intervals: ["1m", "15m", "1h"],
      interval: "15m",
      bucketSeconds: { "1m": 60, "15m": 900, "1h": 3600 },
    },
    expectedContractVersion: "2.0",
  };
});
