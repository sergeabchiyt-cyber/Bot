# Node 1 boundary audit — "market data only"

**Handoff response written from the Node 4 tree. Read-only audit; no Node 1 code
is changed in this branch.**

| | |
|---|---|
| Audited | `origin/Node1` @ `d007137` (crate `xauusd-engine` 0.4.0) |
| Consumers audited | `origin/Node3` @ `fff6bfd` (`node3-strategy`), `origin/Node2` @ `44f84c8` (dashboard), `Node-4` @ `c89c724` (this tree) |
| Date | 2026-10-23 |
| Companion | [`NODE1_FEED_CONTRACT_v1.md`](NODE1_FEED_CONTRACT_v1.md) — the versioned feed contract draft to agree before implementation |

---

## 0. Where the Node 1 code actually is (correction to the handoff note)

The handoff says the current checkout "does not include Node 1 source". That is
true of this working tree, but **not of this repository**:
`sergeabchiyt-cyber/Bot` carries all four nodes as **disjoint-history branches**
— `Node1`, `Node2`, `Node3`, `Node-4` (no common ancestor between `Node1` and
`Node-4`). The Node 1 source is readable from any clone without switching
branches:

```bash
git fetch --depth=1 origin '+refs/heads/Node1:refs/remotes/origin/Node1'
git show origin/Node1:src/main.rs
```

Consequences for this handoff:

* The refactor (work items 2, 4, 5) **must be executed in a session based on
  the `Node1` branch**. This branch is the Node 4 execution service; its README
  scope guard forbids market-data code here, and Node 1's own
  `ci/check_node1_boundary.py` forbids execution code there. Neither tree may
  absorb the other.
* Everything below was established from the real code, not inferred, so the
  Node 1 agent can start at work item 2 with this inventory instead of
  repeating work item 1.
* **Deployment lags the branch.** The recorded live probe
  (`origin/Node1:ci/live/status.json`) shows the deployed engine at
  `engine-southeastasia-sng-main.onrender.com` is **v0.3.0** and still lists the
  removed exchange order-flow feeds; `ci/live/strategy.code` and
  `ci/live/demo.code` are both `404`. The strategy/demo code at the branch head
  has **never been deployed**, so removing it changes no running service — only
  the branch head and the expectations baked into Node 2 and Node 1's CI.

---

## 1. Inventory of `Node1@d007137`

### 1.1 Runtime modules

| Module | Lines | What it does (verified) | Verdict |
|---|---:|---|---|
| `src/main.rs` | 701 | Wiring: cold-start seed, live stream fan-in, session-close level refresh, calendar loop, econ monitor, **strategy + demo tasks**, HTTP/WS server | **KEEP**, minus the strategy/demo blocks (see §2) |
| `src/config.rs` | 330 | Env parsing (~50 variables incl. `PORT` / `CORS_ALLOWED_ORIGIN`, all optional) | **KEEP**, minus the 17 `STRATEGY_*` / `DEMO_*` variables |
| `src/sifting_rest.rs` | 497 | SiftingIO REST history: exactly 2,000 × 15m seed, tail gap-fill when the seed lags the stream, 1m/5m history for profile windows | **KEEP** (quote/candle acquisition) |
| `src/sifting_ws.rs` | 648 | SiftingIO live tick stream → 15m chart / 1m profile aggregators; late ticks for closed buckets dropped; bucket state survives reconnects; publishes `candle` + `tick_volume`; feed status | **KEEP** (quote/candle acquisition) |
| `src/tick_volume.rs` | 114 | `TickVolumeStore`: closed + in-progress tick-volume bars, bounded to 2,000 | **KEEP** (market-data persistence, in-memory) |
| `src/volume_profile.rs` + `histogram/session/swing/timeframe/weekly.rs` | 2,616 | PW / PS / CW / SWING profiles, POC / VAH / VAL, TradingView-parity histogram, New-York session boundaries, 18:00 NY roll | **KEEP** (derived market data — no side, stop or target) |
| `src/calendar.rs` | 402 | Economic calendar (ForexFactory JSON, MCP-browser fallback) | **KEEP** (data feed) |
| `src/econ_monitor.rs` | 588 | Event-window audio capture (`yt-dlp` + `ffmpeg`) → `audio_chunk` frames for Node 3 | **KEEP — review** (data acquisition, not strategy; heavy external deps) |
| `src/mcp_client.rs` | 309 | MCP *browser* client for calendar + live-coverage discovery (`MCP_BROWSER_URL`; **not** `MCP_CHELSEA_URL`) | **KEEP — review** (no broker MCP) |
| `src/ai_cache.rs` | 180 | Caches Node 3's `transcript` / `sentiment` / `health` / `prediction` for `GET /ai` | **KEEP — review** (relay of Node 3 output; makes no decision) |
| `src/status.rs` | 86 | `FeedStatus`: per-feed `state` / `last_msg` / `msgs`, broadcast as `status` frames on change | **KEEP** (feed health) |
| `src/types.rs` | 328 | Wire types: `VpCandle`, `TickVolumeBar`, `VpLevels`, `ProfileMeta`, `WsFrame` | **KEEP**, minus `WsFrame::Strategy` / `WsFrame::Demo` |
| `src/ws_server.rs` | 1,050 | REST + WS router, CORS allow-list, replay-on-subscribe, topic fan-out, inbound AI frames | **KEEP**, minus `/strategy`, `/demo`, their topics and `/status` blocks |
| `src/strategy.rs` | 825 | `StrategyEngine`: EMA 9/21 trend, momentum, tick pressure, value-area position, news blackout → `action` (buy/sell/flat), `confidence`, ATR×mult `stop_loss`, `take_profit`, `reward_risk`, `lots`, `risk_usd` (`size_position` with `risk_pct`, lot step, leverage, margin cap) | **MOVE OUT** — decides side, stop, target and size |
| `src/demo_engine.rs` | 757 | `DemoEngine`: paper account — fills at bid/ask with spread, margin, exits at stop / target / signal flip / stop-out, cooldown, trade history | **MOVE OUT** — a trade state machine |

### 1.2 HTTP endpoints (all `GET`; no write route exists — pinned by test `no_route_accepts_a_broker_write`)

| Route | Serves | Verdict |
|---|---|---|
| `/health` | `ok` | KEEP |
| `/status` | feed states, version, candle-cache edge (`bars`, `first_bar`, `last_bar`, `edge_lag_minutes`), VP model, **`strategy` + `demo` blocks** | KEEP, remove the two blocks, add `contract_version` + `freshness` (contract) |
| `/levels` | cached `VpLevels[]` (PW, PS, CW, SWING_BULL/BEAR) | KEEP |
| `/vp` | profile audit: `?window=`, `?rows=`, `?start=&end=` (400 malformed / 404 not computed) | KEEP |
| `/candles` | 2,000 closed 15m bars, ascending, unique by `time` | KEEP |
| `/tick-volume` | tick-volume bars, `Cache-Control: no-store` | KEEP |
| `/calendar` | calendar snapshot | KEEP |
| `/ai` | today's Node 3 transcript/sentiment digest | KEEP — review |
| `/strategy` | last `StrategyDecision` | **REMOVE** |
| `/demo` | demo account snapshot | **REMOVE** |
| `/ws` | WebSocket feed | KEEP |

### 1.3 WebSocket topics

| Topic | Direction | Verdict |
|---|---|---|
| `levels`, `candle`, `tick_volume`, `calendar`, `status`, `heartbeat` | Node 1 → clients | KEEP (the market contract) |
| `strategy`, `demo` | Node 1 → clients | **REMOVE** |
| `subscribe` | client → Node 1 | KEEP |
| `audio_chunk`, `learn`, `sentiment_req`, `predict_req` | relayed Node 1 → Node 3 | KEEP — review (separate Node 1 ⇄ Node 3 AI contract) |
| `transcript`, `sentiment`, `health`, `prediction` | Node 3 → Node 1 (cached for `/ai`, fanned out) | KEEP — review |
| `trades`, `bubbles` | — | already rejected (negative tests exist) |

### 1.4 Storage

**Nothing is written to disk** (no `std::fs`, no database anywhere in `src/`).
"Persistence" is in-memory and rebuilt at boot:

* candle cache: 2,000 × 15m **closed** bars, upsert by `time`, sorted, bounded;
* tick-volume store: up to 2,000 bars (closed + the forming one);
* levels vector, calendar JSON, AI cache.

Cold start: REST seed → tail gap-fill if the seed lags the stream by > 30 min
→ live stream. `SEED_SYNTHETIC_CANDLES=1` substitutes a deterministic seed for
air-gapped CI only (`source: "synthetic"`). There is **no replay log**: nothing
older than the in-memory window can be replayed, and sequence state does not
survive a restart (see the contract, §8).

### 1.5 Outbound dependencies

SiftingIO REST + WS (the only price source), ForexFactory calendar, MCP browser
(`http://localhost:3001` default), `yt-dlp` / `ffmpeg` subprocesses. **No broker,
no Node 4, no Node 3 outbound connection** (Node 3 dials Node 1).

### 1.6 Node 1 CI today

`cargo fmt/check/clippy/test`, a live calendar test, **`ci/check_node1_boundary.py`**,
release build, `ci/smoke.sh` (boots the binary with the synthetic seed and runs
`ci/verify_{candles,levels,vp,tick_volume,calendar,ws,demo,econ_audio}.py`),
proof log written back to the branch.

---

## 2. What violates the mandate — and what does not

### 2.1 Violations (remove from Node 1)

| Item | Location |
|---|---|
| Strategy evaluator (side / SL / TP / RR / lots) | `src/strategy.rs` |
| Paper trade state machine | `src/demo_engine.rs` |
| Module declarations, imports | `src/main.rs` lines 1–30 |
| `nearest_high_impact_event()` (strategy-only helper) | `src/main.rs` |
| Strategy/demo construction, price-marking task, evaluation timer | `src/main.rs` "Strategy evaluator + demo (paper) execution" block |
| `AppState.strategy`, `AppState.demo` | `src/ws_server.rs` |
| Routes `/strategy`, `/demo`; replay arms and fan-out arms for the two topics; `/status.strategy`, `/status.demo` | `src/ws_server.rs` |
| `WsFrame::Strategy`, `WsFrame::Demo` + `strategy_and_demo_frames_round_trip` test | `src/types.rs` |
| 17 env vars: `STRATEGY_ENABLED/EVAL_SECS/MIN_CONFIDENCE/RISK_PCT/ATR_PERIOD/ATR_STOP_MULT/REWARD_RISK/EVENT_BLACKOUT_SECS` (8), `DEMO_ENABLED/START_BALANCE/LEVERAGE/SPREAD/MIN_LOTS/MAX_LOTS/LOT_STEP/STOP_OUT_PCT/COOLDOWN_SECS` (9) | `src/config.rs`, `README.md` env section |
| Tests `strategy_and_demo_routes_serve_the_plan_and_the_print` and the strategy/demo fixtures in router tests | `src/ws_server.rs` |
| README §"Strategy and demo trading (order flow removed)", topology lines mentioning strategy/demo | `README.md` |
| `FRONTEND_NEXT.md` (a Node 2 handoff to build strategy + demo panels) | root |
| `ci/verify_demo.py`; strategy/demo assertions in `ci/verify_ws.py` (lines ~223–235); matching steps in `ci/smoke.sh` and `ci/live-probe.sh`; `ci/live/strategy.*`, `ci/live/demo.*` | `ci/` |

### 2.2 The CI guard currently enforces the *opposite* decision

`ci/check_node1_boundary.py` was written for an earlier boundary ("market data
+ offline strategy evaluator + paper demo, no broker"). It **positively
requires** `pub struct StrategyEngine`, `pub struct DemoEngine`, routes
`/strategy` and `/demo`, topics `strategy` and `demo`, and pins the
`DEMO_CONTRACT` defaults (1:1000, 0.10, 0.01–1.00 lots, 10,000 balance). It
must be inverted **in the same change** that removes the code, or Node 1 CI goes
red. The inverted guard should:

* move `StrategyEngine`, `DemoEngine`, `/strategy`, `/demo`, `"strategy"`,
  `"demo"`, `STRATEGY_*`, `DEMO_*`, `stop_loss`, `take_profit`, `lots`,
  `risk_pct`, `Action::Buy|Sell` into `FORBIDDEN`;
* delete `DEMO_CONTRACT`, `OFFLINE_MODULES` and the two positive struct checks;
* keep every market positive check (`MARKET_ROUTES_REQUIRED` minus the two
  routes, `MARKET_TOPICS_REQUIRED` minus the two topics, Dockerfile builds
  `xauusd-engine`, no execution subtree);
* add `src/strategy.rs` and `src/demo_engine.rs` to the "must not exist" list.

### 2.3 Not violations — do **not** remove under the guise of "removing the engine"

* Volume-profile levels (POC/VAH/VAL, PW/PS/CW, swing profiles). `direction:
  bullish|bearish` on swing profiles is a **structural label of the anchor**
  (which extreme the profile is anchored to), not a trade side. Keep it; the
  contract documents it as such.
* Tick volume (tick-rule up/down split is an activity gauge, not a signal).
* Calendar, econ audio capture, MCP browser client, `/ai` relay.
* Feed status and the `/status` freshness fields.
* Synthetic seed for CI.

### 2.4 Already absent (confirmed, nothing to do)

Broker adapters, `execution*.rs`, `node3-execution`, MT5 bridge, `trades`
topic (negative test), write routes (negative test), `DERIV_*`,
`MCP_CHELSEA_URL`, `EXECUTION_VENUE`, order-flow adapters (`binance_ws`,
`multi_exchange`, `order_flow`) and the `bubbles` frame.

---

## 3. Where the removed pieces go

| Piece | Destination | Why |
|---|---|---|
| `src/strategy.rs` | Hand to the **Node 3 owner by reference** (`git show d007137:src/strategy.rs`) | Node 3 is the strategy owner and already runs a *different* model (break/retest of PW/PS/CW levels, `node3-strategy/src/strategy.rs` + `risk.rs`). The EMA/ATR evaluator is a second, unrelated strategy — do not merge it blindly; the Node 3 owner accepts, adapts or drops it. |
| `src/demo_engine.rs` | Hand to the **Node 4 owner by reference** (`git show d007137:src/demo_engine.rs`) | Simulated fills are an execution concern. Node 4 already has `EXECUTION_VENUE=none` (dry-run / signal mode) and a durable ledger; a paper venue, if ever wanted, is a Node 4 venue-adapter decision. Node 4 does **not** plan to adopt it. |
| `FRONTEND_NEXT.md` | Node 2 owner, superseded | Its strategy/demo panels must be re-sourced from Node 3 / Node 4 (see §4). |

Nothing is lost by deletion: both files stay in the `Node1` branch history.

---

## 4. Consumers of Node 1 (identified from their code, not from docs)

| Consumer | Transport & topics | Impact of removing strategy/demo | Contract issues found |
|---|---|---|---|
| **Node 3** `node3-strategy` @ `fff6bfd` | Dials `NODE1_WS_URL` (default `wss://engine-southeastasia-sng-main.onrender.com/ws`), sends `subscribe {topics:["candle","levels"]}`, echoes `heartbeat`, reconnects after 5 s, skips unparseable frames | **None** — never subscribes `strategy`/`demo`, never calls the routes | (a) `evaluate_candle_with_diagnostics` **push_backs every `candle` frame** into its 30-bar history — it does not upsert by `time`, so the forming candle (re-broadcast on every tick, 188 of 203 candle frames in the recorded sample) is duplicated into ATR and break/retest logic; the Node 1 README's claim that "duplicate updates replace that bar" is not what Node 3 does. (b) No gap detection. (c) Does not subscribe to `status`, so it has no feed-freshness signal; `expires_at` on the intent is its only staleness guard. (d) `source_candle_time` is taken from whatever candle triggered — possibly a forming one. |
| **Node 2** dashboard @ `44f84c8` | REST `/candles /tick-volume /levels /calendar /strategy /demo /status`; WS topics `levels, candle, tick_volume, strategy, demo, calendar, status`; heartbeat 30 s, silence timeout 75 s, stale 90 s, REST poll fallback 5 s | **Visible regression on the branch head**: the strategy panel and demo account header lose their source (`404` + no frames). Node 2 renders unknowns rather than inventing values and already lives with these `404`s against the deployed 0.3.0, so it degrades, not crashes — but the Node 2 owner must repoint strategy views to Node 3 (`/scanning`, `/signals`, `diagnostics`) and account/trade views to Node 4 (`/open-trades`, `/account`), exactly as Node 3's `docs/BRANCH_ARCHITECTURE.md` prescribes, and drop the two topics. | Consumes forming candles for the chart — correct use; needs the `closed` flag only to label the last bar. |
| **Node 4** (this tree) @ `c89c724` | **Not a consumer.** No `NODE1_*` variable, no candle/levels code (`health.rs` even asserts `/levels` and `/candles` do not exist here). | None | The only Node 1-derived datum is `trade_intent.source_candle_time` (`i64` epoch ms, `#[serde(default)]`), carried as **provenance only**: written to the ledger, never validated, never used in a decision. Node 4's freshness guards are the intent's `expires_at` (Node 3) and the venue's own quote age (`MT5_MAX_QUOTE_AGE_MS`, terminal side). |
| **Node 1 CI probes** `ci/verify_*.py`, `ci/smoke.sh`, `ci/live-probe.sh` | HTTP + raw WS | `verify_ws.py` and `verify_demo.py` fail until their strategy/demo expectations are removed | They are the existing contract tests — extend rather than replace (contract §12). |

---

## 5. Node 4 owner position on the feed contract (work item 3)

1. Node 4 has **no requirement** on the feed's transport, auth, heartbeat,
   sequencing or replay — it never connects to Node 1 and will not add any
   market-data ingestion to satisfy the contract (README scope guard).
2. Node 4 asks for exactly one thing, via Node 3: `source_candle_time` must
   keep meaning *"bucket-open time, epoch ms UTC, of the **closed** candle that
   triggered the intent"*, so ledger provenance stays interpretable. Once v1
   adds `closed`, Node 3 should populate it only from closed candles.
3. Node 4 acknowledges the contract as a non-party and has no veto on it; the
   agreeing parties are the Node 1 (producer), Node 3 (machine consumer) and
   Node 2 (browser consumer) owners.

---

## 6. Execution plan for the `Node1` branch (for the Node 1 agent)

Order matters: the contract first, then the removal, then tests — all in one
PR against `Node1` so CI never has to pass through a half-state.

1. **Agree the contract** (`NODE1_FEED_CONTRACT_v1.md`): resolve its §13 open
   decisions with the Node 3 and Node 2 owners; bump `contract_version` to
   `1.0` only when they sign off.
2. **Remove** everything in §2.1. Keep the diff mechanical: delete the two
   modules, drop the enum variants, routes, state fields, status blocks,
   config fields and docs. Do not touch `sifting_*`, `volume_profile*`,
   `tick_volume`, `calendar`, `status`, `econ_monitor`, `mcp_client`,
   `ai_cache` beyond import clean-up.
3. **Invert `ci/check_node1_boundary.py`** as in §2.2; delete
   `ci/verify_demo.py`; strip the strategy/demo asserts from
   `ci/verify_ws.py`, `ci/smoke.sh`, `ci/live-probe.sh`; remove
   `ci/live/strategy.*` and `ci/live/demo.*`.
4. **Implement the additive contract fields** (`closed`, `symbol`, `interval`,
   envelope `seq`, `gap` frame, `freshness`, `contract_version`) — all
   additive, consumers verified to ignore unknown fields (Node 3: serde default
   behaviour, no `deny_unknown_fields`; Node 2: JS).
5. **Contract tests** per contract §12 (Rust `tests/feed_contract.rs` using the
   existing `tower::ServiceExt::oneshot` dev-dependency, plus `ci/verify_ws.py`
   extensions) and the recorded sample set under a versioned path.
6. **Tell the consumers**: Node 3 owner — upsert by `time`, honour `closed`,
   subscribe `status`, gate intents on `freshness`; Node 2 owner — drop
   `strategy`/`demo`, re-source those panels from Node 3 / Node 4.
7. Deploy; refresh `ci/live/*` with `ci/live-probe.sh`; confirm `/status`
   reports 0.5.x, `contract_version: "1.0"`, no `strategy`/`demo` keys.

---

## 7. Acceptance criteria → evidence

| Criterion | Evidence to produce on `Node1` |
|---|---|
| Node 1 still ingests and serves market data with all strategy/execution code removed | `ci/smoke.sh` green: `/candles` 2,000 aligned bars, `/levels` PW/PS/CW, `/tick-volume`, `/calendar`, `/ws` replay + live frames; unit tests for `sifting_ws` aggregation unchanged |
| Feed contract and health/freshness signals documented and tested | `NODE1_FEED_CONTRACT_v1.md` merged into the Node 1 tree; contract tests of §12 green; `/status.freshness` and `contract_version` present in the refreshed `ci/live/status.json` |
| No Node 1 code can submit an order or decide side/stop/target | inverted `check_node1_boundary.py` green; negative tests for `trade_intent` / `execution_hello` / `trades` frames; `no_route_accepts_a_broker_write`; `grep -rE 'stop_loss\|take_profit\|lots\|Action::' src/` empty |
| Existing consumers identified and not silently broken | §4 of this document; Node 3 unaffected by construction; Node 2 owner notified with the re-sourcing list; `ci/verify_ws.py` asserts the market topics still replay |
