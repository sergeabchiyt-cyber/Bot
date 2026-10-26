# Node 4 `/diagnostics` Live & Local Probe Report

Recorded at: `2026-10-26 12:33:35 UTC`

## 1. Deployed Render Services

### Node 4 (Render default) (`https://execution-southeastasia-sng-main.onrender.com`)

#### HTTP `GET /diagnostics`
```json
{
  "ok": true,
  "status": 200,
  "url": "https://execution-southeastasia-sng-main.onrender.com/diagnostics",
  "headers": {
    "Date": "Mon, 26 Oct 2026 12:33:35 GMT",
    "Content-Type": "application/json",
    "Server": "cloudflare",
    "x-render-origin-server": "Render",
    "CF-RAY": "a5097d857a9a6a44-MSP"
  },
  "raw_len": 31453,
  "summary": {
    "build": "xauusd-node3-execution 1.0.0",
    "role": null,
    "has_risk": false,
    "no_market_feed": true,
    "no_strategy": true,
    "keys": [
      "deriv_account",
      "open_trades",
      "recent_events",
      "scanning",
      "timestamp",
      "work"
    ]
  },
  "work": {
    "service": "xauusd-node3-execution",
    "version": "1.0.0",
    "venue": "DerivDemo",
    "started_at": 1791398084400,
    "uptime_secs": 1619931,
    "node1_ws_url": "wss://engine-southeastasia-sng-main.onrender.com/ws",
    "node1_connected": true,
    "node1_state": "connected",
    "node1_reconnect_count": 78,
    "last_node1_msg_ts": 1793018015744,
    "last_candle_ts": 1793018015744,
    "last_levels_ts": 1793016900216,
    "candles_received": 7259139,
    "levels_received": 872,
    "ws_messages_received": 7314116,
    "candle_buffer_len": 30,
    "candle_buffer_capacity": 30,
    "last_price": 4024.89,
    "last_candle": {
      "time": 1793017800000,
      "open": 4021.25,
      "high": 4024.99,
      "low": 4019.65,
      "close": 4024.89,
      "volume": 1895.0,
      "source": "sifting"
    },
    "atr": 5.3386206896548805,
    "atr_pips": 533.862068965488,
    "current_sl_pips": 202.65280327643978,
    "current_tp_pips": 605.3056065528796,
    "current_rr": 2.986909614702832,
    "volume_threshold": 3500.0,
    "last_candle_volume": 1895.0,
    "volume_ratio": 0.5414285714285715,
    "sl_min_pips": 200.0,
    "sl_max_pips": 300.0,
    "tp_min_pips": 600.0,
    "tp_max_pips": 800.0,
    "rr_min": 2.0,
    "rr_max": 3.0,
    "order_size": 0.5,
    "breaks_detected": 535,
    "breaks_invalidated": 219,
    "retests_rejected_low_volume": 826442,
    "signals_confirmed": 316,
    "trades_executed": 219,
    "trades_failed": 97,
    "last_signal_ts": 1792982257419,
    "last_error": "Deriv WS disconnected \u2014 reconnecting",
    "ws_clients_connected": 0
  },
  "node3": null,
  "risk": null
}
```

#### WebSocket `/ws` (`{"type":"subscribe","topics":["diagnostics"]}`)
```json
{
  "ok": true,
  "ws_url": "wss://execution-southeastasia-sng-main.onrender.com/ws",
  "frames": [
    {
      "type": "diagnostics",
      "top_keys": [
        "data",
        "type"
      ],
      "summary": {
        "build": "xauusd-node3-execution 1.0.0",
        "role": null,
        "has_risk": false,
        "no_market_feed": true,
        "no_strategy": true,
        "keys": [
          "deriv_account",
          "open_trades",
          "recent_events",
          "scanning",
          "timestamp",
          "work"
        ]
      },
      "work": {
        "service": "xauusd-node3-execution",
        "version": "1.0.0",
        "venue": "DerivDemo",
        "started_at": 1791398084400,
        "uptime_secs": 1619932,
        "node1_ws_url": "wss://engine-southeastasia-sng-main.onrender.com/ws",
        "node1_connected": true,
        "node1_state": "connected",
        "node1_reconnect_count": 78,
        "last_node1_msg_ts": 1793018016483,
        "last_candle_ts": 1793018016483,
        "last_levels_ts": 1793016900216,
        "candles_received": 7259146,
        "levels_received": 872,
        "ws_messages_received": 7314123,
        "candle_buffer_len": 30,
        "candle_buffer_capacity": 30,
        "last_price": 4024.95,
        "last_candle": {
          "time": 1793017800000,
          "open": 4021.25,
          "high": 4024.99,
          "low": 4019.65,
          "close": 4024.95,
          "volume": 1902.0,
          "source": "sifting"
        },
        "atr": 5.339999999999691,
        "atr_pips": 533.9999999999691,
        "current_sl_pips": 202.65344911116892,
        "current_tp_pips": 605.3068982223378,
        "current_rr": 2.986906469528119,
        "volume_threshold": 3500.0,
        "last_candle_volume": 1902.0,
        "volume_ratio": 0.5434285714285715,
        "sl_min_pips": 200.0,
        "sl_max_pips": 300.0,
        "tp_min_pips": 600.0,
        "tp_max_pips": 800.0,
        "rr_min": 2.0,
        "rr_max": 3.0,
        "order_size": 0.5,
        "breaks_detected": 535,
        "breaks_invalidated": 219,
        "retests_rejected_low_volume": 826442,
        "signals_confirmed": 316,
        "trades_executed": 219,
        "trades_failed": 97,
        "last_signal_ts": 1792982257419,
        "last_error": "Deriv WS disconnected \u2014 reconnecting",
        "ws_clients_connected": 1
      },
      "has_risk": false,
      "no_market_feed": true,
      "no_strategy": true
    }
  ]
}
```

### Node 3 (Render default) (`https://strategy-southeastasia-sng-main.onrender.com`)

#### HTTP `GET /diagnostics`
```json
{
  "ok": false,
  "url": "https://strategy-southeastasia-sng-main.onrender.com/diagnostics",
  "status": 404,
  "error": "HTTPError 404",
  "body": "Not Found\n"
}
```

#### WebSocket `/ws` (`{"type":"subscribe","topics":["diagnostics"]}`)
```json
{
  "ok": false,
  "ws_url": "wss://strategy-southeastasia-sng-main.onrender.com/ws",
  "error": "InvalidStatus: server rejected WebSocket connection: HTTP 404",
  "frames": []
}
```

## 2. Current Branch `node4-execution` Binary Verification

```json
{
  "unconfigured_fail_closed_mode": {
    "http": {
      "ok": true,
      "status": 200,
      "url": "http://127.0.0.1:19090/diagnostics",
      "headers": {
        "Content-Type": "application/json"
      },
      "raw_len": 4545,
      "summary": {
        "build": "xauusd-node4-execution 1.0.0",
        "role": "execution_only",
        "has_risk": true,
        "no_market_feed": true,
        "no_strategy": true,
        "keys": [
          "deriv_account",
          "mt5",
          "node3",
          "open_trades",
          "recent_events",
          "risk",
          "timestamp",
          "work"
        ]
      },
      "work": {
        "service": "xauusd-node4-execution",
        "version": "1.0.0",
        "role": "execution_only",
        "venue": "none",
        "started_at": 1793018022698,
        "uptime_secs": 0,
        "execution_protocol_version": 1,
        "ledger_path": "/tmp/tmprp158gwl/ledger.jsonl",
        "ledger_available": true,
        "ledger_records": 0,
        "ledger_intents": 0,
        "intents_received": 0,
        "intents_accepted": 0,
        "intents_rejected": 0,
        "intents_expired": 0,
        "intents_duplicate": 0,
        "orders_placed": 0,
        "orders_filled": 0,
        "orders_partial": 0,
        "orders_rejected": 0,
        "orders_unknown": 0,
        "orders_reconciled": 0,
        "last_intent_ts": null,
        "last_report_ts": null,
        "last_error": "NODE3_WS_URL is not set \u2014 Node 4 is an outbound client of the strategy service and cannot accept intents without it",
        "ws_clients_connected": 0
      },
      "node3": {
        "url": "",
        "token_configured": false,
        "protocol_version": 1,
        "connected": false,
        "authenticated": false,
        "state": "unconfigured",
        "session_age_secs": null,
        "last_message_ts": null,
        "last_hello_ack_ts": null,
        "last_report_ts": null,
        "reconnect_count": 0,
        "frames_received": 0,
        "reports_sent": 0,
        "reports_acked": 0,
        "last_error": "NODE3_WS_URL is not set"
      },
      "risk": {
        "enabled": true,
        "sl_min_pips": 200.0,
        "sl_max_pips": 300.0,
        "tp_min_pips": 600.0,
        "tp_max_pips": 800.0,
        "rr_min": 2.0,
        "rr_max": 3.0,
        "pip_value": 0.01,
        "contract_size": 100.0,
        "max_risk_usd": null,
        "max_daily_loss_usd": null,
        "max_drawdown_pct": null,
        "max_open_positions": 3,
        "max_lots": 1.0,
        "cooldown_losses": 3,
        "cooldown_secs": 900,
        "open_positions": 0,
        "exposure_lots": 0.0,
        "daily_pnl_usd": 0.0,
        "realised_pnl_usd": 0.0,
        "current_equity_usd": 0.0,
        "peak_equity_usd": 0.0,
        "drawdown_pct": 0.0,
        "consecutive_losses": 0,
        "cooldown_until_ms": null,
        "intents_checked": 0,
        "intents_rejected": 0,
        "last_rejection_code": null,
        "last_rejection_message": null,
        "updated_at_ms": 0
      }
    },
    "ws": {
      "ok": true,
      "ws_url": "ws://127.0.0.1:19090/ws",
      "frames": [
        {
          "type": "diagnostics",
          "top_keys": [
            "data",
            "type"
          ],
          "summary": {
            "build": "xauusd-node4-execution 1.0.0",
            "role": "execution_only",
            "has_risk": true,
            "no_market_feed": true,
            "no_strategy": true,
            "keys": [
              "deriv_account",
              "mt5",
              "node3",
              "open_trades",
              "recent_events",
              "risk",
              "timestamp",
              "work"
            ]
          },
          "work": {
            "service": "xauusd-node4-execution",
            "version": "1.0.0",
            "role": "execution_only",
            "venue": "none",
            "started_at": 1793018022698,
            "uptime_secs": 0,
            "execution_protocol_version": 1,
            "ledger_path": "/tmp/tmprp158gwl/ledger.jsonl",
            "ledger_available": true,
            "ledger_records": 0,
            "ledger_intents": 0,
            "intents_received": 0,
            "intents_accepted": 0,
            "intents_rejected": 0,
            "intents_expired": 0,
            "intents_duplicate": 0,
            "orders_placed": 0,
            "orders_filled": 0,
            "orders_partial": 0,
            "orders_rejected": 0,
            "orders_unknown": 0,
            "orders_reconciled": 0,
            "last_intent_ts": null,
            "last_report_ts": null,
            "last_error": "NODE3_WS_URL is not set \u2014 Node 4 is an outbound client of the strategy service and cannot accept intents without it",
            "ws_clients_connected": 1
          },
          "has_risk": true,
          "no_market_feed": true,
          "no_strategy": true
        }
      ]
    }
  },
  "node3_mode": {
    "http": {
      "ok": true,
      "status": 200,
      "url": "http://127.0.0.1:19092/diagnostics",
      "headers": {
        "Content-Type": "application/json"
      },
      "raw_len": 4269,
      "summary": {
        "build": "xauusd-node4-execution 1.0.0",
        "role": "execution_only",
        "has_risk": true,
        "no_market_feed": true,
        "no_strategy": true,
        "keys": [
          "deriv_account",
          "mt5",
          "node3",
          "open_trades",
          "recent_events",
          "risk",
          "timestamp",
          "work"
        ]
      },
      "work": {
        "service": "xauusd-node4-execution",
        "version": "1.0.0",
        "role": "execution_only",
        "venue": "none",
        "started_at": 1793018022805,
        "uptime_secs": 0,
        "execution_protocol_version": 1,
        "ledger_path": "/tmp/tmpssch1eky/ledger.jsonl",
        "ledger_available": true,
        "ledger_records": 0,
        "ledger_intents": 0,
        "intents_received": 0,
        "intents_accepted": 0,
        "intents_rejected": 0,
        "intents_expired": 0,
        "intents_duplicate": 0,
        "orders_placed": 0,
        "orders_filled": 0,
        "orders_partial": 0,
        "orders_rejected": 0,
        "orders_unknown": 0,
        "orders_reconciled": 0,
        "last_intent_ts": null,
        "last_report_ts": null,
        "last_error": null,
        "ws_clients_connected": 0
      },
      "node3": {
        "url": "ws://127.0.0.1:19998/execution",
        "token_configured": true,
        "protocol_version": 1,
        "connected": false,
        "authenticated": false,
        "state": "reconnecting",
        "session_age_secs": null,
        "last_message_ts": null,
        "last_hello_ack_ts": null,
        "last_report_ts": null,
        "reconnect_count": 0,
        "frames_received": 0,
        "reports_sent": 0,
        "reports_acked": 0,
        "last_error": "cannot connect to ws://127.0.0.1:19998/execution: IO error: Connection refused (os error 111)"
      },
      "risk": {
        "enabled": true,
        "sl_min_pips": 200.0,
        "sl_max_pips": 300.0,
        "tp_min_pips": 600.0,
        "tp_max_pips": 800.0,
        "rr_min": 2.0,
        "rr_max": 3.0,
        "pip_value": 0.01,
        "contract_size": 100.0,
        "max_risk_usd": null,
        "max_daily_loss_usd": null,
        "max_drawdown_pct": null,
        "max_open_positions": 3,
        "max_lots": 1.0,
        "cooldown_losses": 3,
        "cooldown_secs": 900,
        "open_positions": 0,
        "exposure_lots": 0.0,
        "daily_pnl_usd": 0.0,
        "realised_pnl_usd": 0.0,
        "current_equity_usd": 0.0,
        "peak_equity_usd": 0.0,
        "drawdown_pct": 0.0,
        "consecutive_losses": 0,
        "cooldown_until_ms": null,
        "intents_checked": 0,
        "intents_rejected": 0,
        "last_rejection_code": null,
        "last_rejection_message": null,
        "updated_at_ms": 0
      }
    },
    "ws": {
      "ok": true,
      "ws_url": "ws://127.0.0.1:19092/ws",
      "frames": [
        {
          "type": "diagnostics",
          "top_keys": [
            "data",
            "type"
          ],
          "summary": {
            "build": "xauusd-node4-execution 1.0.0",
            "role": "execution_only",
            "has_risk": true,
            "no_market_feed": true,
            "no_strategy": true,
            "keys": [
              "deriv_account",
              "mt5",
              "node3",
              "open_trades",
              "recent_events",
              "risk",
              "timestamp",
              "work"
            ]
          },
          "work": {
            "service": "xauusd-node4-execution",
            "version": "1.0.0",
            "role": "execution_only",
            "venue": "none",
            "started_at": 1793018022805,
            "uptime_secs": 0,
            "execution_protocol_version": 1,
            "ledger_path": "/tmp/tmpssch1eky/ledger.jsonl",
            "ledger_available": true,
            "ledger_records": 0,
            "ledger_intents": 0,
            "intents_received": 0,
            "intents_accepted": 0,
            "intents_rejected": 0,
            "intents_expired": 0,
            "intents_duplicate": 0,
            "orders_placed": 0,
            "orders_filled": 0,
            "orders_partial": 0,
            "orders_rejected": 0,
            "orders_unknown": 0,
            "orders_reconciled": 0,
            "last_intent_ts": null,
            "last_report_ts": null,
            "last_error": null,
            "ws_clients_connected": 1
          },
          "has_risk": true,
          "no_market_feed": true,
          "no_strategy": true
        }
      ]
    }
  }
}
```

## 3. Deployed Dashboard `config.js`
```json
{
  "https://static-dash-frontend.onrender.com/js/config.js": {
    "status": 200,
    "body": "/* ============================================================\n * XAUUSD Terminal \u2014 browser-safe Node 1 + Node 4 endpoint configuration.\n *\n * Node 2 is a static, read-only client of two services:\n *   Node 1 \u2014 market data, built-in strategy evaluator, demo (paper) account;\n *   Node 4 \u2014 execution diagnostics (market feed, local strategy, venue).\n *\n * Runtime override, installed before this file is loaded:\n *   window.XAUUSD_CONFIG = {\n *     MARKET_HOST: \"https://market.example\",\n *     NODE4_HOST: \"https://execution.example\",\n *   };\n *\n * Only an origin is accepted; the documented public paths are fixed below.\n * No credentials, tokens, or authorization headers belong here.\n * ============================================================ */\n(function (App) {\n  \"use strict\";\n\n  const runtime = window.XAUUSD_CONFIG && typeof window.XAUUSD_CONFIG === \"object\"\n    ? window.XAUUSD_CONFIG\n    : {};\n  const own = (object, key) => Object.prototype.hasOwnProperty.call(object, key);\n\n  const DEFAULTS = {\n    MARKET_HOST: \"https://engine-southeastasia-sng-main.onrender.com\",\n    // Node 4 execution service: GET /diagnostics and the read-only /ws stream.\n    NODE4_HOST: \"https://execution-southeastasia-sng-main.onrender.com\",\n  };\n\n  function normaliseHost(value) {\n    if (value === null || value === false) return \"\";\n    const raw = String(value == null ? \"\" : value).trim();\n    if (!raw) return \"\";\n    const candidate = /^(https?|wss?):\\/\\//i.test(raw) ? raw : `https://${raw}`;\n    try {\n      const url = new URL(candidate);\n      if (![\"http:\", \"https:\", \"ws:\", \"wss:\"].includes(url.protocol)) return \"\";\n      if (url.username || url.password || url.search || url.hash || (url.pathname && url.pathname !== \"/\")) return \"\";\n      const protocol = url.protocol === \"ws:\" ? \"http:\" : url.protocol === \"wss:\" ? \"https:\" : url.protocol;\n      return `${protocol}//${url.host}`;\n    } catch {\n      return \"\";\n    }\n  }\n\n  const MARKET_HOST = normaliseHost(own(runtime, \"MARKET_HOST\") ? runtime.MARKET_HOST : DEFAULTS.MARKET_HOST);\n  const NODE4_HOST = normaliseHost(own(runtime, \"NODE4_HOST\") ? runtime.NODE4_HOST : DEFAULTS.NODE4_HOST);\n  const websocketBase = MARKET_HOST.replace(/^https:/i, \"wss:\").replace(/^http:/i, \"ws:\");\n  const node4WebsocketBase = NODE4_HOST.replace(/^https:/i, \"wss:\").replace(/^http:/i, \"ws:\");\n  const endpoint = (path, websocket) => (MARKET_HOST ? `${websocket ? websocketBase : MARKET_HOST}${path}` : \"\");\n  const executionEndpoint = (path, websocket) =>\n    (NODE4_HOST ? `${websocket ? node4WebsocketBase : NODE4_HOST}${path}` : \"\");\n\n  const endpoints = {\n    ws: endpoint(\"/ws\", true),\n    candles: endpoint(\"/candles\", false),\n    tickVolume: endpoint(\"/tick-volume\", false),\n    levels: endpoint(\"/levels\", false),\n    calendar: endpoint(\"/calendar\", false),\n    strategy: endpoint(\"/strategy\", false),\n    demo: endpoint(\"/demo\", false),\n    status: endpoint(\"/status\", false),\n    // Node 4: the same ExecutionSnapshot over REST and WebSocket.\n    executionDiagnostics: executionEndpoint(\"/diagnostics\", false),\n    executionWs: executionEndpoint(\"/ws\", true),\n  };\n\n  App.config = {\n    MARKET_HOST,\n    NODE4_HOST,\n    endpoints,\n\n    // Exactly the Node 1 topics the dashboard renders, and nothing else.\n    socket: {\n      topics: [\"levels\", \"candle\", \"tick_volume\", \"strategy\", \"demo\", \"calendar\", \"status\"],\n      reconnectMinMs: 1000,\n      reconnectMaxMs: 30000,\n      heartbeatEveryMs: 30000,\n      heartbeatTimeoutMs: 75000,\n      staleAfterMs: 90000,\n      pollFallbackMs: 5000,\n    },\n\n    // Node 4 execution diagnostics. One topic; complete snapshots only.\n    execution: {\n      diagnostics: endpoints.executionDiagnostics,\n      ws: endpoints.executionWs,\n      topics: [\"diagnostics\"],\n      reconnectMinMs: 1000,\n      reconnectMaxMs: 30000,\n      heartbeatEveryMs: 30000,\n      heartbeatTimeoutMs: 75000,\n      pollFallbackMs: 5000,\n      pollGraceMs: 1500,\n    },\n\n    strategy: {\n      staleAfterMs: 60000,\n      historyLimit: 120,\n      stripSize: 30,\n      // Shape only: every value is unknown until /strategy (or the status\n      // frame) supplies it. Rendering shows \u2014 rather than inventing defaults.\n      defaultParams: {\n        enabled: null,\n        min_confidence: null,\n        risk_pct: null,\n        atr_period: null,\n        atr_stop_mult: null,\n        reward_risk: null,\n        min_lots: null,\n        max_lots: null,\n        lot_step: null,\n        fast_ema: null,\n        slow_ema: null,\n        event_blackout_secs: null,\n        leverage: null,\n        max_margin_pct: null,\n      },\n    },\n\n    demo: {\n      maxTrades: 50,\n      profitFactorCap: 99,\n      marginWarnPct: 200,\n      marginDangerPct: 100,\n    },\n\n    calendar: {\n      currency: \"USD\",\n      refreshMs: 5 * 60 * 1000,\n      keepPastMs: 6 * 60 * 60 * 1000,\n      impactRank: { high: 3, medium: 2, low: 1, holiday: 0 },\n    },\n\n    levelsRefreshMs: 60 * 1000,\n    candlesRefreshMs: 60 * 1000,\n    statusRefreshMs: 30 * 1000,\n\n    levels: {\n      PW: {\n        poc: { color: \"#F0B90B\", title: \"PW PoC\", cls: \"poc\", dashed: false },\n        vah: { color: \"#26A69A\", title: \"PW VaH\", cls: \"vah\", dashed: false },\n        val: { color: \"#EF5350\", title: \"PW VaL\", cls: \"val\", dashed: false },\n      },\n      PS: {\n        poc: { color: \"#58A6FF\", title: \"PS PoC\", cls: \"ps-poc\", dashed: false },\n      },\n      CW: {\n        poc: { color: \"#A371F7\", title: \"CW PoC\", cls: \"cw-poc\", dashed: true },\n        vah: { color: \"#F778BA\", title: \"CW VaH\", cls: \"cw-vah\", dashed: true },\n        val: { color: \"#22D3EE\", title: \"CW VaL\", cls: \"cw-val\", dashed: true },\n      },\n      SWING_BULL: {\n        poc: { color: \"#26A69A\", title: \"Bull Swing PoC\", cls: \"swing-bull-poc\", dashed: false },\n        vah: { color: \"#22C55E\", title: \"Bull Swing VaH\", cls: \"swing-bull-vah\", dashed: true },\n        val: { color: \"#86EFAC\", title: \"Bull Swing VaL\", cls: \"swing-bull-val\", dashed: true },\n      },\n      SWING_BEAR: {\n        poc: { color: \"#EF5350\", title: \"Bear Swing PoC\", cls: \"swing-bear-poc\", dashed: false },\n        vah: { color: \"#F97316\", title: \"Bear Swing VaH\", cls: \"swing-bear-vah\", dashed: true },\n        val: { color: \"#FDBA74\", title: \"Bear Swing VaL\", cls: \"swing-bear-val\", dashed: true },\n      },\n    },\n    levelWindows: [\"PW\", \"PS\", \"CW\", \"SWING_BULL\", \"SWING_BEAR\"],\n    swingWindows: [\"SWING_BULL\", \"SWING_BEAR\"],\n    swingLabels: { SWING_BULL: \"Bull Swing\", SWING_BEAR: \"Bear Swing\" },\n    levelKinds: [\"poc\", \"vah\", \"val\"],\n  };\n})((window.App = window.App || {}));\n"
  }
}
```
