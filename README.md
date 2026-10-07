# Node 4 — XAUUSD execution service

This repository contains the **execution service** of the XAUUSD three-node
system, plus the MT5 demo bridge that service delegates terminal order routing
to. Nothing else: the market-data engine (Node 1) and the strategy engine
(Node 3) live in their own services and are not part of this tree.

```
Node 1  market data (candles, volume profile, levels)   → its own service
   │  REST + WS
   ▼
Node 3  strategy: decides side, stop, target, RR        → its own service
   │  WS /execution, protocol v1: `trade_intent`
   ▼
Node 4  execution (THIS REPOSITORY)
   │  exactly one venue, durable idempotency, reports back
   ├── Deriv options demo API     (DERIV_DEMO_API)
   ├── Chelsea live MCP           (MCP_CHELSEA_URL)
   └── Deriv MT5 demo via mt5-bridge (MT5_BRIDGE_TOKEN)
```

Node 4 is an outbound **client** of Node 3: it dials
`NODE3_WS_URL` (`wss://<node3-host>/execution`), authenticates with
`execution_hello`, and refuses to accept any intent until Node 3 answers
`execution_hello_ack { accepted: true }`. It contains **no** market analytics and
**no** strategy logic — it may refuse an unsafe intent, but it never recomputes
the stop-loss or take-profit Node 3 sends.

## Repository layout

| Path | What it is |
|---|---|
| `node4-execution/` | The execution service (intent link, venue policy, ledger, adapters, resources, kill switch). |
| `node4-execution/.env.example` | Every environment variable the service reads, with defaults. |
| `mt5-bridge/` | Terminal-side bridge: runs next to the MT5 terminal, dials **out** to Node 4 at `WS /mt5/bridge`, routes orders to the EA over loopback. |
| `docs/mt5/EXECUTION_ARCHITECTURE.md` | MT5 demo topology, the layered guards, the fail-closed order path and the integration gate. |
| `docs/mt5/FRONTEND_RESOURCES.md` | Field-by-field reference for the JSON/WS resources a frontend can render. |
| `ci/mt5/protocol_lint.py` | Build gate: fails if the EA ⇄ bridge ⇄ Node 4 contract drifts (methods, params, fields, frames). |
| `ci/deriv_probe.py`, `ci/deriv/DERIV.md` | Execution-side Deriv probe/report (which contract shapes are actually tradable). |
| `Dockerfile` | Builds the execution service image only. |
| `.github/workflows/build.yml` | CI: `node4-execution` (check, fmt, clippy `-D warnings`, tests, release) + `mt5-bridge` (protocol lint, check, contract tests). |

## What Node 4 guarantees

* **Authenticated intents only.** No intent is accepted before the handshake is
  acknowledged, and a stale Node 3 link means no new positions (monitoring,
  reconciliation and the kill switch keep working).
* **Durable idempotency.** `intent_id` is the system-wide key. It is written to
  the append-only ledger (`EXECUTION_LEDGER_FILE`, fsynced) **before** any broker
  write; a repeat replays the recorded result and can never place a second order.
  With a real venue selected, an unwritable ledger is a refusal to trade.
* **One venue, no fallback.** Exactly one of `deriv_mt5_demo`, `deriv_demo`,
  `chelsea_live`, `none`. Two configured credential sets are a startup error;
  `none` is an explicit dry-run/signal mode.
* **Broker-authoritative outcomes.** Only a broker-confirmed fill opens a trade.
  A timeout/unclear write is reported `unknown` and reconciled by
  `intent_id`/comment/order/deal/position — never retried as a new order.
* **Demo-only by default.** Live venues are opt-in and gated by the layered demo
  checks described in `docs/mt5/EXECUTION_ARCHITECTURE.md`; the MT5 demo
  integration gate must pass before MT5 trading is enabled.
* **Separate secrets per link.** `NODE4_SHARED_TOKEN` (Node 3 ⇄ Node 4),
  `MT5_BRIDGE_TOKEN` (bridge ⇄ Node 4) and `MT5_CONTROL_TOKEN` (operator kill
  switch) are three distinct values; none of them is ever logged or written to
  the ledger.

## Quick start

```bash
cp node4-execution/.env.example node4-execution/.env   # fill in the values
cd node4-execution
cargo run --release                                    # binds 0.0.0.0:$PORT (default 10000)
```

Dry run with no credentials at all:

```bash
EXECUTION_VENUE=none PORT=10000 cargo run --release
```

Docker (execution service only — the bridge runs on the terminal host):

```bash
docker build -f Dockerfile -t xauusd-node4-execution .
docker run --env-file node4-execution/.env -p 10000:10000 xauusd-node4-execution
```

The MT5 demo venue needs the bridge running next to the terminal; it dials out to
`wss://<node4-host>/mt5/bridge`, so **no inbound port is opened on the terminal
host** and the MT5 password never leaves it. See `mt5-bridge/README.md`.

## Resources

| Endpoint | Access | Description |
|---|---|---|
| `GET /health` | public | `ok` for platform probes. |
| `GET /diagnostics` (alias `/status`) | public | `ExecutionSnapshot`: Node 3 link health (separate from broker health), ledger state, counters, open trades, Deriv account, MT5 bridge view, event log. |
| `GET /open-trades` (alias `/trades`) | public | `OpenTradesSnapshot` — `node4_open_trades`, `deriv_open_trades`, `mt5_open_positions`, `recent_trades`. |
| `GET /deriv` (alias `/account`) | public | Deriv options account: balance, currency, account id, open contracts, unrealized PnL. |
| `GET /mt5/account`, `/mt5/positions`, `/mt5/history`, `/mt5/status` | public | MT5 demo account, broker positions, closed deals, bridge/EA link status. |
| `WS /ws` | public | Live stream of the resources above. Never carries Node 3 intent/report frames. |
| `WS /mt5/bridge` | token | The bridge's private session (`bridge_hello` with `MT5_BRIDGE_TOKEN`). |
| `POST /mt5/control` | token | Kill switch: `halt` (optionally `flatten`), `resume`, `close_all`, `close_position`, with `X-Control-Token: $MT5_CONTROL_TOKEN`. No permissive browser CORS. |

## Validation

```bash
cd node4-execution
cargo fmt --all -- --check
cargo check --all-targets
cargo test  --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release

cd ../mt5-bridge && cargo test --all-targets
cd .. && python3 ci/mt5/protocol_lint.py
```

CI runs the same set on every push (`.github/workflows/build.yml`) and pushes the
failing log back to the branch so an error is readable without the Actions tab.

## Scope guard

This tree deliberately contains no market-data feed, no volume-profile or level
computation, no candle subscription, no break/retest state machine and no other
strategy trigger logic. If a change here starts to look like a trading decision,
it belongs in Node 3.
