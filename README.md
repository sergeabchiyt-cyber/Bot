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
| `/candles` | SiftingIO 15m candles used by the chart/swing profile (latest fixed seed + live closes); `volume` = tick count |
| `/tick-volume` | Live tick-volume bars built since boot (up/down/flat split, tick rate); newest may be in progress |
| `/calendar`| Latest economic calendar snapshot (`source`, `count`, `events`) |
| `/ai`      | What the econ news delivered today: Node3's `transcript`/`sentiment` frames (+ last health), filtered to the current UTC day |
| `/ws`      | WebSocket stream — send `{"type":"subscribe","topics":["candle","tick_volume","levels","bubbles","trades","calendar","status","audio_chunk","learn","transcript","sentiment","health","prediction"]}` |

WebSocket frames are tagged with `type`: `candle`, `tick_volume`, `levels`,
`bubbles`, `trades`, `calendar`, `status`, `heartbeat`, plus the Node3 AI
contract frames `audio_chunk`, `learn`, `transcript`, `sentiment`, `health`,
`prediction` (see "Node 1 ↔ Node 3" below). The four order-flow colors:
`BUY_BUBBLE` green, `SELL_BUBBLE` red, `ABS_BUY` blue, `ABS_SELL` orange.

Subscribing to `levels` replays the cached profiles. Subscribing to `candle`
replays the latest 15 cached 15-minute OHLC bars (enough for Node3 to seed
ATR(14)); `/candles` remains the full chart-history endpoint. Subscriptions to
`candle`, `levels`, and `trades` are supported together. The `trades` topic is
bidirectional: a valid Node3 `TradeEvent` published to `/ws` is fanned out
unchanged to every connected client subscribed to `trades`.

### Tick volume

Tick volume is the number of price updates in a bucket. SiftingIO's historical
bars use that unit for `v`, so the live aggregator counts ticks too, and
`candle.volume` means the same thing across the 2,000-bar seed and the live
edge. Every accepted SiftingIO tick sends a `candle` frame and a `tick_volume`
frame for the same 15m bucket:

```json
{"type":"tick_volume","data":{
  "time":1758873600000, "ticks":412, "up_ticks":150, "down_ticks":140, "flat_ticks":122,
  "close":3383.75, "last_tick":1758874499000, "ticks_per_sec":0.4,
  "closed":false, "source":"sifting"}}
```

* `ticks` always equals the candle's `volume`, and `time` its `time`, so a
  client can draw a volume histogram from `/candles` history and keep it live
  from either frame.
* `up_ticks` / `down_ticks` / `flat_ticks` use the tick rule (price above,
  below, or equal to the previous tick). Spot XAUUSD has no taker side, so this
  shows activity and pressure, not order flow, and it never feeds delta.
* `ticks_per_sec` is a rolling 10 s rate on stream time.
* When a bucket rolls, one final frame with `closed: true` is sent for it
  before the next bucket's first frame.
* The stream state survives reconnects. The cached tick Sifting replays on
  every re-subscribe isn't counted twice, and late ticks for an already closed
  bucket are dropped.
* Subscribing to `tick_volume` replays the live bars built since boot, the
  same list as `GET /tick-volume`.

### CORS (browser clients)

The dashboard (Node2, `https://static-dash-frontend.onrender.com`) is a static
site on its own origin, so it reads these routes cross-origin. `CorsLayer` is
applied to the whole router and allows **exactly one origin**, `CORS_ALLOWED_ORIGIN`:

* `GET /health|/levels|/candles|/tick-volume|/calendar|/status` from that origin get
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
# Sifting REST chart/swing seed is exactly 2,000 15m candles; no Binance price fallback.
FEED_BINANCE=on|off|auto    FEED_BYBIT=on|off|auto      FEED_OKX=on|off|auto
FEED_BITGET=on|off|auto     FEED_GATE=on|off|auto       FEED_KRAKEN=on|off|auto
FEED_ALLTICK=on|off|auto    FEED_ITICK=on|off|auto      FEED_SIFTING=on|off|auto
BITGET_SYMBOL=XAUTUSDT      GATE_SYMBOL=XAUT_USDT       KRAKEN_PRODUCT=PF_XAUTUSD
ALLTICK_TOKEN=              ALLTICK_WS_URL=             ALLTICK_CODE=XAUUSD
ITICK_TOKEN=                ITICK_WS_URL=               ITICK_SYMBOL=XAUUSD
MCP_BROWSER_URL=http://localhost:3001     MCP_BROWSER_TOKEN=
MCP_BROWSER_TOOL_NAVIGATE=  MCP_BROWSER_TOOL_READ=      MCP_SCRAPE_SECS=900
CALENDAR_URL=               CALENDAR_USE_MCP=true
# Econ-news audio monitor: streams live event coverage to Node3 as audio_chunk frames
ECON_MONITOR=true           ECON_SCAN_SECS=60
ECON_WINDOW_BEFORE_SECS=900 ECON_WINDOW_AFTER_SECS=3600
ECON_MIN_IMPACT=High        ECON_CURRENCIES=USD
ECON_STREAM_SOURCES=        # comma list of live pages/media (e.g. https://www.youtube.com/@federalreserve/live)
ECON_CHUNK_MS=1000          ECON_USE_MCP=true
ECON_MAX_STREAM_SECS=7200   ECON_YTDLP=yt-dlp   ECON_FFMPEG=ffmpeg
ECON_TEST_TONE=1            # CI/offline plumbing test: synthetic 16kHz tone instead of live audio
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
| `PW` | Previous trading week (Sun 18:00 NY open → Fri 18:00 NY close), held for the whole current week | on week rollover |
| `PS` | **Last closed session** (18:00 NY → 18:00 NY) | **at every session close** |
| `CW` | Current week (from the week open after Friday's 18:00 close) through the last completed 18:00 NY daily session | at each daily close; first snapshot when Monday closes; reset at the new week boundary |
| `SWING_BULL` / `SWING_BEAR` | Most recent confirmed directional leg | on every closed candle |

The chart and swing seed is one SiftingIO REST request for **exactly 2,000**
latest 15m candles; a short or invalid page is rejected rather than padded or
replaced with Binance prices. PW/PS/CW use paginated **1m** SiftingIO bars from
the PW start through the most recently completed minute, so weekly profiles
don't smear each 15m candle's entire volume across its full wick range. Live
Sifting ticks are folded into completed 1m profile bars and 15m chart/swing
bars. If the 1m history does not cover the PW window, the engine logs a warning
and falls back to the 15m profile input; Binance remains isolated to the
aggTrade order-flow analyzer.

`PS` is not a boot-time constant. A 30-second ticker compares the current
18:00 America/New_York session boundary against the last close already seen;
the moment the boundary moves, the profile is recomputed and fresh `levels`
frames are broadcast. The boundary is re-anchored in local time, so it
stays at 18:00 across DST changes, and the weekend hole (Fri 18:00 → Sun 18:00)
is skipped by walking back up to five sessions for one that actually has data.

`CW` is a completed-day snapshot, not an intraday rolling profile. It starts
accumulating after Friday's 18:00 close and its first levels are drawn once
Monday's session closes. While a day is open, new 15m candles do not change or
rebroadcast CW. At the next 18:00 America/New_York close, the newly completed
session is added and CW is emitted once with the new POC/VAH/VAL. At the
Sunday 18:00 week boundary, the prior CW is cleared and the new week starts
empty until its first daily close.

Only one swing profile exists at a time. It detects confirmed alternating
3-bar fractal pivots: the most recent high followed by a low creates a bearish
profile anchored from the best high to the best low; the most recent low
followed by a high creates a bullish profile anchored from the best low to the
best high. Equal highs/lows still count, anchoring at their most recent touch,
and a short or trend-straight history falls back to a leg bounded to the most
recent session's bars rather than spanning stale history. Time-window profiles
use 1m candle volume allocated to fixed, price-aligned $0.50 rows by each
candle's actual high/low overlap, then POC and the 70% VA are expanded from the
POC. Each `VpLevels` payload carries `start` / `end` (epoch ms), `direction`,
and swing anchor prices so clients can audit exactly which move produced the
levels. TradingView parity still requires matching its symbol/feed, session
anchors, row size, and value-area settings; lower-timeframe bars reduce the
15m approximation error but do not turn OHLC bars into true tick-at-price data.

The implementation is split by window for accuracy: `volume_profile/session.rs`
(18:00 → 18:00 sessions), `volume_profile/weekly.rs` (week anchors),
`volume_profile/swing.rs` (the directional leg), and
`volume_profile/histogram.rs` (the shared POC/VA math), with
`volume_profile.rs` orchestrating ingest, refresh, and fan-out.

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

## Node 1 ↔ Node 3

### Market state and trade fan-out

Node3 (`node3-ai/ws_client.py`) subscribes to `candle`, `levels`, and
`trades` in addition to the audio/learner topics. On each subscribe (including
reconnect), Node1 first replays its cached level frames and last 15 candles.
Node3 stores `PW` / `PS` / `CW` levels and seeds Wilder ATR(14) from those 15
bars; duplicate updates for the current 15-minute candle replace that bar.
`PW.sunday_open` carries the open price of the exact Sunday 18:00 New York
start candle when that candle exists in the history (`PW.start` remains the
boundary timestamp).

When Node3 emits an execution or trade signal, publish the standard event on
the same connection:

```json
{"type":"trades","data":{"trade_id":"n3-1","symbol":"XAUUSD","side":"buy","size":0.01,"entry":2340.0,"sl":2335.0,"tp":2350.0,"status":"signal","timestamp":1700000000000}}
```

Node1 rebroadcasts this unchanged to every `/ws` client subscribed to
`trades`, including Node2 dashboards. Node3's helper is
`Node3Client.send_trade_event(...)`; this only broadcasts the event and does
not itself place an order.

For lowest latency in a Singapore deployment, set Node3's `NODE1_WS_URL` to
Node1's direct secure WebSocket endpoint (`wss://<node1-sg-host>/ws`), using
provider-private networking/internal DNS when available. The existing
`engine-southeastasia-sng-main.onrender.com` WSS address is the public fallback;
keep Node3 and Node1 in the same region and avoid routing execution traffic via
Node2 or another proxy. Network RTT depends on the hosting provider, so the
sub-millisecond target must be verified from the deployed services.

### Econ news audio

The engine also turns economic events into live audio, streams it to Node3 over
the same `/ws` connection Node3 already holds, and reports what the news delivered.

```
calendar event window opens (NFP, CPI, FOMC, Powell, ...)
   │
   ├─ ECON_STREAM_SOURCES watchlist (channel /live pages, direct media URLs)
   ├─ MCP browser (the ~31-tool server) browses the event's live coverage
   │
   ▼
yt-dlp resolves the stream ──► ffmpeg decodes ──► 16kHz mono f32le PCM
   │
   ▼
/ws topic `audio_chunk`  ──►  Node3 ws_client.py  (also candle/levels/trades)
                                 ├─ Moonshine → transcript
                                 └─ FinBERT / FOMC-RoBERTa → sentiment
   ▼                                        │
GET /ai  ◄── engine caches those frames ────┘   (today's UTC day)
```

**Wire contract** (exactly what `node3-ai/ws_client.py` consumes):

- `{"type":"audio_chunk","data":{"data":"<b64 f32le 16kHz mono>","ts":…,"source":…,"event":…,"sample_rate":16000,"format":"f32le","duration_ms":1000}}`
  — one PCM chunk per `ECON_CHUNK_MS` of captured audio
- `{"type":"learn","data":{"features":[…],"target":0|1}}` — forwarded to Node3's
  online learner; any WS client or script may publish `learn`/`audio_chunk`
  frames to `/ws` and the engine rebroadcasts them on the bus
- Node3's replies (`transcript`, `sentiment`, `health`, `prediction`) are
  fanned out on their topics and cached for `GET /ai`
- `sentiment_req` / `predict_req` requests are forwarded to Node3 regardless
  of topic subscription; Node3 subscribes to `audio_chunk`, `learn`, `candle`,
  `levels`, and `trades`

The monitor arms on calendar events matching `ECON_MIN_IMPACT` (default
`High`) and `ECON_CURRENCIES` (default `USD`) inside the window
`[time − ECON_WINDOW_BEFORE_SECS, time + ECON_WINDOW_AFTER_SECS]` (default
15 min before → 1 h after). Fed-type events (FOMC, Powell, press conferences)
automatically add the official Federal Reserve live channel as a candidate
source. Missing yt-dlp/ffmpeg degrade gracefully (direct media URLs in
`ECON_STREAM_SOURCES` need neither extractor), and `ECON_TEST_TONE=1` streams
a synthetic 16kHz tone instead — CI uses that (`ci/verify_econ_audio.py`) to
prove the whole pipeline offline.

## Verifying

```bash
cargo test                         # session-rollover, calendar parsing, CORS policy, AI cache
cargo test -- --ignored --nocapture  # hits the live ForexFactory feed
bash ci/smoke.sh                   # boots the binary and asserts:
                                   #   /levels window bounds, /calendar contents,
                                   #   /candles + /tick-volume payload shape, the CORS allow-list,
                                   #   the econ audio pipeline (audio_chunk/learn on /ws, /ai digest)
                                   #   (allowed / preflight / foreign origin) and
                                   #   that /ws replays levels/15 candles and fans out Node3 trades
```

CI runs all three on every push. The CORS probes in `ci/smoke.sh` are the
authoritative proof of the allow-list, since they run against the real release
binary; `ci/verify_ws.py` does the WebSocket handshake on a raw socket so no
websocket client library is needed.

---

# XAUUSD Node 3 — AI Speech & Sentiment Services (UPGRADED 16GB tier)

This repository branch (`Node3`) hosts the Python AI services for the XAUUSD trading system. Originally tuned for Lightning AI Always-On, it now ships an **upgraded tier for 16GB RAM / 4 vCPU / 400GB disk** (better accuracy → better trades).

- **Analysis & upgraded specs:** see [`node3-ai/README.md`](node3-ai/README.md) (repo walk-through + model tables + tier comparison)
- **Copy-paste launch:** see [`node3-ai/LAUNCH.md`](node3-ai/LAUNCH.md) (systemd / Docker / bare, 30-sec to boot)
- **Upgrades in this branch:** `FOMC-RoBERTa 355M` (93% FOMC), `Moonshine Medium 245M + Silero VAD`, `Adam+Welford learner v2`

## Quick Start (legacy small — still works)

```bash
cd node3-ai
pip install --no-cache-dir -r requirements.txt
python3 download_models.py --tier small
python3 -u ws_client.py
```

## Upgraded for 16GB (recommended — your hardware)

```bash
cd node3-ai
pip install --no-cache-dir -r requirements.txt
python3 download_models.py --tier upgraded   # FOMC-RoBERTa + VAD + medium optional
cp .env.example .env
python3 -u ws_client.py                      # ~1.8GB RSS, FOMC-RoBERTa tier
# or: python3 -m uvicorn main:app --host 0.0.0.0 --port 8000  # HTTP health/test
```
