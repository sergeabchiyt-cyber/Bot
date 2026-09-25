# XAUUSD OrderFlow Engine

Rust backend for a gold (XAUUSD) order-flow desk: it ingests live trades from
multiple venues, detects order-flow events (bubbles / absorption), computes
rolling volume-profile levels, pulls the economic calendar straight from the
ForexFactory weekly feed (browser MCP optional), and exposes everything over a JSON HTTP API and a WebSocket
stream. The engine ships no frontend and serves no static files — point any
external client (dashboard, charting app, script) at the endpoints below.

## Run

```bash
cargo run            # or: docker build -t engine . && docker run -p 10000:10000 engine
```

Then hit **http://localhost:3000/status** for a JSON snapshot of every feed.
There is no `/` route — the engine is API-only.

## HTTP endpoints

| Route      | What                                                |
|------------|-----------------------------------------------------|
| `/health`  | `ok`                                                |
| `/status`  | JSON snapshot of every feed's liveness              |
| `/levels`  | Current PW/PS/CW PoC/VaH/VaL levels (+ window `start`/`end`) |
| `/candles` | SiftingIO 15m candles used by the chart and VP (latest fixed seed + live closes) |
| `/calendar`| Latest economic calendar snapshot (`source`, `count`, `events`) |
| `/ws`      | WebSocket stream — send `{"type":"subscribe","topics":["candle","levels","bubbles","trades","calendar","status"]}` |

WebSocket frames are tagged with `type`: `candle`, `levels`, `bubbles`,
`trades`, `calendar`, `status`, `heartbeat`. The four order-flow colors:
`BUY_BUBBLE` green, `SELL_BUBBLE` red, `ABS_BUY` blue, `ABS_SELL` orange.

### CORS (browser clients)

The dashboard (Node2, `https://static-dash-frontend.onrender.com`) is a static
site on its own origin, so it reads these routes cross-origin. `CorsLayer` is
applied to the whole router and allows **exactly one origin**, `CORS_ALLOWED_ORIGIN`:

* `GET /health|/levels|/candles|/calendar|/status` from that origin get
  `access-control-allow-origin: <origin>`, plus `vary: origin, ...` so a shared
  cache can never hand our grant to a different requester.
* Any other `Origin` (and requests with no `Origin`, e.g. curl or another
  server) gets the payload with **no** CORS grant, so no other page can read it.
* Preflight is limited to `GET, OPTIONS` and `content-type, accept, origin`,
  cached for 600s. No wildcard, no `Any`, and no `allow_credentials` — nothing
  here is cookie/token authenticated, so no key ever rides along with a request
  or a response.
* `/ws` is not subject to CORS; the layer passes the upgrade through untouched.

Set `CORS_ALLOWED_ORIGIN` in Render when the site moves — the value is compared
byte-for-byte with the browser's `Origin` header, so it must have no trailing
slash (a stray one is stripped at startup). If the dashboard is ever served
through a same-origin reverse proxy on the engine's own domain, no grant is
needed: point the frontend at relative paths (`/candles`,
`${location.protocol === "https:" ? "wss:" : "ws:"}//${location.host}/ws`) and
the layer simply never matches.

## Data sources

| Feed    | What it provides | Needs |
|---------|------------------|-------|
| `binance` | XAUUSDT Futures aggTrade (order flow only) | — |
| `bybit`   | XAUUSDT linear trades (order flow) | — |
| `okx`     | XAU-USDT-SWAP trades (order flow) | — |
| `bitget`  | XAUTUSDT trades (order flow) | — |
| `gate`    | XAUT_USDT futures trades (order flow) | — |
| `kraken`  | PF_XAUTUSD trade feed (order flow) | — |
| `alltick` | Spot XAUUSD ticks | `ALLTICK_TOKEN` |
| `itick`   | Spot XAUUSD ticks | `ITICK_TOKEN` |
| `sifting` | Spot XAUUSD REST history + live chart candles | `SIFTING_API_KEY` |

Every feed auto-reconnects with status broadcasting; feeds without taker-side
data count toward volume but never toward delta.

## Environment variables (all optional)

```
PORT=3000
BINANCE_SYMBOL=XAUUSDT
SIFTING_API_KEY=            SIFTING_SYMBOL=XAUUSD
SIFTING_WS_URL=             SIFTING_HIST_URL=https://api.sifting.io
# Sifting REST VP seed is always exactly 2,000 15m candles; no Binance REST fallback.
FEED_BINANCE=on|off|auto    FEED_BYBIT=on|off|auto      FEED_OKX=on|off|auto
FEED_BITGET=on|off|auto     FEED_GATE=on|off|auto       FEED_KRAKEN=on|off|auto
FEED_ALLTICK=on|off|auto    FEED_ITICK=on|off|auto      FEED_SIFTING=on|off|auto
BITGET_SYMBOL=XAUTUSDT      GATE_SYMBOL=XAUT_USDT       KRAKEN_PRODUCT=PF_XAUTUSD
ALLTICK_TOKEN=              ALLTICK_WS_URL=             ALLTICK_CODE=XAUUSD
ITICK_TOKEN=                ITICK_WS_URL=               ITICK_SYMBOL=XAUUSD
MCP_BROWSER_URL=http://localhost:3001     MCP_BROWSER_TOKEN=
MCP_BROWSER_TOOL_NAVIGATE=  MCP_BROWSER_TOOL_READ=      MCP_SCRAPE_SECS=900
CALENDAR_URL=               CALENDAR_USE_MCP=true
LEVEL_PROXIMITY_PIPS=5      SL_MIN_PIPS=10  SL_MAX_PIPS=50  TP_MIN_PIPS=15  TP_MAX_PIPS=100
CORS_ALLOWED_ORIGIN=https://static-dash-frontend.onrender.com   # only origin allowed to call the REST API from a browser
RR_MIN=1 RR_MAX=3           DERIV_DEMO_API=  DERIV_APP_ID=  DERIV_API_URL=
MCP_CHELSEA_URL=            (set to route orders through the ChelseaAI MCP tool)
```

`auto` (default) = on for keyless public feeds, on for tokened feeds only when
the token exists.

## Browser MCP

The engine speaks the MCP Streamable-HTTP transport (`initialize` →
`notifications/initialized` → `tools/list` → `tools/call`, `Mcp-Session-Id`
echo, JSON **and** SSE responses). Point `MCP_BROWSER_URL` at any browser MCP
server, e.g. Playwright MCP in HTTP mode:

```bash
npx @playwright/mcp@latest --transport streamable-http --port 3001
```

Tool names are discovered via `tools/list` (`browser_navigate` +
`browser_snapshot` for Playwright MCP); override with
`MCP_BROWSER_TOOL_NAVIGATE` / `MCP_BROWSER_TOOL_READ` if your server differs.

## Volume profile windows

| Window | Period | Refresh |
|--------|--------|---------|
| `PW` | Previous trading week (Sun 18:00 NY open → Fri 17:00 NY close), held for the whole current week | on week rollover |
| `PS` | **Last closed session** (17:00 NY → 17:00 NY) | **at every session close** |
| `CW` | Current week so far | on every closed candle |
| `SWING_BULL` / `SWING_BEAR` | Most recent confirmed directional leg | on every closed candle |

The VP seed is one SiftingIO REST request for **exactly 2,000** latest 15m
candles. A short or invalid page is rejected rather than padded or replaced
with Binance prices. SiftingIO's live WebSocket supplies the current chart
candle and completed candle updates; Binance remains isolated to the aggTrade
order-flow analyzer.

`PS` is not a boot-time constant. A 30-second ticker compares the current
17:00 America/New_York session boundary against the session `PS` currently
describes; the moment the boundary moves, the profile is recomputed and fresh
`levels` frames are broadcast. The boundary is re-anchored in local time, so it
stays at 17:00 across DST changes, and the weekend hole (Fri 17:00 → Sun 18:00)
is skipped by walking back up to five sessions for one that actually has data.

The swing profile detects confirmed alternating pivots. A high followed by a
low creates a bearish profile anchored from the best high to the best low; a low
followed by a high creates a bullish profile anchored from the best low to the
best high. Candle volume is allocated to bins by actual high/low overlap, then
POC and the 70% VA are expanded from the POC. Each `VpLevels` payload carries
`start` / `end` (epoch ms), `direction`, and swing anchor prices so clients can
audit exactly which move produced the levels.

## Economic calendar

The calendar reads ForexFactory's own weekly export over plain HTTPS — no
browser, no API key:

```
https://nfs.faireconomy.media/ff_calendar_thisweek.json
```

Events are normalized to `{event, currency, impact, time (UTC RFC3339),
timestamp, actual, forecast, previous, gold_relevant}`, sorted chronologically,
with `gold_relevant` flagging high-impact USD prints. If the feed is rate
limited, the engine falls back to the browser MCP server (`CALENDAR_USE_MCP`),
and it retries on the `MCP_SCRAPE_SECS` interval. The latest snapshot is cached,
served at `/calendar`, and replayed to WebSocket clients that subscribe to the
`calendar` topic.

## Verifying

```bash
cargo test                         # session-rollover, calendar parsing, CORS policy
cargo test -- --ignored --nocapture  # hits the live ForexFactory feed
bash ci/smoke.sh                   # boots the binary and asserts:
                                   #   /levels window bounds, /calendar contents,
                                   #   /candles payload shape, the CORS allow-list
                                   #   (allowed / preflight / foreign origin) and
                                   #   that /ws still upgrades and replays frames
```

CI runs all three on every push. The CORS probes in `ci/smoke.sh` are the
authoritative proof of the allow-list, since they run against the real release
binary; `ci/verify_ws.py` does the WebSocket handshake on a raw socket so no
websocket client library is needed.
