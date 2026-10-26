# `xauusd-node3-strategy` (v2.0.0)

Node 3 is the strategy boundary of the XAUUSD system. It:

- builds every volume profile itself from Node 1's closed 1m, 15m, and 1H bars
  (no trust in Node 1 `/levels`),
- publishes PW, PS, and CW levels, FRVP drawings, and the Sunday-open marker,
- runs the break-and-retest state machine on the published levels,
- projects ATR-based stop and target distances, and
- hands one immutable `trade_intent` to Node 4.

Node 3 **decides**. Node 4 **executes**. Node 3 cannot place an order, and it
holds no account, broker, stake, or lot-size state.

> **Verification status.** This crate was written without a Rust toolchain
> available in its build environment. `cargo fmt`, `check`, `test`, `clippy`,
> and `build --release` have **not been run**. Out-of-repository Python checks
> reproduced the golden fixtures' row grid and value area, but they are not a
> substitute for running the Rust test suite.

## Data flow

1. Connect to `NODE1_WS_URL` and subscribe to `candle`, `tick_volume`,
   `calendar`, and `status` (Node 1 contract v2).
2. Validate every frame (see *Feed contract* below). A violation drops the
   connection and reconnects. Node 3 never trades on the violating frame.
3. Store closed bars per resolution (35 days minimum). Open bars only update
   freshness.
4. Recompute PW, PS, and CW from the stored bars. Publish the levels, audits,
   and drawings when they change.
5. On each new closed strategy-interval bar, while the feed is live and the
   market is open, evaluate break and retest for every published level.
6. On a confirmed retest, build one intent with a deterministic `intent_id`,
   queue it, and deliver it to the single authenticated Node 4 client.
7. Keep the intent for replay until Node 4 returns a valid `execution_report`
   or the intent expires.

## Volume-profile windows (decisions D3–D5)

All anchors are **America/New_York 18:00**, DST-aware (`ny.rs`).

| Window | Span | Publishes | Freeze |
|---|---|---|---|
| **PW** (previous week) | previous Sunday 18:00 → Friday 18:00 | PoC, VaH, VaL, `sunday_open` | Frozen at the Sunday open |
| **PS** (previous session) | last closed 18:00→18:00 session. At week open this is Thu 18:00 → Fri 18:00, with the weekend filler walked over | **PoC only** | `PS_MODE=week_open` (default) freezes at week open. `rolling` follows the newest closed session |
| **CW** (current week) | week open → last completed daily 18:00 close | PoC, VaH, VaL | First publish after the `CW_FIRST_PUBLISH_WEEKDAY` close (Mon default, Wed option). Refreshes each daily close. Resets Sunday 18:00 |

`sunday_open` is the open of the first PW bar. It is published on `/levels`
and drawn as a dotted marker.

## Configuration

See [`.env.example`](.env.example) for every variable with its default. The
main ones:

| Variable | Default | Notes |
|---|---|---|
| `NODE1_WS_URL` | Node 1 production URL | C1 v2 feed |
| `PORT` | `10000` | HTTP, public `/ws`, and private `/execution` |
| `NODE4_SHARED_TOKEN` | unset | Required for Node 4 to connect. Never sent to Node 2 |
| `WEEK_ANCHOR` | `sunday-1800-America/New_York` | Only this anchor is supported. Others warn and fall back |
| `VA_PCT` | `70` | Percent. Stored internally as `0.70` |
| `VP_ROWS` | `128` | 2–5000 |
| `VP_BIN_SIZE` | `0.50` | Used only in price-bin mode |
| `VP_LOWER_TF` | `tv` | TradingView lower-timeframe ladder |
| `CW_FIRST_PUBLISH_WEEKDAY` | `Mon` | `Mon` or `Wed` |
| `PS_MODE` | `week_open` | `week_open` or `rolling` |
| `RETAIN_DAYS` | `35` | Never below 35 |
| `STRATEGY_INTERVAL` | `15m` | `1m`, `15m`, or `1h` |
| `VOLUME_THRESHOLD` | `0` | **Off by default (D6).** A positive value enables the gate |
| `SL_MIN_PIPS` / `SL_MAX_PIPS` | `200` / `300` | 1 pip = $0.01 (D9) |
| `TP_MIN_PIPS` / `TP_MAX_PIPS` | `600` / `800` | |
| `RR_MIN` / `RR_MAX` | `2.0` / `3.0` | |
| `SL_ATR_MULT` / `TP_ATR_MULT` | `1.0` / `2.0` | Node 3 design choices before clamping |
| `SIGNAL_TTL_SECS` | `120` | |
| `MAX_PENDING_INTENTS` | `128` | A full queue refuses new intents |

Malformed values never crash the service. They are clamped or replaced by the
default, and each problem is logged as a startup warning.

## Feed contract (Node 1 C1 v2)

Every frame is checked before it reaches the store or the strategy
(`src/feed.rs`). A failed check returns an error, which drops the connection
and triggers a reconnect.

- `contract_version` must be `2.0`.
- `symbol` must be `XAUUSD` where present, and required on candle and tick frames.
- `boot_id` must not change within a connection. A change means Node 1 restarted.
- Live frames carry a contiguous `seq`. A gap, duplicate, or out-of-order value fails. Replay frames carry no `seq`.
- `gap: true` fails.
- `freshness.state` must be `live`, `stale`, `offline`, or `closed_market`.
- Candles must use a supported `interval` and carry `closed`.

A silent feed for more than 90 seconds blocks trading until frames resume.

## Public routes (C2)

| Route | Response |
|---|---|
| `GET /health` | JSON. **HTTP 200 while the process is alive.** `status` is `ok`, `degraded`, or `disconnected` |
| `GET /levels` | `VpLevels[]`: window, poc, vah/val (absent for PS), start, end, `sunday_open`, meta |
| `GET /vp?window=PW\|PS\|CW[&rows=N]` | One published profile (`VpAudit`) |
| `GET /vp?start=<ms>&end=<ms>[&rows=N]` | Profile for an explicit range |
| `GET /drawings` | FRVP rectangles, daily rays, and the Sunday-open marker. Identity `WINDOW:kind:start:end` |
| `GET /scanning` | Setup states, ATR, SL/TP pips, RR |
| `GET /signals` | Pending and recent intents, execution report metadata |
| `GET /diagnostics` (`/status`) | Full diagnostics snapshot |
| `WS /ws` | Topics `levels`, `drawing`, `vp`, `scanning`, `signals`, `diagnostics`. Sends a full replay on subscribe |

Public topics never include private frames. `trade_intent`,
`execution_hello*`, `execution_report`, and `execution_report_ack` are refused
on `/ws` even for `all`.

Node 2 subscribes with:

```json
{"type":"subscribe","topics":["levels","drawing","vp","scanning","signals","diagnostics"]}
```

## Private Node 4 route

Connect to `wss://<node3-host>/execution` and send this as the first frame:

```json
{
  "type": "execution_hello",
  "token": "same value as NODE4_SHARED_TOKEN",
  "service": "xauusd-node4-execution",
  "protocol_version": 1
}
```

Node 3 responds with `execution_hello_ack`, then replays every unexpired
pending intent before streaming new ones. Node 4 must persist `intent_id`
before any broker write. The full schema, idempotency rules, and reconnect
behaviour are in [`../docs/NODE3_NODE4_PROTOCOL.md`](../docs/NODE3_NODE4_PROTOCOL.md).

## Strategy rules

- Retest proximity: ±$0.50 of the level.
- Invalidation: a close $2.00 beyond the level cancels the break.
- One trigger per closed candle, and levels are scanned in sorted-identity order.
- Volume gate: off by default. When enabled, the retest bar must reach `VOLUME_THRESHOLD`.
- Stop and target: ATR(14) (Wilder) multiples, clamped to the pip bands.
- Risk/reward is clamped to `RR_MIN..=RR_MAX`.
- `strategy_version` (`2.0.0`) is bumped on every rule change.

Node 4 must still reject stale, malformed, or unsafe instructions. Strategy
projections are constraints, not broker authorization.

## Layout

```
src/
  main.rs         wiring, Node 1 feed loop, pipeline
  feed.rs         Node 1 C1 v2 validator (fail closed)
  store.rs        closed-bar store, coverage floor
  ny.rs           New York 18:00 calendar and DST rules
  vp/             FRVP port: histogram, TV timeframe ladder, PW/PS/CW windows, audit
  market.rs       publishes levels, audits, drawings; change detection
  drawings.rs     drawing payloads and level identity
  strategy.rs     break and retest state machine, ATR, scanning
  risk.rs         ATR to pips projection (1 pip = $0.01)
  intent.rs       deterministic intent_id and intent construction
  diagnostics.rs  shared state, broadcast, private execution queue
  health.rs       HTTP and WebSocket server, topic filter
  config.rs       environment parsing with warnings
tests/fixtures/   Node 1 golden profiles (PW, PS, CW) for offline parity tests
```

## Develop

```bash
cargo fmt --manifest-path Cargo.toml -- --check
cargo check --manifest-path Cargo.toml --all-targets
cargo test --manifest-path Cargo.toml --all-targets -- --nocapture
cargo clippy --manifest-path Cargo.toml --all-targets -- -D warnings
cargo build --manifest-path Cargo.toml --release --locked
```

CI pins Rust 1.94.0. Tests run offline and use the golden fixtures in
`tests/fixtures/`.
