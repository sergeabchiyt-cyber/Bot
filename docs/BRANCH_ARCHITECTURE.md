# Four-branch service architecture

This is the deployment contract for keeping `Node1`, `Node2`, `Node3`, and the
new `Node4` branch clean, independently deployable, and non-conflicting.
Branches are service boundaries, not feature buckets: a file belongs to the
branch that builds and operates it.

## Ownership matrix

| Branch | Deployed service | Owns | Explicitly forbidden |
|---|---|---|---|
| `Node1` | market-data engine | feed adapters, candles, order flow, tick volume, calendar, volume-profile levels, market WS/REST | strategy state machine, broker adapters, broker credentials, MT5 bridge, account state |
| `Node2` | static dashboard | browser HTML/CSS/JS, read-only views of Nodes 1/3/4 | all secrets, order/control methods, strategy and execution logic |
| `Node3` | strategy engine | break/retest state, ATR price projection, intent queue, public strategy diagnostics, authenticated Node 4 handoff | Deriv/Chelsea/MT5 clients, venue choice, stake/lots, broker account state |
| `Node4` | execution engine | intent validation, idempotency ledger, sizing, venue policy, Deriv/Chelsea adapters, MT5 execution/bridge, broker state, kill switch, audit/reconciliation | feed ingestion, VP computation, strategy trigger decisions |

## Runtime topology

```text
                         read-only market UI
                    ┌─────────────────────────► Node 2
                    │                              ▲
                    │                              │ public diagnostics
Node 1 ── market ───┼──► Node 3 strategy ─────────┼──► Node 4 execution
  ▲                 │      private intent         │       │
  │ feeds           └─────────────────────────────┘       ├── Deriv options
  │                                                       ├── Chelsea MCP
  └── no execution dependency                              └── MT5 bridge/EA
```

Dependencies point one way:

- Node 1 knows nothing about Nodes 3 or 4.
- Node 3 depends on Node 1's market contract.
- Node 4 depends on Node 3's intent contract.
- Node 2 reads public APIs from Nodes 1, 3, and 4 and owns no secret.
- A Node 4 outage must not affect Node 1 market service availability.

## Branch manifests

### `Node1` — keep

- root Rust market engine (`src/*`) after removing execution modules/calls
- market API and WebSocket tests/probes
- volume-profile, order-flow, tick-volume, calendar, and feed documentation

### `Node1` — move to `Node4`, then remove

The current Node 1 tree contains execution material that violates the boundary.
Move it without copying long-term duplicates:

- `mt5-bridge/**`
- `docs/mt5/**`
- `ci/mt5/protocol_lint.py`
- `node3-execution/src/execution.rs`
- `node3-execution/src/execution_deriv.rs`
- `node3-execution/src/execution_chelsea.rs`
- `node3-execution/src/execution_mt5.rs`
- execution-only Deriv probes/reports/workflows
- root `src/execution.rs`, `src/execution_deriv.rs`, and
  `src/execution_chelsea.rs`
- root execution configuration (`DERIV_*`, `MCP_CHELSEA_URL`, SL/TP/RR order
  fields) and the closed-candle execution fan-out

Node 1 may continue publishing `candle` and `levels`. It must stop accepting a
Node 3 `trades` write-back as an execution mechanism. If a read-only trade tape
is desired, Node 2 should read it from Node 4.

### `Node2` — endpoint split

Replace the old single `EXECUTION_HOST` assumption with three explicit public
hosts:

```text
MARKET_HOST   = Node 1: /ws, /candles, /levels, /calendar, /tick-volume
STRATEGY_HOST = Node 3: /ws, /diagnostics, /scanning, /signals
EXECUTION_HOST= Node 4: /ws, /diagnostics, /open-trades, /account, /mt5/*
```

`js/execution.js` must render strategy scanner/signal state from Node 3 and
broker fills/account/positions from Node 4. Node 2 must never call private Node
3 `/execution` or any token-gated Node 4 control route.

Add a Node 2 `README.md` documenting the three read-only dependencies and the
fact that endpoint URLs are public but tokens are forbidden in browser code.

### `Node3` — this branch

- `node3-strategy/**`
- strategy-only root `Dockerfile`
- strategy build workflow
- architecture and protocol documentation

There must be no `execution_deriv.rs`, `execution_chelsea.rs`,
`execution_mt5.rs`, MT5 EA/bridge, Deriv credential handling, or broker account
monitor in this branch.

### `Node4` — new branch/service

Seed Node 4 with execution material moved from Node 1, then adapt the old Node 3
executor into a consumer of the protocol in `NODE3_NODE4_PROTOCOL.md`:

- `node4-execution/**` — authenticated Node 3 client, execution policy,
  idempotency store, venue adapters, broker diagnostics/control
- `mt5-bridge/**` — Rust bridge and `mql5/Mt5BridgeEA.mq5`
- `docs/mt5/**` — rewritten so every “Node 3 execution” reference says Node 4
- `ci/mt5/protocol_lint.py` and execution-only live/probe workflows
- root Docker/build files that build Node 4 only

Node 4 configuration includes `NODE3_WS_URL`, `NODE4_SHARED_TOKEN`, venue
selection, broker credentials, account sizing, and MT5 settings. It must not
accept market frames directly from Node 1 or reimplement strategy logic.

## Deployment ports and URLs

Each service receives its own deployment and hostname. Reusing port `10000`
inside separate containers is safe; no service should bind another service's
port or package another service's binary.

Recommended variables:

| Service | Variables that identify dependencies |
|---|---|
| Node 1 | none (upstream feed variables only) |
| Node 2 | static `MARKET_HOST`, `STRATEGY_HOST`, `EXECUTION_HOST` |
| Node 3 | `NODE1_WS_URL`, `NODE4_SHARED_TOKEN` |
| Node 4 | `NODE3_WS_URL=wss://<node3>/execution`, `NODE4_SHARED_TOKEN`, venue variables |
| MT5 bridge host | `NODE4_WS_URL=wss://<node4>/mt5/bridge`, bridge/EA credentials |

Do not retain the old variable name `NODE3_WS_URL` inside the MT5 bridge after
moving it: the bridge terminates on Node 4, so call it `NODE4_WS_URL` to make a
wrong deployment visually obvious.

## Safe migration order

1. Create Node 4 with trading disabled and import all execution/MT5 files from
   Node 1. Rename contracts and docs from Node 3 execution to Node 4.
2. Deploy Node 4 in monitor-only mode. Verify account/positions/history and the
   MT5 demo guards; no broker writes.
3. Deploy this Node 3 strategy build with `NODE4_SHARED_TOKEN`. Confirm Node 4
   authenticates, receives an intentionally generated dry-run intent, persists
   the ID, and reports `accepted` without execution.
4. Update Node 2 to read strategy state from Node 3 and broker state from Node
   4. Keep control tokens server-side.
5. Remove execution code and credentials from Node 1. Verify Node 1 market
   probes and Node 3 reconnect behavior.
6. Pass the MT5 integration gate in `MT5_NODE4_RUNBOOK.md` on a Deriv MT5 demo
   account before enabling the Node 4 venue.
7. Enable exactly one Node 4 venue. There is no fallback venue.

## Non-conflict checklist

Before deploying any branch:

- Its Dockerfile builds exactly one service.
- Its CI working directory points only at that service.
- Its `.env.example` contains only variables owned by that service.
- Broker secrets exist only in Node 4/bridge deployment environments.
- Browser files contain no shared/control token.
- Node 3 has no venue dependency and Node 1 has no execution dependency.
- Protocol versions are explicit and incompatible versions fail closed.
- MT5 docs, EA protocol lint, and bridge tests live with Node 4, not Node 1.
