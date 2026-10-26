# Node 1 market-data feed contract — v1

**Status: DRAFT — pending agreement.** Nothing in this document is implemented
until the parties below sign off; the Node 1 agent then implements it on the
`Node1` branch (see [`NODE1_BOUNDARY_AUDIT.md`](NODE1_BOUNDARY_AUDIT.md) §6).

| Party | Role | Sign-off |
|---|---|---|
| Node 1 owner | producer (`xauusd-engine`) | ☐ |
| Node 3 owner | machine consumer (`node3-strategy`) | ☐ |
| Node 2 owner | browser consumer (dashboard) | ☐ |
| Node 4 owner | **not a consumer** — acknowledges only (audit §5) | ☑ acknowledged 2026-10-23 |

Every "Today" statement below was verified against `origin/Node1 @ d007137`
and the recorded live probe in `origin/Node1:ci/live/` (deployed build 0.3.0,
captured 2026-10-23). Every "v1" statement is a proposal. All v1 changes are
**additive**: Node 3 (serde, no `deny_unknown_fields`) and Node 2 (JS) both
ignore unknown fields and unknown frame types, so a v1 producer is safe against
v0 consumers.

---

## 1. Scope and non-goals

In scope: XAUUSD market data published by Node 1 — 15m OHLCV candles,
tick-volume bars, volume-profile levels, economic calendar, feed status — over
REST and WebSocket.

**Out of scope, forever:** trade intents, signals, side/stop/target, position
size, account state, broker commands. No frame on `/ws` and no route carries
any of these in either direction. The Node 1 ⇄ Node 3 AI frames
(`audio_chunk`, `learn`, `transcript`, `sentiment`, `health`, `prediction`,
`*_req`) are a *separate* contract and are only mentioned where they affect
the feed (auth, §2).

---

## 2. Transport, endpoints, auth

| | Today | v1 |
|---|---|---|
| Base | `https://<node1-host>` (Render; deployed host `engine-southeastasia-sng-main.onrender.com`) | unchanged |
| Stream | `GET /ws` → WebSocket; text frames, JSON, one frame per message | unchanged |
| Subscribe | client sends `{"type":"subscribe","topics":[...]}`; nothing is delivered before it; re-sending replaces the topic list and triggers a fresh replay | unchanged; **document** that re-subscribe = resync |
| Snapshots | `GET /candles`, `/levels`, `/vp`, `/tick-volume`, `/calendar`, `/status`, `/health` | unchanged; add `Cache-Control: no-store` to `/candles`, `/levels`, `/status` (only `/tick-volume` and `/status` set it today) |
| Read auth | none. Browsers are limited by a single-origin CORS allow-list (`CORS_ALLOWED_ORIGIN`); non-browser clients (Node 3) connect anonymously | **unchanged for v1**: the feed is public read-only market data and the browser consumer cannot hold a secret. Rate limiting is the platform's job. |
| Write-back auth | the inbound AI frames are **unauthenticated**: any client can publish `audio_chunk` into Node 3 or poison `/ai` | not part of the feed contract, but **flagged** as open decision §13.1 — recommend a token in a `hello` frame or a separate `/ws/ai` path for Node 3 |
| Versioning | none | `/status.contract_version: "1.0"` and the same key on the post-subscribe `status` frame. Additive change → minor; rename/removal/semantic change → major **with a parallel topic** (e.g. `candle_v2`) for one release. |

---

## 3. Heartbeat, liveness, reconnect

| | Today | v1 |
|---|---|---|
| Server heartbeat | `{"type":"heartbeat"}` every 30 s to every socket, independent of topics | unchanged (interval becomes part of the contract: **30 s**) |
| Client heartbeat | Node 2 and Node 3 echo a `heartbeat`; Node 1 parses and ignores it | unchanged; echo is optional |
| Dead-link detection (consumer) | Node 2: 75 s silence → reconnect, backoff 1 s → 30 s; Node 3: on close/error → retry after 5 s | **rule:** any frame resets the timer; **75 s** (2.5 × heartbeat) of silence = dead → close, reconnect with exponential backoff capped at 30 s, re-send `subscribe` |
| Dead-link detection (producer) | send failure only | unchanged |
| On reconnect | consumer re-subscribes; producer replays the snapshot burst (§8.3) | unchanged + the consumer must **resync by identity** (§8.4), never append blindly |

---

## 4. Symbol and timeframe naming

| | Today | v1 |
|---|---|---|
| Symbol | implicit — one instrument per process (`SIFTING_SYMBOL`, default `XAUUSD`); **candle frames carry no symbol** | `symbol: "XAUUSD"` added to `candle` and `tick_volume` payloads and to `/status` (uppercase, no separator, no venue suffix) |
| Interval | `/status.candles.interval: "15m"`, `ProfileMeta.input_interval: "1m" \| "5m" \| "15m"`; candle frames carry no interval | `interval: "15m"` added to `candle` and `tick_volume`. Format: integer + unit, lowercase, units `m`/`h`/`d`. Chart feed is 15m **by design**; profile inputs are 1m/5m internally and are only visible via `meta.input_interval`. |
| Profile windows | `window ∈ {PW, PS, CW, SWING_BULL, SWING_BEAR}`; `/vp` also `CUSTOM` | unchanged, enumerated here as the closed set |

---

## 5. Timestamps

| Field | Meaning | Unit |
|---|---|---|
| `candle.time`, `tick_volume.time` | **bucket open** | epoch **milliseconds**, UTC; `time % 900000 == 0` for 15m (verified on all 2,000 recorded bars) |
| `levels.start` / `levels.end` | window `[start, end)` — inclusive / exclusive | epoch ms UTC |
| `levels.timestamp` | when the profile was last recomputed | epoch ms UTC |
| `tick_volume.last_tick` | newest tick counted in the bucket | epoch ms UTC |
| `status.ts`, `status.feeds.*.last_msg` | now / newest upstream message (`0` = never) | epoch ms UTC |
| `calendar.updated`, `calendar.events[].timestamp` | | epoch ms UTC |

Rules: no ISO-8601 strings anywhere on the wire; no seconds-precision values.
Session windows (PS/PW/CW, the 18:00 New York close and the Sunday 18:00 NY
week open) are **defined** in `America/New_York` but **expressed** in epoch ms
UTC. Consumers never need timezone logic to use the feed.

---

## 6. OHLCV field types and semantics

```
candle.data = {
  time:    i64  bucket open, epoch ms UTC
  open:    f64  USD per troy ounce, finite, > 0
  high:    f64  finite, >= max(open, close)
  low:     f64  finite, <= min(open, close), > 0
  close:   f64  finite, > 0
  volume:  f64  TICK COUNT — number of price updates in the bucket.
                Non-negative integer value carried as f64. NOT lots,
                NOT contracts, NOT notional. Equals tick_volume.ticks
                for the same bucket.
  source:  str  "sifting" | "synthetic" (CI-only seed; consumers MAY refuse
                it outside CI)
  closed:  bool v1 — see §7
  symbol:  str  v1 — "XAUUSD"
  interval:str  v1 — "15m"
}
```

`tick_volume.data` adds `ticks`, `up_ticks`, `down_ticks`, `flat_ticks`
(`u64`; `ticks == up + down + flat`), `close`, `last_tick`, `ticks_per_sec`
(`f64`, rolling 10 s), `closed` (already present today). The up/down split is
the tick rule (price above / below the previous tick) — an activity gauge,
**not** order flow and not a signal.

`levels.data` is `VpLevels` as today (`window, poc, vah, val, start, end,
timestamp, direction, swing_high, swing_low, sunday_open?, meta?`).
`direction` is `bullish | bearish` only on swing profiles and names the
**anchor extreme**, not a trade side; it is `neutral` on PW/PS/CW.

---

## 7. Forming vs closed candles — the main defect to fix

**Today.** Every accepted upstream tick re-broadcasts the in-progress 15m bar
as a `candle` frame with the **same `time`** (188 of the 203 candle frames in
the recorded sample are the forming bar). When a bucket rolls, Node 1 publishes
the finished bar once, then the first forming frame of the next bucket. The
`candle` payload has **no `closed` flag**; closedness is only inferable from
the paired `tick_volume.closed == true` frame or from a later frame with a
greater `time`. `GET /candles` and the subscribe replay contain **closed bars
only** (the cache is fed by the close path alone, upserted by `time`).

Node 3 currently appends every candle frame to its history without upserting
by `time` (audit §4), which is exactly the failure mode this section exists to
prevent.

**v1.**

* `candle.data.closed: bool` is **always present**. `false` on every forming
  frame, `true` on exactly one frame per bucket — the final one.
* Ordering guarantee per connection: `closed(T)` is published **before** the
  first `closed=false` frame of bucket `T + interval`.
* `GET /candles` returns closed bars only; its last element satisfies
  `time + 900000 <= now`. (Node 1 must drop a forming bucket if the upstream
  REST seed ever includes one — test §12.)
* Consumer obligations: **upsert by identity** (§8.1); run closed-bar logic
  (ATR, break/retest, intent emission) **only** on `closed == true`; treat a
  `closed=false` frame as a live price update. Node 3's `source_candle_time`
  must come from a closed bar (audit §5).

---

## 8. Identity, ordering, sequence, replay, gaps and duplicates

### 8.1 Identity (stable IDs)

| Frame | Identity | Version within identity |
|---|---|---|
| `candle` | (`symbol`, `interval`, `time`) | `closed` (false → true once) |
| `tick_volume` | (`symbol`, `interval`, `time`) | `closed` |
| `levels` | (`window`, `start`, `end`) | `timestamp` (newer wins) |
| `calendar` | singleton | `updated` |
| `status` | singleton | `ts` |

No synthetic UUIDs are introduced: the natural key is deterministic across
restarts and across producers, which a per-process id would not be. A string
form `XAUUSD:15m:1792767600000` MAY be used in logs.

### 8.2 Ordering guarantees

* Live `candle` / `tick_volume` frames are **non-decreasing in `time`** on a
  connection (the aggregator drops late ticks for closed buckets, so the series
  cannot roll backwards).
* `GET /candles` is **strictly ascending, unique by `time`**.
* The series is **not contiguous**: the recorded 2,000 bars contain 30 min, 1 h
  and 2 h 45 min gaps (weekend/holiday closes, feed outages). A gap in `time`
  is **not** an error; consumers must detect it arithmetically
  (`next.time - prev.time > interval_ms`) and must not assume `n` bars = `n ×
  15 min`.
* The replay burst (§8.3) is ascending by `time` within each topic.

### 8.3 Replay on (re)subscribe — what exists

On every `subscribe` the producer sends, in this order: all cached `levels`,
the **last 15 closed candles**, the tick-volume store (closed + forming, up to
2,000 bars — ~270 KB), the calendar snapshot, then one `status` frame
(unconditionally, even if `status` is not in the topic list). After that, live
frames for the subscribed topics.

This is a **snapshot replay, not a log replay**: there is no persistent event
log, nothing older than the in-memory window is replayable, and sequence state
does not survive a restart. Deeper resync is `GET /candles` (2,000 closed
bars) diffed by `time`.

### 8.4 Sequence numbers and gap detection — v1 additions

Today a slow consumer that overruns the 4,096-frame broadcast buffer gets
frames **silently dropped** (`RecvError::Lagged(n)` is only logged); the
consumer has no way to know. v1:

* Envelope field `seq: u64` on every **live** frame: process-wide, strictly
  increasing across all topics, assigned at publish time. Replayed frames carry
  `"replay": true` and **no** `seq`.
* `{"type":"gap","data":{"dropped": n}}` is sent to a session instead of
  silently skipping `n` frames. `gap` bypasses the topic filter (like
  `heartbeat`).
* `/status` and the `status` frame carry `boot_id` (random string per process
  start).
* Consumer rules: `seq` jumps by more than 1 **or** a `gap` frame → resync
  (`GET /candles` + `/levels`, or re-send `subscribe`); `seq` goes
  **backwards** or `boot_id` changes → producer restarted → full resync.
* Duplicates are handled by identity (§8.1): a frame whose identity and version
  were already seen is idempotent. This also covers the overlap between the
  replay burst and the first live frames after a reconnect.

---

## 9. Stale-data behaviour and health

### 9.1 Signals that exist today

| Signal | Where | Meaning |
|---|---|---|
| `status.feeds.sifting.state` | `/status`, `status` frame | `connecting \| connected \| disconnected \| error` (also `calendar`, `econ_monitor`, `mcp_browser`) |
| `status.feeds.sifting.last_msg` / `msgs` | same | newest upstream message (ms) / count — the raw freshness datum |
| `candles.edge_lag_minutes` | `/status` | minutes since the newest **closed** bar's open. Healthy live feed: < 30. The recorded probe shows `27`. Grows all weekend. |
| `candles.bars/seed_bars/first_bar/last_bar` | `/status` | cache extent |
| `/health` | | process liveness only (`ok`) — says nothing about data freshness; platform probes only |

### 9.2 v1: an explicit freshness verdict

```json
"freshness": {
  "feed": "sifting",
  "last_tick_ms": 1792781867153,
  "age_ms": 1240,
  "state": "live",
  "market_open": true
}
```

* `state = live` — connected and `age_ms <= 60000` while `market_open`;
* `state = stale` — connected but `age_ms > 60000` while `market_open`;
* `state = offline` — feed not connected (or never received a message);
* `state = closed_market` — outside the XAUUSD session (Fri 17:00 → Sun 18:00
  New York; Node 1 already owns the NY session logic in
  `volume_profile/session.rs`), regardless of age — prevents weekend false
  alarms.

Published in `/status` and in every `status` frame; a `status` frame is
broadcast on every `state` transition (today it is broadcast on feed-state
changes only).

### 9.3 Consumer obligations

* Subscribe to `status` (Node 3 does not today) or poll `/status`.
* Do not derive decisions from a bar whose `closed == true` arrived while
  `freshness.state != live`; do not emit a trade intent whose triggering bar
  satisfies `time + interval_ms < now - 2 × interval_ms` (30 min) — this is
  Node 3's rule to enforce; the contract only guarantees the signal.
* The 90 s "DELAYED" badge rule Node 2 already applies is compatible.

---

## 10. What the contract forbids

* No frame on `/ws` and no route may carry a trade intent, signal, side, stop,
  target, size, order, fill or account state — in either direction.
* Node 1 does not parse `trades`, `trade_intent`, `execution_hello`,
  `execution_report`, `order` or `bubbles` frames; such input is ignored and
  **never** rebroadcast (negative tests, §12).
* No `POST`/`PUT`/`PATCH`/`DELETE` route exists (`no_route_accepts_a_broker_write`).

---

## 11. Sample payloads

Recorded from the deployed 0.3.0 service (`origin/Node1:ci/live/ws-sample.jsonl`,
1,918 frames: 4 `levels`, 203 `candle`, 1,706 `tick_volume`, 1 `calendar`,
1 `status`, 2 `heartbeat`). v1 additions are marked `// v1`.

```jsonc
// subscribe (client → Node 1)
{"type":"subscribe","topics":["candle","levels","tick_volume","status"]}

// replayed closed candle (first after subscribe)
{"type":"candle","replay":true,                                  // v1: replay marker
 "data":{"time":1792767600000,"open":4000.28,"high":4003.54,"low":3993.08,
         "close":3995.92,"volume":6329.0,"source":"sifting",
         "closed":true,"symbol":"XAUUSD","interval":"15m"}}      // v1

// live forming candle — repeated on every tick with the same `time`
{"type":"candle","seq":48213,                                    // v1
 "data":{"time":1792780200000,"open":4001.82,"high":4002.17,"low":3994.64,
         "close":3996.86,"volume":6428.0,"source":"sifting",
         "closed":false,"symbol":"XAUUSD","interval":"15m"}}     // v1

// the same bucket, final frame — exactly once
{"type":"candle","seq":48397,
 "data":{"time":1792780200000,"open":4001.82,"high":4002.90,"low":3994.64,
         "close":3997.10,"volume":6511.0,"source":"sifting",
         "closed":true,"symbol":"XAUUSD","interval":"15m"}}

// tick-volume bar, closed (recorded verbatim; `closed` already exists)
{"type":"tick_volume","data":{"time":1791345600000,"ticks":3111,"up_ticks":426,
 "down_ticks":470,"flat_ticks":2215,"close":4142.46,"last_tick":1791346498853,
 "ticks_per_sec":4.6,"closed":true,"source":"sifting"}}

// levels (recorded verbatim)
{"type":"levels","data":{"window":"PW","poc":4156.695,"vah":4183.25,"val":4141.44,
 "start":1791756000000,"end":1792188000000,"timestamp":1792781820240,
 "direction":"neutral","swing_high":null,"swing_low":null,"sunday_open":4194.13,
 "meta":{"row_mode":"rows","row_height":1.13,"rows":128,"range_high":4217.33,
         "range_low":4073.64,"input_interval":"5m","input_bars":1438,
         "total_volume":3012721.0,"va_pct":0.7}}}

// status (recorded, trimmed to the sifting feed) + v1 fields
{"type":"status","data":{
  "feeds":{"sifting":{"state":"connected","last_msg":1792781875635,"msgs":7047933}},
  "ts":1792781876162,
  "contract_version":"1.0","boot_id":"b7f3c2","                 // v1
  "freshness":{"feed":"sifting","last_tick_ms":1792781875635,"age_ms":527,
               "state":"live","market_open":true}}}             // v1

// gap notice (v1) — the session overran the broadcast buffer
{"type":"gap","data":{"dropped":37}}

// heartbeat — every 30 s, both directions allowed
{"type":"heartbeat"}
```

`GET /status` (recorded, 0.3.0 — note the removed exchange feeds still present
on the deployment and the absence of `strategy`/`demo`):

```json
{"candles":{"interval":"15m","seed_bars":2000,"bars":2000,
            "first_bar":1790901900000,"last_bar":1792780200000,"edge_lag_minutes":27},
 "status":{"feeds":{"sifting":{"state":"connected","last_msg":1792781867153,"msgs":7047922},
                    "calendar":{"state":"connected","last_msg":0,"msgs":0},
                    "econ_monitor":{"state":"idle (no active event windows)","last_msg":0,"msgs":0},
                    "mcp_browser":{"state":"error","last_msg":0,"msgs":0}},
           "ts":1792781867220},
 "version":"0.3.0",
 "volume_profile":{"row_mode":"rows","rows":128,"bin_size":0.5,"va_pct":70.0,
                   "input":"tv","audit":"/vp?window=PW"}}
```

The Node 1 agent should keep the full recorded set as the v1 fixture
(`ci/live/ws-sample.jsonl`, `candles.json`, `levels.json`, `tick-volume.json`,
`status.json`) and re-record it after deploying v1.

---

## 12. Contract test matrix (to implement on `Node1`)

Rust tests drive the router with the existing `tower::ServiceExt::oneshot`
dev-dependency and the aggregator with a fake clock; `ci/verify_ws.py` covers
the live process in `ci/smoke.sh`.

### Ingest — malformed / out-of-order (producer side)

| Case | Expected |
|---|---|
| tick with non-finite or `<= 0` price | dropped; feed counter `rejected` incremented; no frame |
| tick older than the current bucket (late tick) | dropped (exists: `sifting_ws` late-tick test) — aggregator never rolls backwards |
| tick more than 5 s in the future | dropped + logged (clock skew guard) |
| tick `ts` in seconds (< 1e12) | dropped + logged (unit guard) |
| REST seed with unaligned `time` | rejected row (today `verify_candles.py` asserts the grid) |
| REST seed non-monotonic / duplicate `time` | merged by `time`, ascending (exists: `BTreeMap` merge in tail fill) |
| REST seed includes the forming bucket | dropped: last `/candles` bar has `time + 900000 <= now` |
| closed candle when the cache channel is full | must not be lost (`try_send` today can drop it silently — switch to `send().await` or widen + alarm) |

### Publication — ordering, identity, closedness

| Case | Expected |
|---|---|
| per connection, `candle.time` non-decreasing | assert over a replay + live sequence |
| `closed == true` exactly once per bucket | count per `time` |
| `closed(T)` precedes first frame of `T + 15m` | positional assert |
| replay burst ascending; last 15 candles all `closed == true` | |
| `/candles` ascending, unique by `time`, all closed | |
| `seq` strictly increasing across topics on live frames; absent on replay frames | |
| `gap` frame emitted when a session lags | drive a slow subscriber past the 4,096 buffer |
| `boot_id` constant within a process, present in `/status` and `status` frames | |
| `symbol`/`interval` present on every `candle` and `tick_volume` frame | |
| levels identity (`window,start,end`) with newer `timestamp` replaces older | |

### Stale data / health

| Case | Expected |
|---|---|
| no ticks for 61 s while `market_open` | `freshness.state: stale`, a `status` frame is broadcast on the transition |
| upstream socket closed | `offline`, then `live` again after reconnect without losing the forming bucket's count (exists: reconnect-safe `TickState`) |
| Friday 17:00 NY → Sunday 18:00 NY with a fake clock | `closed_market`, no stale alarm |
| `edge_lag_minutes` grows while no bucket closes | |

### Boundary (negative)

| Case | Expected |
|---|---|
| inbound `trades`, `trade_intent`, `execution_hello`, `execution_report`, `order`, `bubbles` frames | do not parse / ignored; **never** rebroadcast to any subscriber (extend the existing `trades` + `bubbles` tests) |
| any `POST`/`PUT`/`PATCH`/`DELETE` on any route | 405/404 (exists) |
| published frames contain none of: `side`, `stop_loss`, `take_profit`, `lots`, `risk_usd`, `action`, `venue` | key scan over a recorded session |
| `ci/check_node1_boundary.py` (inverted, audit §2.2) | green |

---

## 13. Open decisions for the owners

1. **Write-back auth** (§2): token in a `hello` frame vs a separate `/ws/ai`
   path for Node 3's inbound AI frames vs leave as is. (Node 1 + Node 3.)
2. **`seq` placement**: envelope (proposed, consumers ignore it today) vs
   inside `data`. (Node 1 + Node 3.)
3. **Replay size**: keep replaying the whole tick-volume store (~270 KB) on
   every subscribe, or cap it to the last N bars and point to
   `GET /tick-volume` for the rest. (Node 1 + Node 2.)
4. **Stale thresholds**: 60 s `stale` / `market_open` calendar as proposed, or
   different values; whether holidays need a calendar beyond the weekly
   session. (Node 1 + Node 3.)
5. **Duplicate upstream ticks** (same `ts` and price): count as a tick (matches
   SiftingIO's `v` definition) or de-duplicate. (Node 1.)
6. **Node 2 re-sourcing**: which Node 3 / Node 4 resources replace the removed
   `strategy` / `demo` panels (Node 3's `docs/BRANCH_ARCHITECTURE.md` already
   proposes `/scanning`, `/signals`, `/open-trades`, `/account`). (Node 2 +
   Node 3 + Node 4.)
