# Node 4 — local strategy on the Node 1 market feed

**Status: opt-in, default off, fail-closed.** This document describes the
`NODE4_INTENT_SOURCE=node1_strategy` path. It exists because the Node 4 handoff
asks Node 4 to consume Node 1 market data, run the approved strategy and execute
through the existing guarded pipeline. The default `node3` path is unchanged.

> **The Node 1 feed contract is not signed off.** The implementation targets
> `docs/node1/NODE1_FEED_CONTRACT_v1.md` from commit `39896e2` (PR #42), which is
> still marked **DRAFT pending agreement** and currently lists Node 4 as *“not a
> consumer — acknowledges only”*. Until that changes and every party signs, this
> mode must stay off. The deployed Node 1 (`origin/Node1 @ d007137`, build
> 0.3.0) does not emit any of the v1 fields, so the consumer keeps its gate
> closed and executes nothing against it.

## Source provenance

| Artefact | Source | Use |
|---|---|---|
| Feed contract | `docs/node1/NODE1_FEED_CONTRACT_v1.md` @ `39896e2` | transport, envelope, ordering, replay, candle-close and freshness semantics |
| Strategy | `origin/Node3 @ fff6bfd`, `node3-strategy/src/{strategy,risk}.rs` and `main.rs::build_intent` | verbatim port in `src/strategy.rs` |
| Execution pipeline | `src/node3.rs::handle_intent` | shared by both sources; not duplicated |

No field, threshold or rule was invented for this path. Where the draft leaves a
decision open (§13 of the contract), the consumer refuses to trade instead of
guessing.

## Flow

```
Node 1 /ws ──► src/node1.rs ──► FeedEvent ──► src/strategy_runner.rs
                (contract,         (closed       (replay = history only;
                 seq, gap,          candle,       live + trading_ready = decide)
                 freshness,         level)
                 backoff)                 │
                                          ▼
                                   src/strategy.rs
                                   (vp_break_retest_v1,
                                    ATR SL/TP/RR)
                                          │ immutable TradeIntent
                                          ▼
                              src/node3.rs::handle_local_intent
                              validate → ledger dedupe → persist
                              → venue policy → at most one broker
                              order → broker-authoritative report
                              → reconcile `unknown`, never resend
```

The strategy module has no clock, no network and no broker adapter. The runner
supplies the timestamp; the execution pipeline owns everything after the intent.

## What the consumer requires from Node 1

The consumer subscribes to `candle`, `levels`, `tick_volume` and `status` and
accepts data only when all of the following hold:

* `status.data.contract_version` equals `NODE1_CONTRACT_VERSION` (`1.0`);
* every live frame carries `seq`; replay frames (`replay: true`) must not;
* `seq` is strictly increasing — a duplicate is dropped, a gap or a lower value
  forces a reconnect (a fresh replay), never a best-effort append;
* a `gap` frame forces a reconnect;
* candles carry `closed`, `symbol`, `interval`, are boundary-aligned, valid
  OHLCV and strictly increasing by closed time;
* only `closed: true` candles reach the strategy, and never twice;
* `status.freshness.state == "live"`, `market_open == true`,
  `age_ms <= NODE1_STALE_MS`;
* the socket has not been silent longer than `NODE1_HEARTBEAT_TIMEOUT_MS`;
* the last closed candle is younger than `NODE1_MAX_CANDLE_AGE_MS`;
* at least `NODE1_MIN_CLOSED_CANDLES` closed candles were observed.

Any violation closes the gate. The gate is checked when a candle triggers a
decision **and again** inside the execution pipeline immediately before the
broker write (`LinkShared::source_gate`).

## Deterministic intent identity

```
n4-<strategy>-<symbol>-<interval>-<source_candle_time>-<side>-<level_id>
```

e.g. `n4-vp-break-retest-v1-xauusd-15m-1792780200000-buy-pw-poc`.

* The source candle is the **closed** bar's bucket-open epoch-ms; no wall clock
  is part of the key.
* A reconnect, resubscribe, duplicate feed frame or replay of the same event
  produces the same `intent_id`.
* The `n4-` prefix deliberately differs from Node 3's `n3-…` so the two sources
  can share one durable ledger without ever colliding.
* The ledger is written **before** any broker write. A duplicate replays the
  recorded result; it never places a second order. An unclear write stays
  `unknown` and is reconciled by `intent_id`, never resent as a new order.

## Startup validation

`NODE4_INTENT_SOURCE=node1_strategy` is a fatal startup error unless:

* `NODE1_FEED_ENABLED=1`;
* `NODE4_STRATEGY_APPROVED` is set (the joint sign-off gate for the draft
  contract **and** the strategy specification);
* `NODE1_WS_URL` is a `ws(s)://` URL (plaintext only for loopback unless
  `NODE1_REQUIRE_TLS=0`);
* `NODE1_INTERVAL` parses as `<int><m|h|d>`;
* `NODE1_FEED_REQUIRE_AUTH=1` implies `NODE1_FEED_TOKEN` is set;
* `NODE1_SYMBOL` and `NODE1_CONTRACT_VERSION` are non-empty.

With a real venue, `NODE4_DEMO_INTEGRATION_PASSED=1` is also required, and
`EXECUTION_VENUE=chelsea_live` additionally requires
`NODE4_LIVE_TRADING_APPROVED=1`. `EXECUTION_VENUE=none` (dry-run) remains the
default, and with it the pipeline validates and durably records the intent but
places no order.

## Activation checklist

1. The Node 1 owner updates the contract to list Node 4 as a consumer and every
   party signs; the §13 open decisions that affect this consumer (sequence
   placement, replay size, stale thresholds, duplicate ticks) are resolved.
2. Node 1 ships the v1 producer (`closed`, `symbol`, `interval`, `seq`, `gap`,
   `contract_version`, `freshness`).
3. A read-authentication decision exists. Contract v1 today has **no read
   auth** (only CORS); the consumer can send `Authorization: Bearer` but cannot
   make Node 1 verify it.
4. The strategy specification is signed off as `vp_break_retest_v1` with the
   parameters in `node4-execution/.env.example`.
5. Run against a Node 1 staging feed with `EXECUTION_VENUE=none` and verify
   `/diagnostics` (`market_feed`, `strategy`), resyncs, duplicate counts and
   suppressed decisions.
6. Run the demo integration gate with a demo venue; only then set
   `NODE4_DEMO_INTEGRATION_PASSED=1`. Live stays opt-in with
   `NODE4_LIVE_TRADING_APPROVED=1`.

## Tests

* `src/node1.rs` — replay→live readiness, sequence gaps, missing v1 fields,
  duplicate closed candles, stale freshness/closed market, forbidden execution
  frames, exact gate reasons, replay size.
* `src/strategy.rs` — level mapping, break/retest/invalidation, low-volume
  rejection, deterministic IDs, ATR, risk projection and RR clamping.
* `src/strategy_runner.rs` — replay never emits, monitor mode, and a replayed
  strategy event produces exactly one ledger entry and zero orders.
* `src/node3.rs` — the existing idempotency, reconciliation and report tests
  still run against the shared pipeline.

## Rollback

Set `NODE4_INTENT_SOURCE=node3` (or remove it) and restart. The feed task and
strategy runner are not spawned in that mode; `/diagnostics` reports the feed as
disabled and the strategy as `monitor`. The Node 3 execution path and the
independent MT5 bridge protocol are untouched by this change.
