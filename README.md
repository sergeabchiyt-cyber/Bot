# XAUUSD Terminal — Node 2

A static, browser-only, read-only XAUUSD dashboard. The original terminal shell, chart library, typography, and panel styling are retained; its data wiring now reads market data from Node 1 and published levels/setups/signals from Node 3.

## Service configuration

`js/node2-client.js` is the sole typed HTTP client and centralized base-URL configuration. Defaults:

- Node 1: `https://engine-southeastasia-sng-main.onrender.com`
- Node 3: `https://strategy-api-vp-levels.onrender.com`

For deployment overrides, set `window.XAUUSD_CONFIG` before the client loads with `NODE1_BASE_URL` and/or `NODE3_BASE_URL`. Configure origins only, never credentials, paths, query strings, or secrets.

Node 1 GETs: `/candles?interval=1m|15m|1h`, `/tick-volume`, `/calendar`, `/status`; Node 1 WebSocket topics: `candle`, `tick_volume`, `calendar`, `status`. Node 3 GETs: `/health`, `/levels`, `/drawings`, `/scanning`, `/signals`, `/diagnostics`, plus optional `/vp`; Node 3 WebSocket topics: `levels`, `drawing`, `vp`, `scanning`, `signals`, `diagnostics`. All HTTP requests are GET. The browser does not call Node 4 or any execution endpoint.

Node 1 must allow the deployed dashboard origin in its CORS configuration. If its browser requests fail, the dashboard reports the likely CORS configuration requirement and keeps the last-good view.

## Display boundaries

- Candles are selectable at 1m, 15m, and 1h. Published Node 3 drawings use the payload color and line style; drawing identity is `id` plus `timeframe`.
- PS displays PoC only; missing VaH/VaL are not inferred.
- Scanner values and projected SL/TP/RR are presentation-only. Projected plans are labelled **display only** and **Node 4 sizes this**.
- Stale/offline freshness and data gaps show visible warnings; a Node 1 gap refetches candles.
- No broker, account, order-entry, cancellation, or sizing UI is present.

## Run and test

```sh
python3 -m http.server 8080 --bind 0.0.0.0
npm test
npm run lint
```

`tests/fixtures/dashboard-contracts.json` contains contract-shaped Node 1/Node 3 snapshots used by parser and mocked-client smoke tests. They are fixture tests, not claims of a live backend capture. See `docs/DASHBOARD_WIRING.md` for the pre-change fetch/WebSocket inventory.
