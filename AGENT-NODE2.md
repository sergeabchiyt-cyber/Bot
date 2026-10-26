# AGENT BRIEF — Node 2: pure frontend dashboard (Node 3 endpoint + better design)

**Paste this into a separate Arena session whose working branch is `Node2`.**
**Language:** vanilla HTML / CSS / JS, no build step, no framework, no runtime
dependency. **Chart library:** lightweight-charts v4.2.0 (already loaded from
CDN in `index.html`). **Branch head:** `890559d`.
**Master plan:** `PLAN.md` (sections 2, 3, 4, 5-Agent-2, 6, 7).
**This brief is self-contained; you do not need the other branches to start.**

---

## 0. Decisions already made by the owner (do not re-litigate)

| ID | Decision |
|---|---|
| D1 | **Node 3 decides → Node 4 executes.** Node 3 owns all VP maths, the break/retest scanner and ATR SL/TP/RR projection; Node 4 owns risk policy, sizing, demo engine and reports. |
| D2 | **Four disjoint branches** — one service per branch, one Arena session per branch. Never copy another service's `src/` into your branch. |
| D3 | **DST-aware `America/New_York` 18:00** is the week/session anchor. |
| D4 | **CW = current behaviour**: first snapshot after Monday's 18:00 close, refreshed at every daily close, cleared at Sunday 18:00. |
| D5 | **PS = last closed 18:00→18:00 session** (at week open: Thu 18:00 open → Fri 18:00 close), **PoC only**. |
| D6 | **No 10.5K volume-spike filter** — break + retest alone confirm. |
| **D7** | **"Node 2 is frontend only, keep Node 4 out of it, then wire backend endpoints into it."** The `Node2` branch is the ONE AND ONLY dashboard. No server, proxy, mock backend, `serve.mjs`, or any Node 4 source file. Node 4 ships no dashboard at all (its embedded `node2/` is deleted by the Node 4 agent). |
| D8 | **No screenshot needed.** Ray colours/styles follow the existing `js/levels.js` colour map and lightweight-charts `createPriceLine` conventions; verified by geometry unit tests. |
| D9 | **1 pip = $0.01** (SL 200–300 pips = $2–3, TP 600–800 pips = $6–8). |

**Consequence for you: the `Node2` branch contains browser code and browser code
ONLY.** It calls three public service origins directly. No secrets, no tokens,
no POST/PUT/PATCH/DELETE, no order ticket, no strategy/VP maths in the browser.

---

## 1. What is in your branch today (verified inventory)

`index.html` (236 lines), `style.css` (1,575 lines, dark design tokens),
20 flat `js/*.js` modules (`api.js`, `app.js`, `axis.js`, `calendar.js`,
`chart.js`, `config.js`, `demo.js`, `execution.js`, `format.js`, `levels.js`,
`normalize.js`, `overlays.js`, `socket.js`, `status.js`, `strategy.js`,
`strategy-math.js`, `utils.js`), `package.json` (scripts: `lint`, `test` with
`node --test tests/*.test.js`), 10 test files in `tests/`, plus
`docs/EXECUTION_SNAPSHOT.md` and `docs/SERVICE_ARCHITECTURE.md`.

Current wiring: `js/config.js` defines **two** hosts — `MARKET_HOST` (Node 1)
and `NODE4_HOST` (Node 4) — with a `window.XAUUSD_CONFIG` override and strict
origin validation. It subscribes to Node 1 topics `levels`, `candle`,
`tick_volume`, `strategy`, `demo`, `calendar`, `status` and to Node 4's
`diagnostics` topic. It renders Node 1 `/strategy` and `/demo` panels — **both
endpoints are being deleted**. It has **no Node 3 host at all**.

`js/chart.js` uses lightweight-charts v4.2.0 with a compacted display timeline
(`js/axis.js`) and a fixed 15M view. `js/levels.js` draws PW/PS/CW/SWING price
lines from a colour map; `js/overlays.js` draws entry/SL/TP lines, an ATR band
and buy/sell markers.

---

## 2. The three backend endpoints to wire in

Keep the existing `window.XAUUSD_CONFIG` override pattern and its strict origin
validation (http(s)/ws(s) only; reject credentials, query strings, fragments,
custom paths and loopback hosts). Apply it to **three** hosts:

```
MARKET_HOST    = Node 1  (market data + calendar)      -> C1 v2
STRATEGY_HOST  = Node 3  (VP, drawings, scanner,        -> C2
                          signals, strategy diagnostics)
EXECUTION_HOST = Node 4  (execution + risk diagnostics) -> C4
```

Each host yields its own REST base and its own WS base. **Three independent
sockets** with their own heartbeat (30 s), silence watchdog (75 s), bounded
reconnect backoff (1 s → 30 s), visibility recovery, and a 5 s polling fallback
with a `DELAYED` badge. One service being down must never disturb the others.

All three services must whitelist the deployed dashboard origin in CORS (and
their WebSocket origin checks must accept it). **No proxy is introduced** — the
browser calls the services directly.

### Node 1 (C1 v2) — keep
`GET /candles?interval=1m|15m|1h`, `/tick-volume`, `/calendar`, `/status`;
`WS /ws` topics `candle`, `tick_volume`, `calendar`, `status`, `heartbeat`.
`status` carries `contract_version`, `boot_id`, `freshness`, `market_open`.

### Node 3 (C2) — NEW, this is the main addition
| Resource | Purpose |
|---|---|
| `GET /levels` | array of `VpLevels` (`window`, `poc`, `vah`, `val`, `start`, `end`, `timestamp`, `sunday_open`, `meta`) |
| `GET /vp?window=PW\|PS\|CW` and `/vp?start=&end=` | full FRVP histogram audit (rows, poc/vah/val row indices, input interval/bars, total volume) |
| `GET /drawings` | **1H FRVP rectangles + daily horizontal rays + Sunday-open marker** — each drawing has `id`, `label`, `price`, `color`, `line_style`, `ray_from_time`, `start`, `end`, `timeframe: "1H"\|"daily"`, `histogram` rows |
| `GET /scanning` | per-level break/retest state, distance, projected SL/TP/RR, ATR |
| `GET /signals` | pending/recent `TradeIntent`s + Node 4 reports (no credentials) |
| `GET /diagnostics`, `GET /health` | strategy health |
| `WS /ws` | topics `levels`, `drawing`, `vp`, `scanning`, `signals`, `diagnostics` |

### Node 4 (C4) — keep + new `risk` block
`GET /diagnostics` (adds `risk`: limits, current exposure, daily PnL, drawdown,
rejection reasons), `/open-trades`, `/deriv` (`/account`),
`/mt5/account`, `/mt5/positions`, `/mt5/history`, `/mt5/status`;
`WS /ws` topic `diagnostics`. Field reference: Node 4's
`docs/mt5/FRONTEND_RESOURCES.md` (the Node 4 agent is updating it).

**Never call:** Node 3's private `/execution`, Node 4's `POST /mt5/control`,
Node 1's `/strategy` or `/demo`, or any route with a secret.

---

## 3. Work items

1. **Pure frontend, no Node 4 code (D7).** Keep the `Node2` branch tree exactly
   as it stands — `index.html`, flat `js/*.js`, `style.css`, `tests/`. Do NOT
   copy `Node-4/node2/`: no `public/src/` ES-module rewrite, no `serve.mjs`, no
   `mock/` server, no `package.json` with a `ws` devDependency, no proxy. Update
   `README.md` and `docs/SERVICE_ARCHITECTURE.md` to document **three** public
   read-only dependencies and the "no server files in this branch" rule.
2. **Wire the three hosts in.** `js/config.js`: `MARKET_HOST`, `STRATEGY_HOST`,
   `EXECUTION_HOST`; derive REST + WS bases for each; apply the existing strict
   origin validation to all three; add a third independent socket. Update
   `js/api.js`, `js/socket.js`, `js/app.js` frame routing and the
   `DELAYED`/polling logic to cover three streams.
3. **New Node 3 client** (`js/api-node3.js`, matching the flat `js/` layout):
   on mount, seed `GET /levels`, `/vp?window=PW`, `/drawings`, `/scanning`,
   `/signals`, `/diagnostics` in parallel — GET only, `cache: "no-store"`,
   per-endpoint error tolerance (a 404/unavailable shows a calm "unavailable"
   state, never a broken page). Then stay live over the Node 3 WS topics.
   **Remove** all Node 1 `/strategy`, `/demo` calls and their WS topics, and
   stop polling Node 1 `/levels` (levels now come from Node 3).
4. **Chart & drawings — the important part.**
   - Two synced views: **1H chart** (FRVP rectangles for PW/PS/CW with exact
     `start`/`end` times, histogram overlay, a Sunday-open vertical marker at
     Sunday 18:00 NY) and **daily chart** with **one horizontal ray per level**
     (PW PoC/VaH/VaL, PS PoC, CW PoC/VaH/VaL, Sunday open) styled from the
     `/drawings` payload. Add a `1H | D` toggle; daily rays persist when
     switching. The 1H view is where the FRVP is drawn for time accuracy; the
     daily view is where the rays are marked — exactly the manual workflow.
   - Break/retest markers from `/signals` on the 1H chart (arrowUp/arrowDown at
     `source_candle_time`), entry/SL/TP price lines from Node 4
     `/open-trades`, optional ATR band.
   - **CW is optional until the first daily close (Monday 18:00 NY, D4=C)**:
     keep the existing stable "CW pending" placeholder in the levels legend and
     table; never assume a CW object exists. Handle a week reset that drops CW.
   - Level identity is `(window, kind, start, end)`; replace a ray when the
     identity changes rather than stacking stale rays.
   - Reuse `js/levels.js` colour map (gold/teal/coral/blue/violet/pink/cyan)
     and `js/overlays.js` line helpers. **Never compute VP maths in the
     browser** — render only what Node 3 supplies.
   - **No reference screenshot (D8):** assert ray/rectangle geometry in unit
     tests; styling follows the existing colour map + lightweight-charts
     `createPriceLine` conventions.
5. **Better design.** Build a real design system on top of the existing tokens:
   colour/spacing/type-scale/radii/elevation tokens; 3/2/1-column responsive
   layout with the chart dominant; sticky top bar with last price + per-service
   health pills; a week/session timeline strip (PW / PS / CW windows +
   Sunday-open marker + countdown to next 18:00 close); a scanner table
   (level, state, distance in $/pips, projected SL/TP/RR, volume status); a
   signals tape; a risk card (SL/TP/RR bands, ATR, exposure, daily PnL,
   drawdown, rejection reasons from Node 4's `risk` block); an execution card
   (venue, DRY RUN banner, open trades, account); consistent empty/loading/
   stale skeletons; accessible contrast + keyboard focus + ARIA; no layout
   shift on live updates; mobile-first below 720px.
6. **Dead panels out:** Node 1 strategy-evaluator and demo-account panels,
   order-flow/bubbles references, the "four order-flow colours" legend,
   `js/strategy.js` / `js/demo.js` / `js/strategy-math.js` if no longer used.
7. **Tests.** Keep the existing `node --test` suites in `tests/`. Add
   `node3-endpoints.test.js`, `drawings.test.js` (ray geometry, 1H→daily
   projection, missing CW, Sunday-open marker, level identity replacement),
   `three-host-config.test.js`. Extend `security.test.js` to assert **no POST,
   no token, no `/execution`, no `/mt5/control`** and **no server/proxy/
   Node-4-origin file** anywhere in the branch. Use plain JSON fixtures in
   `tests/fixtures/` (recorded from the shared `contracts/` golden fixtures)
   instead of a mock server. Run `npm test` and `npm run lint`.

---

## 4. Acceptance criteria

- Three hosts configurable via `window.XAUUSD_CONFIG`; zero network calls to
  Node 1 `/strategy` or `/demo`; zero references to `/execution` or any control
  route in browser code.
- 1H FRVP rectangles + Sunday-open marker + daily horizontal rays for all
  PW/PS/CW levels are drawn from Node 3's `/drawings`, verified by geometry
  unit tests and a quick browser render (no screenshot needed, D8).
- CW pending state is stable and never throws when the CW object is absent.
- All panels degrade gracefully when any one of the three services is down.
- `npm test` + `npm run lint` green; no secret/token in browser code;
  **the branch contains no backend/server files at all**.

**Depends on:** C2 (Agent 3) and C4 (Agent 4) schemas — but you can develop
fully against local `tests/fixtures/` JSON in parallel.
**Must not:** add a server, a proxy, a token, a write method, or any VP/strategy
computation.
