/* ============================================================
 * XAUUSD Terminal — browser-safe Node 1 + Node 4 endpoint configuration.
 *
 * Node 2 is a static, read-only client of two services:
 *   Node 1 — market data, built-in strategy evaluator, demo (paper) account;
 *   Node 4 — execution diagnostics (market feed, local strategy, venue).
 *
 * Runtime override, installed before this file is loaded:
 *   window.XAUUSD_CONFIG = {
 *     MARKET_HOST: "https://market.example",
 *     NODE4_HOST: "https://execution.example",
 *   };
 *
 * Only an origin is accepted; the documented public paths are fixed below.
 * No credentials, tokens, or authorization headers belong here.
 * ============================================================ */
(function (App) {
  "use strict";

  const runtime = window.XAUUSD_CONFIG && typeof window.XAUUSD_CONFIG === "object"
    ? window.XAUUSD_CONFIG
    : {};
  const own = (object, key) => Object.prototype.hasOwnProperty.call(object, key);

  const DEFAULTS = {
    MARKET_HOST: "https://engine-southeastasia-sng-main.onrender.com",
    // Node 4 execution service: GET /diagnostics and the read-only /ws stream.
    NODE4_HOST: "https://execution-southeastasia-sng-main.onrender.com",
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

  const MARKET_HOST = normaliseHost(own(runtime, "MARKET_HOST") ? runtime.MARKET_HOST : DEFAULTS.MARKET_HOST);
  const NODE4_HOST = normaliseHost(own(runtime, "NODE4_HOST") ? runtime.NODE4_HOST : DEFAULTS.NODE4_HOST);
  const websocketBase = MARKET_HOST.replace(/^https:/i, "wss:").replace(/^http:/i, "ws:");
  const node4WebsocketBase = NODE4_HOST.replace(/^https:/i, "wss:").replace(/^http:/i, "ws:");
  const endpoint = (path, websocket) => (MARKET_HOST ? `${websocket ? websocketBase : MARKET_HOST}${path}` : "");
  const executionEndpoint = (path, websocket) =>
    (NODE4_HOST ? `${websocket ? node4WebsocketBase : NODE4_HOST}${path}` : "");

  const endpoints = {
    ws: endpoint("/ws", true),
    candles: endpoint("/candles", false),
    tickVolume: endpoint("/tick-volume", false),
    levels: endpoint("/levels", false),
    calendar: endpoint("/calendar", false),
    strategy: endpoint("/strategy", false),
    demo: endpoint("/demo", false),
    status: endpoint("/status", false),
    // Node 4: the same ExecutionSnapshot over REST and WebSocket.
    executionDiagnostics: executionEndpoint("/diagnostics", false),
    executionWs: executionEndpoint("/ws", true),
  };

  App.config = {
    MARKET_HOST,
    NODE4_HOST,
    endpoints,

    // Exactly the Node 1 topics the dashboard renders, and nothing else.
    socket: {
      topics: ["levels", "candle", "tick_volume", "strategy", "demo", "calendar", "status"],
      reconnectMinMs: 1000,
      reconnectMaxMs: 30000,
      heartbeatEveryMs: 30000,
      heartbeatTimeoutMs: 75000,
      staleAfterMs: 90000,
      pollFallbackMs: 5000,
    },

    // Node 4 execution diagnostics. One topic; complete snapshots only.
    execution: {
      diagnostics: endpoints.executionDiagnostics,
      ws: endpoints.executionWs,
      topics: ["diagnostics"],
      reconnectMinMs: 1000,
      reconnectMaxMs: 30000,
      heartbeatEveryMs: 30000,
      heartbeatTimeoutMs: 75000,
      pollFallbackMs: 5000,
      pollGraceMs: 1500,
    },

    strategy: {
      staleAfterMs: 60000,
      historyLimit: 120,
      stripSize: 30,
      // Shape only: every value is unknown until /strategy (or the status
      // frame) supplies it. Rendering shows — rather than inventing defaults.
      defaultParams: {
        enabled: null,
        min_confidence: null,
        risk_pct: null,
        atr_period: null,
        atr_stop_mult: null,
        reward_risk: null,
        min_lots: null,
        max_lots: null,
        lot_step: null,
        fast_ema: null,
        slow_ema: null,
        event_blackout_secs: null,
        leverage: null,
        max_margin_pct: null,
      },
    },

    demo: {
      maxTrades: 50,
      profitFactorCap: 99,
      marginWarnPct: 200,
      marginDangerPct: 100,
    },

    calendar: {
      currency: "USD",
      refreshMs: 5 * 60 * 1000,
      keepPastMs: 6 * 60 * 60 * 1000,
      impactRank: { high: 3, medium: 2, low: 1, holiday: 0 },
    },

    levelsRefreshMs: 60 * 1000,
    candlesRefreshMs: 60 * 1000,
    statusRefreshMs: 30 * 1000,

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
