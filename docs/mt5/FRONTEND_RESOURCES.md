# MT5 demo resources for the frontend (Node 2)

Concrete wire contract for the Deriv MT5 **demo** venue, so the dashboard can be
built without waiting for `FRONTEND_HANDOFF.md` (which is **not** in this
repository — see the note at the bottom).

Everything here is served by Node 4 (`https://execution-southeastasia-sng-main.onrender.com`,
`PORT=10000`). Nothing here talks to the bridge or the terminal directly: the
browser must never be given MT5 credentials, and there is no public bridge port
by design.

Node 2 is a **pure frontend** on its own branch: it calls three public service
origins directly (`MARKET_HOST` = Node 1, `STRATEGY_HOST` = Node 3,
`EXECUTION_HOST` = Node 4). This document covers only C4, the Node 4 surface;
see `contracts/c2-node3-public.schema.json` for the Node 3 levels/drawings
payload. All three services must whitelist the deployed dashboard origin in
CORS and in their WebSocket origin checks.

The field names below are the authoritative ones: they are generated from
`node4-execution/src/types.rs`, which CI
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
| `configured=false` | `MT5_BRIDGE_TOKEN` is not set on Node 4 — the MT5 venue is off. `error` names the missing variable. |
| `configured=true, connected=false` | The bridge has not dialled in. `setup_hint` explains where to look (bridge process, `NODE4_WS_URL`, token). |
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
next to `node4_open_trades` and `deriv_open_trades`, so the two must stay visually
distinct: a Node 4 trade is only real when a matching MT5 ticket exists.

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
append-only store on the MT5 host, so it survives restarts of Node 4 **and** the
bridge.

### `GET /mt5/status` → `Mt5BridgeStatus`

```json
{
  "configured": true,
  "connected": true,
  "authorized": true,
  "protocol": 1,
  "bridge_version": "0.1.0",
  "node4_url": "wss://execution-southeastasia-sng-main.onrender.com/ws",
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

Header `X-Control-Token: <MT5_CONTROL_TOKEN>`. **403 unless Node 4 has
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

### `GET /diagnostics` (alias `/status`) → `ExecutionSnapshot`

Read-only. Top-level keys: `timestamp`, `node3` (private link health),
**`risk`**, `work` (execution counters, venue, ledger state), `open_trades`,
`deriv_account`, `mt5`, `recent_events`.

> **Breaking change (C4 v1):** the old `market_feed` and `strategy` blocks were
> removed when Node 4's duplicated local-strategy mode was deleted. Node 4 has
> no market feed and no strategy. A dashboard that still renders those panels
> must switch them off; their replacement is the `risk` block below. See
> `contracts/c4-diagnostics.schema.json`.

```json
{
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
  "open_positions": 1,
  "exposure_lots": 0.01,
  "daily_pnl_usd": -2.5,
  "realised_pnl_usd": 12.0,
  "current_equity_usd": 12.0,
  "peak_equity_usd": 15.0,
  "drawdown_pct": 20.0,
  "consecutive_losses": 1,
  "cooldown_until_ms": null,
  "intents_checked": 42,
  "intents_rejected": 3,
  "last_rejection_code": "risk_sl_out_of_band",
  "last_rejection_message": "stop 120.0 pips is outside the 200-300 pip band",
  "updated_at_ms": 1792781876162
}
```

UI rules:

| Field | Meaning |
|---|---|
| `enabled` | Always true; the risk module is the pre-write gate. |
| `sl_*` / `tp_*` / `rr_*` | The enforced strategy price bands (1 pip = $0.01 ⇒ $2–3 / $6–8). |
| `open_positions` / `max_open_positions` | Current exposure vs the hard cap. |
| `daily_pnl_usd` / `max_daily_loss_usd` | Realised PnL since 00:00 UTC; `null` cap = disabled. |
| `drawdown_pct` / `max_drawdown_pct` | Drawdown on realised equity; `null` cap = disabled. |
| `cooldown_until_ms` | Non-null and in the future ⇒ new intents are refused (`risk_cooldown`). |
| `intents_rejected`, `last_rejection_code` | The ONLY place a suppressed/refused signal is visible. Stable codes: `risk_sl_out_of_band`, `risk_tp_out_of_band`, `risk_rr_out_of_band`, `risk_rr_mismatch`, `risk_lot_limit`, `risk_per_trade_cap`, `risk_max_open_positions`, `risk_cooldown`, `risk_daily_loss_cap`, `risk_max_drawdown`. |

Realised PnL comes from the MT5 bridge positions snapshot: when a ticket
disappears, its last reported `profit + swap` is booked as a close. With
`EXECUTION_VENUE=none` no fills occur, so the PnL-based counters stay at zero
while the price-band checks still run.

---

## 2. WebSocket (`wss://…/ws` for frontends)

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

`bridge_event` names (exactly these, in the bridge's own words):
`order_filled`, `order_rejected`, `order_unknown`, `halted`, `resumed`,
`ea_link_disconnected`, `trade_transaction`.

* `order_filled` / `order_rejected` / `order_unknown` carry an `OrderOutcome`
  (intent id, tickets, price, volume, retcode, `reconciled`).
* `halted` carries `{reason, auto, flattened, halted}` — `auto: true` is how a
  demo-guard failure or an EA disconnect surfaces; a `resume` must come from an
  operator (`POST /mt5/control`).
* `resumed` carries `{halted: false}`.
* `ea_link_disconnected` carries `{reason}`.
* Anything else about position state arrives as a fresh `mt5_positions` /
  `mt5_history` snapshot (the bridge pushes one after every fill, halt and
  position change, and on its own interval anyway) — the dashboard should not
  wait for a per-position event.

Bridge-only frames (`bridge_hello`, `bridge_hello_ack`, `bridge_ack`) are never
forwarded to browser clients, whatever they subscribe to.

---

## 3. What is *not* exposed

* `MT5_LOGIN`, `MT5_PASSWORD`, `MT5_EA_TOKEN` never reach Node 4, any REST
  response or any WebSocket frame.
* There is no endpoint that places an order. Orders are only placed by Node 4
  when Node 3 sends an intent; the operator's lever is halt/resume/close.
* `/diagnostics` contains **no** `market_feed`, `strategy`, `scanning` or
  `levels` block. Node 4 never reproduces a strategy decision; those come from
  Node 3's public C2 resources.
* The bridge has no public port: it dials out to Node 4, so there is nothing to
  firewall off except the loopback EA link on the MT5 host.

> If a `FRONTEND_HANDOFF.md` ever lands in this repository with different field
> names, change `Mt5*` structs in **both** `mt5-bridge/src/snapshot.rs` and
> `node4-execution/src/types.rs` — CI's protocol lint fails otherwise.
