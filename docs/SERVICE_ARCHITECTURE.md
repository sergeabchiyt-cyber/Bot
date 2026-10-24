# Node 2 service architecture

Node 2 is a static, read-only browser client of **two** services.

```text
                    ┌─────────────────────────────┐
   REST (GET only)  │  Node 1                     │
  ─────────────────►│  market data                │
                    │  built-in strategy evaluator│
   WS /ws           │  demo (paper) account       │
  ◄────────────────►│                             │
                    └─────────────────────────────┘
                    ┌─────────────────────────────┐
   GET /diagnostics │  Node 4                     │
  ─────────────────►│  execution snapshot         │
   WS /ws           │  market feed · local strategy│
  ◄────────────────►│  venue · Node 3 link         │
                    └─────────────────────────────┘
                                 ▲
                                 │ public GET + WS
                          ┌──────┴──────┐
                          │   Node 2    │  static HTML/CSS/JS
                          │  dashboard  │  no backend, no proxy
                          └─────────────┘
```

Node 2 has no database, no proxy, no server-side rendering and no write path.
It renders what Node 1 and Node 4 publish and never places, modifies or closes
orders. The Node 1 demo engine trades itself; Node 4's local strategy is
rendered, never re-implemented.

## Endpoint map

`js/config.js` defines `MARKET_HOST` (Node 1) and `NODE4_HOST` (Node 4) and
derives:

```text
Node 1
GET  /candles        chart history
GET  /tick-volume    volume / TPS snapshot
GET  /levels         volume-profile levels
GET  /calendar       USD economic calendar
GET  /strategy       StrategySnapshot seed
GET  /demo           DemoSnapshot seed
GET  /status         status + feeds
WS   /ws             levels · candle · tick_volume · strategy · demo · calendar · status

Node 4
GET  /diagnostics    ExecutionSnapshot seed (market_feed · strategy · work · node3)
WS   /ws             diagnostics only
```

Every path is fixed in code. The runtime override (`window.XAUUSD_CONFIG`)
accepts origins only — never a path, credential, query or fragment. An empty
host disables that service: an empty `MARKET_HOST` is reported as `NOT
CONFIGURED`, an empty `NODE4_HOST` leaves the execution cards in a calm
"not configured" state.

## Module graph

```text
config ─┬─ normalize ─┬─ utils ─┬─ api
        │             │         ├─ levels
        │             │         ├─ calendar
        │             │         └─ strategy, demo, status, execution
        ├─ format ────┴─ socket, chart, strategy, demo, overlays, status, execution
        ├─ strategy-math ── strategy, demo, overlays, status
        └─ axis ── chart ── (levels, overlays)
```

Only `js/app.js` wires Node 1 frame types to modules. `js/execution.js` owns the
Node 4 socket, because execution diagnostics are a second service with their own
reconnect budget: a dead Node 4 must never disturb the market stream, and a dead
Node 1 must not stop the execution cards from rendering. The strategy, demo,
chart, levels and calendar modules never talk to a service directly except
through `App.api` / `App.utils.fetchPublicJson` and never construct their own
socket.

## State

| Topic | Owner | Semantics |
|---|---|---|
| `strategy` | `js/strategy.js` | one reducer keyed on `decision.ts`; `last` moves forward, history ascending, capped at 120 |
| `demo` | `js/demo.js` | complete snapshot replaces the previous one; guarded against an older `ts` |
| `levels` | `js/levels.js` | per-window upsert; array payload is authoritative |
| candles | `js/chart.js` | upsert by bucket; REST snapshots guarded by a WS revision |
| `calendar` | `js/calendar.js` | sorted USD events, impact filter |
| `status` | `js/status.js` | last status snapshot, feeds rendered generically |
| `diagnostics` (Node 4) | `js/execution.js` | **complete** `ExecutionSnapshot` replaces the previous one; never merged. Only an older REST seed is rejected |

The REST payload is only a seed. A REST response whose decision data started
before a newer WebSocket frame is discarded (`wsRevision` guard); the evaluator
configuration (enabled + params) is still seeded because decision frames do not
carry it. `strategy.ts` dedupes replayed decisions, so a reconnect cannot
duplicate history.

## WebSocket lifecycle (`js/socket.js`, Node 1)

- exactly one socket, created from `endpoints.ws`;
- on every open: send the full `{"type":"subscribe","topics":[…]}` payload —
  the server replay is what reseeds the panels after a reconnect;
- heartbeat sent every 30 s; 75 s of silence forces a reconnect;
- bounded exponential backoff, 1 s → 30 s, reset on a successful open;
- watchdog republishes state every 5 s so `STALE` appears without a frame;
- returns to a prompt reconnect when the tab becomes visible.

State published to the header badge: `LIVE`, `CONNECTING`, `RECONNECTING`,
`STALE`, `OFFLINE`, `NOT CONFIGURED`.

## Node 4 socket (`js/execution.js`)

- one socket to `NODE4_HOST + /ws`, subscribing to exactly `["diagnostics"]`;
- every `diagnostics` frame is a complete snapshot and replaces the stored one;
- heartbeat every 30 s, silence over 75 s forces a reconnect;
- bounded backoff 1 s → 30 s, visibility recovery, same shape as the market
  socket;
- while the socket is down, `GET /diagnostics` is polled every 5 s; the poller
  stops as soon as a socket is open;
- a render is a pure function of the snapshot. Identical frames reuse the
  already-painted markup, so repeated snapshots do not flicker or duplicate
  rows, and a snapshot with cleared counters visibly resets the cards.

## Polling fallback

`js/app.js` watches `App.socket.onState`. If the socket is configured but not
connected for more than 1.5 s, it:

1. sets `App.runtime.polling = true`;
2. shows the `DELAYED` badge;
3. polls `GET /strategy` and `GET /demo` every 5 s.

A reconnect clears the badge, stops the poller and lets the replay take over.
While the socket is healthy there is no polling of those routes.

## Rendering domains

- **Strategy panel** — action, confidence, score, regime, volatility, price/ATR,
  stop/target, size/risk, reasons sorted by `|contribution|`, flat notes,
  blackout countdown, 30-decision history strip.
- **Demo strip** — mode, leverage, spread, balance, equity, free margin, margin
  level, open PnL. Values come from `demo.account` / `demo.params`.
- **Open position** — entry/SL/TP, lots, floating PnL, margin, risk, reason,
  stop→target progress with bid/ask marker.
- **Performance** — the six `demo.stats` figures, profit factor capped at `99+`.
- **Trade history** — last 50 closed trades, newest first, exit-reason chips.
- **Chart** — candles and volume on the compacted `axis.js` timeline, levels,
  position lines, action marker, optional ATR band.
- **Execution cards** — market-data gate (`trading_ready`, with
  `not_ready_reason` verbatim and a feed-integrity row), the local strategy
  runner (action, ATR, bands, counters, emitted/suppressed intents), the
  execution venue (dry-run banner when `venue === "none"`) and the Node 3
  intent link (grey "not used — local strategy" in local mode).
- **Drawer** — read-only connection, strategy/demo blocks, parameters, feeds.

All money is USD and all prices are XAUUSD price units. There is no pip or
contract-size conversion in the browser.

## Formatting boundary

`js/format.js` is the only place that turns a number into visible text:
price 2 dp with grouping, lots 2 dp, money 2 dp with `$`, PnL signed, percent
1 dp, R multiple 2 dp signed, local `HH:mm:ss` / `DD MMM`, confidence as a
0–100 % value. Nulls render as `—`.

## Failure modes

| Failure | Behaviour |
|---|---|
| REST unreachable on mount | skeletons with an explanatory empty state; socket keeps retrying |
| socket down | `DELAYED` badge + 5 s REST polling |
| socket open but silent | `STALE` badge after 90 s; strategy panel dims after 60 s without a decision |
| malformed JSON frame | logged, ignored; connection stays up |
| Node 4 unreachable | execution cards show a calm "waiting / not configured" state; `/diagnostics` polled every 5 s; market panels unaffected |
| Node 4 older than `market_feed` / `strategy` | those cards name the endpoint, the reported build and the missing key, and recommend redeploying Node 4; all other panels render |
| a non-Node-4 service answering `/diagnostics` | payload ignored; the cards keep waiting rather than blaming a missing section |
| feed `state === "live"` but `trading_ready === false` | market card reads **not ready** in amber; `state` never drives colour |
| diagnostics frame with cleared counters | the UI resets (replace, never merge) |
| chart library missing | chart area empty; strategy/demo/levels/calendar still render |
| older REST snapshot | discarded by the revision / `ts` guard |
| demo engine disabled | dimmed account strip + "demo engine off" |
