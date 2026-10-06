# Node 3 — XAUUSD Strategy Engine

This branch contains **one service only**: the Node 3 strategy engine. Node 3
consumes market data and volume-profile levels from Node 1, evaluates the
break-and-retest strategy, and emits versioned, idempotent trade intents to
Node 4 over an authenticated private WebSocket.

Node 3 does **not** connect to MT5, Deriv, Chelsea MCP, or any other broker. It
has no broker credentials, no account sizing, and no order placement code.
Execution and broker-authoritative state belong exclusively to the new Node 4
execution branch/service.

## Clean service split

```text
Node 1 (market/data) ── candle + levels ──► Node 3 (strategy)
                                                   │
                                      private /execution
                                      immutable trade_intent
                                                   │
                                                   ▼
Node 2 (browser) ◄── public diagnostics ── Node 4 (execution) ──► MT5 / venues
        │                                      │
        └── Node 1 market + Node 3 strategy ───┘
```

| Node | Branch/service responsibility | Must not contain |
|---|---|---|
| Node 1 | feeds, candles, order flow, volume profile, calendar | strategy decisions, broker credentials, execution clients, MT5 bridge |
| Node 2 | static browser dashboard | secrets, broker writes, strategy or execution logic |
| Node 3 | strategy state machine, risk-price projection, trade intents | broker adapters, account sizing, MT5/Deriv credentials |
| Node 4 | execution policy, idempotency, broker adapters, MT5 bridge and broker state | market-data analytics or strategy trigger logic |

See [the branch architecture](docs/BRANCH_ARCHITECTURE.md), the
[Node 3 ↔ Node 4 protocol](docs/NODE3_NODE4_PROTOCOL.md), and the
[Node 4 MT5 handoff/runbook](docs/MT5_NODE4_RUNBOOK.md).

## Run Node 3

```bash
cd node3-strategy
cp .env.example .env
# Set NODE4_SHARED_TOKEN to the same random service secret configured on Node 4.
cargo run --release
```

Or build from the repository root:

```bash
docker build -t xauusd-node3-strategy .
docker run --env-file node3-strategy/.env -p 10000:10000 xauusd-node3-strategy
```

## Interfaces

### Node 1 → Node 3

Node 3 connects to `NODE1_WS_URL`, subscribes only to `candle` and `levels`,
and never publishes trades back to Node 1.

### Public diagnostics (Node 2/operator)

| Resource | Purpose |
|---|---|
| `GET /health` | platform health probe |
| `GET /diagnostics` (`/status`) | strategy health, counters, scanner, and signal delivery state |
| `GET /scanning` | current level scanner state and strategy-side SL/TP projections |
| `GET /signals` | pending/recent intents and Node 4 reports (no credentials) |
| `WS /ws` | `diagnostics`, `scanning`, `signals`, `diagnostic_event`, `heartbeat` |

### Private execution link (Node 4 only)

`WS /execution` requires the first frame to be `execution_hello` with the
shared `NODE4_SHARED_TOKEN`. Only one authenticated Node 4 consumer is allowed.
Node 3 sends `trade_intent`; Node 4 persists the intent ID, validates it,
executes at most once, and returns `execution_report`.

The public `/ws` route cannot subscribe to or receive private execution frames,
even with `topics: ["all"]`.

## Safety properties

- A deterministic `intent_id` is the cross-service idempotency key.
- Unacknowledged, unexpired intents replay after Node 4 reconnects.
- Intent TTL defaults to 120 seconds; Node 4 must also reject expired intents.
- The pending queue is bounded. A full queue refuses a new intent instead of
  emitting an untracked order.
- Node 3 contains no stake, lot-size, account, venue, password, API token, or
  MT5 control configuration. Node 4 owns all account-aware decisions.
- No fallback execution exists in Node 3.

## Test

```bash
cargo test --manifest-path node3-strategy/Cargo.toml --all-targets -- --nocapture
cargo check --manifest-path node3-strategy/Cargo.toml --all-targets
```
