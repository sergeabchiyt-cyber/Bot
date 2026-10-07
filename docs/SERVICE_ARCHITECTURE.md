# Node 2 service architecture

Node 2 is a static read-only client. The deployed data and write boundaries are:

```text
Node 1 (market) ── public candles / levels / flow / calendar ──┐
Node 3 (strategy) ── public scanner / intents / diagnostics ───┼─► Node 2
Node 4 (execution) ── public broker / account / bridge state ──┘

Node 3 ── authenticated trade_intent / execution_report protocol ── Node 4
```

The Node 2 browser never participates in the authenticated Node 3 ↔ Node 4 link. Node 3 decides strategy meaning; Node 4 is authoritative for venue acceptance and broker outcomes. Node 2 labels these as different data domains.

## Runtime endpoint map

`js/config.js` defines `MARKET_HOST`, `STRATEGY_HOST`, `EXECUTION_HOST` and derives these explicit browser endpoints:

```text
Node 1
  marketWs, candles, tickVolume, levels, calendar
Node 3
  strategyWs, strategyDiagnostics, strategyScanning, strategySignals
Node 4
  executionWs, executionDiagnostics, executionOpenTrades,
  executionAccount, executionDeriv, mt5Account, mt5Positions,
  mt5History, mt5Status
```

The checked-in Node 4 frontend contract documents `execution-southeastasia-sng-main.onrender.com`; this is the source for the default Node 4 host. Node 4's service configuration supplies the Node 3 host used by its private link; Node 2 uses only that same host's public resources. Node 1's public market host is configured in the existing Node 2 market client and Node 3's market URL configuration.

For another environment, inject `window.XAUUSD_CONFIG` before `js/config.js`:

```js
window.XAUUSD_CONFIG = {
  MARKET_HOST: "https://market.example",
  STRATEGY_HOST: "https://strategy.example",
  EXECUTION_HOST: "https://execution.example"
};
```

An explicitly empty host disables the corresponding service and its badge reads “NOT CONFIGURED”. Overrides accept only an HTTP(S)/WS(S) origin; endpoint paths remain fixed in code to the documented public routes. Values with credentials, query strings, fragments, or custom paths are rejected. Runtime configuration has no credential, header, token, or account fields.

## Endpoint and topic ownership

### Node 1 — market

Node 2 connects to public `WS /ws` and reads public REST `/candles`, `/tick-volume`, `/levels`, and `/calendar`. Market topics used by the dashboard are `levels`, `candle`, `tick_volume`, `bubbles`, `trades`, `calendar`, and `sentiment`; heartbeat frames are handled by the shared connection lifecycle.

The market chart and its existing chart/volume-profile logic remain in the Node 1 adapter. Node 3 also consumes Node 1 `candle` and `levels`, but the browser's market chart does not depend on the Node 3 stream.

### Node 3 — strategy

Public HTTP:

- `GET /diagnostics` and `GET /status` → `DiagnosticsSnapshot`
- `GET /scanning` → `ScanningSnapshot`
- `GET /signals` → `SignalSnapshot`

Public WS topics subscribed by Node 2: `diagnostics`, `scanning`, `signals`, `diagnostic_event`.

The renderer uses the contract fields in `node3-strategy/src/types.rs`:

- `StrategyWorkDiagnostics`: Node 1 state/reconnects, candle/level counters and timestamps, Node 4 connection state, uptime, current projected risk, and last error.
- `ScanningSnapshot` / `ScannedLevelSetup`: active setup/level counts, price/volume/ATR, scanner state, pending side, distances, retest and invalidation levels, volume confirmation, projected buy/sell orders, active projection, break time/price, and latest note.
- `SignalSnapshot`: `pending`, `recent`, `execution_reports`, `pending_count`, `node4_connected`, `node4_token_configured`, `timestamp`.
- `ActivityLogEntry`: scanner, signal, queue, and connection messages.

Intent cards show the exact strategy intent fields, including expiration countdown. They explicitly say “intent only — not an execution or fill.”

### Node 4 — execution and broker monitoring

Public HTTP:

- `GET /diagnostics` (`/status` alias) → `ExecutionSnapshot`
- `GET /open-trades` (`/trades` alias) → `OpenTradesSnapshot`
- `GET /deriv` (`/account` alias) → `DerivAccountSnapshot`
- `GET /mt5/account` → `Mt5AccountSnapshot`
- `GET /mt5/positions` → `Mt5PositionsSnapshot`
- `GET /mt5/history` → `Mt5HistorySnapshot`
- `GET /mt5/status` → `Mt5BridgeStatus`

Node 2 subscribes to public `/ws` topics implemented by Node 4:

```text
diagnostics, open_trades, trades, deriv_account,
mt5_account, mt5_positions, mt5_history, bridge_status,
bridge_event, diagnostic_event, heartbeat
```

The Node 4 Rust `WsFrame::Trades` variant serializes as `type: "trade"`, while its topic filter matches the subscription name `trades`. `bridge_event` is the event/data envelope from the bridge; bridge/authentication and acknowledgement frames are private and are not requested.

Node 4 has no public structured execution-report frame or endpoint in the deployed `WsFrame` / route contract. It does publish structured `ExecutionReport` values to Node 3 over their private service link. Node 3's public `SignalSnapshot.execution_reports` is therefore the browser's structured report mirror. Node 2 joins each report to a recent/pending Node 3 intent only to display the original `reference_price`; report status/fill/quantity/error fields come from the Node 4 report mirror. Execution IDs are displayed when present; report ticket fields are shown only if a public payload includes the actual typed fields.

Node 4's `ExecutionSnapshot` also drives venue/ledger counters and its Node 3 link health. `OpenTradesSnapshot` fields are exactly `node4_open_trades`, `deriv_open_trades`, `mt5_open_positions`, `recent_trades`, `total_open_count`, `mt5_open_count`, and `timestamp`; Node 2 does not look for an assumed `node3_open_trades` field.

## Quantity and freshness rules

- Deriv options `buy_price` is the contract stake in account currency; `total_open_stake` is also currency.
- MT5 `Mt5Position.volume` and `Mt5Deal.volume` are lots. MT5 PnL is denominated in the public MT5 account currency. These are never added to Deriv currency stake.
- Node 4 `ExecutionReport.quantity_unit` is shown beside its `quantity`; the browser does not infer a unit.
- Generic `ExecutionTrade.size` has no unit and its `unrealized_pnl` has no currency in its public type. It is shown in a separate table as a Node 4 service record and is not aggregated with venue-specific quantities or PnL.
- Node 4's public `Mt5BridgeStatus` does not contain bridge reconnect/frame/request counters or a last-bridge-frame field. Node 2 displays the fields the contract exposes (protocol/version, links, heartbeat/snapshot ages, order counters, positions/history counts, halt/error, and uptime) and does not invent missing counters.
- Account margin level is not a serialized Node 4 field. If equity and non-zero margin are available, Node 2 labels `equity ÷ margin × 100` as derived.
- Closed `Mt5Deal` records expose one `price`; an open price is only displayed when an `entry: in` deal for the same position is present in the loaded snapshot. Otherwise it remains unavailable rather than being guessed.

## Independent stream lifecycle

`js/stream.js` is one reusable lifecycle manager instantiated separately for Market, Strategy, and Execution. Each instance owns its own:

- WebSocket connection and public subscription
- reconnect timer and bounded 1–30 second exponential backoff
- JSON heartbeat send/receive handling and silence timeout
- last message/data timestamps, error state, and stale badge
- visibility-change recovery and snapshot refresh callback

REST polling is owned by each service module. Strategy refreshes diagnostics/scanning/signals, and Execution refreshes diagnostics plus standalone public resources when the diagnostics snapshot is missing/incomplete. Market uses its existing chart/levels/calendar readers. A per-domain revision guard rejects REST data if a newer WS frame arrived in-flight. Snapshot fields and activity arrays are normalized before rendering, capped, deduplicated, and escaped.

Topbar values are `LIVE`, `RECONNECTING`, `STALE`, or `OFFLINE`. A configured-but-unreachable service keeps retrying without changing the other badges. An empty host disables only that service.

## Node 2 security boundary

Browser code may issue only public `GET` snapshots and public diagnostics WebSocket subscriptions. It must not include or transmit credentials or authorization headers; it must not connect to the private strategy/execution transport, the MT5 bridge session, any operator control route, or any broker write method. Safety state is informational and has no operator action control.

Unknown outcomes remain visually distinct and prominent. They mean an order write may have reached the broker and require reconciliation; Node 2 never labels them rejected or safe to retry.

## Run and troubleshoot

Run without a bundler:

```bash
python3 -m http.server 8080 --bind 0.0.0.0
```

Then open `http://localhost:8080`. For a hosted/static preview, public Node services must allow the page origin for REST CORS and WebSocket `Origin` checks. CORS failures do not justify adding browser credentials. For independent offline/stale diagnosis, check the corresponding badge tooltip, that service's diagnostics panel, and browser Network/Console. A live heartbeat with no current data becomes `STALE`; REST snapshots can keep sections populated while a stream reconnects.

Run validation:

```bash
npm run lint
npm test
```
