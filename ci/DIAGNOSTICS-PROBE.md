# Node 4 diagnostics probe

Recorded by `.github/workflows/diagnostics-probe.yml` on 2026-10-24 14:29 UTC.

```
=== Target: https://execution-southeastasia-sng-main.onrender.com
GET https://execution-southeastasia-sng-main.onrender.com/diagnostics -> HTTP 200 (29875 bytes)
  header server: cloudflare
  header cf-ray: a4f9ad291c2b1f56-LAX
  header x-render-origin-server: Render
  header content-type: application/json

--- jq check on GET /diagnostics ---
{
  "build": "xauusd-node3-execution 1.0.0",
  "role": null,
  "has_market_feed": false,
  "has_strategy": false,
  "keys": [
    "deriv_account",
    "open_trades",
    "recent_events",
    "scanning",
    "timestamp",
    "work"
  ]
}

--- work & node3 excerpt ---
{
  "work": {
    "service": "xauusd-node3-execution",
    "version": "1.0.0",
    "venue": "DerivDemo",
    "started_at": 1791398084400,
    "uptime_secs": 1454110,
    "node1_ws_url": "wss://engine-southeastasia-sng-main.onrender.com/ws",
    "node1_connected": true,
    "node1_state": "connected",
    "node1_reconnect_count": 78,
    "last_node1_msg_ts": 1792852168587,
    "last_candle_ts": 1792841938604,
    "last_levels_ts": 1792841938601,
    "candles_received": 6849614,
    "levels_received": 840,
    "ws_messages_received": 6899031,
    "candle_buffer_len": 30,
    "candle_buffer_capacity": 30,
    "last_price": 3998.16,
    "last_candle": {
      "time": 1792833300000,
      "open": 3998.16,
      "high": 3998.16,
      "low": 3998.16,
      "close": 3998.16,
      "volume": 77.0,
      "source": "sifting"
    },
    "atr": 0.3179310344826419,
    "atr_pips": 31.793103448264194,
    "current_sl_pips": 202.0,
    "current_tp_pips": 604.0,
    "current_rr": 2.99009900990099,
    "volume_threshold": 3500.0,
    "last_candle_volume": 77.0,
    "volume_ratio": 0.022,
    "sl_min_pips": 200.0,
    "sl_max_pips": 300.0,
    "tp_min_pips": 600.0,
    "tp_max_pips": 800.0,
    "rr_min": 2.0,
    "rr_max": 3.0,
    "order_size": 0.5,
    "breaks_detected": 521,
    "breaks_invalidated": 211,
    "retests_rejected_low_volume": 800716,
    "signals_confirmed": 310,
    "trades_executed": 213,
    "trades_failed": 97,
    "last_signal_ts": 1792638412510,
    "last_error": "Deriv WS disconnected \u2014 reconnecting",
    "ws_clients_connected": 1
  },
  "node3": null,
  "has_market_feed": false,
  "has_strategy": false,
  "market_feed_state": null,
  "strategy_mode": null,
  "recent_events": [
    {
      "timestamp": 1792841938601,
      "level": "info",
      "category": "levels",
      "message": "Updated SWING_BEAR levels (PoC: 3993.61, VaH: 3993.62, VaL: 3993.61)"
    },
    {
      "timestamp": 1792841938601,
      "level": "info",
      "category": "levels",
      "message": "Updated CW levels (PoC: 4121.19, VaH: 4182.51, VaL: 4105.23)"
    },
    {
      "timestamp": 1792841938601,
      "level": "info",
      "category": "levels",
      "message": "Updated PS levels (PoC: 4023.34, VaH: 4030.32, VaL: 4004.67)"
    },
    {
      "timestamp": 1792841938595,
      "level": "info",
      "category": "levels",
      "message": "Updated PW levels (PoC: 4156.69, VaH: 4183.25, VaL: 4141.44)"
    },
    {
      "timestamp": 1792841938590,
      "level": "info",
      "category": "node1",
      "message": "Node 1 stream state: connected"
    }
  ]
}

--- WS wss://execution-southeastasia-sng-main.onrender.com/ws subscribe diagnostics ---
{
  "ok": true,
  "frame_type": "diagnostics",
  "frames_seen": [
    "diagnostics"
  ],
  "summary": {
    "build": "xauusd-node3-execution 1.0.0",
    "role": null,
    "has_market_feed": false,
    "has_strategy": false,
    "keys": [
      "deriv_account",
      "open_trades",
      "recent_events",
      "scanning",
      "timestamp",
      "work"
    ]
  }
}

=== Target: https://strategy-southeastasia-sng-main.onrender.com
GET https://strategy-southeastasia-sng-main.onrender.com/diagnostics -> HTTP 404 (10 bytes)
  header server: cloudflare
  header cf-ray: a4f9ad51ef46db5e-LAX
  header content-type: text/plain; charset=utf-8
Body: Not Found


--- WS wss://strategy-southeastasia-sng-main.onrender.com/ws subscribe diagnostics ---
{
  "ok": false,
  "error": "InvalidStatus: server rejected WebSocket connection: HTTP 404"
}

=== Node 2 deployed config.js ===
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
```
