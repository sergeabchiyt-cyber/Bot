/* ============================================================
 * XAUUSD Terminal — browser-safe endpoint configuration
 *
 * Runtime overrides may be supplied before this file is loaded:
 *   window.XAUUSD_CONFIG = {
 *     MARKET_HOST: "https://market.example",
 *     STRATEGY_HOST: "https://strategy.example",
 *     EXECUTION_HOST: "https://execution.example"
 *   };
 *
 * An explicitly empty host disables that service. Only origins are accepted;
 * documented public paths are fixed below. No credentials belong here.
 * ============================================================ */
(function (App) {
  "use strict";

  const runtime = window.XAUUSD_CONFIG && typeof window.XAUUSD_CONFIG === "object"
    ? window.XAUUSD_CONFIG
    : {};
  const own = (object, key) => Object.prototype.hasOwnProperty.call(object, key);

  // These hosts are taken from the checked-in Node deployment documentation:
  // Node 1's market URL, Node 4's NODE3_WS_URL, and Node 4's frontend resource
  // guide respectively. They can all be replaced at runtime without a build.
  const DEFAULTS = {
    MARKET_HOST: "https://engine-southeastasia-sng-main.onrender.com",
    STRATEGY_HOST: "https://strategy-southeastasia-sng-main.onrender.com",
    EXECUTION_HOST: "https://execution-southeastasia-sng-main.onrender.com",
  };

  function normaliseHost(value) {
    if (value === null || value === false) return "";
    const raw = String(value == null ? "" : value).trim();
    if (!raw) return "";
    const candidate = /^(https?|wss?):\/\//i.test(raw) ? raw : `https://${raw}`;
    try {
      const url = new URL(candidate);
      if (!["http:", "https:", "ws:", "wss:"].includes(url.protocol)) return "";
      if (url.username || url.password || url.search || url.hash || (url.pathname && url.pathname !== "/")) return "";
      const protocol = url.protocol === "ws:" ? "http:" : url.protocol === "wss:" ? "https:" : url.protocol;
      return `${protocol}//${url.host}`;
    } catch {
      return "";
    }
  }

  function resolveHost(name) {
    const value = own(runtime, name) ? runtime[name] : DEFAULTS[name];
    return normaliseHost(value);
  }

  function websocketBase(host) {
    if (!host) return "";
    return host.replace(/^https:/i, "wss:").replace(/^http:/i, "ws:");
  }

  const MARKET_HOST = resolveHost("MARKET_HOST");
  const STRATEGY_HOST = resolveHost("STRATEGY_HOST");
  const EXECUTION_HOST = resolveHost("EXECUTION_HOST");

  function endpoint(host, path, websocket) {
    if (!host) return "";
    const base = websocket ? websocketBase(host) : host;
    return `${base}${path}`;
  }

  const endpoints = {
    // Node 1 — market data only.
    marketWs: endpoint(MARKET_HOST, "/ws", true),
    candles: endpoint(MARKET_HOST, "/candles", false),
    tickVolume: endpoint(MARKET_HOST, "/tick-volume", false),
    levels: endpoint(MARKET_HOST, "/levels", false),
    calendar: endpoint(MARKET_HOST, "/calendar", false),

    // Node 3 — public strategy diagnostics only; the private intent/report link is not used here.
    strategyWs: endpoint(STRATEGY_HOST, "/ws", true),
    strategyDiagnostics: endpoint(STRATEGY_HOST, "/diagnostics", false),
    strategyScanning: endpoint(STRATEGY_HOST, "/scanning", false),
    strategySignals: endpoint(STRATEGY_HOST, "/signals", false),

    // Node 4 — public, read-only execution and broker snapshots.
    executionWs: endpoint(EXECUTION_HOST, "/ws", true),
    executionDiagnostics: endpoint(EXECUTION_HOST, "/diagnostics", false),
    executionOpenTrades: endpoint(EXECUTION_HOST, "/open-trades", false),
    executionAccount: endpoint(EXECUTION_HOST, "/account", false),
    executionDeriv: endpoint(EXECUTION_HOST, "/deriv", false),
    mt5Account: endpoint(EXECUTION_HOST, "/mt5/account", false),
    mt5Positions: endpoint(EXECUTION_HOST, "/mt5/positions", false),
    mt5History: endpoint(EXECUTION_HOST, "/mt5/history", false),
    mt5Status: endpoint(EXECUTION_HOST, "/mt5/status", false),
  };

  App.config = {
    MARKET_HOST,
    STRATEGY_HOST,
    EXECUTION_HOST,
    endpoints,

    market: {
      topics: ["levels", "candle", "bubbles", "trades", "calendar", "tick_volume", "sentiment"],
      reconnectMinMs: 1000,
      reconnectMaxMs: 30000,
      staleAfterMs: 90000,
      heartbeatIntervalMs: 20000,
      heartbeatTimeoutMs: 75000,
      snapshotRefreshMs: 60000,
    },

    strategy: {
      topics: ["diagnostics", "scanning", "signals", "diagnostic_event"],
      reconnectMinMs: 1000,
      reconnectMaxMs: 30000,
      staleAfterMs: 45000,
      heartbeatIntervalMs: 20000,
      heartbeatTimeoutMs: 75000,
      snapshotRefreshMs: 20000,
    },

    execution: {
      // `trades` is the Node 4 subscription topic. Its wire frame type is
      // `trade` (the exact public WsFrame schema).
      topics: [
        "diagnostics", "open_trades", "trades", "deriv_account",
        "mt5_account", "mt5_positions", "mt5_history", "bridge_status",
        "bridge_event", "diagnostic_event", "heartbeat",
      ],
      reconnectMinMs: 1000,
      reconnectMaxMs: 30000,
      staleAfterMs: 45000,
      heartbeatIntervalMs: 20000,
      heartbeatTimeoutMs: 75000,
      snapshotRefreshMs: 30000,
      brokerFreshMs: 15000,
      maxRows: 100,
    },

    calendar: {
      currency: "USD",
      refreshMs: 5 * 60 * 1000,
      keepPastMs: 6 * 60 * 60 * 1000,
      impactRank: { high: 3, medium: 2, low: 1, holiday: 0 },
    },

    levelsRefreshMs: 60 * 1000,
    marketSnapshotRefreshMs: 60 * 1000,

    bubbles: {
      maxOnChart: 60,
      maxRows: 40,
      scaleCap: 5000,
      scaleFloor: 100,
      decayEveryMs: 30000,
      decayFactor: 0.95,
    },

    levels: {
      PW: {
        poc: { color: "#F0B90B", title: "PW PoC", cls: "poc", dashed: false },
        vah: { color: "#26A69A", title: "PW VaH", cls: "vah", dashed: false },
        val: { color: "#EF5350", title: "PW VaL", cls: "val", dashed: false },
      },
      PS: {
        poc: { color: "#58A6FF", title: "PS PoC", cls: "ps-poc", dashed: false },
      },
      CW: {
        poc: { color: "#A371F7", title: "CW PoC", cls: "cw-poc", dashed: true },
        vah: { color: "#F778BA", title: "CW VaH", cls: "cw-vah", dashed: true },
        val: { color: "#22D3EE", title: "CW VaL", cls: "cw-val", dashed: true },
      },
      SWING_BULL: {
        poc: { color: "#26A69A", title: "Bull Swing PoC", cls: "swing-bull-poc", dashed: false },
        vah: { color: "#22C55E", title: "Bull Swing VaH", cls: "swing-bull-vah", dashed: true },
        val: { color: "#86EFAC", title: "Bull Swing VaL", cls: "swing-bull-val", dashed: true },
      },
      SWING_BEAR: {
        poc: { color: "#EF5350", title: "Bear Swing PoC", cls: "swing-bear-poc", dashed: false },
        vah: { color: "#F97316", title: "Bear Swing VaH", cls: "swing-bear-vah", dashed: true },
        val: { color: "#FDBA74", title: "Bear Swing VaL", cls: "swing-bear-val", dashed: true },
      },
    },
    levelWindows: ["PW", "PS", "CW", "SWING_BULL", "SWING_BEAR"],
    swingWindows: ["SWING_BULL", "SWING_BEAR"],
    swingLabels: { SWING_BULL: "Bull Swing", SWING_BEAR: "Bear Swing" },
    levelKinds: ["poc", "vah", "val"],
  };
})((window.App = window.App || {}));
