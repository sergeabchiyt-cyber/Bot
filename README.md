# XAUUSD Terminal — Node 2

A dependency-free, static, **read-only** frontend. The existing lightweight-charts
renderer and visual shell are preserved; Node 3 and Node 1 v2 data are wired
through one typed client.

## Services

| Service | Default origin | Browser responsibilities |
|---|---|---|
| Node 3 | `https://strategy-api-vp-levels.onrender.com` | `/health`, `/levels`, `/drawings`, `/vp`, `/scanning`, `/signals`, `/diagnostics`, public `/ws` |
| Node 1 | `https://engine-southeastasia-sng-main.onrender.com` | `/candles`, `/tick-volume`, `/calendar`, `/status`, public `/ws` |

Node 4 is not a browser backend. There is no Node 4 origin, secret, order entry,
cancel, broker, venue, or account control anywhere in this bundle. Projected
stops/targets/RR are labelled **“Node 4 sizes this”** and are display-only.

Open `index.html` from any static host, or serve the repository root. To change
origins, install an override before `js/config.js`:

```html
<script>
  window.XAUUSD_CONFIG = {
    NODE1_HOST: "https://your-node1-host",
    NODE3_HOST: "https://strategy-api-vp-levels.onrender.com"
  };
</script>
```

`NODE1_HOST` also accepts a same-origin proxy prefix such as `/api/node1`;
`NODE3_HOST` accepts `/api/node3`. Both bases are origin-only. The two browsers'
sockets are derived from the same bases, and browser code never points at a
loopback address.

## Contract

- Node 1 publishes `candle`, `tick_volume`, `calendar`, `status` on `/ws`.
  Replay frames have `replay: true`; live frames have a strictly increasing
  `seq`. A `gap` frame or a missed sequence produces a visible warning and a
  `/candles` refetch. `/candles` accepts `interval=1m|15m|1h`.
- Node 3 publishes `levels`, `drawing`, `vp`, `scanning`, `signals`,
  `diagnostics` on `/ws`. Each frame is treated as a full replacement for its
  topic. Drawings are deduped by `id` + `timeframe`.
- Both services publish contract version `2.0`; the header shows both.
- `/health` failure retains the last-good chart and panels and shows
  “Node 3 unreachable” with the last-good timestamp.
- `/diagnostics` provides `work.contract_violations` and `work.last_violation`.
- /vp custom ranges validate `start < end` and `rows` 2–5000. HTTP 400 error
  bodies are surfaced in the FRVP panel.

## What the dashboard does

- Existing candlestick chart with 1m / 15m / 1h switch, forming-bar treatment,
  and a compact display timeline.
- Node 3 `/drawings` overlays: horizontal levels, Sunday-open dotted line,
  daily rays from `ray_from_time`, and FRVP rectangles whose rows carry
  `volume`, `up_volume`, and `down_volume`. No profile values are computed here.
- PW / PS / CW levels table (PS is PoC-only), setup cards grouped by window,
  pending/recent signals, execution reports, tick volume, calendar, and a full
  read-only diagnostics panel.
- Header health pill, Node 1 freshness badge, contract versions, connection
  banner, and a service-status drawer.
- REST seeds paint before sockets open; the two public sockets then replay and
  replace whole topics. If a socket is unavailable, a labelled REST poll is
  used instead of leaving silent stale data.

## Files

- `js/config.js` — the only base-URL configuration.
- `js/client.js` — the only `fetch` and `WebSocket` owner; all typed parsers.
- `js/streams.js` — two public subscription routers.
- `js/panels.js` — read-only views and warning/banner state.
- `js/drawings.js` — chart primitive from published geometry.
- `js/chart.js`, `js/axis.js`, `js/calendar.js` — preserved chart/calendar
  implementation with minimal contract wiring.
- `docs/DASHBOARD_WIRING.md` — pre-change endpoint inventory and target shape.
- `tests/fixtures/` — recorded Node 3 snapshots and Node 1 candle fixtures.

## Checks

```sh
npm run lint
npm test
npm run build
# or all three:
npm run verify
```

`npm run build` copies only assets loaded by `index.html` into ignored `dist/`
and fails if that output contains a loopback URL, a private route, a retired
endpoint, a write verb, or a transport constructor outside `js/client.js`.
