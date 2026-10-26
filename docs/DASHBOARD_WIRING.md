# Dashboard wiring inventory (before Node 2 contract migration)

Inventory captured from the `Node2` branch before implementation changes. URLs below are resolved from `js/config.js`; the default origins were `MARKET_HOST=https://engine-southeastasia-sng-main.onrender.com` and `NODE4_HOST=https://execution-southeastasia-sng-main.onrender.com`.

| Call site | URL / topic | Backend | Fields consumed by current UI | Contract status |
|---|---|---|---|---|
| `js/chart.js` (`fetchPublicJson`) | `GET {MARKET_HOST}/candles` | Node 1 | Candle `time`, `open`, `high`, `low`, `close`, `volume`, `symbol`, `interval`; chart and tick-volume rendering | Route exists, but request omits required interval selector; currently only the legacy timeframe is displayed. |
| `js/chart.js` (`fetchPublicJson`) | `GET {MARKET_HOST}/tick-volume` | Node 1 | `ticks`, `up_ticks`, `down_ticks`, `flat_ticks`, `ticks_per_sec`, `last_tick`, `closed` | Route and intended fields match. |
| `js/calendar.js` (`fetchPublicJson`) | `GET {MARKET_HOST}/calendar` | Node 1 | Calendar event timestamps, currency, impact, title/name and event details (normalized by `js/calendar.js`) | Route matches; existing display is a USD event list. |
| `js/status.js` (`App.api.fetchStatus`) | `GET {MARKET_HOST}/status` | Node 1 | Legacy `status.feeds`, market/feed health and strategy/demo summary | Route overlaps, but current renderer is for old Node 1 status and does not implement Node 1 `freshness` / `contract_version` contract. |
| `js/levels.js` (`fetchPublicJson`) | `GET {MARKET_HOST}/levels` | Node 1 | Legacy windows, PoC/VaH/VaL and level metadata; also supplies chart markers | **Must be rewired:** Node 1 `/levels` is prohibited. New source is Node 3 `/levels`; PS must omit VaH/VaL. |
| `js/strategy.js` (`App.api.fetchStrategy`) | `GET {MARKET_HOST}/strategy` | Node 1 | Decision action, price, score/confidence, stops/targets, reasons, parameters and history | **Must be removed:** Node 1 strategy is prohibited; Node 3 `/scanning` is the permitted setup source. |
| `js/demo.js` (`App.api.fetchDemo`) | `GET {MARKET_HOST}/demo` | Node 1 | Simulated account, position, trade history and performance | **Must be removed:** Node 1 `/demo` is prohibited and account UI is out of scope. |
| `js/socket.js` | `WS {MARKET_HOST}/ws`; sends subscribe topics `levels`, `candle`, `tick_volume`, `strategy`, `demo`, `calendar`, `status`, and periodic `heartbeat` | Node 1 | Full frame snapshots routed to levels, chart, calendar, strategy, demo and status readers | **Must be rewired:** `levels`, `strategy`, and `demo` are prohibited Node 1 topics. Node 1 allowed topics are `candle`, `tick_volume`, `calendar`, `status`; use separate Node 3 socket and its six public topics. `gap` handling/refetch is absent. |
| `js/execution.js` | `GET {NODE4_HOST}/diagnostics`; `WS {NODE4_HOST}/ws`, topic `diagnostics`; periodic heartbeat | Node 4 | Execution diagnostics, market-feed/strategy/venue/Node 3 linkage | **Remove:** Node 4 must never be called by this dashboard (D7). |

There were no Node 3 requests in the pre-migration implementation. There were no `POST`, `PUT`, `PATCH`, or `DELETE` requests. `js/utils.js` contains the shared `fetch()` transport used by the REST call sites above, and `js/socket.js` / `js/execution.js` construct the two WebSockets.

This file deliberately preserves the before-state inventory; the live contract wiring is described in the migrated source and acceptance tests.
