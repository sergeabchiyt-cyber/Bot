# AGENT BRIEF — Node 1: "market data only + calendar"

**Paste this into a separate Arena session whose working branch is `Node1`.**
**Language:** Rust. **Service:** `xauusd-engine` (branch head `d007137`, v0.4.0).
**Master plan:** `PLAN.md` (sections 2, 3, 4, 5-Agent-1, 6, 7).
**This brief is self-contained; you do not need the other branches to start.**

---

## 0. Decisions already made by the owner (do not re-litigate)

| ID | Decision |
|---|---|
| D1 | **Node 3 decides → Node 4 executes.** Node 3 owns all VP maths + break/retest + ATR SL/TP/RR projection and emits `trade_intent`; Node 4 owns risk policy, sizing, demo engine and reports. |
| D2 | **Four disjoint branches** (`Node1`, `Node2`, `Node3`, `Node-4`) — one service per branch, one Arena session per branch. Never copy another service's `src/` into your branch. |
| D3 | **DST-aware `America/New_York` 18:00** is the week/session anchor (UTC−4 EDT / UTC−5 EST). |
| D4 | **CW = current engine behaviour** (first snapshot after Monday's 18:00 close, refreshed at each daily close). Node 3 owns this; you do not. |
| D5 | **PS = last closed 18:00→18:00 session** (at week open: Thu 18:00 open → Fri 18:00 close), PoC only. Node 3 owns this. |
| D6 | **No 10.5K volume-spike filter** (break + retest alone confirm). Node 3 owns this. |
| D7 | **Node 2 is frontend only**; Node 4 ships no dashboard. |
| D8 | No screenshot needed; drawing styling follows `js/levels.js` colour map. |
| D9 | **1 pip = $0.01** (SL 200–300 pips = $2–3, TP 600–800 pips = $6–8). |

**Consequence for you: Node 1 must contain NO volume-profile maths, NO strategy,
NO demo engine, NO SL/TP/RR, NO intents and NO broker anything.** It is a
versioned, read-only market-data + calendar feed.

---

## 1. What is in your branch today (verified inventory)

`src/` (20 modules, ~8k lines): `main.rs` (701), `config.rs` (330),
`sifting_rest.rs` (497), `sifting_ws.rs` (648), `tick_volume.rs` (114),
`volume_profile.rs` + `volume_profile/{histogram,session,swing,timeframe,weekly}.rs`
(2,616), `calendar.rs` (402), `econ_monitor.rs` (588), `mcp_client.rs` (309),
`ai_cache.rs` (180), `status.rs` (86), `types.rs` (328), `ws_server.rs` (1,050),
`strategy.rs` (825), `demo_engine.rs` (757).

Endpoints today: `/health`, `/status`, `/levels`, `/vp`, `/candles`,
`/tick-volume`, `/calendar`, `/ai`, `/strategy`, `/demo`, `/ws`.
WS topics today: `candle`, `tick_volume`, `levels`, `calendar`, `status`,
`heartbeat`, `strategy`, `demo`, plus the Node 3 AI relay topics
(`audio_chunk`, `learn`, `transcript`, `sentiment`, `health`, `prediction`).

Storage: in-memory only (2,000 × 15m closed bars, up to 2,000 tick-volume bars,
levels vector, calendar JSON, AI cache). No disk, no replay log, no DB.
Upstream: SiftingIO REST + WS (only price source), ForexFactory calendar,
optional MCP browser, `yt-dlp`/`ffmpeg` for econ audio. **No broker, no Node 3/4
outbound connection.**

The deployed service is still **v0.3.0**; `ci/live/strategy.code` and
`ci/live/demo.code` are 404, so the branch-head strategy/demo was never
deployed. Removing it changes no running service.

---

## 2. Contract C1 v2 — Node 1 → consumers (you are the producer)

This is the signed version of the draft currently in the Node-4 branch at
`docs/node1/NODE1_FEED_CONTRACT_v1.md` (§8–12). Copy that document into
`contracts/NODE1_FEED_CONTRACT_v2.md` on **all four branches** byte-identically
(Phase 0, coordinated with the other agents).

**REST (all GET; no POST/PUT/PATCH/DELETE anywhere):**

| Route | Purpose |
|---|---|
| `GET /health` | `ok` — process liveness only |
| `GET /status` | feed states, version, candle-cache edge, **`contract_version`**, **`boot_id`**, **`freshness`**, `market_open` |
| `GET /candles?interval=1m\|15m\|1h` | closed bars, ascending, unique by `time`, boundary-aligned |
| `GET /tick-volume` | tick-volume bars, `Cache-Control: no-store` |
| `GET /calendar` | calendar snapshot |
| `GET /ws` | WebSocket upgrade |

**WS topics (Node 1 → clients):** `candle`, `tick_volume`, `calendar`, `status`,
`heartbeat`. Client → Node 1: `subscribe` with a topic list, `heartbeat`.

**Envelope rules:**
- `contract_version: "2.0"` and `boot_id` (constant within a process) in
  `/status` and every `status` frame.
- **Live** frames carry a strictly increasing `seq`; **replay** frames carry
  `replay: true` and NO `seq`. A replay burst is ascending.
- Emit `{"type":"gap","data":{"dropped":N}}` when a slow subscriber overruns the
  broadcast buffer (4,096).
- Every `candle` carries `symbol: "XAUUSD"`, `interval` (`1m`/`15m`/`1h`),
  `closed`, `time` (bucket-open epoch ms), valid OHLCV, `source`.
  `closed == true` is emitted **exactly once** per bucket, before the first frame
  of the next bucket.
- `tick_volume` carries `symbol`/`interval` too; `ticks == candle.volume`.
- `freshness: {feed, last_tick_ms, age_ms, state, market_open}` where
  `state ∈ {live, stale, offline, closed_market}`: `live` when connected and
  `age_ms ≤ 60_000` while `market_open`; `stale` when `age_ms > 60_000`;
  `offline` when never/ disconnected; `closed_market` outside the XAUUSD session
  (Fri 17:00 → Sun 18:00 New York). **Broadcast a `status` frame on every state
  transition**, not only on feed-state changes.
- 30 s heartbeat, both directions allowed.
- Inbound frames that look like execution (`trades`, `trade_intent`,
  `execution_hello`, `execution_report`, `order`, `bubbles`) must be ignored and
  **never** rebroadcast.

**Freshness + ordering test matrix** (implement in Rust with `tower::ServiceExt::oneshot`
and a fake clock, plus `ci/verify_ws.py` against the live process):
non-finite/≤0 price ticks dropped; late ticks dropped; ticks >5 s in the future
dropped; `ts` in seconds dropped; unaligned REST seed rows rejected; duplicate
seed `time` merged ascending; forming bucket excluded from `/candles`; full
cache channel never drops a closed candle; `seq` strictly increasing across
topics; `gap` frame on overrun; `boot_id` stable; levels identity
`(window,start,end)` replaced by newer `timestamp`; no write route exists;
published frames contain none of `side`, `stop_loss`, `take_profit`, `lots`,
`risk_usd`, `action`, `venue`.

Keep the recorded fixtures `ci/live/ws-sample.jsonl`, `candles.json`,
`levels.json`, `tick-volume.json`, `status.json`, `vp-{PW,PS,CW}.json` as the
v2 golden set (they are the FRVP parity baseline for Node 3).

---

## 3. Work items

1. **Implement C1 v2** exactly as in §2 above. Add `contract_version`, `boot_id`,
   `seq`, `replay`, `gap`, `symbol`, `interval`, `closed`, `freshness` to
   `src/types.rs` / `src/ws_server.rs` / `src/status.rs`.
2. **Multi-timeframe candles.** Today only 15m closed bars are served (plus an
   internal 1m profile aggregator). Expose `GET /candles?interval=1m|15m|1h`
   and matching WS `candle` frames. All boundary-aligned, ascending, unique by
   `time`; the forming bar explicitly `closed: false`. Node 3 needs **1H for the
   drawing boundaries** and **1m/5m for FRVP precision**. Keep the 2,000-bar
   seed, tail gap-fill, reconnect-safe bucket state and the synthetic-seed
   option for air-gapped CI.
3. **Keep `src/calendar.rs`** (ForexFactory weekly JSON + MCP-browser fallback)
   and publish `/calendar` + WS `calendar`. Add `market_open` / session state to
   `/status` and `status` frames so weekend false alarms are avoided.
4. **Delete the strategy + demo engine:**
   - `src/strategy.rs`, `src/demo_engine.rs`
   - module declarations/imports in `src/main.rs` (lines 1–30)
   - `nearest_high_impact_event()` and the entire "Strategy evaluator + demo
     (paper) execution" block in `src/main.rs` (~548–700): construction,
     price-marking task, evaluation timer
   - `AppState.strategy`, `AppState.demo`, `/strategy`, `/demo`, their replay
     arms and fan-out arms, `/status.strategy`, `/status.demo` in `ws_server.rs`
   - `WsFrame::Strategy`, `WsFrame::Demo` and their round-trip test in `types.rs`
   - the 17 `STRATEGY_*` / `DEMO_*` env vars in `config.rs` + README env section
   - `FRONTEND_NEXT.md` at branch root
   - `ci/verify_demo.py`; strategy/demo assertions in `ci/verify_ws.py`
     (~lines 223–235); matching steps in `ci/smoke.sh` and `ci/live-probe.sh`;
     `ci/live/strategy.*` and `ci/live/demo.*` fixtures
   - the strategy/demo sections of `README.md`
5. **Hand off VP maths to Node 3.** `src/volume_profile.rs` +
   `src/volume_profile/{histogram,session,swing,timeframe,weekly}.rs` and
   `ProfileMeta` in `types.rs` are the source for Agent 3's
   `node3-strategy/src/vp/`. **Do not delete them until Node 3 has ported
   them**; export them as a standalone module/fixture pack (see AGENT-NODE3.md
   §3 item 1). During migration, keep `/levels` and `/vp` as a **deprecated,
   feature-flagged read-through proxy** to Node 3 (`LEVELS_PROXY_URL`), then
   remove both routes, the VP model section of `/status`, and all VP code in
   the Phase 5 cutover. **After cutover no VP computation remains in Node 1.**
   (Coordinate the cutover with the other agents.)
6. **Triage the AI/audio relay** (`econ_monitor.rs`, `mcp_client.rs`,
   `ai_cache.rs`, WS topics `audio_chunk`/`learn`/`transcript`/`sentiment`/
   `health`/`prediction`): these are Node 3 inputs, not market data. Move them
   to Node 3 as an optional module, or delete them if unused. Keep `/ai` only
   if Node 3 still writes to it; otherwise remove.
7. **Rewrite `ci/check_node1_boundary.py`** so it enforces the NEW boundary
   (today it enforces the OLD one): ban `mod strategy`, `mod demo_engine`,
   `volume_profile`, `SL_*`/`TP_*`/`RR_*`, `trade_intent`, `intent_id`,
   `TradeIntent`, any broker/venue identifier, and any write route. Keep the
   existing `no_route_accepts_a_broker_write` test. Remember the guard file is
   the denylist and must skip itself when scanning.
8. **CI/tests:** `cargo fmt --check`, `cargo check --all-targets`,
   `cargo test --all-targets`, `cargo clippy --all-targets -- -D warnings`,
   release build, `ci/smoke.sh` with the synthetic seed, the boundary guard,
   the C1 contract matrix, and a re-recorded live probe.

---

## 4. Acceptance criteria

- No `strategy`, `demo`, `volume_profile`, `SL_`, `TP_`, `RR_`, `intent` or
  broker identifier in `src/` after cutover; `check_node1_boundary.py` green.
- `GET /candles?interval=1h` returns aligned 1H bars; `/candles` for every
  interval is ascending, unique by `time`, all closed.
- `/status` reports `contract_version: "2.0"`, `boot_id` and `freshness`; a
  `status` frame is broadcast on every freshness-state transition.
- **Node 3 can recompute PW/PS/CW from Node 1 candles alone** and match the
  recorded golden fixtures within one histogram row.
- No write route exists; no published frame carries a side/stop/target/size.
- `cargo fmt/check/test/clippy` + `ci/smoke.sh` + live probe all green.

**Depends on:** Phase 0 (signed C1 schema in `contracts/`).
**Blocks:** Agent 3's integration test.
**Must not:** compute a level, choose a side, size a trade, or add a write route.
