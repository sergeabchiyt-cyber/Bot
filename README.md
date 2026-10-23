# XAUUSD Terminal — Node 2 dashboard

A static, browser-only, **read-only** dashboard for the Node 1 service:

- XAUUSD candles, tick volume, volume-profile levels and the USD calendar;
- the **built-in strategy evaluator** (`GET /strategy`, WS `strategy`);
- the **demo (paper) account** (`GET /demo`, WS `demo`) — XAUUSD, 1:1000
  leverage, 0.10 spread, 0.01–1.00 lots.

There is no build step, no runtime dependency, no server-side rendering and no
backend in this repository. The page is plain HTML/CSS/ES2020 scripts loaded
with `<script>` tags, served from any static host.

## Read-only by construction

The browser makes **GET requests only**. There is no `POST`, `PUT`, `PATCH` or
`DELETE` in the codebase, no order ticket, no buy/sell button, no lot input and
no close button. The Node 1 demo engine opens and closes its own simulated
trades; the dashboard only observes the resulting decisions, positions and
account snapshots.

Node 2 also never derives money, margin or PnL client-side. `position.margin`,
`account.margin_level`, `risk_usd`, `r_multiple` and every other money value are
rendered exactly as supplied.

## Endpoints

`js/config.js` holds one host, `MARKET_HOST`. All paths are fixed in code.

| Resource | Path | Used for |
|---|---|---|
| WebSocket | `WS /ws` | every live topic |
| Strategy | `GET /strategy` | initial `StrategySnapshot` seed |
| Demo | `GET /demo` | initial `DemoSnapshot` seed |
| Status | `GET /status` | settings drawer, feed liveness |
| Candles | `GET /candles` | chart history |
| Tick volume | `GET /tick-volume` | volume bars |
| Levels | `GET /levels` | POC / VAH / VAL lines |
| Calendar | `GET /calendar` | USD event list |

`/strategy` and `/demo` send `Cache-Control: no-store`; the client also requests
`cache: "no-store"` and never sends a body or authorization header. Both are
fetched on mount so the UI has state before the first frame, and are then kept
live by the WebSocket.

### Host override

Install a host-only override before `js/config.js`:

```html
<script>
  window.XAUUSD_CONFIG = { MARKET_HOST: "https://your-node1-host" };
</script>
```

Only an `http(s)`/`ws(s)` origin is accepted. Values containing credentials,
query strings, fragments or custom paths are rejected. An empty host disables
the service and the header badge reads `NOT CONFIGURED`. Configure Node 1's
`CORS_ALLOWED_ORIGIN` (and its WebSocket origin check) for the deployed
dashboard origin.

## WebSocket contract

One socket, one subscription:

```json
{"type":"subscribe","topics":["levels","candle","tick_volume","strategy","demo","calendar","status"]}
```

| Frame | Payload | Handling |
|---|---|---|
| `strategy` | `StrategyDecision` | replaces `last`, appends to history keyed on `ts` |
| `demo` | `DemoSnapshot` | complete snapshot; replaces the previous one |
| `status` | status snapshot | drawer + feed liveness |
| `levels` | snapshot / one window | price lines and levels table |
| `candle` | candle | chart upsert |
| `tick_volume` | tick bar | volume histogram + TPS |
| `calendar` | event list | calendar panel + next-event ticker |

On subscribe (and on every reconnect) Node 1 replays the latest `strategy`
frame, a `demo` snapshot and a `status` frame. The client re-sends the **full**
subscribe payload on every open, so the replay reseeds the panels. History is
keyed on `decision.ts`: a replay replaces its entry and never duplicates it.

The socket sends `{"type":"heartbeat"}` every 30 s, reconnects with bounded
exponential backoff (1 s → 30 s), forces a reconnect after 75 s of silence, and
reconnects promptly when the tab becomes visible again.

### Polling fallback

While the socket is down, `GET /strategy` and `GET /demo` are polled every 5 s
and the header shows a `DELAYED` badge. When the socket returns, polling stops
and the WebSocket replay takes over again.

## Panels

### Strategy (top of the right rail)

Action badge (`BUY`/`SELL`/`FLAT`), confidence gauge with a tick at
`params.min_confidence`, zero-centred score bar (−1…+1), regime and volatility
chips, price/ATR, stop/target, lots and risk, age from `decision.ts`, and the
full factor list sorted by `|contribution|` with the engine's `detail` line.
`notes[]` render as amber footnotes when the decision is `flat`; a future
`next_event_ts` adds "event in N min — standing aside". A 30-decision strip
shows action colours with a confidence sparkline. The panel greys out after
60 s without a decision and reports the socket state.

### Demo account strip

`DEMO 1:1000 · spread 0.10` (read from the payload, never hardcoded) followed by
Balance, Equity, Free margin, Margin level and Open PnL. PnL is green/red with a
leading sign; margin level is amber below 200 % and red below 100 %. A flat
account shows `—`, not 0 % or infinity. Negative equity is red and not clamped.

### Open position

Side, lots, entry, stop, target, floating PnL, margin, risk, reason and opening
time. The progress bar spans stop → target with the profit side shaded green,
entry marked, and the live marker at the bid (long) or ask (short). When flat the
card says "No open position — the engine is waiting for a qualifying signal" and
shows the last engine note.

### Performance and trade history

Closed trades, win rate, net PnL, profit factor (— with no closed trades, capped
at `99+`), average R and max drawdown. The trade table shows the last 50 closed
trades newest first: `#`, side, lots, entry, exit, P&L, R, exit reason chip
(SL/TP/flip/stop-out), opened and closed times.

### Status & settings drawer

Read-only connection state, the `/status` strategy and demo blocks, the full
`/strategy` and `/demo` parameters, and `status.feeds` liveness. There are no
inputs and nothing to save.

### Chart overlays

Levels (POC/VAH/VAL) plus the open position's entry/SL/TP lines, a marker on the
candle where the current buy/sell action began, and an optional ATR band at
decision price ± ATR. No other overlays exist.

## Formatting rules

Every displayed value goes through `js/format.js`:

| Value | Format | Example |
|---|---|---|
| Price | 2 dp, grouped thousands | `3,415.10` |
| Lots | 2 dp | `0.18`, `1.00` |
| Money | 2 dp, `$`, grouped | `$10,000.00` |
| PnL | signed money | `+$12.40`, `−$61.00` |
| Percent | 1 dp | `62.0%` |
| R multiple | 2 dp, signed, `R` | `+1.85R`, `−1.00R` |
| Confidence | 0–100 %, 1 dp | `62.0%` |
| Time | local `HH:mm:ss` | `14:03:22` |
| Date | local `DD MMM` | `23 Oct` |

Null, blank and non-finite values always render as `—`. Raw floats, `null` and
`NaN` never reach the DOM. Prices are price units — there is no pip conversion.

## Edge cases covered

- `strategy.last === null` — "Waiting for the first evaluation…" skeleton.
- `action === "flat"` with zero lots — badge and notes, never a zero-lot position.
- `demo.position === null` with closed trades — history still renders.
- `margin_level === 0` — `—`, not `0 %`.
- negative equity — shown in red, never clamped.
- null `stop_loss` / `take_profit` — `—` in the panel and no chart line.
- stale decision (> 60 s) — dimmed strategy panel plus socket state.
- reconnect replay — history keyed on `ts`, no duplicates.
- `closed_trades === 0` — win rate `0.0%`, profit factor `—`.
- API unreachable — skeleton states, no crash; the socket keeps retrying.
- malformed frames and hostile strings — rejected or HTML-escaped.

## Run locally

No install or build is required:

```bash
cd /path/to/Bot
python3 -m http.server 8080 --bind 0.0.0.0
```

Open `http://localhost:8080`. The chart library is loaded from the existing
Lightweight Charts CDN reference; without it the chart area stays empty while
the strategy and demo panels keep working.

Checks:

```bash
npm run lint   # node --check on every js/*.js
npm test       # node --test tests/*.test.js
```

## File layout

```text
index.html          page shell and panel containers
style.css           dark theme, rail + drawer layout
js/
  config.js         Node 1 host, endpoints, topics, timings
  normalize.js      frame unwrapping, timestamp/number coercion, escaping
  format.js         the one formatter (price/lots/money/percent/R/time)
  strategy-math.js  pure reducers, score/progress geometry, reason sorting
  utils.js          DOM helpers, timestamps, and the public GET helper
  api.js            GET /strategy · /demo · /status
  socket.js         the one WebSocket: subscribe, heartbeat, backoff, dispatch
  axis.js           compacted chart timeline (closed-market gaps)
  chart.js          candles, volume, price lines, reset hook
  levels.js         volume-profile lines and the levels table
  calendar.js       USD calendar
  strategy.js       strategy panel + decision reducer
  demo.js           demo strip, position, performance, trade table
  overlays.js       position lines, action marker, ATR band
  status.js         read-only status & settings drawer
  app.js            boot, frame routing, polling fallback
tests/              node:test suites (config, format, math, socket, status, normalization, fixtures, fetch)
docs/SERVICE_ARCHITECTURE.md  client-side architecture notes
```

## Security boundary

- No broker or service credentials in browser code or runtime config.
- Public read-only GET/WS resources only; no authorization headers.
- No control, halt, close or broker-write endpoint is requested.
- The demo account is simulated: no broker, no real money.
