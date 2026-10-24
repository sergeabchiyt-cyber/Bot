# Node 4 execution snapshot — the fields Node 2 renders

Frontend-facing copy of the two **additive** sections of `ExecutionSnapshot`
introduced by Node 4 PR #44 (`NODE4_INTENT_SOURCE=node1_strategy`).

The authoritative definition is `node4-execution/src/types.rs`. This document
exists so the dashboard's rendering rules stay next to the field names they
depend on. A field rename here is a cross-repo change: CI
(`ci/mt5/protocol_lint.py`) pins `types.rs` against `mt5-bridge/src/snapshot.rs`.

> Node 4 owns `docs/mt5/FRONTEND_RESOURCES.md`. Section 3 ("What is *not*
> exposed") still needs these two tables added on the `Node-4` branch — this
> file is the Node 2 copy, kept field-accurate until that lands.

## Transport (unchanged, additive)

Both places carry the same shape, with no envelope on REST:

| Transport | Path | Payload |
|---|---|---|
| REST | `GET /diagnostics` (alias `GET /status`) | bare `ExecutionSnapshot` |
| WebSocket | `/ws` (aliases `/`, `/stream`, `/diagnostics/ws`) | `{"type":"diagnostics","data":ExecutionSnapshot}` |

Subscribe with `{"type":"subscribe","topics":["diagnostics"]}`. An empty or
omitted topic list means "everything"; `"all"` and `"*"` are wildcards.

**Every frame is a complete snapshot.** Node 2 replaces its stored snapshot
wholesale and never merges, so resets (reconnects, resyncs, cleared counters)
reach the UI. The only guard is that an *older* REST seed (smaller `timestamp`)
cannot overwrite a newer socket frame.

All `*_ms` / `*_ts` values are epoch milliseconds (Node 2 also accepts epoch
seconds from a hand-written fixture). `Option` fields serialise as `null`, never
omitted. Field names are exactly the `snake_case` below.

## `market_feed` — Node 1 consumer health

Always present; `enabled: false` and the default state when the feed is off.

| Field | Type | Meaning |
|---|---|---|
| `enabled` | bool | The feed task is running (always true in `node1_strategy`; may also be true as a read-only monitor in `node3` mode) |
| `state` | string | Transport state — exactly the set below |
| `trading_ready` | bool | **The only field the execution gate reads**; recomputed on every read |
| `not_ready_reason` | string \| null | Operator-facing reason the gate is closed; `null` only when ready |
| `url` | string | Feed URL (no credentials in it) |
| `auth` | string | `"none (contract v1)"` or `"bearer"` — never the token |
| `contract_version` | string \| null | Version Node 1 actually reported |
| `contract_required` | string | Pin Node 4 accepts (`"1.0"`) |
| `symbol`, `interval` | string | `XAUUSD`, `15m` |
| `connected` | bool | Socket up |
| `last_frame_ms` | int \| null | Last frame of any kind |
| `last_status_ms` | int \| null | Last `status` frame (contract + freshness) |
| `last_candle_time` | int \| null | Last candle bar, forming or closed |
| `last_closed_candle_time` | int \| null | Last **closed** bar (bucket-open time) |
| `last_levels_ts` | int \| null | Timestamp inside the last `levels` frame |
| `last_seq` | int \| null | Last live envelope sequence; replay frames don't advance it |
| `session_closed_candles` | int | Closed candles since this socket connected (replay seeds 15) |
| `sync_complete` | bool | A live (non-replay) frame has arrived after the subscribe replay |
| `contract_ok` | bool | Reported version equals the pin |
| `freshness_state` | string \| null | Node 1's verdict: `live` \| `stale` \| `offline` \| `closed_market` |
| `freshness_age_ms` | int \| null | Age Node 1 reports for its data |
| `market_open` | bool \| null | Node 1's market-hours verdict |
| `reconnect_count` | int | Successful (re)connects |
| `frames_received` | int | Frames of any kind |
| `candles_received` | int | Candle frames (forming + closed) |
| `closed_candles` | int | Closed candle frames accepted |
| `levels_received` | int | Levels frames accepted |
| `status_frames` | int | Status frames accepted |
| `duplicates` | int | Duplicate frames dropped |
| `out_of_order` | int | Frames older than the accepted sequence |
| `gaps` | int | Sequence gaps / `gap` frames |
| `contract_violations` | int | Malformed or forbidden frames |
| `resyncs` | int | Forced resyncs (reconnects) |
| `last_error` | string \| null | Last transport/contract error |
| `last_event_ms` | int \| null | Last state change |

### `state` — the eight values the code actually sets

`disabled`, `connecting`, `subscribing`, `live`, `resyncing`,
`contract_mismatch`, `contract_violation`, `offline`.

The Rust doc comment also lists `syncing`, `stale` and `market_closed`, but the
code never sets them. Staleness and a closed market are conveyed by
`freshness_state`, `trading_ready` and `not_ready_reason` — not by `state`.

`state` also only upgrades to `live`; a silent socket does not downgrade it
until the watchdog reconnects. **`state` can therefore read `live` while
`trading_ready` is `false`.** The dashboard never derives health colour from
`state` (see below).

## `strategy` — the local strategy runner

Always present; disabled/default in `node3` mode.

| Field | Type | Meaning |
|---|---|---|
| `enabled` | bool | The local strategy is the intent source |
| `mode` | string | `"local"` = this service decides; `"monitor"` = feed may run, no intents; `"node3"` = unattached default, treat as monitor |
| `strategy_version` | string | `vp_break_retest_v1` |
| `symbol`, `interval` | string | What it trades |
| `volume_threshold` | float | Tick-volume confirmation threshold (10500) |
| `sl_min_pips`, `sl_max_pips`, `tp_min_pips`, `tp_max_pips` | float | Risk band |
| `rr_min`, `rr_max` | float | RR clamp band |
| `ttl_secs` | int | Intent lifetime |
| `levels_tracked` | int | Volume-profile levels in the state machine (max 7) |
| `candles_observed` | int | Closed candles fed to the history |
| `last_candle_time` | int \| null | Last observed closed bar |
| `atr` | float | Current ATR used for SL/TP projection |
| `last_decision_at_ms` | int \| null | Last evaluation |
| `last_action` | string | `buy` \| `sell` \| `flat` (last decision) |
| `last_trigger_level` | string \| null | Level that fired, e.g. `"PW PoC"` |
| `last_intent_id` | string \| null | Deterministic id, e.g. `n4-vp-break-retest-v1-xauusd-15m-1792780200000-buy-pw-poc` |
| `intents_emitted` | int | Intents produced |
| `intents_suppressed` | int | Signals dropped because the feed gate was closed |
| `last_suppression_reason` | string \| null | Why the last signal was dropped |
| `last_error` | string \| null | Runner error |
| `updated_at` | int \| null | Last handle update |
| `breaks_detected`, `breaks_invalidated`, `retests_rejected_low_volume`, `signals_confirmed` | int | State-machine counters (monotonic since start) |

## How Node 2 renders it

| Rule | Implementation |
|---|---|
| Readiness | `trading_ready === true` → green "Market data ready". Otherwise amber when the feed is up (`enabled && connected`), red when it is not connected or `state` is `contract_mismatch` / `contract_violation`. `enabled === false` → grey "Feed disabled". |
| Reason | `not_ready_reason` is rendered verbatim, in the backend's own words. The dashboard never synthesises readiness from `last_candle_time`, `freshness_age_ms` or `state`. |
| `state` | Shown as an explanatory value only ("Transport"). It never drives colour. |
| Monitor mode | `strategy.mode !== "local"` labels both cards **monitor only** and the panel header "MONITOR ONLY", even when `trading_ready` is true. |
| Node 3 link | In local mode the link is grey **"Not used — local strategy"** — Node 4 never dials Node 3, so `node3.state` stays `starting`, `connected: false`, `url: ""`. That is not a failure. |
| Suppression | Only visible as `strategy.intents_suppressed` + `strategy.last_suppression_reason`; feed and strategy changes produce no `recent_events` / `diagnostic_event` entries. |
| Feed integrity | `duplicates`, `gaps`, `out_of_order`, `contract_violations`, `resyncs` as a compact row; the `contract_violations` badge turns red on any non-zero value. |
| Dry run | `work.venue === "none"` shows a full-width **DRY RUN** banner and a venue-card note: intents are recorded, no broker order is placed. |
| Counts | `work.intents_duplicate` is shown as **"duplicates rejected"**, neutral: it is the expected count after a reconnect, never an error. |
| Missing sections | A snapshot without `market_feed` / `strategy` renders those cards as "unavailable" and leaves every other panel working. Nothing throws. |

## What is deliberately not represented

* There is no `intent_source` field. The mode comes from `strategy.mode`
  (`"local"` vs everything else). A first-class flag would be a Node 4 change.
* There are no feed/strategy entries in `recent_events` and no
  `diagnostic_event` frames for them.
* There is no per-intent strategy history ("why did it not fire"); only
  `last_action`, `last_suppression_reason` and the counters.
* `work.role` stays `"execution_only"` in both modes. It names the execution
  boundary, not the origin of the decision, and is not a mode indicator.
* `market_feed.auth` is a label. No token is ever exposed, and
  `market_feed.url` is Node 4's **outbound** feed URL — the browser never
  connects to it and it is rendered as plain text, never as a link.
