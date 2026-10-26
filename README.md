# XAUUSD market-data & calendar feed — **Node 1** (contract C1 v2)

Node 1 is the **read-only market feed** for the XAUUSD desk. It ingests the live
SiftingIO spot stream, builds multi-timeframe candles, counts tick volume, pulls
the economic calendar straight from the ForexFactory weekly export (browser MCP
optional), and republishes all of it over a JSON HTTP API and a WebSocket feed.

That is the whole job. Node 1 **does not compute a level, choose a side, size a
trade or accept a write**:

* no volume-profile maths, no PW/PS/CW/VaL/VaH — those are Node 3's;
* no strategy evaluator, no demo/paper account, no SL/TP/RR, no lots;
* no trade intent, no order, no fill, no broker/venue identifier;
* no `POST`/`PUT`/`PATCH`/`DELETE` handler anywhere — every route is `GET`.

The published wire format is the signed **C1 v2** contract in
[`contracts/`](contracts/): `contract_version: "2.0"`, a per-process `boot_id`,
a strictly increasing `seq`, `replay` bursts, `gap` notices and a `freshness`
state machine. Consumers fail closed on an unknown contract version.

---

## Service boundary

| Node | What it is | Where it lives |
|------|------------|----------------|
| **Node 1** (this branch) | Market data, tick volume, economic calendar. **Read-only, no write path.** | `Node1` branch |
| **Node 2** | Read-only browser dashboard (static site). Renders Node 1's candles. | `Node2` branch |
| **Node 3** | Strategy, volume profile (PW/PS/CW), FRVP, ATR projection. Consumes Node 1 candles. | `Node3` branch |
| **Node 4** | Live execution / MT5. Owns every trade decision and every write. | `Node-4` branch |

```
   upstream feed
   SiftingIO spot ──────► ┌────────────────────────────────┐
                          │            NODE 1              │
   ForexFactory  ───────► │  candles 1m · 15m · 1h         │
   weekly export          │  tick volume (15m)             │
                          │  economic calendar             │
   browser MCP            │  freshness · session clock     │
   (calendar fallback) ──►│  GET only · no write route     │
                          └───┬───────────────┬────────────┘
                              │ REST          │ WS  /ws
                              ▼               ▼
                        Node 2 (chart)   Node 3 (strategy)
                                         Node 2 (chart)

   Node 1 ⇸ Node 3 ⇸ Node 4 : Node 1 is never on the order path.
```

Node 1 never depends on Node 2, Node 3 or Node 4 being available.

### What Node 1 owns — and what it does not

| Node 1 owns | Node 1 does **not** own |
|---|---|
| The SiftingIO market/feed adapter | Level or volume-profile maths (Node 3) |
| Candles at 1m / 15m / 1h | Strategy, signals, ATR projection (Node 3) |
| Tick volume (counts, up/down/flat split) | Position sizing, SL/TP/RR (Node 3 → Node 4) |
| The economic calendar | Orders, fills, intents, broker credentials (Node 4) |
| Feed freshness and the market clock | Account state, equity, margin |
| The read-only REST + WS surface | Any write route, in any direction |

---

## Run

```bash
cargo run                      # reads the environment, binds 0.0.0.0:$PORT
# or
docker build -t engine . && docker run -p 10000:10000 engine
```

Then hit **`http://localhost:10000/status`**. There is no `/` route — the engine
is API-only and serves no static files.

Every configuration value is optional; the only credential Node 1 ever reads is
the SiftingIO market-data API key. Without it the process still boots, serves
`/status`, `/calendar` and the WS surface, and reports `freshness.state =
"offline"` (or `closed_market` at the weekend) until a feed is configured.

## HTTP endpoints

| Route | What |
|---|---|
| `GET /health` | `ok` |
| `GET /status` | Feed snapshot: `contract_version`, `boot_id`, `uptime_ms`, `market_open`, `session`, `freshness`, `feeds`, per-interval candle book + tick counters, calendar summary, WS block |
| `GET /candles?interval=1m\|15m\|1h` | Closed bars for one resolution, ascending and unique by `time`. Defaults to `15m`; anything else is a `400` naming the contract version (`60m` is accepted as `1h`). |
| `GET /tick-volume` | Live tick-volume bars, oldest first; only the newest may be `closed: false` |
| `GET /calendar` | Normalized ForexFactory events (`source`, `count`, `updated`, `events[]`) |
| `GET /ws` | The C1 v2 stream — see below |

All REST responses carry `Cache-Control: no-store`, and **`GET` is the only
method**: `POST`/`PUT`/`PATCH`/`DELETE` return `405` on every registered path
(unit-tested in `ws_server.rs::no_route_accepts_a_broker_write` and asserted
over the live process by `ci/smoke.sh`).

### Candles

| Interval | Bucket | Retained | Replayed on subscribe |
|---|---|---|---|
| `1m` | 60 s | 43,200 bars (30 days) | 240 |
| `15m` | 900 s | 2,000 bars | 96 |
| `1h` | 3,600 s | 2,000 bars | 48 |

Every bar is `{symbol, interval, time, open, high, low, close, volume, source,
closed}`:

* `time` is the bucket-open epoch millisecond and is always bucket-aligned;
* `volume` is the **tick count**, the same unit SiftingIO's historical bars use,
  so history and live bars plot on one scale;
* `closed: true` on REST, exactly once per bucket on the WS stream — and always
  *before* the first frame of the following bucket;
* `/candles` never serves a forming bucket (that exists only on the WS stream);
* `source` is `"sifting"` for the real feed and `"synthetic"` for the CI seed.

Node 3 draws its PW/PS/CW boundaries from `1h` and folds FRVP from the finer
resolutions, so all three come out of the same book.

### Tick volume

One `TickVolumeBar` per 15m bucket: `{symbol, interval, time, ticks, up_ticks,
down_ticks, flat_ticks, close, last_tick, ticks_per_sec, closed, source}`.

`ticks` **is** the candle's `volume` for the same bucket. Spot XAUUSD has no
taker side, so the split uses the tick rule (a print above the previous print is
`up`, below is `down`, unchanged is `flat`) — an activity gauge, not order flow.
The last 2,000 closed bars are retained for replay.

### Freshness, sessions and the clock

`/status.freshness` (and the payload of every `status` frame) is:

```json
{
  "feed": "sifting",
  "last_tick_ms": 1792781867153,
  "age_ms": 812,
  "state": "live",
  "market_open": true
}
```

| `state` | When |
|---|---|
| `live` | connected **and** `age_ms <= 60_000` **and** the market is open |
| `stale` | connected, but no tick for longer than 60 s |
| `offline` | the socket is down, or no tick has ever arrived |
| `closed_market` | outside the XAUUSD session — wins over everything else |

The session is DST-aware `America/New_York`: it opens Sunday 18:00 and closes
Friday 17:00 local time (a 23-hour day in spring, 25 hours in autumn).
`/status.session` publishes the exact `open_ms` / `close_ms` epoch millisecond
bounds, so a consumer never has to re-derive them.

## WebSocket `/ws`

The stream is **one-way**. A client may send exactly two frames:

```json
{"type": "subscribe", "topics": ["candle", "tick_volume", "calendar", "status"],
 "intervals": ["1m", "15m", "1h"]}
{"type": "heartbeat"}
```

`topics` and `intervals` are both optional: omitting `intervals` subscribes to
all three resolutions, and the filter only applies to `candle` frames.

### Frames

| `type` | Payload |
|---|---|
| `candle` | one bar (`closed` true/false) |
| `tick_volume` | one 15m tick-volume bar |
| `calendar` | the whole normalized calendar (`source`, `count`, `updated`, `events[]`) |
| `status` | the same document as `GET /status` |
| `heartbeat` | keepalive, both directions, every 30 s, **no** payload and **no** `seq` |
| `gap` | `{"dropped": N}` — this subscriber overran the 4,096-frame buffer |

### Envelope and ordering

* Live frames carry `seq`, an integer that starts at 1 and is **strictly
  increasing across all topics** — one counter per process, shared by candles,
  tick volume, calendar and status. Losing a frame is therefore always visible.
* The burst a subscriber gets right after `subscribe` is `"replay": true` and
  **never** carries `seq`. It contains closed candles only (240/96/48 by
  resolution), the retained tick-volume bars, the cached calendar and one status
  document — a fresh client is charting immediately.
* `gap` and `heartbeat` carry neither `seq` nor `replay`.
* A `status` frame is published on **every** transition of the freshness state,
  the `market_open` flag or a feed state, so a consumer never has to poll.
* Inbound frames that look like execution — `trades`, `trade_intent`,
  `execution_hello`, `execution_report`, `order`, `bubbles` — are **ignored and
  never rebroadcast**. Nothing execution-shaped is parsed, cached or fanned out.

## Data sources

| Feed | Source | Fallback |
|---|---|---|
| Candles + ticks | SiftingIO spot stream (`wss://stream.sifting.io/ws/v1`) | none — `freshness` says `offline` |
| History seed | SiftingIO `/v1/hist/commodities/XAUUSD/bars` (2,000-bar seed + tail gap-fill) | `SEED_SYNTHETIC_CANDLES=1` (CI only) |
| Economic calendar | ForexFactory weekly export over HTTPS | browser MCP server (`MCP_BROWSER_URL`) |

## Environment variables (all optional)

```bash
# ---- Server ----
PORT=3000                                    # listen port (0.0.0.0)
RUST_LOG=info

# ---- Market data ----
SIFTING_API_KEY=                             # the only credential Node 1 reads
SIFTING_WS_URL=wss://stream.sifting.io/ws/v1
SIFTING_HIST_URL=https://api.sifting.io
SIFTING_SYMBOL=XAUUSD
# Feed switch: on | off | auto   (auto = on only when SIFTING_API_KEY exists)
FEED_SIFTING=auto

# ---- Browser MCP (calendar fallback only) ----
MCP_BROWSER_URL=http://localhost:3001
MCP_BROWSER_TOKEN=
MCP_SCRAPE_SECS=900                          # calendar refresh period (min 60)

# ---- Economic calendar ----
CALENDAR_URL=                                # override the direct feed URL
CALENDAR_USE_MCP=true                        # use the browser MCP as a fallback

# ---- Browser CORS (the Node 2 static site) ----
CORS_ALLOWED_ORIGIN=https://static-dash-frontend.onrender.com

# ---- CI / air-gapped only ----
SEED_SYNTHETIC_CANDLES=0                     # deterministic seed, source="synthetic"
```

There are no `STRATEGY_*`, `DEMO_*`, `ECON_*`, `VP_*`, `SL_*`, `TP_*` or `RR_*`
variables left: sizing, stops, targets and the venue all live on Node 4, behind
Node 3's decisions.

### CORS

The Node 2 dashboard is a static site on its own origin, so exactly that origin
is allowed — no wildcard, no credentials, `GET`/`OPTIONS` only, `max-age=600`,
and `Vary: Origin` so a shared cache cannot hand one origin's grant to another.
Every other origin gets a normal response with no `Access-Control-*` header.
Override with `CORS_ALLOWED_ORIGIN` when the site moves.

## Contract and handoff

| Path | What |
|---|---|
| [`contracts/NODE1_FEED_CONTRACT_v2.md`](contracts/NODE1_FEED_CONTRACT_v2.md) | The signed C1 v2 document (envelope, topics, freshness, negative guarantees) |
| [`contracts/c1-market-feed.schema.json`](contracts/c1-market-feed.schema.json) | JSON-Schema for every frame; `ci/verify_contract.py` validates against it |
| [`contracts/MANIFEST.sha256`](contracts/MANIFEST.sha256) | Byte-hash of every contract file |
| [`handoff/`](handoff/README.md) | The volume-profile and audio modules that moved to Node 3, hash-pinned |

The contracts are **byte-identical on all four branches**: `python3
ci/contract_sync.py` fails if this copy drifts from the manifest, and every
consumer fails closed on an unknown `contract_version`.

`handoff/node3-vp/` (`volume_profile.rs`, `histogram.rs`, `session.rs`,
`swing.rs`, `timeframe.rs`, `weekly.rs`) is Node 3's source, stored here until
Agent 3 has ported it; nothing in `src/` references it.

## Verifying

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
python3 ci/contract_sync.py          # contracts are identical to the manifest
python3 ci/check_node1_boundary.py . # no strategy / VP / intent / write route
cargo build --release
bash ci/smoke.sh                     # boots the binary and proves the contract
```

`ci/smoke.sh` starts the release binary with a synthetic seed and a **local
calendar fixture** (so the run is hermetic and the first calendar fetch lands
while the WS clients are connected), then proves against the live process:

1. `/status` publishes `contract_version`, `boot_id`, `freshness`, the session
   clock and per-interval book sizes — and none of the retired sections;
2. `/candles` is ascending, unique, bucket-aligned and closed on 1m, 15m and 1h,
   `?interval=5m` is a `400`, `?interval=60m` is `1h`;
3. `/tick-volume` and `/calendar` are well-formed;
4. CORS grants exactly the Node 2 origin, `GET`/`OPTIONS` only;
5. every non-`GET` method is `405` on every route;
6. `/ws` (raw sockets in `ci/verify_ws.py`) replays closed bars per resolution,
   numbers live frames with one strictly increasing `seq`, answers heartbeats,
   and ignores six execution-shaped inbound frames without echoing them;
7. `ci/verify_contract.py` validates every recorded payload and frame against
   `contracts/c1-market-feed.schema.json` (the C1 contract matrix).

Unit tests cover the rules a socket cannot be made to reproduce on demand: a
tick with a non-finite or non-positive price, a seconds-based timestamp, a tick
more than 5 s in the future and a tick for an already-closed bucket are all
dropped (and counted by reason in `/status`); a broadcast overrun produces
`gap`; `seq` is strictly increasing; `boot_id` is stable; replay never carries a
`seq`; `closed == true` is emitted exactly once per bucket.

`ci/live-probe.sh` records what the **deployed** engine is serving into
`ci/live/` and writes the digest to `ci/LIVE.md` (run it from the Actions tab
after a redeploy — the dev sandbox cannot reach the Render host).

## Migration notes (v0.4.0 → C1 v2)

| Removed from Node 1 | Where it went |
|---|---|
| `src/volume_profile*.rs`, `ProfileMeta`, `/levels`, `/vp` | `handoff/node3-vp/` → **Node 3** owns all VP maths |
| Volume-profile sections of `/status` | Node 3's own status |
| `src/strategy.rs`, `/strategy`, `strategy` topic | Node 3 |
| `src/demo_engine.rs`, `/demo`, `demo` topic | gone — nothing simulates a fill on the feed branch |
| `src/econ_monitor.rs`, `src/ai_cache.rs`, `/ai`, `audio_chunk`/`learn`/`transcript`/`sentiment`/`health`/`prediction` topics | `handoff/node3-ai/` (optional; nothing on Node 3 consumes them today) |
| `bubbles` (order flow) | gone since the order-flow removal |
| 17 `STRATEGY_*`/`DEMO_*` env vars, `FRONTEND_NEXT.md` | gone |

The golden fixtures recorded from the last v0.4.0 deployment stay in
`ci/live/` (`ws-sample.jsonl`, `candles.json`, `levels.json`,
`tick-volume.json`, `status.json`, `vp-{PW,PS,CW}.json`): Node 3 uses them as
its FRVP parity baseline, and the next live probe replaces the ones it
re-records.
