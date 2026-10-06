# MT5 demo resources for the frontend (Node 2)

Concrete wire contract for the Deriv MT5 **demo** venue, so the dashboard can be
built without waiting for `FRONTEND_HANDOFF.md` (which is **not** in this
repository — see the note at the bottom).

Everything here is served by Node 3 (`https://execution-southeastasia-sng-main.onrender.com`,
`PORT=10000`). Nothing here talks to the bridge or the terminal directly: the
browser must never be given MT5 credentials, and there is no public bridge port
by design.

The field names below are the authoritative ones: they are generated from
`node3-execution/src/types.rs`, which CI
(`ci/mt5/protocol_lint.py`) keeps in sync with `mt5-bridge/src/snapshot.rs`.

---

## 1. REST

### `GET /mt5/account` → `Mt5AccountSnapshot`

```json
{
  "configured": true,
  "connected": true,
  "authorized": true,
  "account_type": "demo",
  "login": 123456,
  "server": "Deriv-Demo",
  "company": "Deriv",
  "currency": "USD",
  "balance": 10000.0,
  "equity": 10037.4,
  "margin": 1.32,
  "margin_free": 10036.08,
  "leverage": 100,
  "trade_allowed": true,
  "halted": false,
  "halt_reason": null,
  "requested_symbol": "XAUUSD",
  "broker_symbol": "XAUUSD",
  "symbol_digits": 3,
  "symbol_volume_min": 0.01,
  "symbol_volume_max": 100.0,
  "symbol_volume_step": 0.01,
  "symbol_contract_size": 100.0,
  "terminal_build": 4755,
  "ea_version": "1.0.0",
  "bridge_version": "0.1.0",
  "latency_ms": 42,
  "last_heartbeat": 1759700000000,
  "last_updated": 1759700000000,
  "error": null,
  "setup_hint": null
}
```

UI rules that the backend already enforces, so the dashboard can mirror them:

| Field state | Meaning for the operator |
|---|---|
| `configured=false` | `MT5_BRIDGE_TOKEN` is not set on Node 3 — the MT5 venue is off. `error` names the missing variable. |
| `configured=true, connected=false` | The bridge has not dialled in. `setup_hint` explains where to look (bridge process, `NODE3_WS_URL`, token). |
| `connected=true, authorized=false` | The bridge is up but must not trade (EA link down, demo guard failed, login mismatch). Show `error`. |
| `account_type != "demo"` | The terminal is **not** a demo account. Trading is refused everywhere; this must be red. |
| `halted=true` | Kill switch is engaged; `halt_reason` says why (`operator`, `ea_disconnect`, `demo_guard`, …). |
| `last_heartbeat` older than ~15 s while `connected=true` | Treat as stale: the bridge is silent. |

### `GET /mt5/positions` → `Mt5PositionsSnapshot`

```json
{
  "positions": [
    {
      "ticket": 123456789,
      "symbol": "XAUUSD",
      "side": "buy",
      "volume": 0.01,
      "price_open": 2650.12,
      "sl": 2647.5,
      "tp": 2656.0,
      "profit": 2.4,
      "swap": 0.0,
      "comment": "N3-1759700000000-1",
      "magic": 330033,
      "time_ms": 1759700000123,
      "current_price": 2650.36,
      "unrealized_pnl": 2.4
    }
  ],
  "count": 1,
  "total_volume": 0.01,
  "total_unrealized_pnl": 2.4,
  "halted": false,
  "halt_reason": null,
  "account_login": 123456,
  "account_type": "demo",
  "source": "bridge",
  "timestamp": 1759700001000
}
```

These are **broker** positions (MT5 tickets), not strategy simulations. On
`GET /open-trades` they also appear as `mt5_open_positions` / `mt5_open_count`,
next to `node3_open_trades` and `deriv_open_trades`, so the two must stay visually
distinct: a Node 3 trade is only real when a matching MT5 ticket exists.

### `GET /mt5/history` → `Mt5HistorySnapshot`

```json
{
  "deals": [
    {
      "ticket": 987654321,
      "order_ticket": 123456790,
      "position_ticket": 123456789,
      "symbol": "XAUUSD",
      "side": "sell",
      "volume": 0.01,
      "price": 2656.02,
      "profit": 5.9,
      "swap": 0.0,
      "commission": -0.07,
      "comment": "N3-1759700000000-1",
      "magic": 330033,
      "time_ms": 1759700600000,
      "entry": "out",
      "reason": "tp"
    }
  ],
  "count": 1,
  "cursor": "42",
  "complete": true,
  "total_realized_pnl": 5.83,
  "first_ms": 1759700600000,
  "last_ms": 1759700600000,
  "source": "bridge",
  "timestamp": 1759700601000
}
```

`entry` is `in` | `out` | `inout`; `reason` is `client`, `expert`, `sl`, `tp`, … —
both symbolic, no raw MT5 enums. History is mirrored from the broker into an
append-only store on the MT5 host, so it survives restarts of Node 3 **and** the
bridge.

### `GET /mt5/status` → `Mt5BridgeStatus`

```json
{
  "configured": true,
  "connected": true,
  "authorized": true,
  "protocol": 1,
  "bridge_version": "0.1.0",
  "node3_url": "wss://execution-southeastasia-sng-main.onrender.com/ws",
  "ea_connected": true,
  "ea_write_enabled": true,
  "ea_mode": "demo",
  "ea_login": 123456,
  "ea_server": "Deriv-Demo",
  "ea_last_heartbeat": 1759700000000,
  "halted": false,
  "halt_reason": null,
  "trading_enabled": true,
  "orders_sent": 12,
  "orders_filled": 11,
  "orders_rejected": 1,
  "orders_unknown": 0,
  "positions_open": 1,
  "history_deals": 24,
  "last_error": null,
  "uptime_secs": 3600,
  "timestamp": 1759700001000
}
```

`orders_unknown` is the number that matters: it counts intents whose outcome the
broker never confirmed. Those are reconciled from broker state and never marked
open — surface them, do not hide them.

### `POST /mt5/control` (operator kill switch)

Header `X-Control-Token: <MT5_CONTROL_TOKEN>`. **403 unless Node 3 has
`MT5_CONTROL_TOKEN` set**, so the endpoint is dead by default. There is
deliberately no unauthenticated way to flatten an account.

```bash
# stop opening new trades, keep what is open
-d '{"action":"halt","reason":"operator"}'
# halt and flatten everything the bridge owns
-d '{"action":"halt","reason":"operator","flatten":true}'
-d '{"action":"close_all","reason":"operator flatten"}'
# one position
-d '{"action":"close_position","position_ticket":123456789}'
# allow trading again (the demo guard and link must be healthy)
-d '{"action":"resume"}'
```

`200` means the **bridge confirmed** the action, not merely that it was queued:

```json
{ "ok": true, "action": "halt", "req_id": "halt-1759700000000-4", "data": { "halted": true } }
```

Errors: `400` bad action/payload, `403` token missing or wrong, `405` not POST,
`502` the bridge answered with an error, `503` MT5 not configured or bridge not
connected. Body shape: `{"ok":false,"error":{"code":"...","message":"..."}}`.

---

## 2. WebSocket (`wss://…/ws`)

All MT5 frames use the same envelope as the rest of the stream:
`{"type": "<name>", "data": { … }}`.

| `type` | `data` |
|---|---|
| `mt5_account` | `Mt5AccountSnapshot` (as above) |
| `mt5_positions` | `Mt5PositionsSnapshot` |
| `mt5_history` | `Mt5HistorySnapshot` |
| `bridge_status` | `Mt5BridgeStatus` |
| `bridge_event` | `{"event": "<name>", "data": { … }}` — see below |

Subscribe with:

```json
{"type": "subscribe", "topics": ["mt5_account", "mt5_positions", "mt5_history", "bridge_status", "bridge_event", "open_trades", "diagnostics"]}
```

An empty `topics` list (or omitting the frame) means "everything". New clients
receive the current snapshots immediately.

`bridge_event` names worth rendering: `order_filled`, `order_rejected`,
`order_unknown`, `position_opened`, `position_closed`, `halted`, `resumed`,
`ea_link_disconnected`, `demo_guard_failed`, `reconciled`. Payloads carry the
intent id / position ticket / reason.

Bridge-only frames (`bridge_hello`, `bridge_hello_ack`, `bridge_ack`) are never
forwarded to browser clients, whatever they subscribe to.

---

## 3. What is *not* exposed

* `MT5_LOGIN`, `MT5_PASSWORD`, `MT5_EA_TOKEN` never reach Node 3, any REST
  response or any WebSocket frame.
* There is no endpoint that places an order. Orders are only placed by the
  strategy; the operator's lever is halt/resume/close.
* The bridge has no public port: it dials out to Node 3, so there is nothing to
  firewall off except the loopback EA link on the MT5 host.

> If a `FRONTEND_HANDOFF.md` ever lands in this repository with different field
> names, change `Mt5*` structs in **both** `mt5-bridge/src/snapshot.rs` and
> `node3-execution/src/types.rs` — CI's protocol lint fails otherwise.
