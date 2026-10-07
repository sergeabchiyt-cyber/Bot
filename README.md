# XAUUSD Terminal — Node 2 dashboard

Node 2 is a static, browser-only, **read-only** dashboard. It visualizes market data from Node 1, strategy state from Node 3, and broker-authoritative execution/account state from Node 4. It has no build step or runtime package dependencies.

## Four-node topology and ownership

```text
Node 1 — market data
  candles · tick volume · levels · calendar · order-flow events
            │ public REST + WebSocket
            ▼
Node 2 — this static dashboard (read-only browser)
            ▲ public diagnostics / snapshots
            │
Node 3 — strategy scanner
  break/retest setups · projected SL/TP/RR · pending/recent intents
  delivery state and Node 1 / Node 4 connection state
            │ authenticated service-to-service intent/report link
            ▼
Node 4 — execution and broker state
  venue selection/health · execution outcomes · Deriv options
  MT5 account/bridge/EA · positions/history · PnL · safety state
```

Ownership is intentionally not blended in the UI:

- **Node 1** owns the market chart, candles, tick volume, levels, calendar, and order-flow events.
- **Node 3** owns strategy scanning, active setups, strategy projections, intents, delivery state, and its view of the Node 1 / Node 4 links. A Node 3 intent is not a fill.
- **Node 4** owns intent processing, execution outcomes, venue health, Deriv account/contracts, MT5 account/bridge/positions/closed deals, PnL, and halt/kill-switch state.
- **Node 2** reads public HTTP and WebSocket resources only. There are no trading, halt, resume, flatten, close, or broker-control buttons.

The implementation was checked against the current `Node3` and `Node-4` branch contracts in `docs/NODE3_NODE4_PROTOCOL.md`, `node3-strategy/src/types.rs`, `node3-strategy/README.md`, the Node 4 README and `node4-execution/src/types.rs`, Node 4 public route/topic code, and the MT5 frontend/architecture guides.

## Service endpoints

`js/config.js` has three explicit host values and endpoint groups. Defaults are sourced from the checked-in service configuration/documentation:

| Host | Default base | Browser resources |
|---|---|---|
| `MARKET_HOST` → Node 1 | `https://engine-southeastasia-sng-main.onrender.com` | `WS /ws`, `GET /candles`, `/tick-volume`, `/levels`, `/calendar` |
| `STRATEGY_HOST` → Node 3 | `https://strategy-southeastasia-sng-main.onrender.com` | `WS /ws`, `GET /diagnostics`, `/scanning`, `/signals` |
| `EXECUTION_HOST` → Node 4 | `https://execution-southeastasia-sng-main.onrender.com` | `WS /ws`, `GET /diagnostics`, `/open-trades`, `/deriv`, `/account`, and `/mt5/*` snapshots |

The Node 3 public hostname is taken from Node 4's checked-in service URL configuration. The Node 4 hostname is taken from `docs/mt5/FRONTEND_RESOURCES.md`; Node 2 does not guess a hostname.

### Runtime host overrides

A host-only override can be installed before `js/config.js` runs. For example, add this small script before the config script in `index.html`, or inject it from the static host:

```html
<script>
  window.XAUUSD_CONFIG = {
    MARKET_HOST: "https://market.example",
    STRATEGY_HOST: "https://strategy.example",
    EXECUTION_HOST: "https://execution.example"
  };
</script>
```

An explicitly empty host disables that service and its badge reports **NOT CONFIGURED** (also announced in its accessible label). Overrides accept only an HTTP(S)/WS(S) origin: endpoint paths are fixed in code to the documented public routes, and values containing credentials, query strings, fragments, or custom paths are rejected. Runtime configuration must contain **hostnames only**: never add credentials, tokens, passwords, or authorization headers. For a private/local development host, use its explicit `http://` / `ws://` scheme; bare hostnames default to HTTPS/WSS.

## Public topics consumed

### Node 1 market stream

Node 2 keeps the existing chart/market adapter and subscribes on public `/ws` to the market topics needed by the page: `levels`, `candle`, `bubbles`, `trades`, `calendar`, `tick_volume`, and `sentiment`. Browser code does not call a market vendor or broker directly.

### Node 3 strategy stream

Node 2 subscribes to public `/ws` topics:

```json
{"type":"subscribe","topics":["diagnostics","scanning","signals","diagnostic_event"]}
```

The dashboard reads `DiagnosticsSnapshot`, `ScanningSnapshot`, `SignalSnapshot`, and `ActivityLogEntry` shapes from Node 3. `signals` includes pending/recent intents and the broker-authoritative `execution_reports` mirror. The authenticated Node 3-to-Node 4 transport is not used by Node 2.

### Node 4 execution stream

Node 2 subscribes to public `/ws` topics implemented by Node 4:

```json
{"type":"subscribe","topics":["diagnostics","open_trades","trades","deriv_account","mt5_account","mt5_positions","mt5_history","bridge_status","bridge_event","diagnostic_event","heartbeat"]}
```

The `trades` **subscription topic** is emitted with the exact public frame type `{"type":"trade","data": ExecutionTrade}`. Other snapshot frames use their implemented types: `diagnostics`, `open_trades`, `deriv_account`, `mt5_account`, `mt5_positions`, `mt5_history`, `bridge_status`, and the `bridge_event` / `diagnostic_event` event envelopes. Node 4 does **not** publish a structured `execution_report` topic to browsers. Node 2 therefore renders reports from Node 3's public `signals.execution_reports` mirror and labels that source; it does not fabricate a Node 4 frame or report fields.

Node 4 public REST resources consumed as snapshot/fallback sources:

- `GET /diagnostics` (also `/status`)
- `GET /open-trades` (also `/trades`)
- `GET /deriv` (also `/account`)
- `GET /mt5/account`
- `GET /mt5/positions`
- `GET /mt5/history`
- `GET /mt5/status`

The diagnostics snapshot is preferred; individual public resources are used when it is unavailable or incomplete.

## Schema and unit distinctions

- Strategy setup and intent prices use Node 3's exact `level_price`, `retest_zone_low/high`, `invalidation_price`, `projected_buy`, `projected_sell`, `active_projection`, `reference_price`, `stop_loss`, `take_profit`, `risk_reward`, `created_at`, and `expires_at` fields.
- Broker reports use the shared report fields exposed in Node 3's public type: `intent_id`, `status`, `venue`, `symbol`, `side`, `timestamp`, `execution_id`, `filled_price`, `quantity`, `quantity_unit`, `error_code`, and `error_message`. Ticket fields are shown only if a public report actually contains them; separate MT5 ticket IDs are displayed in the MT5 tables.
- Deriv options `buy_price` / open stake and contract payout are currency amounts. They are never presented as MT5 lots.
- MT5 position/deal `volume` is shown in **lots**. MT5 PnL is denominated in the public MT5 account currency; MT5 values remain separate from Deriv stake.
- Node 4's generic `ExecutionTrade.size` has no unit and its `unrealized_pnl` has no currency in its public type; Node 2 labels both as not supplied and does not aggregate them with Deriv stake or MT5 lots/account-currency PnL.
- MT5 `margin_level` is not a Node 4 field; when available, Node 2 labels the computed equity-to-margin ratio as derived.
- Node 4 does not expose bridge reconnect/frame/request counters in `Mt5BridgeStatus`; the dashboard shows the actual public order counters and explicitly notes those absent fields.

## Independent connections and stale data

Market, Strategy, and Execution have separate WebSocket instances, badges, reconnect timers, bounded exponential backoff, heartbeat handling, last-message/data times, stale detection, error state, and visibility recovery. One offline service cannot mark the other two offline or blank their panels.

REST reads provide initial and fallback snapshots. Per-domain revision guards discard a REST response if a newer WebSocket frame arrived while it was in flight. Market candles, levels, and calendar use the same protection. Activity and history collections are bounded and deduplicated in the browser.

The badges' accessible labels and tooltips show the last data update and last WebSocket message time. Node 1 is marked stale after 90 seconds without market data; Node 3 / Node 4 after 45 seconds without service data. MT5 account/position freshness is separately shown using the public snapshot timestamps and the Node 4 bridge freshness contract.

## Security boundary

- No broker or service credentials are stored in this repository's browser code or runtime host config.
- The browser uses public read-only resources only; it does not attach authorization headers.
- No private strategy/execution transport, MT5 bridge session, control resource, or broker write endpoint is requested.
- `unknown` execution outcomes are highlighted as ambiguous broker outcomes, not rejected orders or safe retries.
- Halt and kill-switch information is displayed read-only. Operator actions remain outside Node 2.

## Run locally

No build or install step is required:

```bash
cd /path/to/Bot
python3 -m http.server 8080 --bind 0.0.0.0
```

Open `http://localhost:8080`. A local static host can serve the same files. For remote service testing, configure CORS on each public service for the dashboard origin. The embedded chart library is loaded from the existing Lightweight Charts CDN reference.

Checks:

```bash
npm run lint
npm test
```

## Troubleshooting

- **A badge shows OFFLINE or NOT CONFIGURED**: check that service's runtime host override and confirm it runs before `js/config.js`.
- **One badge reconnects while the others are live**: inspect that service's browser network/console entries; failures remain scoped to that service.
- **REST returns a CORS error**: allow the deployed Node 2 origin on the relevant public GET/WS service. Do not fix this by adding credentials or calling a private route.
- **Badge is STALE with a live socket**: heartbeats prove the connection is open, but no fresh service data has arrived. Check the upstream producer and the service's own diagnostics.
- **Node 3 shows no scanner setups**: verify Node 3 receives Node 1 candles and levels; inspect Node 3 diagnostics, Node 1 state, and last candle/level timestamps.
- **Execution reports are absent**: Node 2 reads the report mirror from Node 3 `signals`; Node 4's public WebSocket has no report topic. Check the Node 3-to-Node 4 service link and Node 3's report receive counters.
- **Deriv or MT5 says unauthorized / not configured**: the browser only renders Node 4's public health/error/setup fields. Resolve configuration on Node 4 or the bridge host, never in Node 2.
- **MT5 shows stale or unknown mode**: treat it as a safety warning; inspect Node 4's account, status, and bridge snapshots. Node 2 cannot resume or control the account.
