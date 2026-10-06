# `xauusd-node3-strategy`

Node 3 is the strategy boundary of the XAUUSD system. It consumes Node 1
market frames, tracks PW/PS/CW levels, evaluates the volume-profile
break-and-retest state machine, projects strategy SL/TP prices, and hands an
immutable intent to Node 4. It cannot place an order.

## Data flow

1. Connect to `NODE1_WS_URL`.
2. Subscribe to `candle` and `levels` only.
3. Track PW PoC/VaH/VaL, PS PoC, and CW PoC/VaH/VaL.
4. Confirm a clean break, retest, and volume threshold.
5. Compute the strategy stop, target, and risk/reward projection.
6. Queue a versioned `trade_intent` with a deterministic `intent_id`.
7. Deliver it to the one authenticated Node 4 client on `/execution`.
8. Retain it for replay until Node 4 returns a valid `execution_report` or the
   intent expires.

Node 4 performs account sizing, venue selection, symbol normalization, quote
freshness checks, broker validation, order placement, reconciliation, and all
MT5 operations.

## Configuration

```env
NODE1_WS_URL=wss://engine-southeastasia-sng-main.onrender.com/ws
PORT=10000
NODE4_SHARED_TOKEN=replace-with-a-long-random-service-secret
SIGNAL_TTL_SECS=120
MAX_PENDING_INTENTS=128

VOLUME_THRESHOLD=10500
SL_MIN_PIPS=200
SL_MAX_PIPS=300
TP_MIN_PIPS=600
TP_MAX_PIPS=800
RR_MIN=2.0
RR_MAX=3.0
RUST_LOG=info
```

`NODE4_SHARED_TOKEN` is a service-to-service secret. Put the same value on Node
3 and Node 4, never in Node 2. If it is absent, Node 3 still scans and queues
signals for diagnostics, but `/execution` rejects every client.

Broker variables such as `DERIV_DEMO_API`, `DERIV_APP_ID`,
`MCP_CHELSEA_URL`, `MT5_LOGIN`, `MT5_PASSWORD`, `MT5_BRIDGE_TOKEN`,
`MT5_VOLUME_LOTS`, and `EXECUTION_VENUE` are intentionally unsupported here.
They belong on Node 4 or its colocated MT5 bridge.

## Public routes

| Route | Response |
|---|---|
| `/health` | `ok` |
| `/diagnostics`, `/status` | full `DiagnosticsSnapshot` |
| `/scanning` | `ScanningSnapshot` |
| `/signals` | `SignalSnapshot` |
| `/ws` | public diagnostics WebSocket |

Node 2 subscribes with:

```json
{"type":"subscribe","topics":["diagnostics","scanning","signals","diagnostic_event"]}
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

Node 3 responds with `execution_hello_ack`, then replays all unexpired pending
intents before streaming new ones. Node 4 must persist `intent_id` before any
broker write and return at least an `accepted`, `rejected`, or `unknown`
`execution_report`. Later `filled`, `partial`, `cancelled`, or `closed` reports
may follow.

The full schema and reconnect rules are in
[`../docs/NODE3_NODE4_PROTOCOL.md`](../docs/NODE3_NODE4_PROTOCOL.md).

## Strategy rules

- Retest proximity: ±$0.50.
- Invalidation distance: $2.00 beyond the level.
- Volume confirmation: `candle.volume >= VOLUME_THRESHOLD`.
- SL/TP distance is ATR-adjusted within the configured pip bounds.
- Risk/reward is clamped to `RR_MIN..=RR_MAX`.
- Node 3 emits at most one level trigger for a source candle.

The projection is a strategy constraint, not broker authorization. Node 4 must
still reject stale, malformed, unsafe, or unsupported instructions.

## Develop

```bash
cargo fmt --manifest-path Cargo.toml -- --check
cargo check --manifest-path Cargo.toml --all-targets
cargo test --manifest-path Cargo.toml --all-targets -- --nocapture
```
