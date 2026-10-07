# XAUUSD Market Data & Volume Profile Engine — **Node 1**

Node 1 is the **market-data and volume-profile service** for the XAUUSD desk.

It ingests live trades from multiple venues, counts tick volume, detects
order-flow events (bubbles / absorption), computes rolling volume-profile
levels (PW / PS / CW / swing), pulls the economic calendar straight from the
ForexFactory weekly feed (browser MCP optional), captures economic-news audio,
and exposes all of it over a JSON HTTP API and a WebSocket stream.

**Node 1 does not trade.** It holds no broker credential, has no execution
venue, no order path, and no MT5 bridge. It cannot place, modify or close an
order. There is no fallback or hidden execution path — the capability is not
present in this branch at all.

The engine ships no frontend and serves no static files — point any external
client (dashboard, charting app, script, strategy service) at the endpoints
below.

---

## Service boundary

| Node | What it is | Where it lives |
|------|------------|----------------|
| **Node 1** (this branch) | Market data, tick volume, exchange order flow, volume profile (PW/PS/CW/swing), economic calendar, econ-news audio capture. **Read-only market service.** | `Node1` branch |
| **Node 2** | Read-only browser dashboard (static site). Consumes Node 1's REST API and market WS topics. | `Node2` branch |
| **Node 3** | External **strategy** service. Subscribes to Node 1's `candle` and `levels`, runs the AI/sentiment models, decides *whether* to trade. | `Node3` branch |
| **Node 4** | External **execution / MT5** service. Owns broker credentials, the MT5 bridge and the order path. | `Node-4` branch |

Node 1 never depends on Node 3 or Node 4 being available. If either is down,
Node 1 keeps ingesting feeds, computing profiles and serving market data.

```
                       ┌──────────────────────────────────────────┐
  upstream feeds  ───► │                NODE 1                    │
  SiftingIO            │  market data · tick volume · order flow  │
  Binance / Bybit      │  volume profile (PW/PS/CW/swing)         │
  OKX / Bitget / Gate  │  economic calendar · econ-news audio     │
  Kraken / AllTick     │                                          │
  iTick                │  no broker credential · no order path    │
                       └───┬──────────────┬──────────────┬────────┘
                           │              │              │
              market WS    │              │ REST         │ audio_chunk
              candle       │              │ /health      │ learn
              levels       │              │ /status      │
              tick_volume  │              │ /levels /vp  │
              bubbles      │              │ /candles     │
              calendar     │              │ /tick-volume │
              status       │              │ /calendar    │
                           ▼              ▼              ▼
                     ┌──────────┐   ┌──────────┐   ┌──────────┐
                     │  NODE 3  │   │  NODE 2  │   │  NODE 3  │
                     │ strategy │   │ read-only│   │ AI / NLP │
                     │ (external)│  │ dashboard│   │(external)│
                     └────┬─────┘   └──────────┘   └────┬─────┘
                          │  decides to trade           │ transcript
                          │                             │ sentiment
                          ▼                             ▼
                     ┌──────────┐                  (back to Node 1
                     │  NODE 4  │                   GET /ai cache)
                     │execution │
                     │ MT5 bridge│
                     │ broker    │
                     └──────────┘

  Node 1 ──► Node 3 : market data only (candle, levels, audio_chunk, learn)
  Node 3 ──► Node 1 : AI results only (transcript, sentiment, health,
                      prediction) — cached for GET /ai
  Node 3 ──► Node 4 : orders (Node 1 is not on this path)
  Node 1 ⇸  Node 4 : NO dependency, in either direction
```

### What Node 1 owns — and what it does not

| Node 1 owns | Node 1 does **not** own |
|---|---|
| Upstream market / feed adapters | Strategy or signal decisions |
| Sifting candles and tick volume | Broker execution or fills |
| Exchange order flow (bubbles) | Account / balance monitoring |
| Volume-profile calculations | Broker credentials of any kind |
| PW / PS / CW / swing levels | MT5 bridge or terminal link |
| Economic calendar + econ-news audio | Execution venues or venue selection |
| Market REST endpoints | Stake / lot sizing, SL / TP / R:R |
| Market WebSocket topics | Execution controls (halt, kill switch, …) |
| Market diagnostics and probes | Order routing, reconciliation |

---

## Run

```bash
cargo run            # or: docker build -t engine . && docker run -p 10000:10000 engine
```

Then hit **http://localhost:10000/status** for a JSON snapshot of every feed.
There is no `/` route — the engine is API-only.

Node 1 starts with **no broker or execution environment variable set**. Every
configuration value below is optional; the only credential it ever reads is
the SiftingIO market-data API key (and the optional AllTick / iTick market-data
tokens).

## HTTP endpoints

| Route      | What                                                |
|------------|-----------------------------------------------------|
| `/health`  | `ok`                                                |
| `/status`  | JSON snapshot of every feed's liveness, plus the candle contract (`interval`, `bars`, `first_bar`, `last_bar`, `edge_lag_minutes`) and the active VP model |
| `/levels`  | Current PW/PS/CW PoC/VaH/VaL levels (+ window `start`/`end`, row/input audit in `meta`) |
| `/vp`      | Full volume-profile histogram: `?window=PW\|PS\|CW\|SWING_*`, `?rows=128` to test a Row Size, `?start=<ms>&end=<ms>` for a hand-selected (Fixed Range) window — the TradingView audit trail |
| `/candles` | SiftingIO 15m candles used by the chart/swing profile (latest fixed seed + live closes); `volume` = tick count |
| `/tick-volume` | Live tick-volume bars built since boot (up/down/flat split, tick rate); newest may be in progress |
| `/calendar`| Latest economic calendar snapshot (`source`, `count`, `events`) |
| `/ai`      | What the econ news delivered today: Node 3's `transcript`/`sentiment` frames (+ last health), filtered to the current UTC day |
| `/ws`      | WebSocket stream — send `{"type":"subscribe","topics":["candle","tick_volume","levels","bubbles","calendar","status","audio_chunk","learn","transcript","sentiment","health","prediction"]}` |

Every route is **read-only**. There is no `POST`/`PUT`/`PATCH`/`DELETE` route
anywhere in this service — nothing here accepts an order, a fill or any other
broker write.

WebSocket frames are tagged with `type`:

| Direction | Frames |
|---|---|
| Node 1 → clients (market) | `candle`, `tick_volume`, `levels`, `bubbles`, `calendar`, `status`, `heartbeat` |
| Node 1 → Node 3 (market/audio) | `audio_chunk`, `learn`, plus the market frames above |
| Node 3 → Node 1 (AI results) | `transcript`, `sentiment`, `health`, `prediction` |
| Node 1 → Node 3 (requests) | `sentiment_req`, `predict_req` |

The four order-flow colors: `BUY_BUBBLE` green, `SELL_BUBBLE` red, `ABS_BUY`
blue, `ABS_SELL` orange.

There is **no `trades` topic**. Node 1 does not accept, parse or forward a
trade/execution/fill frame — orders are Node 4's job, and Node 4 is not
reachable from Node 1. Exchange *trade ticks* (`AggTrade`) are unaffected:
they are inbound market data from the upstream venues and are what the
order-flow `bubbles` are computed from.

Subscribing to `levels` replays the cached profiles. Subscribing to `candle`
replays the latest 15 cached 15-minute OHLC bars (enough for Node 3 to seed
ATR(14)); `/candles` remains the full chart-history endpoint. Subscribing to
`tick_volume` replays the live bars built since boot.

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

The dashboard (Node 2, `https://static-dash-frontend.onrender.com`) is a static
site on its own origin, so it reads these routes cross-origin. `CorsLayer` is
applied to the whole router and allows **exactly one origin**, `CORS_ALLOWED_ORIGIN`:

* `GET /health|/levels|/vp|/candles|/tick-volume|/calendar|/status|/ai` from
  that origin get `access-control-allow-origin: <origin>`, plus
  `vary: origin, ...` so a shared cache can never hand our grant to a different
  requester.
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

All of these are **market data** feeds. None of them is a broker, an execution
venue or an order route.

## Environment variables (all optional)

Every variable below is owned by Node 1. There is no Deriv, Chelsea, MT5,
execution-venue, stake, lot-size or SL/TP/R:R variable in this service — those
live on Node 4 and are not read here.

```
# ---- Server ----
PORT=3000

# ---- Market data / feeds ----
BINANCE_SYMBOL=XAUUSDT                      # builds BINANCE_WS_URL
BINANCE_WS_URL=                             # default wss://fstream.binance.com/ws/<symbol>@aggTrade
SIFTING_API_KEY=                            SIFTING_SYMBOL=XAUUSD
SIFTING_WS_URL=wss://stream.sifting.io/ws/v1
SIFTING_HIST_URL=https://api.sifting.io

# Feed switches: on | off | auto   (auto = on for keyless public feeds, and for
# tokened feeds only when the token exists)
FEED_BINANCE=auto     FEED_BYBIT=auto      FEED_OKX=auto
FEED_BITGET=auto      FEED_GATE=auto       FEED_KRAKEN=auto
FEED_ALLTICK=auto     FEED_ITICK=auto      FEED_SIFTING=auto

BITGET_SYMBOL=XAUTUSDT      GATE_SYMBOL=XAUT_USDT       KRAKEN_PRODUCT=PF_XAUTUSD
ALLTICK_TOKEN=              ALLTICK_WS_URL=             ALLTICK_CODE=XAUUSD
ITICK_TOKEN=                ITICK_WS_URL=               ITICK_SYMBOL=XAUUSD

# ---- Browser MCP (calendar fallback + econ coverage discovery) ----
MCP_BROWSER_URL=http://localhost:3001     MCP_BROWSER_TOKEN=
MCP_BROWSER_TOOL_NAVIGATE=  MCP_BROWSER_TOOL_READ=      MCP_SCRAPE_SECS=900

# ---- Economic calendar ----
CALENDAR_URL=               CALENDAR_USE_MCP=true

# ---- Econ-news audio monitor: streams live event coverage to Node 3 as audio_chunk ----
ECON_MONITOR=true           ECON_SCAN_SECS=60
ECON_WINDOW_BEFORE_SECS=900 ECON_WINDOW_AFTER_SECS=3600
ECON_MIN_IMPACT=High        ECON_CURRENCIES=USD
ECON_STREAM_SOURCES=        # comma list of live pages/media (e.g. https://www.youtube.com/@federalreserve/live)
ECON_CHUNK_MS=1000          ECON_USE_MCP=true
ECON_MAX_STREAM_SECS=7200   ECON_YTDLP=yt-dlp   ECON_FFMPEG=ffmpeg
ECON_TEST_TONE=1            # CI/offline plumbing test: synthetic 16kHz tone instead of live audio

# ---- CI / air-gapped only: synthetic candle seed when no upstream history ----
SEED_SYNTHETIC_CANDLES=0

# ---- Volume-profile histogram model (TradingView parity) ----
VP_ROW_MODE=rows            # rows = TV "Number Of Rows" layout | price = fixed-height rows
VP_ROWS=128                 # TV "Row Size" when VP_ROW_MODE=rows
VP_BIN_SIZE=0.50            # row height when VP_ROW_MODE=price
VP_TICK_SIZE=0.01           # XAUUSD tick: row height is rounded to whole ticks
VP_VA_PCT=70                # TV "Value Area Volume"
VP_LOWER_TF=tv              # tv = TradingView's 5,000-bar ladder per window | 1m/5m/15m/...
VP_FALLBACK_INTERVAL=5m     # second try when 1m history cannot cover PW

# ---- Browser CORS (the Node 2 static site) ----
CORS_ALLOWED_ORIGIN=https://static-dash-frontend.onrender.com
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
replaced with Binance prices. SiftingIO's REST history can lag its own live
stream (measured: the newest seeded bar was 15 hours behind the live bucket),
and the live stream only ever appends from *now*, so the boot path also asks
for the tail explicitly (`order=asc` from the seed's edge) and merges it —
otherwise every restart would leave a permanent hole between the seed and the
first live close. `/status` reports the result: `candles.bars`,
`candles.first_bar`, `candles.last_bar` and `candles.edge_lag_minutes` (minutes
between the newest candle's bucket and now — a healthy feed sits inside the
in-progress bucket, and a large value is the hole). `ci/verify_candles.py`
fails if any bar is off the 15-minute grid. PW/PS/CW use paginated **1m** SiftingIO bars from
the PW start through the most recently completed minute, so weekly profiles
don't smear each 15m candle's entire volume across its full wick range. Live
Sifting ticks are folded into completed 1m profile bars and 15m chart/swing
bars. If the 1m history does not cover the PW window, the engine logs a warning
and falls back to the 15m profile input; Binance remains isolated to the
aggTrade order-flow analyzer.

`PS` describes the last session that **traded**. SiftingIO keeps publishing a
bodyless bar (`open == close`) for every bucket the venue is shut, and those
fillers carry volume, so the weekend session is not empty — it is a flat line.
A session therefore only counts when at least one of its bars has a body;
otherwise the engine walks back to the previous 18:00 NY close. Live, that
means a Monday morning reports **Friday's** session instead of a $1.5 sliver
sitting on the Friday close at 4137.5 while the market trades 4150+.

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
recent session's bars rather than spanning stale history. The histogram itself
is a TradingView-parity model (see **TradingView parity** below): rows are laid
out exactly like the FRVP tool, volume is allocated to the rows each bar's
high/low actually overlaps, and the value area expands from the POC with
TradingView's tie-breaks. Each `VpLevels` payload carries `start` / `end`
(epoch ms), `direction`, swing anchor prices and a `meta` block (row mode, row
height, produced row count, profile range, input resolution and bar count,
total volume, value-area percentage) so clients can audit exactly which move —
and which row model — produced the levels. OHLC bars are still not
tick-at-price data: the finest history available decides how sharp a profile
can be, and the input resolution is reported rather than assumed.

The implementation is split by window for accuracy: `volume_profile/session.rs`
(18:00 → 18:00 sessions), `volume_profile/weekly.rs` (week anchors),
`volume_profile/swing.rs` (the directional leg), and
`volume_profile/histogram.rs` (the shared POC/VA math), with
`volume_profile.rs` orchestrating ingest, refresh, and fan-out.

## TradingView parity

The engine computes the same histogram a TradingView **Fixed Range Volume
Profile** would, given the same range. The mapping is 1:1, so the settings can
be copied straight off the chart:

| TradingView FRVP input | Engine | Default |
|---|---|---|
| Rows Layout = Number Of Rows | `VP_ROW_MODE=rows` | `rows` |
| Row Size = 128 | `VP_ROWS=128` | `128` |
| Rows Layout = Ticks Per Row | `VP_ROW_MODE=price` + `VP_BIN_SIZE` | `0.50` |
| Volume = Up/Down | every `/vp` row reports `up_volume` / `down_volume` (POC/VA always use the total) | — |
| Value Area Volume = 70 | `VP_VA_PCT=70` | `70` |
| Extend Right | n/a (the engine always covers its window) | — |

**Rows.** With "Number Of Rows" the row height is not a fixed dollar amount:
TradingView derives it from the profile range and rounds it to whole symbol
ticks — `Ticks Per Row = round((Histogram Top - Histogram Bottom) / Rows / Tick Size)`
— and adds rows when the range is not an exact multiple. For XAUUSD
(`tick = 0.01`):

* a $150 weekly range over 128 rows -> `150 / 128 / 0.01 = 117.2` ticks: the
  candidates are 117 ticks (129 rows) and 118 ticks (128 rows); **118 ticks =
  $1.18 per row** wins because it lands exactly on the requested 128 rows;
* the same 100-tick range over 30 rows -> `3.3` ticks -> **3 ticks per row**,
  34 rows (TradingView's own worked example);
* the old fixed $0.50 grid is only correct when the range happens to divide
  evenly — it is still available as `VP_ROW_MODE=price`, but it shifts every
  POC/VAH/VAL by up to half a bin versus the chart.

**Input resolution.** TradingView does not build a profile from the chart's
bars: it walks the `1, 5, 15, 30, 60, 240, 1D` ladder and takes the first
resolution whose bar count for the selected range stays under 5,000. For gold
(~23 trading hours per day) that means a **weekly** range (~6,900 1m bars) is
built from **5m** bars, while a single 18:00 -> 18:00 session (~1,380 1m bars)
stays on **1m**. `VP_LOWER_TF=tv` (default) reproduces that per window from the
engine's own 1m history; pin a resolution (`VP_LOWER_TF=1m`) to always use the
finest bars instead. If 1m history cannot cover the previous week, the engine
retries with `VP_FALLBACK_INTERVAL` (5m) and only then falls back to the 15m
chart seed — which is logged loudly and disclosed in `meta.input_interval`.

**Auditing a mismatch.** `GET /vp?window=PW` returns every row of the profile
behind the emitted levels plus the settings that produced it (`row_mode`,
`row_height`, `rows`, `range_low`/`range_high`, `input_interval`, `input_bars`,
`total_volume`, `va_pct`, row volumes with the up/down split). Compare it with
the chart:

* `range_low` / `range_high` must match the FRVP's histogram top and bottom —
  the engine's PW window is Sunday 18:00 -> Friday 18:00 America/New_York;
* `rows` / `row_height` must match what the chart shows after its rounding;
* `input_interval` should be the resolution TradingView picks (5m for a weekly
  range), and `input_bars` should be in the same ballpark;
* POC = the centre of the heaviest row, VaH/VaL = the top/bottom edges of the
  outermost value-area rows — exactly where the FRVP draws its lines.

A **Fixed Range** profile is drawn by hand, so its window is whatever the
chart selected — `GET /vp?start=<epoch-ms>&end=<epoch-ms>` profiles exactly
that range from the retained 1m history (`window=CUSTOM`) and picks the input
resolution from that range's own bar count, the same way TradingView does.
An inverted range or a non-numeric bound is a 400; a range that reaches beyond
the retained history is a 409 that reports the retained bounds, so a
hand-copied timestamp from the chart cannot be mistaken for an engine fault.

Different data feeds (SiftingIO spot ticks vs a broker's CFD feed) can still
move the POC by a few ticks; matching the window, the row model and the input
resolution removes the structural differences.

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

## Node 1 → Node 3 (market data out, AI results back)

### Market state

Node 3 subscribes to `candle` and `levels` (plus the audio/learner topics) on
`/ws`. On each subscribe (including reconnect), Node 1 first replays its cached
level frames and last 15 candles. Node 3 stores `PW` / `PS` / `CW` levels and
seeds Wilder ATR(14) from those 15 bars; duplicate updates for the current
15-minute candle replace that bar. `PW.sunday_open` carries the open price of
the exact Sunday 18:00 New York start candle when that candle exists in the
history (`PW.start` remains the boundary timestamp).

That is the whole market-data contract. Node 3 decides what to do with it;
Node 1 does not evaluate a strategy, and Node 3 cannot publish an order back
through Node 1 (there is no `trades` topic and no order route — orders go to
Node 4).

For lowest latency, point Node 3's `NODE1_WS_URL` at Node 1's direct secure
WebSocket endpoint (`wss://<node1-host>/ws`), using provider-private
networking / internal DNS when available, and keep Node 3 and Node 1 in the
same region.

### Econ news audio

The engine also turns economic events into live audio, streams it to Node 3 over
the same `/ws` connection Node 3 already holds, and reports what the news delivered.

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
/ws topic `audio_chunk`  ──►  Node 3 ws_client.py  (also candle + levels)
                                 ├─ Moonshine → transcript
                                 └─ FinBERT / FOMC-RoBERTa → sentiment
   ▼                                        │
GET /ai  ◄── engine caches those frames ────┘   (today's UTC day)
```

**Wire contract** (what Node 3's `ws_client.py` consumes):

- `{"type":"audio_chunk","data":{"data":"<b64 f32le 16kHz mono>","ts":…,"source":…,"event":…,"sample_rate":16000,"format":"f32le","duration_ms":1000}}`
  — one PCM chunk per `ECON_CHUNK_MS` of captured audio
- `{"type":"learn","data":{"features":[…],"target":0|1}}` — forwarded to Node 3's
  online learner; any WS client or script may publish `learn`/`audio_chunk`
  frames to `/ws` and the engine rebroadcasts them on the bus
- Node 3's replies (`transcript`, `sentiment`, `health`, `prediction`) are
  fanned out on their topics and cached for `GET /ai`
- `sentiment_req` / `predict_req` requests are forwarded to Node 3 regardless
  of topic subscription; Node 3 subscribes to `audio_chunk`, `learn`,
  `candle` and `levels`

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
cargo fmt --all -- --check
cargo check --all-targets
cargo test --all-targets -- --nocapture
cargo clippy --all-targets -- -D warnings
cargo build --release

cargo test -- --ignored --nocapture  # hits the live ForexFactory feed
python3 ci/check_node1_boundary.py . # Node1 owns market data only
bash ci/smoke.sh                     # boots the release binary and asserts:
                                     #   /levels window bounds, /calendar contents,
                                     #   /vp histogram (contiguous rows, volume conservation,
                                     #   POC = heaviest row, VAL <= POC <= VAH),
                                     #   /candles + /tick-volume payload shape, the CORS allow-list,
                                     #   the econ audio pipeline (audio_chunk/learn on /ws, /ai digest)
                                     #   and that /ws replays levels/15 candles and accepts no
                                     #   execution/fill frame
```

CI runs all of these on every push. The CORS probes in `ci/smoke.sh` are the
authoritative proof of the allow-list, since they run against the real release
binary; `ci/verify_ws.py` does the WebSocket handshake on a raw socket so no
websocket client library is needed. `ci/check_node1_boundary.py` is the guard
that keeps execution, broker credentials and MT5 from creeping back in.

Live market probes live in `ci/live-probe.sh` (REST payloads + a WebSocket
sample recorded into `ci/live/`, digested into `ci/LIVE.md`) and run from the
Actions tab against the deployed engine.
