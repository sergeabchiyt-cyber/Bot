# Node 2 dashboard wiring inventory (pre-change)

This inventory was taken on `arena/37693168-bot` at the `Node2` baseline before
rewiring. It records every browser network entry point, every backend endpoint
string used by runtime code, and the fields the UI read from each response.

**Conclusion:** the baseline mixes three incompatible contracts. Node 3 did not
exist in the dashboard; Node 1's retired strategy/demo/levels surfaces were
still wired; and Node 4 diagnostics had a dedicated browser socket even though
Node 4 is out of scope (decision D7). The fix is to replace all of these with
`js/client.js`, whose only configured origins are Node 1 and Node 3.

## Runtime request constructors

| Location | Request | URL / topic | Backend | Fields read by the baseline | Contract status |
|---|---|---|---|---|---|
| `js/utils.js:33` | `fetch(url, {method:"GET", cache:"no-store"})` | URL supplied by callers | Node 1 or Node 4 | whole JSON body; unwrapped by each caller | Transport shape is reusable, but it is untyped, does not surface an HTTP 400 body, and is called from six modules. Move it into the typed client. |
| `js/socket.js:195` | `new WebSocket(endpoint)` | `wss://engine-southeastasia-sng-main.onrender.com/ws` | Node 1 | `frame.type`, `frame.data`, heartbeat timing | **Mismatch.** Subscribes to `levels`, `candle`, `tick_volume`, `strategy`, `demo`, `calendar`, `status`; Node 1 v2 supports only `candle`, `tick_volume`, `calendar`, `status`. Ignores `seq`, `replay:true`, and `gap`. |
| `js/execution.js:859` | `new WebSocket(cfg.ws)` | `wss://execution-southeastasia-sng-main.onrender.com/ws` | Node 4 | `diagnostics` frames, heartbeat timing | **Remove.** Node 4 must never be called by the browser and there is no Node 4 dashboard. |

## Runtime REST call sites

| Location | URL built from | Backend | Route | Fields read by the baseline | Contract status / action |
|---|---|---|---|---|---|
| `js/api.js:14` → `fetchPublicJson` | `config.endpoints.strategy` | Node 1 | `GET /strategy` | `snapshot.symbol`, `enabled`, `params`, `last`, `history`; decision `ts/action/score/confidence/price/atr/stop_loss/take_profit/reward_risk/lots/reasons/notes` | **Forbidden retired route.** Remove Node 1 strategy UI; setups now come from Node 3 `/scanning`. |
| `js/api.js:18` → `fetchPublicJson` | `config.endpoints.demo` | Node 1 | `GET /demo` | `account`, `params`, `position`, `stats`, `recent_trades`, `quote` | **Forbidden retired route.** Remove demo account/order UI. |
| `js/api.js:22` → `fetchPublicJson` | `config.endpoints.status` | Node 1 | `GET /status` | legacy `strategy`, `demo`, `feeds`, `ts` | Path remains, parser must change to `contract_version`, `boot_id`, `freshness`, `state`, `feeds`. |
| `js/api.js:27` → `fetchPublicJson` | `config.endpoints.executionDiagnostics` | Node 4 | `GET /diagnostics` | Node 4 `ExecutionSnapshot` (`market_feed`, `strategy`, `work`, `node3`) | **Remove.** Use Node 3 `/diagnostics` for contract violations, not Node 4. |
| `js/chart.js:295` → `fetchPublicJson` | `config.endpoints.candles` | Node 1 | `GET /candles` | `time/open/high/low/close/volume` | Path remains; needs the required `interval=1m\|15m\|1h` query and a parser that preserves `symbol`, `interval`, `source`, and `closed`. |
| `js/chart.js:327` → `fetchPublicJson` | `config.endpoints.tickVolume` | Node 1 | `GET /tick-volume` | array of `time/ticks/up_ticks/down_ticks/close/ticks_per_sec` | Path remains; parse the complete bar fields including `flat_ticks`, `last_tick`, and `closed`. The last bar may be in progress. |
| `js/levels.js:235` → `fetchPublicJson` | `config.endpoints.levels` | Node 1 | `GET /levels` | `window/poc/vah/val/swing_high/swing_low` and chart line styling from local config | **Wrong backend.** Rewire to Node 3 `/levels`, support PW/PS/CW only, absent `vah`/`val` for PS, and use `/drawings` for colour/style. |
| `js/calendar.js:196` → `fetchPublicJson` | `config.endpoints.calendar` | Node 1 | `GET /calendar` | `count/source/updated/events[]` (`event`, `impact`, `timestamp`, `time`, `actual`, `forecast`, `previous`, `currency`, `gold_relevant`) | Matches Node 1; call through the typed client. |
| `js/execution.js:801` → `fetchPublicJson` | `config.execution.diagnostics` | Node 4 | `GET /diagnostics` | legacy Node 4 execution snapshot | **Remove.** |

## Endpoint strings defined in config and HTML

| Location | String | Backend | Fields expected | Contract status / action |
|---|---|---|---|---|
| `js/config.js:26` | `MARKET_HOST=https://engine-southeastasia-sng-main.onrender.com` | Node 1 | origin only | Keep as the configurable default Node 1 base. |
| `js/config.js:28` | `NODE4_HOST=https://execution-southeastasia-sng-main.onrender.com` | Node 4 | origin only | **Remove.** Node 2 has no Node 4 base or secret. |
| `js/config.js:56` | `/ws` (Node 1) | Node 1 | `levels`, `candle`, `tick_volume`, `strategy`, `demo`, `calendar`, `status` | Narrow topics to `candle`, `tick_volume`, `calendar`, `status`; handle `replay`/`seq`/`gap`. |
| `js/config.js:57` | `/candles` | Node 1 | candle array | Keep, with interval query. |
| `js/config.js:58` | `/tick-volume` | Node 1 | tick-volume bar array | Keep. |
| `js/config.js:59` | `/levels` | Node 1 | legacy levels | **Move to Node 3.** |
| `js/config.js:60` | `/calendar` | Node 1 | calendar object | Keep. |
| `js/config.js:61` | `/strategy` | Node 1 | legacy strategy snapshot | **Remove.** |
| `js/config.js:62` | `/demo` | Node 1 | legacy demo snapshot | **Remove.** |
| `js/config.js:63` | `/status` | Node 1 | legacy status | Keep with the C1 v2 parser. |
| `js/config.js:65` | `/diagnostics` (Node 4) | Node 4 | `ExecutionSnapshot` | **Remove.** Node 3 `/diagnostics` is read through the Node 3 client instead. |
| `js/config.js:66` | `/ws` (Node 4) | Node 4 | `diagnostics` | **Remove.** |
| `index.html` | `https://unpkg.com/lightweight-charts@4.2.0/dist/lightweight-charts.standalone.production.js` | static CDN, not a backend | chart library global | Existing chart dependency; no backend data and no write. Keep because the chart is explicitly out of scope for visual changes. |

## WebSocket topics and frames

| Location | Topics / frame types | Backend | Fields read | Contract status / action |
|---|---|---|---|---|
| `js/config.js:72` | `levels`, `candle`, `tick_volume`, `strategy`, `demo`, `calendar`, `status` | Node 1 | dispatched by `frame.type` | **Mismatch:** remove `levels`, `strategy`, `demo`; keep the four C1 v2 topics. |
| `js/socket.js` | `subscribe`, `heartbeat`, unknown frames ignored | Node 1 | `type`, `data` | Add replay acknowledgement, `seq` gap detection, and explicit `gap` handling. Client still sends only `subscribe`/`heartbeat`. |
| `js/config.js:91` | `diagnostics` | Node 4 | full snapshot | **Remove.** |
| `js/execution.js` | `diagnostics`, `heartbeat` | Node 4 | full snapshot | **Remove.** |
| `js/app.js` | legacy `levels/strategy/demo/status/candle/tick_volume/calendar` routes | Node 1 | `frame.data` | Replace with separate Node 1 and Node 3 routers. Node 3 topics: `levels`, `drawing`, `vp`, `scanning`, `signals`, `diagnostics`; each frame is a full replacement. |

## Non-runtime endpoint references

These are documentation or test fixtures, not browser requests:

- `README.md`, `docs/SERVICE_ARCHITECTURE.md`, and `docs/EXECUTION_SNAPSHOT.md`
  describe the retired Node 1 strategy/demo/levels endpoints, the Node 4
  diagnostics socket, and `POST /mt5/control` as an unwired control. They are
  historical docs and must be replaced or clearly marked obsolete.
- `tests/config.test.js`, `tests/socket.test.js`, `tests/execution.test.js`,
  `tests/dashboard-fixtures.test.js`, `tests/status.test.js`, and
  `tests/public-fetch.test.js` contain endpoint strings and recorded legacy
  payload shapes. They assert the baseline contract and must be replaced with
  Node 3/Node 1 v2 fixtures.
- `tests/strategy-math.test.js`, `tests/format.test.js`, and
  `tests/normalization.test.js` are pure helper tests and contain no backend
  call. Only the strategy-math suite is tied to the removed evaluator.

## Required target shape

After rewiring, the only runtime transport module is `js/client.js`. It owns
both base-URL configs, every `fetch`, and both WebSocket constructors. Node 1
is candles/tick-volume/calendar/feed-status only. Node 3 is
health/levels/drawings/vp/scanning/signals/diagnostics only. There is no Node 4
client, no private execution route, no write request, and no browser reference
to a loopback address or credential.

## Implementation status

This target is now implemented by `js/config.js`, `js/client.js`,
`js/streams.js`, `js/panels.js`, `js/drawings.js`, `js/chart-controls.js`, and
the minimally rewired `js/chart.js` / `js/calendar.js`. The retired modules in
the first two tables were deleted, and `npm run build` enforces the boundary on
the emitted static bundle. `npm run verify` runs lint, recorded-fixture tests,
and that boundary check.
