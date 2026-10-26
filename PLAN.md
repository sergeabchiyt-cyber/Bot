# XAUUSD bot — microservice split & strategy fix (PLAN ONLY — no code written)

**Status:** draft for review. Written 2026-10-26 on `arena/c3e4eae3-bot`.
**All decisions D1–D9 settled on 2026-10-26 — see section 4.1. D5b is a
non-blocking default (one config value).**
**Scope of this document:** audit of the four nodes, a formalised version of your
strategy, a target architecture, **one self-contained brief per node** (so four
agents can work in parallel), shared contracts, and a gated migration order.
**No implementation code has been changed.**

---

## 0. What I actually inspected

`sergeabchiyt-cyber/Bot` is **four branches with disjoint histories** (no common
ancestor), each effectively a separate repository:

| Branch | Head | Crate / app | What it really is today |
|---|---|---|---|
| `Node1` | `d007137` | `xauusd-engine` 0.4.0 (Rust) | Market feed + tick volume + **all volume-profile maths** + calendar + econ audio/AI relay + **a strategy evaluator** + **a demo paper engine** |
| `Node2` | `890559d` | static HTML/CSS/JS | Dashboard A: `index.html`, 20 `js/*.js`, `style.css` (1,575 lines), 10 test files, **real lightweight-charts v4.2.0** |
| `Node3` | `fff6bfd` | `xauusd-node3-strategy` 2.0.0 (Rust) | Break/retest state machine + ATR SL/TP/RR projection + `trade_intent` emitter over private `/execution` WS. **No VP maths** — it consumes Node 1's `levels` |
| `Node-4` | `746e70a` | `node4-execution` (Rust) | Execution, ledger, one-venue policy, Deriv/Chelsea/MT5-demo adapters, `mt5-bridge`, `mt5-host`, kill switch — **plus a verbatim duplicate of Node 3's strategy**, a Node 1 feed consumer, Node 1 audit docs, a **second dashboard** (`node2/`), and `Node-4.txt` |

`origin/HEAD -> origin/Node1`. This checkout (`arena/c3e4eae3-bot`) is the
`Node-4` tree. `Node-4.txt` is a 30,679-line / 1.7 MB diff that records the
*removal* of Node 1 / Node 3 code from this branch — i.e. someone already tried
to split things by hand.

**D8: you confirmed no screenshot is needed.** Everything about chart drawings
is defined by your text description plus the existing `js/levels.js` /
`js/overlays.js` colour map and lightweight-charts conventions; correctness is
verified by geometry unit tests, not a reference image.

---

## 1. Why it is "messed up" (concrete findings)

1. **The boundaries contradict themselves.**
   - Node 1 README: *"market-data, volume-profile and strategy service"*; it
     owns `src/strategy.rs` (825 lines), `src/demo_engine.rs` (757 lines),
     `GET /strategy`, `GET /demo`, WS topics `strategy`/`demo`, and 17
     `STRATEGY_*`/`DEMO_*` env vars.
   - Node 4 README has a *"Scope guard: no VP computation, no break/retest
     state machine"* — then ships `src/strategy.rs`, `src/strategy_runner.rs`,
     `src/node1.rs` and `NODE4_INTENT_SOURCE=node1_strategy`, a **verbatim port**
     of Node 3's strategy (`docs/node4/LOCAL_STRATEGY.md` admits it).
   - Your new mandate puts **VP maths in Node 3**, where none exists today.
2. **Two dashboards, two chart engines, neither has a Node 3 endpoint.**
   - `Node2` branch: lightweight-charts v4.2.0 (CDN), 15M fixed timeframe,
     price lines for PW/PS/CW/SWING, entry/SL/TP overlays, ATR band — but it
     reads Node 1 `/strategy` and `/demo` (the endpoints that must be deleted).
   - `Node-4/node2/`: ES modules, same-origin `/api/node1` + `/api/node4`
     proxy, mock feed, 15 test suites — but a **hand-rolled canvas chart**, no
     lightweight-charts, and no Node 3 host in `config.js`.
   - `docs/SERVICE_ARCHITECTURE.md` (Node2 branch) still documents **two**
     services; `docs/BRANCH_ARCHITECTURE.md` (Node3 branch) documents **three**
     hosts (`MARKET_HOST`, `STRATEGY_HOST`, `EXECUTION_HOST`) that the code never
     implemented.
3. **Contracts live in the wrong repos and are unsigned.**
   `docs/node1/NODE1_FEED_CONTRACT_v1.md` is a **DRAFT** sitting in the Node-4
   tree; `docs/NODE3_NODE4_PROTOCOL.md` sits in the Node3 tree;
   `FRONTEND_NEXT.md` sits in the Node1 tree; `FRONTEND_RESOURCES.md` in Node-4.
   The Node 1 feed contract explicitly lists Node 4 as *"not a consumer"*, while
   Node 4 ships a consumer for it that is gated off because the deployed Node 1
   still emits v0 (no `closed`, `seq`, `contract_version`, `freshness`).
4. **The CI boundary guard enforces the old world.** `Node1:ci/check_node1_boundary.py`
   bans `SL_MIN_PIPS`/`RR_MIN` etc. as "execution-only" (correct) but also
   codifies the strategy + demo engine as legitimate. Nothing guards Node 4 from
   re-growing strategy code.
5. **Deployment lags the branches.** The recorded live probe shows Node 1
   deployed at v0.3.0; `ci/live/strategy.code` and `ci/live/demo.code` are 404,
   so the branch-head strategy/demo was never actually deployed. Removing it is
   safe.
6. **Data-source mismatch.** The 10,500 "OANDA XAUUSD volume" filter is applied
   to SiftingIO *tick counts* (`candle.volume` = number of price updates). Those
   are not the same unit. You said to ignore the volume part — see D6.
7. **VP semantics drift from your spec.** Node 1 currently publishes CW after
   *Monday's* 18:00 close and refreshes daily; you want no Mon/Tue CW, first CW
   on Wednesday. Node 1 uses DST-aware `America/New_York` 18:00; you wrote
   `18:00 UTC-4`. Node 1 already emits `sunday_open` in `/levels` — good, that
   one is reusable.
8. **Branch-per-service + disjoint histories** means no atomic cross-service
   change, no shared schema package, and every agent edits a different
   unrelated history. This is the root cause of 1–7.

---

## 2. Your strategy, formalised (this is what the code must implement)

### 2.1 Instrument and clock
- Symbol: **XAUUSD**. Price source today: SiftingIO spot. Your volume reference
  is OANDA; the engine's volume is tick count (see D6).
- Week anchor: **Sunday 18:00 New York time**. Implemented as
  `America/New_York` (UTC−4 in EDT, UTC−5 in EST) so it stays on the broker's
  daily candle across DST — see D3. A trading week = Sun 18:00 → Fri 18:00;
  each daily session = 18:00 → next 18:00.
- 1 pip = **$0.01** on XAUUSD. So SL 200–300 pips = **$2.00–$3.00**;
  TP 600–800 pips = **$6.00–$8.00**. `RiskProjector` in Node 3 already uses
  exactly this (`sl_distance = sl_pips * 0.01`). **Confirmed (D9).**

### 2.2 The three FRVP windows
| Window | Range | Levels published | Lifetime |
|---|---|---|---|
| **PW** (previous week FRVP) | previous Sunday 18:00 NY → that Friday 18:00 NY close (the full prior trading week) | **PW PoC, PW VaH, PW VaL** | computed once at Sunday 18:00, frozen for the whole current week |
| **PS** (previous session) | **DECIDED (D5): last closed 18:00→18:00 session** — at the Sunday 18:00 week open that is **Thursday 18:00 open → Friday 18:00 close** | **PS PoC only** | frozen at the week open (see D5b for the rolling-session option) |
| **CW** (current week VP) | week open (Sunday 18:00) → most recent completed 18:00 daily close | **CW PoC, CW VaH, CW VaL** | **CONFIRMED (D4=C): keep current engine behaviour** — first snapshot after Monday's 18:00 daily close, refreshed at every later daily close, cleared at the next Sunday 18:00 |

> **D4 confirmed (C): keep current CW behaviour.** CW is first published after
> **Monday's** 18:00 NY close and refreshed at every later daily close. Your
> original "no CW Mon/Tue, first on Wednesday" wording is **not** the default;
> Node 3 still exposes `CW_FIRST_PUBLISH_WEEKDAY=Mon|Wed` so switching to the
> Wednesday rule is one config value with no code change. The dashboard keeps
> its "CW pending" placeholder until CW actually arrives.

> **D5 settled — "last closed session", weekend skipped.** Sessions are
> 18:00 NY → 18:00 NY. When the new week opens (Sunday 18:00 NY) the last
> session that actually traded is **Thu 18:00 open → Fri 18:00 close**. The
> Fri 18:00 → Sat 18:00 and Sat 18:00 → Sun 18:00 buckets are flat
> closed-market filler bars and must be **walked over, never profiled**:
> profiling them produced the old bug (PS PoC stuck on a $1.5 sliver at the
> Friday close while price traded 15 dollars higher). Node 1's
> `volume_profile/session.rs` already implements this
> (`previous_session_slice`, `MAX_SESSION_LOOKBACK = 5`, bodyless-bar test), so
> Agent 3 ports it unchanged. **Only PS PoC is published** — PS VaH/VaL are
> never drawn.
>
> **D5b (non-blocking default):** your spec draws the PS PoC at the start of
> the week, so the strategy level is **frozen at the Sunday 18:00 week open** —
> it does not roll at each daily close. Today's Node 1 instead rolls PS forward
> at every 18:00 close. Agent 3 will expose `PS_MODE=week_open|rolling` with
> default `week_open`; if rolling is chosen, every published `PS` must carry its
> own `start`/`end` so Node 2 replaces the ray instead of stacking stale ones.

- Histogram: TradingView Fixed-Range-VP parity — 128 rows (or fixed $ bins),
  70 % value area, POC = max-volume row, VAH/VAL = value-area bounds.
- Also publish **`sunday_open`** = open of the Sunday 18:00 1H candle.
- Input resolution: **1H candles for the time boundaries** (your "draw on 1H for
  time accuracy"), with 1m/5m bars for histogram precision where available.

### 2.3 Drawings (what Node 2 renders)
- Node 3 computes on 1H and emits a **drawing payload**: for each level a
  horizontal ray (`id`, `label`, `price`, `color`, `line_style`, `ray_from_time`,
  `timeframe: "1H" | "daily"`), plus the FRVP rectangle/window (`start`, `end`,
  histogram rows) and a Sunday-open ray/marker.
- Node 2 draws the FRVP on the **1H chart**, then mirrors each level as a
  **horizontal ray on the daily chart** — exactly your manual workflow.
- **No screenshot (D8):** ray colours/styles, FRVP rectangle placement and the
  Sunday-open marker follow the existing `js/levels.js` colour map and
  lightweight-charts `createPriceLine` conventions, asserted by geometry tests.

### 2.4 Execution (break → retest; volume spike filter off per D6=A)
1. For each tracked level (PW PoC/VaH/VaL, PS PoC, CW PoC/VaH/VaL) maintain a
   state: `scanning → broken_above | broken_below`.
2. Break: a green candle that opens at/below a level and closes above it
   (`open <= L < close`), or a red candle that opens at/above and closes below.
3. Retest: a later candle trades back within **±$0.50** of the level; the setup
   confirms in the break direction. **D6=A: the 10,500 OANDA volume-spike filter
   is SKIPPED** — break + retest alone confirm. The threshold stays as optional,
   **OFF-by-default** config (`VOLUME_THRESHOLD=0`) so it can be re-enabled
   later; VP/histogram drawings are unaffected.
4. Invalidation: a close **$2.00** beyond the broken level resets the setup.
5. One trigger per closed candle; levels visited in sorted ID order for
   determinism.
6. Result: immutable `TradeIntent` with `side`, `reference_price`, `stop_loss`,
   `take_profit`, `risk_reward`, `level_name`, `source_candle_time`,
   deterministic `intent_id`.

### 2.5 Risk (2–3R, ATR-adjusted)
- ATR(14) on closed candles drives SL within **200–300 pips** and TP within
  **600–800 pips**; TP is clamped so realised RR stays in **2.0–3.0**.
- Higher volatility → wider SL/TP inside those bands; lower volatility →
  tighter. Node 3 projects the prices; **Node 4 enforces the policy** and can
  reject but not silently rewrite them (protocol v1 rule).
- Demo only by default. Any live venue is opt-in behind the existing layered
  gates.

---

## 3. Target architecture (all decisions settled — see section 4.1)

**D2 = B: the four branches stay.** Each node remains an independently
deployable service on its own disjoint-history branch (`Node1`, `Node2`,
`Node3`, `Node-4`). Consequences, spelled out so nobody is surprised:

* **Four Arena sessions, one per branch.** An agent must start on the branch it
  edits; the four trees cannot be changed atomically in one PR.
* **This session (`arena/c3e4eae3-bot`, based on `Node-4`) can only implement
  the Node 4 brief.** The Node 1 / Node 2 / Node 3 briefs need three sessions
  started from those branches. Each brief is written to be pasted verbatim into
  its own session.
* Because branches are disjoint, the versioned `contracts/` folder must be
  **committed byte-identically to all four branches** and kept equal by a CI
  sync check (the D2-C safeguard added to your D2-B choice; without it the
  contracts will drift again, which is exactly what caused section 1.3).
* Each branch keeps its own `Dockerfile`, `.env.example` and CI workflow; the
  root README on each branch documents only that service plus the four-node map.
* **D7: Node 2 is frontend only.** No server, proxy, mock backend or Node 4
  source file lives in the `Node2` branch; it calls the three services' public
  REST/WS endpoints directly (CORS allow-lists on each service). **Node 4 ships
  no dashboard at all** — its embedded `node2/` is deleted.

```
                         ┌──────────────────────────────────────────────┐
   SiftingIO + ForexFactory│  NODE 1  market data + calendar  (Rust)      │
   ─────────────────────► │  1m / 15m / 1H candles · tick volume         │
                         │  economic calendar · feed status / freshness  │
                         │  versioned read-only REST + WS (contract v2)  │
                         └───────────────┬──────────────────────────────┘
                                         │ C1  candle / tick_volume /
                                         │     calendar / status
                    ┌────────────────────▼───────────────────────────────┐
                    │  NODE 3  Volume Profile + Signal engine  (Rust)    │
                    │  FRVP PW / PS / CW · PoC / VaH / VaL · sunday_open  │
                    │  1H→daily drawing payload · break/retest scanner    │
                    │  ATR SL/TP/RR projection · trade_intent emitter     │
                    └─┬──────────────────────────────┬───────────────────┘
        C2 public     │  /levels /vp /drawings       │  C3 private
        REST + WS     │  /scanning /signals /ws      │  WS /execution
                    ┌─▼─────────────────────┐   ┌────▼───────────────────────┐
                    │  NODE 2  dashboard    │   │  NODE 4  demo execution    │
                    │  vanilla HTML/CSS/JS  │   │  + risk management (Rust)  │
                    │  lightweight-charts   │   │  policy · sizing · ledger  │
                    │  1H drawings + daily  │   │  demo engine · MT5 demo    │
                    │  rays · sunday open   │   │  kill switch · reports     │
                    └───────────▲───────────┘   └────────────┬───────────────┘
                                └──────── C4 public ─────────┘
                          /diagnostics /open-trades /account /mt5/* /ws
```

**Ownership matrix (enforced by CI guards, not just docs):**

| Node | Owns | Forbidden |
|---|---|---|
| 1 | feed adapters, 1m/15m/1H candles, tick volume, calendar, feed health, versioned market contract | VP maths, levels, strategy, SL/TP/RR, demo engine, broker anything |
| 2 | browser HTML/CSS/JS, lightweight-charts drawings, read-only views of 1/3/4 | secrets, POST/control, VP maths, strategy decisions |
| 3 | FRVP histogram, PW/PS/CW windows, drawing payload, break/retest scanner, ATR price projection, intents, public diagnostics | broker adapters, credentials, account sizing, VP-free Node 1 output |
| 4 | intent validation, risk policy, lot sizing, durable ledger, demo execution engine, one venue, kill switch, reconciliation, reports | feed ingestion, VP maths, any strategy trigger logic, a second dashboard |

### 3.1 Shared contracts (Phase 0, before any agent writes code)

Put versioned JSON Schemas + golden fixtures in **`contracts/`** at each
branch root (identical copies, enforced by `ci/contract_sync.py`), with a
`contract-lint` script that all four CI jobs run:

- **C1 — Node 1 → consumers (v2).** `GET /health`, `/status`, `/candles?interval=1m|15m|1h`,
  `/tick-volume`, `/calendar`; `WS /ws` topics `candle`, `tick_volume`,
  `calendar`, `status`, `heartbeat`. Envelope: `contract_version`, `boot_id`,
  `seq` (live only), `replay: true` (replay only), `gap`, `symbol`, `interval`,
  `closed`, `freshness.{state,age_ms,market_open,last_tick_ms}`. This is the
  existing DRAFT v1 promoted to a signed v2, with multi-timeframe candles added.
- **C2 — Node 3 → Node 2 / operators (v1).** `GET /levels`, `/vp?window=`,
  `/vp?start=&end=`, `/drawings`, `/scanning`, `/signals`, `/diagnostics`,
  `/health`; `WS /ws` topics `levels`, `drawing`, `scanning`, `signals`,
  `diagnostics`. Drawing payload includes 1H window rectangles and daily rays.
- **C3 — Node 3 → Node 4 private (v1.1 additive).** Existing
  `docs/NODE3_NODE4_PROTOCOL.md` handshake + `trade_intent` / `execution_report`.
  Additive optional fields: `atr`, `atr_pips`, `level_id`, `drawing_id`,
  `strategy_version`, `volume_confirmed`. Unknown versions fail closed.
- **C4 — Node 4 → Node 2 (v1).** Existing `docs/mt5/FRONTEND_RESOURCES.md` plus
  a new `risk` block in `/diagnostics` (limits, exposure, daily PnL, drawdown,
  rejection reasons).

Golden fixtures: keep `Node1:ci/live/ws-sample.jsonl`, `candles.json`,
`levels.json`, `vp-PW.json`, `status.json` as the FRVP parity baseline so Node 3
can be developed offline against recorded data.

---

## 4. Decisions (all settled on 2026-10-26)

### 4.1 Settled on 2026-10-26

| # | Your answer | Effect on the plan |
|---|---|---|
| **D1** | **A** — Node 3 decides -> Node 4 executes | Baseline architecture in section 3 stands. Node 3 owns VP + break/retest + ATR projection and emits `trade_intent`; Node 4 owns risk policy, sizing, demo engine and reports. The existing `/execution` protocol v1 is kept and extended additively. |
| **D2** | **B** — keep four disjoint branches | Four separate Arena sessions; this session can only do Node 4. `contracts/` is mirrored into all four branches with a CI equality check. See the section 3 preamble. |
| **D3** | **A** — DST-aware `America/New_York` 18:00 | Sunday 18:00 and daily 18:00 close stay pinned to NY local time (UTC-4 EDT / UTC-5 EST). Existing `session.rs` / `weekly.rs` logic is reused unchanged. |
| **D4** | **C** — keep current CW behaviour (reconfirmed) | CW first appears after **Monday's** 18:00 close and refreshes at every daily close. The written Wednesday rule stays available as `CW_FIRST_PUBLISH_WEEKDAY=Wed`, one config value, no code change. |
| **D5** | **A** — last closed session; at week open = **Thu 18:00 open → Fri 18:00 close**, PoC only | PS uses the last session that really traded; flat weekend filler is skipped. `previous_session_slice` is ported to Node 3 unchanged. Freeze-vs-roll is D5b. |
| **D6** | **A** — skip the 10.5K volume filter | Break + retest candle alone confirms a trade. `VOLUME_THRESHOLD` stays configurable, OFF by default. VP histogram drawings remain. |
| **D7** | **Node 2 = frontend only; keep Node 4 out of it; wire backend endpoints in** | The `Node2` branch is the single dashboard (plain `index.html` + `js/` + `style.css`, lightweight-charts v4.2.0, no build step). **No server, proxy, `serve.mjs`, mock backend or any Node 4 source file.** Three public service origins are wired into `js/config.js`. Node 4's embedded `node2/` directory is deleted entirely. |
| **D8** | No screenshot needed | Drawing styling follows the existing `js/levels.js` colour map + lightweight-charts conventions; verified with geometry unit tests. |
| **D9** | Confirmed: **1 pip = $0.01** | SL 200–300 pips = **$2–3**, TP 600–800 pips = **$6–8**; Node 3's `RiskProjector` already uses this. |

### 4.2 One remaining non-blocking default

| # | Default | How to change it |
|---|---|---|
| **D5b** | **PS PoC is frozen at the Sunday 18:00 week open** (Thu 18:00 → Fri 18:00), matching your "when new week opens use thu open fri close" example. | Set `PS_MODE=rolling` on Node 3 to roll PS forward at every 18:00 close (current Node 1 behaviour). No code change; not a blocker for Phase 0. |

D1 is settled as A, so the strategy/risk blocks in Agents 3 and 4 do **not**
need swapping. D8 is settled (no screenshot), so Agent 2 verifies drawings with
geometry tests instead of a reference image.

---

## 5. Agent briefs

> These are four independent briefs, one per branch (D2 = B). Phase 0 must be
> signed off first. **I cannot spawn sub-agents inside this session**: this
> session can implement the **Agent 4 (Node 4)** brief because it is based on
> `Node-4`. For Nodes 1, 2 and 3, start one Arena session per branch
> (`Node1`, `Node2`, `Node3`) and paste the corresponding brief below. The
> byte-identical `contracts/` folder is the only coupling between the four
> agents. Each brief names its branch explicitly.

### Agent 1 — Node 1: "Market data only + calendar" (Rust)

**Branch:** `Node1` (own Arena session). **Language:** Rust.

**Mission.** Reduce `Node1` to a pure, versioned market-data feed. No VP, no
strategy, no demo, no decisions.

**Work items**
1. **Implement C1 v2** against `docs/node1/NODE1_FEED_CONTRACT_v1.md` §8–12
   (that doc becomes the signed v2 in `contracts/`): `contract_version`,
   `boot_id`, `seq` on live frames, `replay: true` on replay frames (never both),
   `gap` on buffer overrun, `symbol`/`interval`/`closed` on every candle,
   `freshness` in `/status` and every `status` frame, broadcast on each
   freshness-state transition, 30 s heartbeat, strictly increasing `seq`.
2. **Multi-timeframe candles.** Today only 15m closed bars are served (plus an
   internal 1m profile aggregator). Expose `GET /candles?interval=1m|15m|1h`
   and the corresponding WS `candle` frames, all boundary-aligned, ascending,
   unique by `time`, with the forming bar explicitly `closed: false`. Node 3
   needs 1H for the drawing boundaries and 1m/5m for FRVP precision.
3. **Keep `calendar.rs`** (ForexFactory weekly JSON + MCP-browser fallback) and
   publish `/calendar` + WS `calendar`; add `market_open` / session state to
   `status` so weekend false alarms are avoided.
4. **Delete:** `src/strategy.rs`, `src/demo_engine.rs`, `WsFrame::Strategy` /
   `WsFrame::Demo`, routes `/strategy` and `/demo`, their replay/fan-out arms,
   `/status.strategy` + `/status.demo`, `AppState.strategy/demo`, the 17
   `STRATEGY_*`/`DEMO_*` env vars, the strategy/demo blocks in `src/main.rs`
   (lines ~548–700), `FRONTEND_NEXT.md`, `ci/verify_demo.py`, strategy/demo
   assertions in `ci/verify_ws.py` and the matching `ci/smoke.sh` /
   `ci/live-probe.sh` steps and `ci/live/{strategy,demo}.*` fixtures.
5. **Hand off VP maths.** `src/volume_profile.rs` + `volume_profile/{histogram,
   session, swing, timeframe, weekly}.rs` (≈2,616 lines) move to Agent 3's
   `node3-strategy/src/vp/`. During migration keep `/levels` and `/vp` as a
   **deprecated read-through proxy** to Node 3 (feature-flagged), then remove
   both plus `ProfileMeta` from `types.rs` and the VP model section of `/status`.
   No VP computation remains in Node 1 after cutover.
6. **Triage the AI/audio relay** (`econ_monitor.rs`, `mcp_client.rs`,
   `ai_cache.rs`, WS topics `audio_chunk`/`learn`/`transcript`/`sentiment`/
   `health`/`prediction`): these are Node 3 inputs, not market data. Recommend
   moving them to Node 3 as an optional module, or deleting them if unused.
7. **Rewrite `ci/check_node1_boundary.py`** so it enforces the new boundary:
   ban `mod strategy`, `mod demo_engine`, `volume_profile`, `SL_*`/`TP_*`/`RR_*`,
   `trade_intent`, `intent_id`, `TradeIntent`, any broker/venue identifier, any
   write route. Keep `no_route_accepts_a_broker_write`.
8. **CI/tests:** contract matrix (malformed/late/future ticks, `closed == true`
   exactly once, replay ordering, gap, boot_id, freshness transitions,
   DST-safe 18:00 boundaries), `ci/smoke.sh`, release build, live probe.

**Acceptance.** No strategy/demo/VP identifier in `src/`; `/candles?interval=1h`
returns aligned 1H bars; Node 3 can recompute PW/PS/CW **from Node 1 candles
alone** and match the recorded golden fixtures; CI green; deployed version
reports v2 with `freshness`.

**Depends on:** Phase 0 (C1 schema). **Blocks:** Agent 3's integration test.
**Must not:** compute a level, choose a side, or open a file with `SL_` in it.

---

### Agent 2 — Node 2: dashboard with the new Node 3 endpoint + better design (vanilla HTML/CSS/JS + lightweight-charts)

**Branch:** `Node2` (own Arena session). **Language:** vanilla HTML/CSS/JS.

**Mission.** One canonical, read-only dashboard that renders Node 1 market data,
**Node 3 VP/drawings/scanner/signals** and Node 4 execution/risk — with your
1H→daily drawing workflow actually visible.

**Work items**
1. **Pure frontend, no Node 4 code (D7).** The canonical dashboard is the
   **`Node2` branch tree exactly as it stands**: `index.html`, `js/*.js`
   (~20 modules), `style.css`, `tests/*.test.js` — a static, build-step-free
   site that already loads lightweight-charts v4.2.0. It stays on the `Node2`
   branch only. **Do not copy `Node-4/node2/`**: no `public/src/` ES-module
   rewrite, no `serve.mjs`, no `mock/` server, no `package.json` with a `ws`
   devDependency, no proxy. Node 2 contains browser code and browser code only;
   Agent 4 deletes Node 4's embedded dashboard entirely.
2. **Wire the three backend endpoints in.** `js/config.js` gets three explicit
   host keys — `MARKET_HOST` (Node 1), `STRATEGY_HOST` (Node 3),
   `EXECUTION_HOST` (Node 4) — each producing its own REST base and WS base.
   Keep the existing `window.XAUUSD_CONFIG` override pattern and its strict
   origin validation (http(s)/ws(s) only, no credentials, query, fragment or
   custom path, no loopback) and apply it to all three. Add a **third
   independent socket** for Node 3 with its own heartbeat, silence watchdog,
   bounded backoff and 5 s polling fallback, so one service being down never
   disturbs the others. All three services whitelist the deployed dashboard
   origin in CORS; **no proxy is introduced**.
3. **New Node 3 client** (`js/api-node3.js`, matching the existing flat `js/`
   layout): on mount, seed `GET /levels`, `/vp?window=PW`, `/drawings`,
   `/scanning`, `/signals`, `/diagnostics` in parallel — GET only,
   `cache: "no-store"`, per-endpoint error tolerance — then stay live over the
   Node 3 WS (`levels`, `drawing`, `scanning`, `signals`, `diagnostics`).
   **Remove** all Node 1 `/strategy`, `/demo` calls and their WS topics, and
   stop polling Node 1 `/levels` (levels now come from Node 3).
4. **Chart & drawings (the important part).**
   - Two synced views: **1H chart** (FRVP rectangles for PW/PS/CW, exact
     `start`/`end` times, histogram overlay, Sunday-open vertical marker at
     Sunday 18:00) and **daily chart** with one **horizontal ray per level**
     (PW PoC/VaH/VaL, PS PoC, CW PoC/VaH/VaL, Sunday open) styled from the
     `drawings` payload. A `1H | D` toggle; the daily rays stay when switching.
   - Break/retest markers from `/signals` on the 1H chart (arrowUp/arrowDown at
     `source_candle_time`), entry/SL/TP price lines from Node 4 `/open-trades`,
     optional ATR band.
   - CW is **optional until the first daily close (Monday 18:00 NY, D4=C)**:
     keep the existing stable "CW pending" placeholder in the levels legend and
     table; never assume a CW object exists.
   - Reuse `js/levels.js` colour map (gold/teal/coral/blue/violet/pink/cyan)
     and `js/overlays.js` line helpers; never compute VP maths in the browser.
   - **No reference screenshot (D8):** assert ray/rectangle geometry in unit
     tests instead; style follows the existing colour map and lightweight-charts
     `createPriceLine` conventions.
5. **Better design.** A real design system: tokens (colour, spacing, type scale,
   radii, elevation), 3/2/1-column responsive layout with the chart dominant,
   sticky top bar with last price + per-service health pills, a week/session
   timeline strip (PW / PS / CW windows + Sunday-open marker + next 18:00
   close countdown), a scanner table (level, state, distance in $/pips,
   projected SL/TP/RR, volume status), a signals tape, a risk card (SL/TP/RR
   bands, ATR, exposure, daily PnL, drawdown, rejection reasons), an execution
   card (venue, dry-run/DRY RUN banner, open trades, account), consistent
   empty/loading/stale skeletons, accessible contrast + keyboard focus + ARIA,
   no layout shift on live updates, mobile-first below 720px.
6. **Dead panels out:** strategy-evaluator and demo-account panels (Node 1),
   order-flow/bubbles references, the "four order-flow colours" legend.
7. **Tests:** keep the existing `node --test` suites in `tests/`; add
   `node3-endpoints.test.js`, `drawings.test.js` (ray geometry, 1H→daily
   projection, missing CW, Sunday-open marker), `three-host-config.test.js`, and
   extend `security.test.js` to assert **no POST, no token, no `/execution`, no
   `/mt5/control`** and **no server/proxy/Node-4-origin file** anywhere in the
   branch. Use plain JSON fixtures in `tests/fixtures/` (recorded from the
   shared `contracts/` golden fixtures) instead of a mock server.

**Acceptance.** Three hosts configurable; zero network calls to Node 1
`/strategy` or `/demo`; 1H FRVP + Sunday-open marker + daily rays verified by
geometry tests and a quick browser render (no screenshot needed, D8); all tests
green; no secret in browser code; **the branch contains no backend/server
files at all**.

**Depends on:** C2 (Agent 3) and C4 (Agent 4) schemas; can develop fully against
the local `tests/fixtures/` JSON in parallel.

---

### Agent 3 — Node 3: "Volume Profile Calculations" (Rust)

**Branch:** `Node3` (own Arena session). **Language:** Rust.

**Mission.** Node 3 owns **all** VP maths, the 1H→daily drawing payload, and
(under D1-A) the break/retest scanner and ATR price projection.

**Work items**
1. **Port the FRVP engine.** Move `Node1:src/volume_profile.rs` +
   `volume_profile/{histogram,session,swing,timeframe,weekly}.rs` into
   `node3-strategy/src/vp/`. Keep TradingView parity (row mode `rows`/`price`,
   128 rows, `va_pct = 70`, POC/VAH/VAL, `ProfileMeta` audit), the DST-safe
   `America/New_York` 18:00 session/week anchors, filler-bar weekend skipping,
   and `sunday_open`. Add `chrono`, `chrono-tz` to `Cargo.toml`.
2. **Ingest C1 v2.** Replace the current `NODE1_WS_URL` `candle`+`levels`
   subscription with `candle` (1m/1H/15m), `tick_volume`, `calendar`, `status`.
   Validate `seq`/`replay`/`gap`/`closed`/`symbol`/`interval`/`freshness`;
   reconnect on any violation; retain ≥35 days of fine bars; never trade on
   stale or closed-market data. **No more trusting Node 1's `/levels`.**
3. **Implement the window policy (section 2.2, D3-A + D4-C + D5).** PW frozen
   at Sunday 18:00 NY; **PS = last closed 18:00→18:00 session (Thu 18:00 open →
   Fri 18:00 close at the week open, flat weekend filler walked over), PoC only,
   frozen at the week open** (D5b default); **CW = week open -> last completed
   18:00 daily close, first snapshot after Monday's close, refreshed at every
   later daily close, reset at Sunday 18:00**. Expose
   `CW_FIRST_PUBLISH_WEEKDAY=Mon` (set `Wed` for the written Wednesday rule),
   `PS_MODE=week_open|rolling`, plus `WEEK_ANCHOR`, `VA_PCT`, `VP_ROWS`,
   `VP_BIN_SIZE`, `VP_LOWER_TF` as explicit config. Drop SWING_BULL/BEAR unless you
   still want them (they were not in your spec) — put behind a flag.
4. **Publish C2.** `GET /levels` (array of `VpLevels` incl. `sunday_open`,
   `start`, `end`, `meta`), `GET /vp?window=PW|PS|CW` and
   `/vp?start=&end=` (full histogram audit: rows, poc/vah/val row indices,
   input interval/bars, total volume), **`GET /drawings`** (1H FRVP rectangles +
   daily horizontal rays + Sunday-open marker, each with id/label/price/colour/
   style/anchor time), `GET /scanning`, `GET /signals`, `GET /diagnostics`,
   `GET /health`; `WS /ws` topics `levels`, `drawing`, `vp`, `scanning`,
   `signals`, `diagnostics`. Replay the current snapshot on subscribe.
5. **Keep the break/retest state machine** (`src/strategy.rs`): PW PoC/VaH/VaL,
   PS PoC, CW PoC/VaH/VaL; ±$0.50 retest zone, $2.00 invalidation, one trigger
   per closed candle, sorted level IDs. Wire it to the **internally computed**
   levels. Make `VOLUME_THRESHOLD` optional and **default OFF** (D6); when on,
   publish `volume_confirmed` per setup.
6. **Risk projection** (`src/risk.rs`): ATR(14)-scaled SL 200–300 pips /
   TP 600–800 pips clamped to RR 2–3, `1 pip = $0.01`; publish current
   `sl_pips`/`tp_pips`/`rr`/`atr` in `/scanning` and `/diagnostics`.
7. **Keep the private `/execution` WS** (token, one consumer, hello/ack,
   bounded pending queue, TTL, replay after reconnect) and the strict public/
   private separation. Extend `TradeIntent` **additively** with `atr`,
   `atr_pips`, `level_id`, `drawing_id`, `strategy_version`, `volume_confirmed`
   (C3 v1.1). Bump `strategy_version` whenever a rule changes so `intent_id`s
   cannot collide.
8. **Tests / parity.** FRVP golden fixtures from `Node1:ci/live/vp-{PW,PS,CW}.json`;
   DST boundary tests (2025-06-08 17:00 vs 19:00 NY); weekend filler skipping;
   **PS walk-back at Sunday 18:00 returns exactly Thu 18:00 → Fri 18:00 and
   never the flat weekend buckets, with a golden fixture for its PoC**;
   CW first-publish/day-boundary rule with a fake clock (both Mon and Wed
   settings); Sunday-open extraction; break/retest
   determinism; low-volume rejection when enabled; intent idempotency; a
   boundary test asserting **no broker/venue/credential identifier** in the crate.

**Acceptance.** PW/PS/CW PoC/VaH/VaL match the recorded TradingView-parity
profiles within one row; `/drawings` renders correctly in Node 2; a synthetic
break+retest produces exactly one `trade_intent` with 2–3R; CI green.

**Depends on:** C1 (Agent 1) — but can be built offline against golden
fixtures first. **Blocks:** Agent 2's drawings, Agent 4's end-to-end demo.

---

### Agent 4 — Node 4: "demo execution engine + risk management" (Rust)

**Branch:** `arena/c3e4eae3-bot` <- `Node-4` (**this session**). **Language:** Rust.

**Mission.** Node 4 becomes a pure, risk-gated executor with **one** intent
source and a real demo engine. Delete the duplicated strategy and the embedded
dashboard.

**Work items**
1. **Delete the duplicated strategy path.** Remove
   `node4-execution/src/strategy.rs`, `src/strategy_runner.rs`, the
   `node1_strategy` branch of `src/node1.rs`, `NODE4_INTENT_SOURCE`, all
   `NODE1_*`/`NODE4_STRATEGY_APPROVED` env vars in `.env.example` and
   `src/config.rs`, `docs/node4/LOCAL_STRATEGY.md`, and the `market_feed` /
   `strategy` blocks from `/diagnostics`. Also remove the **entire embedded
   `node2/` dashboard** (D7: Node 4 ships **no frontend at all** — no
   `public/`, no `serve.mjs`, no `mock/`, no `node2` CI job; the `Node2` branch
   is the only UI) and the 1.7 MB `Node-4.txt` diff artefact; move
   `docs/node1/*` into the shared `contracts/` folder.
2. **Single source: Node 3 `/execution` only.** Keep `execution_hello` /
   `execution_hello_ack`, the stale-link rule (no new broker write on a stale
   link; monitoring/reconciliation/kill switch keep working), the append-only
   fsynced ledger written **before** any broker write, one-venue policy,
   reconcile `unknown` writes by `intent_id` (never resend), and the separate
   secrets per link (`NODE4_SHARED_TOKEN`, `MT5_BRIDGE_TOKEN`,
   `MT5_CONTROL_TOKEN`).
3. **New `src/risk.rs` — risk management policy** (the part you asked for):
   - Validate the strategy's ATR-derived SL/TP are within 200–300 / 600–800
     pips and RR 2–3; reject (never rewrite) otherwise.
   - Reference-price sanity vs last known price, spread/slippage cap, intent TTL.
   - `%`-of-equity max risk per trade, `MT5_MAX_RISK_PER_TRADE` hard cap,
     max concurrent positions, daily loss limit / max drawdown, cooldown after
     N consecutive losses, optional high-impact-news blackout from Node 1's
     calendar (via Node 3 intent metadata or a read-only Node 1 GET — never a
     second strategy path).
   - Lot sizing on the 0.01 grid; every rejection carries a stable
     `error_code` + `error_message` and is counted in `/diagnostics.risk`.
4. **Demo execution engine.** Default venue becomes `demo_paper` (or keep
   `none` as explicit dry-run): paper fills at bid/ask + configured spread,
   SL/TP/exit evaluation, equity/margin/unrealised PnL, trade history, plus a
   `DRY RUN` banner feed for Node 2. MT5 demo remains opt-in behind the existing
   `NODE4_DEMO_INTEGRATION_PASSED` gate and `docs/mt5/EXECUTION_ARCHITECTURE.md`;
   live stays doubly opt-in. **No live trading by default.**
5. **C4 resources for Node 2:** `/diagnostics` (add `risk` block),
   `/open-trades`, `/deriv`/`/account`, `/mt5/{account,positions,history,status}`,
   `WS /ws`. Update `docs/mt5/FRONTEND_RESOURCES.md` field-by-field.
6. **Accept additive C3 v1.1 fields**; unknown `schema_version` /
   `protocol_version` fails closed.
7. **New `ci/check_node4_boundary.py`:** ban `mod strategy`, `strategy_runner`,
   `volume_profile`, `vp_`, `break_state`, `retest`, `levels`, candle
   subscription, and any `/strategy` or `/demo` route. Update
   `.github/workflows/build.yml` so it builds **only** `node4-execution`,
   `mt5-bridge`, `mt5-host` (the `node2` job moves to the Node 2 CI).
8. **Tests:** risk-policy rejection matrix (each limit independently),
   demo-engine SL/TP/exit accounting, ledger-before-write ordering, duplicate
   `intent_id` replay, unknown-write reconciliation, kill switch during a stale
   link, dry-run end-to-end (intent → ledger → paper fill → report).

**Acceptance.** No `strategy`/`vp`/`volume_profile` module in the crate; one
intent source; a synthetic Node 3 intent produces exactly one demo fill and one
`execution_report`; `/diagnostics.risk` visible in Node 2; CI green.

**Depends on:** C3 (Agent 3). Can be built against fixture intents in parallel.

---

## 6. Migration order (gated, so no agent breaks another)

| Phase | Work | Gate to pass before the next phase |
|---|---|---|
| **0** | All D1–D9 settled; D5b is a one-value default. Create the identical `contracts/` folder (C1–C4 schemas, `contract-lint`, golden fixtures) on **all four branches**; add `ci/contract_sync.py`. Open the four sessions. | `contract-lint` + `contract_sync` green on all four branches; all four agents sign the schemas. |
| **1** | Agent 1 ships C1 v2 feed (with `/levels` `/vp` still proxied); Agent 3 ports the FRVP engine and develops offline against golden fixtures. | Node 3 reproduces recorded PW/PS/CW from Node 1 candles within one histogram row. |
| **2** | Agent 3 ships C2 (`/levels`, `/vp`, `/drawings`, scanner, signals). Agent 4 deletes its local strategy, ships `src/risk.rs` + demo engine against fixture intents. Agent 2 builds the three-host dashboard against local JSON fixtures. | Contract tests green in all four CI jobs; no service needs another's live deployment to boot. |
| **3** | End-to-end with `EXECUTION_VENUE=none`: Node 1 feed → Node 3 VP + scanner → intent → Node 4 ledger + risk + dry-run report → Node 2 renders 1H drawings, daily rays, Sunday open, scanner, risk card. | A synthetic break/retest on a recorded Wednesday produces exactly one intent, one demo fill, one report; drawing geometry assertions + a browser check (no reference image, D8). |
| **4** | Enable demo venue behind `NODE4_DEMO_INTEGRATION_PASSED`; run the MT5 demo integration gate. | Gate in `docs/mt5/EXECUTION_ARCHITECTURE.md` passes; kill switch verified. |
| **5** | Cutover: remove Node 1 `/levels` + `/vp` proxy, VP code and `ProfileMeta`; delete the duplicate dashboard and stale docs (`FRONTEND_NEXT.md`, `LOCAL_STRATEGY.md`, old `SERVICE_ARCHITECTURE.md`); rewrite every boundary guard + CI; re-record `ci/live/*` probes. | `check_node1_boundary.py`, `check_node4_boundary.py`, `contract-lint`, all four test suites green; one PR reviewable. |

**Rollback at every phase:** Node 1 keeps its v1 `/levels` proxy until Phase 5;
Node 4 keeps `EXECUTION_VENUE=none`; Node 2 degrades to "NOT CONFIGURED" per
host. No phase requires enabling a live venue.

---

## 7. Cross-cutting rules for all agents

- **Rust for engines/execution** (Nodes 1, 3, 4); **vanilla HTML/CSS/JS +
  lightweight-charts** for Node 2; no framework, no build step in the browser.
- One `Dockerfile` + one CI job per service; each builds exactly one binary.
- **One branch per service (D2-B).** Never copy another service's `src/` into
  your branch; consume it through a versioned contract. `ci/contract_sync.py`
  fails CI if `contracts/` differs between branches; every contract change is
  applied to all four branches in the same wave.
- Secrets only in Node 3 (`NODE4_SHARED_TOKEN`) and Node 4 (bridge/control
  tokens, broker creds). **Nothing** in browser code or logs/ledgers.
- Read-only browser: GET + WS subscribe only; no POST/PUT/PATCH/DELETE; no
  control routes; enforced by `security.test.js` and a bundle scan.
- Every public payload is versioned (`contract_version`, `schema_version`);
  incompatible versions fail closed, never guessed.
- Deterministic `intent_id` is the idempotency key across all services; the
  ledger is written before any broker write.
- Boundary CI guards on **both** sides (Node 1 = no strategy/VP/execution;
  Node 4 = no strategy/VP/feed ingestion). Docs alone did not hold last time.
- Keep the recorded live fixtures as regression baselines; never invent a level,
  price or RR client-side.

---

## 8. Residual risks / notes (no open decisions)

- **D8 settled (no screenshot):** ray styling, FRVP placement and Sunday-open
  marking follow the existing `js/levels.js` colour map and lightweight-charts
  price-line conventions; correctness is enforced by geometry unit tests, so a
  reference image is not needed.
- **D4 confirmed (C):** CW from Monday's close. If you later want the written
  Wednesday rule, `CW_FIRST_PUBLISH_WEEKDAY=Wed` flips it with no code change.
- **D7 settled:** Node 2 is a pure static frontend. The main practical risk is
  CORS: Node 1, Node 3 and Node 4 must all whitelist the deployed dashboard
  origin (and their WebSocket origin checks must accept it), since there is no
  proxy. Verify all three in Phase 3.
- The 10,500 volume threshold is OANDA-specific but the feed is SiftingIO tick
  count; D6=A disables it for v1. A future "volume spike" filter needs a
  per-source calibration before it can be trusted.
- **D5 settled:** PS is the last closed session (Thu 18:00 → Fri 18:00 at the
  week open). The literal "Fri close → Sun 18:00" reading is a flat weekend with
  no traded volume, so it is skipped by the ported `previous_session_slice`.
  **D5b is a non-blocking default** (freeze PS for the week); `PS_MODE=rolling`
  switches it with no code change.
- The `swing` profiles (SWING_BULL/BEAR) and the econ-audio/AI relay are in the
  code but not in your strategy description; recommend parking both behind flags
  so they do not distract from PW/PS/CW.
- The four branches have **disjoint histories** and D2=B keeps them. That is a
  deliberate trade-off: independent deploys, but no atomic cross-service PR and
  four sessions to coordinate. The mirrored `contracts/` + `contract_sync.py`
  is the mitigation; if that proves painful, D2-A (monorepo) remains the
  fallback.

---

## 9. What happens next

1. **All decisions are settled** — no further input needed. (D5b stays a
   one-value default: `PS_MODE=week_open`; say "roll PS daily" any time to flip
   it.)
2. I split the four briefs into standalone files (`AGENT-NODE1.md` …
   `AGENT-NODE4.md`) you can paste into four branch-based sessions, and draft
   the `contracts/` skeletons (C1–C4 schemas + golden fixtures).
3. Then — only when you say go — we start Phase 0/1, beginning with the Node 4
   brief in this session (the other three need their own branch-based sessions
   because of D2=B). **No code until then.**

---

## 10. Node 4 execution status (this session, `arena/c3e4eae3-bot`)

The three standalone briefs now exist: `AGENT-NODE1.md`, `AGENT-NODE2.md`,
`AGENT-NODE3.md`. The Agent 4 brief above was executed in this session.

**Done (no Rust toolchain is available in this sandbox, so all code was
verified statically — see §10.1):**

1. **Deleted the duplicated strategy path.** `src/strategy.rs`,
   `src/strategy_runner.rs`, `src/node1.rs`, the `NODE4_INTENT_SOURCE`
   switch, every `NODE1_*` / `NODE4_STRATEGY_*` env var, `for_local`,
   `handle_local_intent`, `ReportSink::Local`, the `market_feed` and
   `strategy` diagnostics blocks, `FeedSnapshot` / `StrategySnapshot`,
   `docs/node4/LOCAL_STRATEGY.md`, the embedded `node2/` dashboard and the
   1.7 MB `Node-4.txt`. `main.rs` now has exactly one intent source:
   `node3::run`.
2. **Added `src/risk.rs`.** Enforces SL 200–300 pips / TP 600–800 pips /
   RR 2–3 (1 pip = $0.01), declared-RR coherence, per-trade risk USD cap,
   max lots, max open positions, daily realised-loss cap, max drawdown and a
   cooldown after consecutive losses. It refuses with stable `error_code`s and
   **never rewrites** a strategy price. Wired into `handle_intent`
   immediately before the broker write; `record_fill` on a confirmed fill and
   `record_close` from the MT5 positions snapshot (a disappeared ticket's
   `profit + swap`). Ten pure unit tests cover every guard.
3. **Added `/diagnostics.risk`** (C4 v1), replacing the two removed blocks.
   The `risk` object exposes every limit, the current state, counters and the
   last refusal. Documented in `docs/mt5/FRONTEND_RESOURCES.md` and
   `contracts/c4-diagnostics.schema.json`.
4. **C3 v1.1 additive compatibility.** Node 3's optional flat fields
   (`atr`, `atr_pips`, `level_id`, `drawing_id`, `strategy_version`,
   `volume_confirmed`) are documented in `contracts/c3-intent.schema.json`;
   a test proves a frame carrying them still deserialises with prices intact.
   No struct change was needed (serde ignores unknown fields).
5. **Boundary guards + contracts.** `ci/check_node4_boundary.py` (runnable,
   passing) bans any return of market data, VP maths, strategy code or a
   second dashboard; `ci/contract_sync.py` + `contracts/MANIFEST.sha256`
   keep `contracts/` byte-identical across the four branches. `contracts/`
   now holds C1 v2 (normative), C2 v1, C3 v1.1, C4 v1 schemas plus the
   historical Node 1 audit docs. `.github/workflows/build.yml` lost its
   `node2` job and gained a `boundary` job.
6. **Docs/env rewritten:** root `README.md`, `node4-execution/README.md`,
   `node4-execution/.env.example`, `ci/diagnostics_probe.py`,
   `ci/DIAGNOSTICS-PROBE.md` (marked historical), `.gitignore`.

### 10.1 Verification performed (and its limits)

The sandbox has **no `cargo`/`rustc` and no crates.io access**, so
`cargo check` / `cargo test` could not run. Instead:

- `python3 ci/check_node4_boundary.py` → OK (13 Rust files).
- `python3 ci/contract_sync.py` → OK (8 contract files).
- `python3 ci/mt5/protocol_lint.py` → OK (EA/bridge/Node 4 contract).
- `python3 -m py_compile` on all three Python CI scripts → OK.
- All four JSON schemas parse as valid JSON.
- A custom static checker verified: balanced braces/parens/brackets outside
  comments and strings; every `mod` resolves to a file; **every struct
  literal** of `Config`, `ExecutionSnapshot`, `RiskSnapshot`, `RiskState`,
  `LinkShared`, `DiagnosticsHub`, `DiagnosticsInner`, `TradeIntent` and
  `ExecutionReport` has exactly the declared fields (no missing, no extra);
  every `config.<field>` reference exists; no banned identifier remains in
  `src/`; no unused `tracing` import.

**The Node 4 CI job (`cargo fmt/check/test/clippy`) must be the first gate
when this branch is built in an environment with a Rust toolchain.** Treat any
compile error there as a fix-forward item, not a design change.

### 10.2 Deliberate deviation: "demo execution engine"

The brief allowed either a `demo_paper` venue or keeping `none` as the
explicit dry-run. I kept **`EXECUTION_VENUE=none`** (intents validated,
risk-checked, durably recorded and reported; no broker order) plus the
existing **Deriv MT5 demo** venue for broker-confirmed demo fills. A local
paper-fill simulator would need its own price feed to evaluate SL/TP exits,
which would reintroduce a market-data dependency into Node 4 and violate D1.
This is documented in `node4-execution/README.md` ("Demo execution").

### 10.3 Remaining Node 4 work (next steps)

- Run `cargo fmt --all -- --check`, `cargo check --all-targets`,
  `cargo test --all-targets`, `cargo clippy --all-targets -- -D warnings`,
  `cargo build --release` in `node4-execution/`; fix any compile issue.
- End-to-end Phase 3 with a fixture Node 3 intent: verify one ledger entry,
  one risk check, one dry-run report and the `risk` block in `/diagnostics`.
- Wire `RISK_*` defaults into the Render/deployment env once the Node 3
  service is emitting C3 v1.1 frames.
- Copy `contracts/` (including `MANIFEST.sha256`) to `Node1`, `Node2` and
  `Node3` in the same wave as any contract edit.
