# AGENT BRIEF — Node 3: "Volume Profile Calculations" + strategy engine

**Paste this into a separate Arena session whose working branch is `Node3`.**
**Language:** Rust. **Service:** `xauusd-node3-strategy` v2.0.0
(branch head `fff6bfd`, crate at `node3-strategy/`).
**Master plan:** `PLAN.md` (sections 2, 3, 4, 5-Agent-3, 6, 7).
**This brief is self-contained; you do not need the other branches to start.**

---

## 0. Decisions already made by the owner (do not re-litigate)

| ID | Decision |
|---|---|
| **D1** | **Node 3 decides → Node 4 executes.** You own ALL VP maths, the 1H→daily drawing payload, the break/retest scanner and ATR SL/TP/RR projection, and emit `trade_intent` over the private `/execution` WS. Node 4 owns risk policy, sizing, the demo engine and reports. |
| D2 | **Four disjoint branches** — one service per branch, one Arena session per branch. Never copy another service's `src/` into your branch. |
| D3 | **DST-aware `America/New_York` 18:00** is the week/session anchor (UTC−4 EDT / UTC−5 EST). |
| **D4** | **CW = current engine behaviour**: first snapshot after **Monday's** 18:00 close (week open → that close), refreshed at every later daily close, cleared at Sunday 18:00. Expose `CW_FIRST_PUBLISH_WEEKDAY=Mon` (set `Wed` for the owner's written Wednesday rule — one config value, no code change). |
| **D5** | **PS = last closed 18:00→18:00 session.** At the Sunday 18:00 week open that is **Thu 18:00 open → Fri 18:00 close**. Flat weekend filler buckets are walked over, never profiled. **Only PS PoC** is published (no VaH/VaL). D5b default: **freeze PS for the week** (`PS_MODE=week_open`); `rolling` is one config value away. |
| D6 | **No 10.5K volume-spike filter** — break + retest candle alone confirm. Keep `VOLUME_THRESHOLD=0` / optional and OFF by default; publish `volume_confirmed` only when enabled. |
| D7 | **Node 2 is frontend only**; it consumes your public C2 endpoints directly. No Node 4 code in it. |
| D8 | No screenshot needed; drawing styling follows Node 2's `js/levels.js` colour map. |
| D9 | **1 pip = $0.01** — SL 200–300 pips = $2–3, TP 600–800 pips = $6–8, RR 2–3. |

**Consequence for you: Node 3 becomes the VP engine.** Today it has NO VP maths
— it subscribes to Node 1's `levels` topic and trusts them. After this work it
computes every level itself from Node 1's raw candles.

---

## 1. What is in your branch today (verified inventory)

`node3-strategy/src/`: `main.rs`, `config.rs`, `diagnostics.rs`, `health.rs`,
`risk.rs`, `strategy.rs`, `types.rs`. `Cargo.toml` (tokio, tokio-tungstenite,
futures-util, serde, serde_json, chrono, tracing, tracing-subscriber, anyhow).
`docs/BRANCH_ARCHITECTURE.md`, `docs/NODE3_NODE4_PROTOCOL.md`,
`docs/MT5_NODE4_RUNBOOK.md`.

Today `main.rs` connects to `NODE1_WS_URL` and subscribes to **`candle` +
`levels`**. `strategy.rs` implements the break/retest state machine over levels
labelled `PW` (PoC/VaH/VaL), `PS` (PoC), `CW` (PoC/VaH/VaL) with
`PROXIMITY_DOLLARS = 0.50`, `INVALIDATION_DOLLARS = 2.00`, one trigger per
candle, sorted level IDs and a `volume_threshold` check. `risk.rs`
(`RiskProjector`) computes ATR-scaled `sl_min_pips=200`, `sl_max_pips=300`,
`tp_min_pips=600`, `tp_max_pips=800`, `rr_min=2.0`, `rr_max=3.0`, with
`sl_distance = sl_pips * 0.01`. `types.rs` defines `VpLevels`, `VpCandle`,
`ProjectedOrder`, `ScannedLevelSetup`, `ScanningSnapshot`, `TradeIntent`,
`ExecutionReport`, `SignalSnapshot`, `StrategyWorkDiagnostics`.

Public endpoints today: `GET /health`, `/diagnostics` (`/status`), `/scanning`,
`/signals`, `WS /ws`. Private: `WS /execution` (token `NODE4_SHARED_TOKEN`,
`execution_hello` → `execution_hello_ack`, `trade_intent` → `execution_report`).
Keep all of them.

---

## 2. The strategy you must implement (owner's spec, formalised)

**Instrument:** XAUUSD. Price source: SiftingIO spot (via Node 1). Volume is
tick count. **Week anchor: Sunday 18:00 `America/New_York`** (DST-aware).
Trading week = Sun 18:00 → Fri 18:00; each daily session = 18:00 → next 18:00.

### 2.1 The three FRVP windows

| Window | Range | Levels published | Lifetime |
|---|---|---|---|
| **PW** (previous week FRVP) | previous Sunday 18:00 NY → that Friday 18:00 NY close | **PW PoC, PW VaH, PW VaL** | computed once at Sunday 18:00, frozen all week |
| **PS** (previous session) | **last closed 18:00→18:00 session** — at week open: Thu 18:00 open → Fri 18:00 close | **PS PoC only** | frozen at week open (D5b default) |
| **CW** (current week VP) | week open (Sunday 18:00) → most recent completed 18:00 daily close | **CW PoC, CW VaH, CW VaL** | first snapshot after **Monday's** 18:00 close (D4=C), refreshed at each later daily close, cleared at Sunday 18:00 |

- **Histogram:** TradingView Fixed-Range-VP parity — 128 rows (or fixed $ bins),
  `va_pct = 70`, POC = max-volume row, VAH/VAL = value-area bounds. Keep the
  `ProfileMeta` audit (row mode, row height, rows, range high/low, input
  interval, input bars, total volume).
- **`sunday_open`** = open of the Sunday 18:00 1H candle. Publish it with the
  levels and as its own drawing.
- **Input resolution:** **1H candles for the time boundaries** (the owner's
  "draw on the 1H chart for time accuracy"), with 1m/5m bars for histogram
  precision where available.
- **Weekend filler:** SiftingIO emits bodyless bars (`open == close`) while the
  market is shut. A session counts only if at least one bar has a body. Skip
  Fri 18:00 → Sat 18:00 and Sat 18:00 → Sun 18:00 rather than profiling them —
  profiling them produced the old bug (PS PoC stuck on a $1.5 sliver at Friday's
  close while price traded $15 higher).

### 2.2 Drawings (what you publish, what Node 2 renders)

Compute on 1H and emit a **drawing payload**: for each level a horizontal ray
(`id`, `label`, `price`, `color`, `line_style`, `ray_from_time`, `start`, `end`,
`timeframe: "1H"|"daily"`), plus the FRVP rectangle/window (`start`, `end`,
histogram rows) and a Sunday-open ray/marker. Node 2 draws the FRVP on the 1H
chart, then mirrors each level as a **horizontal ray on the daily chart**.
Level identity is `(window, kind, start, end)` so a week reset replaces rays.

### 2.3 Execution (break → retest; volume filter OFF)

1. Per level state: `scanning → broken_above | broken_below`.
2. Break: green candle with `open <= L < close`, or red candle with
   `open >= L > close`.
3. Retest: a later candle trades back within **±$0.50** of the level; the setup
   confirms in the break direction. **No volume requirement (D6).**
4. Invalidation: a close **$2.00** beyond the broken level resets the setup.
5. One trigger per closed candle; levels visited in sorted ID order.
6. Result: immutable `TradeIntent` (`side`, `reference_price`, `stop_loss`,
   `take_profit`, `risk_reward`, `level_name`, `source_candle_time`,
   deterministic `intent_id`).

### 2.4 Risk projection (2–3R, ATR-adjusted)

ATR(14) on closed candles drives SL within **200–300 pips** ($2–3) and TP within
**600–800 pips** ($6–8); TP is clamped so RR stays in **2.0–3.0**. Higher
volatility → wider SL/TP inside the bands. **You project prices; Node 4 enforces
policy and may reject but never silently rewrite them.** Publish current
`sl_pips`/`tp_pips`/`rr`/`atr` in `/scanning` and `/diagnostics`.

---

## 3. Contracts

**C1 v2 (consume; Node 1 is the producer).** `GET /status`,
`/candles?interval=1m|15m|1h`, `/tick-volume`, `/calendar`; `WS /ws` topics
`candle`, `tick_volume`, `calendar`, `status`, `heartbeat`. Envelope:
`contract_version: "2.0"`, `boot_id`, `seq` (live only), `replay: true` (replay
only, never both), `gap`, `symbol`, `interval`, `closed`, `freshness`
(`state ∈ live|stale|offline|closed_market`, `age_ms`, `market_open`,
`last_tick_ms`). Only `closed: true` candles reach the strategy, exactly once.
The full signed schema lives in `contracts/NODE1_FEED_CONTRACT_v2.md` (Phase 0).

**C2 v1 (publish; Node 2 + operators consume).** `GET /levels`, `/vp?window=`,
`/vp?start=&end=`, **`/drawings`**, `/scanning`, `/signals`, `/diagnostics`,
`/health`; `WS /ws` topics `levels`, `drawing`, `vp`, `scanning`, `signals`,
`diagnostics`. Replay the current snapshot on subscribe. All public payloads are
secret-free.

**C3 v1.1 (private; Node 4 consumes).** Keep `docs/NODE3_NODE4_PROTOCOL.md`:
`WS /execution`, token `NODE4_SHARED_TOKEN`, one authenticated consumer,
`execution_hello`/`ack`, `trade_intent`, `execution_report`, bounded pending
queue with replay after reconnect, TTL. **Extend `TradeIntent` additively** with
optional fields `atr`, `atr_pips`, `level_id`, `drawing_id`,
`strategy_version`, `volume_confirmed`. Unknown versions fail closed. Bump
`strategy_version` whenever a rule changes so `intent_id`s cannot collide. The
intent still carries **no** venue, account, stake, lots or credential.

---

## 4. Work items

1. **Port the FRVP engine.** Take `Node1:src/volume_profile.rs` +
   `volume_profile/{histogram,session,swing,timeframe,weekly}.rs` (≈2,616
   lines) and `ProfileMeta` from `Node1:src/types.rs` into
   `node3-strategy/src/vp/`. Keep TradingView parity, the DST-safe NY 18:00
   anchors (`most_recent_week_start_utc`, `previous_week_bounds_utc`,
   `last_session_close_utc`, `previous_session_slice`,
   `MAX_SESSION_LOOKBACK = 5`), filler-bar weekend skipping and `sunday_open`.
   Add `chrono-tz` to `Cargo.toml`. Drop `SWING_BULL`/`SWING_BEAR` behind a
   flag — they are not in the owner's spec.
2. **Ingest C1 v2.** Replace the `candle`+`levels` subscription with `candle`
   (1m/1H/15m), `tick_volume`, `calendar`, `status`. Validate `seq`, `replay`,
   `gap`, `closed`, `symbol`, `interval`, `freshness`; reconnect on any
   violation; retain ≥35 days of fine bars; never trade on stale or
   closed-market data. **No more trusting Node 1's `/levels`.**
3. **Implement the window policy** (§2.1): PW frozen at Sunday 18:00; PS = last
   closed session (Thu 18:00 → Fri 18:00 at week open, weekend filler walked
   over), PoC only, frozen at week open; CW = week open → last completed 18:00
   daily close, first snapshot after Monday's close, refreshed at every later
   daily close, reset at Sunday 18:00. Expose `CW_FIRST_PUBLISH_WEEKDAY=Mon`,
   `PS_MODE=week_open|rolling`, `WEEK_ANCHOR`, `VA_PCT`, `VP_ROWS`,
   `VP_BIN_SIZE`, `VP_LOWER_TF`, `VOLUME_THRESHOLD=0` as explicit config.
4. **Publish C2.** `GET /levels` (array of `VpLevels` incl. `sunday_open`,
   `start`, `end`, `meta`), `GET /vp?window=PW|PS|CW` and `/vp?start=&end=`
   (full histogram audit with poc/vah/val row indices), **`GET /drawings`**
   (1H FRVP rectangles + daily horizontal rays + Sunday-open marker, each with
   id/label/price/colour/style/anchor time), `GET /scanning`, `GET /signals`,
   `GET /diagnostics`, `GET /health`; `WS /ws` topics listed in §3. Replay the
   current snapshot on subscribe. Keep the private/public WS separation — the
   public socket must never send `trade_intent`, `execution_hello*` or
   `execution_report`, even for `topics: ["all"]`.
5. **Keep the break/retest state machine** (`src/strategy.rs`): PW PoC/VaH/VaL,
   PS PoC, CW PoC/VaH/VaL; ±$0.50 retest zone, $2.00 invalidation, one trigger
   per closed candle, sorted level IDs. Wire it to the **internally computed**
   levels. Make `VOLUME_THRESHOLD` optional and **default OFF** (D6).
6. **Risk projection** (`src/risk.rs`): ATR(14)-scaled SL 200–300 pips / TP
   600–800 pips clamped to RR 2–3, `1 pip = $0.01`; publish `sl_pips`,
   `tp_pips`, `rr`, `atr` in `/scanning` and `/diagnostics`.
7. **Keep the private `/execution` WS** and extend `TradeIntent` additively with
   the C3 v1.1 fields (§3). Keep `intent_id` deterministic and idempotent.
8. **Tests / parity.** FRVP golden fixtures from `Node1:ci/live/vp-{PW,PS,CW}.json`;
   DST boundary tests (2025-06-08 17:00 vs 19:00 NY); weekend filler skipping;
   **PS walk-back at Sunday 18:00 returns exactly Thu 18:00 → Fri 18:00 and
   never the flat weekend buckets, with a golden fixture for its PoC**;
   CW first-publish/day-boundary rule with a fake clock (both `Mon` and `Wed`
   settings); Sunday-open extraction; break/retest determinism; low-volume
   rejection when enabled; intent idempotency; a boundary test asserting **no
   broker/venue/credential identifier** in the crate. Run
   `cargo fmt --check`, `cargo check --all-targets`, `cargo test --all-targets`,
   `cargo clippy --all-targets -- -D warnings`, release build.

---

## 5. Acceptance criteria

- PW/PS/CW PoC/VaH/VaL match the recorded TradingView-parity profiles within
  one histogram row; PS publishes **PoC only**; `sunday_open` is present.
- `/drawings` renders correctly in Node 2 (1H FRVP rectangles, daily rays,
  Sunday-open marker); CW pending state is stable before the first daily close.
- A synthetic break + retest produces **exactly one** `trade_intent` with
  RR 2–3, and no volume requirement blocks it (D6).
- No broker/venue/credential identifier in the crate; `cargo fmt/check/test/
  clippy` + release build green.
- `cargo test` can run offline against the golden fixtures while Node 1 is down.

**Depends on:** C1 (Agent 1) — but build offline against golden fixtures first.
**Blocks:** Agent 2's drawings, Agent 4's end-to-end demo.
**Must not:** add a broker adapter, venue choice, stake/lot size, account state,
or any VP-free shortcut that trusts Node 1's old `/levels`.
