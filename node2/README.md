# Node 2 — Read-Only Demo Market Dashboard

A static, read-only dashboard that presents:

- **Node 1** market data (candles, volume-profile levels, tick volume,
  calendar, strategy decisions, demo engine state) over the versioned
  REST/WS contract published in Node 1's `FRONTEND_NEXT.md`
- **Node 4** execution state (diagnostics, open trades, MT5 account /
  positions / history / bridge status, event log) over the public resources
  documented in `docs/mt5/FRONTEND_RESOURCES.md`

The dashboard makes **no trading decisions**, submits **no orders**, and
**never holds tokens or broker credentials in browser code**.

## What it is NOT

- Not a strategy engine — Node 1 owns decisions.
- Not an order client — Node 4 owns the order lifecycle; there is no
  order-ticket UI and no POST anywhere in the browser bundle.
- Not a control surface — Node 4's operator endpoints (control/bridge) are
  never referenced from browser code (enforced by `tests/security.test.js`).

## Layout

```
node2/
  serve.mjs              dev/demo server: static files + REST proxy + WS proxy
  mock/feed.js           contract-shaped synthetic feeds (demo + tests)
  public/                the browser app (no build step, ES modules)
    index.html           shell
    styles.css           responsive dark theme (3/2/1 columns)
    src/
      config.js          endpoint resolution (?node1=/?node4= overrides,
                         loopback URLs rejected)
      api/               GET-only REST helpers (node1.js, node4.js)
      ws/                reconnecting socket + polling fallback + topics
      state/store.js     ts-keyed upserts, caps, frame reducers
      lib/               format, freshness, lifecycle, parse, safejson
      render/            dom helper, chart, panels, layout breakpoints
      main.js            bootstrap and wiring
  tests/                 node --test suites (parsing, freshness, lifecycle,
                         reconnect/disconnect, stale data, layout, security,
                         store, integration)
```

## Run

```bash
npm install          # devDependency: ws (dev server only; browser is dependency-free)
npm start            # http://localhost:3000 (mock mode by default)
npm test             # node --test suite
```

### Modes

| Env var | Values | Meaning |
| --- | --- | --- |
| `NODE1_MODE` | `mock` \| `live` | Serve Node 1 resources from the synthetic feed or proxy to the engine |
| `NODE4_MODE` | `mock` \| `live` | Serve Node 4 resources from the synthetic feed or proxy to the service |
| `NODE1_BASE` | URL | Node 1 engine base (default `https://engine-southeastasia-sng-main.onrender.com`) |
| `NODE4_BASE` | URL | Node 4 service base (default `https://execution-southeastasia-sng-main.onrender.com`) |
| `PORT` / `HOST` | int / host | Bind address (default `3000` / `0.0.0.0`) |

The server binds `0.0.0.0`, serves the dashboard, and maps `/api/node1/*`
and `/api/node4/*` (REST, GET-only — other methods get 405) to the live
services in `live` mode or to the synthetic feed in `mock` mode.
WebSocket upgrades on `/api/node1/ws` and `/api/node4/ws` proxy to the
services or run the mock sessions (initial replay in the contract order,
then heartbeats and refreshes).

`mock` mode is clearly labeled (the header `DEMO` badge, mock strategy
notes, mock engine version). The browser never knows it is talking to the
mock — it speaks the same contract either way.

## Browser-side configuration

Defaults are **same-origin relative paths** (`/api/node1`, `/api/node4`,
`/api/node1/ws`, `/api/node4/ws`), so the browser never depends on
cross-origin CORS and never targets localhost. For direct
cross-origin deployments (no reverse proxy), override with query params:

```
https://dash.example/?node1=https://engine-southeastasia-sng-main.onrender.com&node4=https://execution-southeastasia-sng-main.onrender.com
```

Notes:

- Node 1 grants CORS only to configured origins — the Node 1 operator must
  add the dashboard origin to `CORS_ALLOWED_ORIGIN` when calling it
  directly. The same-origin reverse-proxy path needs no CORS change.
- Absolute URLs pointing at loopback hosts (`localhost`, `127.0.0.1`,
  `::1`, …) are **rejected** by `config.js` and reported via the `rejected`
  list.
- `?mock=1` can force the demo indicator; the server also advertises
  `mock`/`demo` via `runtime-config.js`.

## Contract behaviors

- **Freshness**: candle freshness uses Node 1's reported edge lag and the
  newest-bar age; heartbeats mark a live stream stale after 2.5× the
  interval. Disconnected feeds show a `Disconnected` pill and trigger a
  REST polling fallback (Node 1: `/strategy` + `/demo` every 5s, marked
  "delayed").
- **TS-keyed upserts**: candles and strategy history are keyed by
  timestamp, so the replay sent on every (re)connect overwrites, never
  duplicates.
- **Lifecycle rendering**: `accepted`, `filled`, `partial`, `rejected`,
  `unknown`, `cancelled`, `closed` each render distinctly. A fill is
  shown as broker-confirmed only when `filled|partial` **and**
  `filled_volume > 0` (mirrors `Mt5OrderOutcome::is_confirmed_fill`).
  `accepted` and `unknown` are never presented as confirmed fills, and
  `orders_unknown` is surfaced as a distinct counter.
- **Unavailable endpoints**: the deployed engine currently 404s
  `/strategy` and `/demo`; the panels degrade to explicit
  "endpoint unavailable" states instead of failing.

## Relationship to the earlier Node 1-only dashboard

The `Node2` branch carries an earlier, mature Node 1-only dashboard (plain
scripts, `MARKET_HOST` override, its own test suite). It was inspected per the
handoff: it is already read-only by construction (no `POST`/`PUT`/`DELETE`, no
order ticket, no broker credentials) and its `strategy-math` is presentation
math only — no local trading decisions. This implementation supersedes it by
adding the Node 4 execution surface (items 3–4 of the handoff), the lifecycle
rendering rules, the security scan, and the dev/mock server, while keeping the
same read-only posture and demo indicators.

## Security constraints (enforced by tests)

- No tokens/credentials in browser code: `tests/security.test.js` scans
  every shipped file.
- No broker order endpoints in browser code; REST layer is GET-only.
- `/mt5/control` and `/mt5/bridge` are never referenced from the browser.
- No localhost/loopback targets in browser code; the config layer rejects
  loopback overrides.
- The dev server refuses non-GET requests (405) and never forwards
  anything but GETs to upstreams.
- The whole experience is marked **DEMO** (header badge, demo account
  badge, mock engine version).
