# Node 4 `/diagnostics` Live & Local Probe Report

Recorded at: `2026-10-24 15:16:31 UTC`

## 1. Deployed Render Services

### Node 4 (Render default) (`https://execution-southeastasia-sng-main.onrender.com`)

#### HTTP `GET /diagnostics`
```json
{
  "ok": true,
  "status": 200,
  "url": "https://execution-southeastasia-sng-main.onrender.com/diagnostics",
  "headers": {
    "Date": "Sat, 24 Oct 2026 15:16:32 GMT",
    "Content-Type": "application/json",
    "Server": "cloudflare",
    "x-render-origin-server": "Render",
    "CF-RAY": "a4f9f1737e84c8e0-IAD"
  },
  "raw_len": 29875,
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
  },
  "work": {
    "service": "xauusd-node3-execution",
    "version": "1.0.0",
    "venue": "DerivDemo",
    "started_at": 1791398084400,
    "uptime_secs": 1456907,
    "node1_ws_url": "wss://engine-southeastasia-sng-main.onrender.com/ws",
    "node1_connected": true,
    "node1_state": "connected",
    "node1_reconnect_count": 78,
    "last_node1_msg_ts": 1792854988587,
    "last_candle_ts": 1792841938604,
    "last_levels_ts": 1792841938601,
    "candles_received": 6849614,
    "levels_received": 840,
    "ws_messages_received": 6899125,
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
    "ws_clients_connected": 0
  },
  "node3": null,
  "market_feed": null,
  "strategy": null
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
      },
      "work": {
        "service": "xauusd-node3-execution",
        "version": "1.0.0",
        "venue": "DerivDemo",
        "started_at": 1791398084400,
        "uptime_secs": 1456908,
        "node1_ws_url": "wss://engine-southeastasia-sng-main.onrender.com/ws",
        "node1_connected": true,
        "node1_state": "connected",
        "node1_reconnect_count": 78,
        "last_node1_msg_ts": 1792854988587,
        "last_candle_ts": 1792841938604,
        "last_levels_ts": 1792841938601,
        "candles_received": 6849614,
        "levels_received": 840,
        "ws_messages_received": 6899125,
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
      "has_market_feed": false,
      "has_strategy": false
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
      "raw_len": 5349,
      "summary": {
        "build": "xauusd-node4-execution 1.0.0",
        "role": "execution_only",
        "has_market_feed": true,
        "has_strategy": true,
        "keys": [
          "deriv_account",
          "market_feed",
          "mt5",
          "node3",
          "open_trades",
          "recent_events",
          "strategy",
          "timestamp",
          "work"
        ]
      },
      "work": {
        "service": "xauusd-node4-execution",
        "version": "1.0.0",
        "role": "execution_only",
        "venue": "none",
        "started_at": 1792854999111,
        "uptime_secs": 0,
        "execution_protocol_version": 1,
        "ledger_path": "/tmp/tmpr_n2g_p8/ledger.jsonl",
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
        "last_error": "NODE3_WS_URL is not set \u2014 in `node3` mode Node 4 is an outbound client of the strategy service and cannot accept intents without it",
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
      "market_feed": {
        "enabled": false,
        "state": "disabled",
        "trading_ready": false,
        "not_ready_reason": "NODE1_FEED_ENABLED is not set",
        "url": "",
        "auth": "none",
        "contract_version": null,
        "contract_required": "1.0",
        "symbol": "XAUUSD",
        "interval": "15m",
        "connected": false,
        "last_frame_ms": null,
        "last_status_ms": null,
        "last_candle_time": null,
        "last_closed_candle_time": null,
        "last_levels_ts": null,
        "last_seq": null,
        "session_closed_candles": 0,
        "sync_complete": false,
        "contract_ok": false,
        "freshness_state": null,
        "freshness_age_ms": null,
        "market_open": null,
        "reconnect_count": 0,
        "frames_received": 0,
        "candles_received": 0,
        "closed_candles": 0,
        "levels_received": 0,
        "status_frames": 0,
        "duplicates": 0,
        "out_of_order": 0,
        "gaps": 0,
        "contract_violations": 0,
        "resyncs": 0,
        "last_error": null,
        "last_event_ms": null
      },
      "strategy": {
        "enabled": false,
        "mode": "monitor",
        "strategy_version": "vp_break_retest_v1",
        "symbol": "XAUUSD",
        "interval": "15m",
        "volume_threshold": 10500.0,
        "sl_min_pips": 200.0,
        "sl_max_pips": 300.0,
        "tp_min_pips": 600.0,
        "tp_max_pips": 800.0,
        "rr_min": 2.0,
        "rr_max": 3.0,
        "ttl_secs": 120,
        "levels_tracked": 0,
        "candles_observed": 0,
        "last_candle_time": null,
        "atr": 0.0,
        "last_decision_at_ms": null,
        "last_action": "flat",
        "last_trigger_level": null,
        "last_intent_id": null,
        "intents_emitted": 0,
        "intents_suppressed": 0,
        "last_suppression_reason": null,
        "last_error": null,
        "updated_at": null,
        "breaks_detected": 0,
        "breaks_invalidated": 0,
        "retests_rejected_low_volume": 0,
        "signals_confirmed": 0
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
            "has_market_feed": true,
            "has_strategy": true,
            "keys": [
              "deriv_account",
              "market_feed",
              "mt5",
              "node3",
              "open_trades",
              "recent_events",
              "strategy",
              "timestamp",
              "work"
            ]
          },
          "work": {
            "service": "xauusd-node4-execution",
            "version": "1.0.0",
            "role": "execution_only",
            "venue": "none",
            "started_at": 1792854999111,
            "uptime_secs": 0,
            "execution_protocol_version": 1,
            "ledger_path": "/tmp/tmpr_n2g_p8/ledger.jsonl",
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
            "last_error": "NODE3_WS_URL is not set \u2014 in `node3` mode Node 4 is an outbound client of the strategy service and cannot accept intents without it",
            "ws_clients_connected": 1
          },
          "has_market_feed": true,
          "has_strategy": true
        }
      ]
    }
  },
  "node1_strategy_mode": {
    "http": {
      "ok": true,
      "status": 200,
      "url": "http://127.0.0.1:19091/diagnostics",
      "headers": {
        "Content-Type": "application/json"
      },
      "raw_len": 4733,
      "summary": {
        "build": "xauusd-node4-execution 1.0.0",
        "role": "execution_only",
        "has_market_feed": true,
        "has_strategy": true,
        "keys": [
          "deriv_account",
          "market_feed",
          "mt5",
          "node3",
          "open_trades",
          "recent_events",
          "strategy",
          "timestamp",
          "work"
        ]
      },
      "work": {
        "service": "xauusd-node4-execution",
        "version": "1.0.0",
        "role": "execution_only",
        "venue": "none",
        "started_at": 1792854999220,
        "uptime_secs": 0,
        "execution_protocol_version": 1,
        "ledger_path": "/tmp/tmpekwkji6x/ledger.jsonl",
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
        "url": "",
        "token_configured": false,
        "protocol_version": 1,
        "connected": false,
        "authenticated": false,
        "state": "starting",
        "session_age_secs": null,
        "last_message_ts": null,
        "last_hello_ack_ts": null,
        "last_report_ts": null,
        "reconnect_count": 0,
        "frames_received": 0,
        "reports_sent": 0,
        "reports_acked": 0,
        "last_error": null
      },
      "market_feed": {
        "enabled": true,
        "state": "offline",
        "trading_ready": false,
        "not_ready_reason": "market feed is not connected",
        "url": "ws://127.0.0.1:19999/ws",
        "auth": "none (contract v1)",
        "contract_version": null,
        "contract_required": "1.0",
        "symbol": "XAUUSD",
        "interval": "15m",
        "connected": false,
        "last_frame_ms": null,
        "last_status_ms": null,
        "last_candle_time": null,
        "last_closed_candle_time": null,
        "last_levels_ts": null,
        "last_seq": null,
        "session_closed_candles": 0,
        "sync_complete": false,
        "contract_ok": false,
        "freshness_state": null,
        "freshness_age_ms": null,
        "market_open": null,
        "reconnect_count": 0,
        "frames_received": 0,
        "candles_received": 0,
        "closed_candles": 0,
        "levels_received": 0,
        "status_frames": 0,
        "duplicates": 0,
        "out_of_order": 0,
        "gaps": 0,
        "contract_violations": 0,
        "resyncs": 0,
        "last_error": "cannot connect to ws://127.0.0.1:19999/ws: IO error: Connection refused (os error 111)",
        "last_event_ms": 1792854999220
      },
      "strategy": {
        "enabled": true,
        "mode": "local",
        "strategy_version": "vp_break_retest_v1",
        "symbol": "XAUUSD",
        "interval": "15m",
        "volume_threshold": 10500.0,
        "sl_min_pips": 200.0,
        "sl_max_pips": 300.0,
        "tp_min_pips": 600.0,
        "tp_max_pips": 800.0,
        "rr_min": 2.0,
        "rr_max": 3.0,
        "ttl_secs": 120,
        "levels_tracked": 0,
        "candles_observed": 0,
        "last_candle_time": null,
        "atr": 0.0,
        "last_decision_at_ms": null,
        "last_action": "flat",
        "last_trigger_level": null,
        "last_intent_id": null,
        "intents_emitted": 0,
        "intents_suppressed": 0,
        "last_suppression_reason": null,
        "last_error": null,
        "updated_at": null,
        "breaks_detected": 0,
        "breaks_invalidated": 0,
        "retests_rejected_low_volume": 0,
        "signals_confirmed": 0
      }
    },
    "ws": {
      "ok": true,
      "ws_url": "ws://127.0.0.1:19091/ws",
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
            "has_market_feed": true,
            "has_strategy": true,
            "keys": [
              "deriv_account",
              "market_feed",
              "mt5",
              "node3",
              "open_trades",
              "recent_events",
              "strategy",
              "timestamp",
              "work"
            ]
          },
          "work": {
            "service": "xauusd-node4-execution",
            "version": "1.0.0",
            "role": "execution_only",
            "venue": "none",
            "started_at": 1792854999220,
            "uptime_secs": 0,
            "execution_protocol_version": 1,
            "ledger_path": "/tmp/tmpekwkji6x/ledger.jsonl",
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
          "has_market_feed": true,
          "has_strategy": true
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
      "raw_len": 5041,
      "summary": {
        "build": "xauusd-node4-execution 1.0.0",
        "role": "execution_only",
        "has_market_feed": true,
        "has_strategy": true,
        "keys": [
          "deriv_account",
          "market_feed",
          "mt5",
          "node3",
          "open_trades",
          "recent_events",
          "strategy",
          "timestamp",
          "work"
        ]
      },
      "work": {
        "service": "xauusd-node4-execution",
        "version": "1.0.0",
        "role": "execution_only",
        "venue": "none",
        "started_at": 1792854999330,
        "uptime_secs": 0,
        "execution_protocol_version": 1,
        "ledger_path": "/tmp/tmpgmj77lcd/ledger.jsonl",
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
      "market_feed": {
        "enabled": false,
        "state": "disabled",
        "trading_ready": false,
        "not_ready_reason": "NODE1_FEED_ENABLED is not set",
        "url": "",
        "auth": "none",
        "contract_version": null,
        "contract_required": "1.0",
        "symbol": "XAUUSD",
        "interval": "15m",
        "connected": false,
        "last_frame_ms": null,
        "last_status_ms": null,
        "last_candle_time": null,
        "last_closed_candle_time": null,
        "last_levels_ts": null,
        "last_seq": null,
        "session_closed_candles": 0,
        "sync_complete": false,
        "contract_ok": false,
        "freshness_state": null,
        "freshness_age_ms": null,
        "market_open": null,
        "reconnect_count": 0,
        "frames_received": 0,
        "candles_received": 0,
        "closed_candles": 0,
        "levels_received": 0,
        "status_frames": 0,
        "duplicates": 0,
        "out_of_order": 0,
        "gaps": 0,
        "contract_violations": 0,
        "resyncs": 0,
        "last_error": null,
        "last_event_ms": null
      },
      "strategy": {
        "enabled": false,
        "mode": "monitor",
        "strategy_version": "vp_break_retest_v1",
        "symbol": "XAUUSD",
        "interval": "15m",
        "volume_threshold": 10500.0,
        "sl_min_pips": 200.0,
        "sl_max_pips": 300.0,
        "tp_min_pips": 600.0,
        "tp_max_pips": 800.0,
        "rr_min": 2.0,
        "rr_max": 3.0,
        "ttl_secs": 120,
        "levels_tracked": 0,
        "candles_observed": 0,
        "last_candle_time": null,
        "atr": 0.0,
        "last_decision_at_ms": null,
        "last_action": "flat",
        "last_trigger_level": null,
        "last_intent_id": null,
        "intents_emitted": 0,
        "intents_suppressed": 0,
        "last_suppression_reason": null,
        "last_error": null,
        "updated_at": null,
        "breaks_detected": 0,
        "breaks_invalidated": 0,
        "retests_rejected_low_volume": 0,
        "signals_confirmed": 0
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
            "has_market_feed": true,
            "has_strategy": true,
            "keys": [
              "deriv_account",
              "market_feed",
              "mt5",
              "node3",
              "open_trades",
              "recent_events",
              "strategy",
              "timestamp",
              "work"
            ]
          },
          "work": {
            "service": "xauusd-node4-execution",
            "version": "1.0.0",
            "role": "execution_only",
            "venue": "none",
            "started_at": 1792854999330,
            "uptime_secs": 0,
            "execution_protocol_version": 1,
            "ledger_path": "/tmp/tmpgmj77lcd/ledger.jsonl",
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
          "has_market_feed": true,
          "has_strategy": true
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
