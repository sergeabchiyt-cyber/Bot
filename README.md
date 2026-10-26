# Node 4 — XAUUSD demo execution + risk management

This branch is the **execution service** of the four-node XAUUSD system. It is
a Rust service that consumes **authenticated trade intents from Node 3**,
validates them against its risk policy, durably records the `intent_id`, places
**at most one** order per intent, and reports the broker-authoritative outcome.

It contains **no market data, no volume-profile maths and no strategy**, and it
ships **no dashboard**. Node 2 is a separate, frontend-only branch.

```
Node 1  market data + calendar              → its own branch (`Node1`)
   │  C1 v2: candles (1m/15m/1h), tick volume, calendar, status
   ▼
Node 3  VP + strategy                       → its own branch (`Node3`)
   │  C2 public: /levels /vp /drawings /scanning /signals
   │  C3 private: WS /execution, `trade_intent`
   ▼
Node 4  execution + risk (THIS BRANCH)
   │  C4 public: /diagnostics (incl. `risk`), /open-trades, /mt5/*, /ws
   ├── none            explicit dry-run / signal mode (default)
   ├── deriv_mt5_demo  Deriv MT5 **demo** via mt5-bridge
   ├── deriv_demo      Deriv options demo
   └── chelsea_live    LIVE, doubly opt-in (never the default)
            ▲
Node 2  vanilla HTML/CSS/JS + lightweight-charts   → own branch (`Node2`)
```

**One intent source only.** Node 4 is an outbound client of Node 3: it dials
`NODE3_WS_URL` (`wss://<node3-host>/execution`), authenticates with
`execution_hello`, and refuses any intent until Node 3 answers
`execution_hello_ack { accepted: true }`. The old opt-in
`NODE4_INTENT_SOURCE=node1_strategy` mode — a verbatim copy of Node 3's
strategy — has been **deleted**. The Node 1 feed consumer, the strategy
modules, the embedded `node2/` dashboard and `Node-4.txt` are gone.

See [`PLAN.md`](PLAN.md) for the full refactor plan and
[`contracts/README.md`](contracts/README.md) for the versioned service
contracts (C1–C4). The other three nodes have standalone briefs:
[`AGENT-NODE1.md`](AGENT-NODE1.md), [`AGENT-NODE2.md`](AGENT-NODE2.md),
[`AGENT-NODE3.md`](AGENT-NODE3.md).

## Repository layout

| Path | What it is |
|---|---|
| `node4-execution/` | The execution service (Node 3 link, risk policy, ledger, venue adapters, resources, kill switch). |
| `node4-execution/.env.example` | Every environment variable the service reads, with defaults. |
| `mt5-bridge/` | Terminal-side bridge: runs next to the MT5 terminal, dials **out** to Node 4 at `WS /mt5/bridge`. |
| `mt5-host/` | Headless Linux host that installs/supervises the MT5 terminal under Wine. |
| `contracts/` | Shared C1–C4 contracts, mirrored byte-identically on all four branches. |
| `ci/check_node4_boundary.py` | Build gate: fails if market data, VP maths, strategy code or a second dashboard reappears. |
| `ci/contract_sync.py` | Build gate: verifies `contracts/MANIFEST.sha256` so contracts cannot drift between branches. |
| `ci/mt5/protocol_lint.py` | Build gate: fails when the EA ⇄ bridge ⇄ Node 4 contract drifts. |
| `docs/mt5/EXECUTION_ARCHITECTURE.md` | MT5 demo topology, layered guards, fail-closed order path. |
| `docs/mt5/FRONTEND_RESOURCES.md` | Field-by-field reference for the JSON/WS resources Node 2 renders. |
| `Dockerfile` | Builds the execution service image only. |

## What Node 4 guarantees

* **Authenticated intents only.** No intent is accepted before the Node 3
  handshake is acknowledged, and a stale Node 3 link means no new positions.
* **Risk before every write.** `src/risk.rs` enforces the ATR-derived bands —
  SL 200–300 pips ($2–3), TP 600–800 pips ($6–8), RR 2–3 — plus a per-trade
  risk cap, max lots, max open positions, daily loss limit, max drawdown and a
  cooldown after consecutive losses. A violation is a **refusal** with a stable
  `error_code`; Node 4 never silently rewrites a strategy price.
* **Durable idempotency.** `intent_id` is the system-wide key, written to the
  append-only ledger (fsynced) **before** any broker write. A repeat replays the
  recorded result and can never place a second order.
* **One venue, no fallback.** Exactly one of `deriv_mt5_demo`, `deriv_demo`,
  `chelsea_live`, `none`. `none` is an explicit dry-run/demo mode.
* **Demo only by default.** A real venue requires
  `NODE4_DEMO_INTEGRATION_PASSED=1`; `chelsea_live` additionally requires
  `NODE4_LIVE_TRADING_APPROVED=1`.
* **Broker-authoritative outcomes.** Only a broker-confirmed fill opens a
  trade; an unclear write is reported `unknown` and reconciled by `intent_id`,
  never retried as a new order.
* **Separate secrets per link.** `NODE4_SHARED_TOKEN`, `MT5_BRIDGE_TOKEN` and
  `MT5_CONTROL_TOKEN` are three distinct values, never logged or written to the
  ledger.

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

Docker (execution service only):

```bash
docker build -f Dockerfile -t xauusd-node4-execution .
docker run --env-file node4-execution/.env -p 10000:10000 xauusd-node4-execution
```

## Resources

| Endpoint | Access | Description |
|---|---|---|
| `GET /health` | public | `ok` for platform probes. |
| `GET /diagnostics` (alias `/status`) | public | `ExecutionSnapshot`: Node 3 link health, **`risk`** block, ledger state, counters, open trades, Deriv account, MT5 bridge view, event log. |
| `GET /open-trades` (alias `/trades`) | public | `OpenTradesSnapshot`. |
| `GET /deriv` (alias `/account`) | public | Deriv options account. |
| `GET /mt5/account`, `/mt5/positions`, `/mt5/history`, `/mt5/status` | public | MT5 demo account, positions, closed deals, bridge/EA link status. |
| `WS /ws` | public | Live stream of the resources above. Never carries private intent/report frames. |
| `WS /mt5/bridge` | token | The bridge's private session (`bridge_hello` with `MT5_BRIDGE_TOKEN`). |
| `POST /mt5/control` | token | Kill switch: `halt`, `resume`, `close_all`, `close_position` with `X-Control-Token: $MT5_CONTROL_TOKEN`. |

`GET /diagnostics` no longer contains `market_feed` or `strategy`; those blocks
belonged to the deleted local-strategy mode. The replacement `risk` object is
specified in [`contracts/c4-diagnostics.schema.json`](contracts/c4-diagnostics.schema.json).

## Validation

```bash
cd node4-execution
cargo fmt --all -- --check
cargo check --all-targets
cargo test  --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release

cd ..
python3 ci/check_node4_boundary.py
python3 ci/contract_sync.py
python3 ci/mt5/protocol_lint.py
cd mt5-bridge && cargo test --all-targets
```

CI runs the same set on every push (`.github/workflows/build.yml`).

## Scope guard

`ci/check_node4_boundary.py` enforces the boundary in CI: no `mod strategy`,
no `strategy_runner`, no `mod node1`, no `NODE4_INTENT_SOURCE`, no
`volume_profile`, no `StrategyEngine`, no `MarketFeed`, no `FeedSnapshot`/
`StrategySnapshot`, no `node2/` dashboard and no `Node-4.txt`. If a change here
starts to look like a market feed, a VP calculation or a trading decision, it
belongs in Node 1 or Node 3.
