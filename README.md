# XAUUSD Node 3 — Rust Automated Execution Service

This repository branch (`Node3`) hosts the standalone automated execution service built in **Rust** for the XAUUSD trading system. It directly interfaces with **Node 1 (`xauusd-engine`)** over low-latency WebSockets and executes trades based on the Volume Profile break-and-retest strategy, while streaming live execution diagnostics, scanned setups, open trades, and Deriv Demo account state over its own WebSocket server.

For detailed architecture, configuration parameters, WebSocket wire schema, and strategy details, see **[`node3-execution/README.md`](node3-execution/README.md)**.

## Features

- **Direct WSS Integration**: Subscribes directly to live candle, level, and trade streams from Node 1.
- **Live Execution & Diagnostics WSS Stream (`/ws`)**: Broadcasts real-time `diagnostics`, `scanning`, `open_trades`, `deriv_account`, `trades`, and `diagnostic_event` frames to connected dashboards (e.g., Node 2 frontend at `wss://execution-southeastasia-sng-main.onrender.com/ws`).
- **Live Deriv Demo Account Monitor**: Streams real-time Deriv Demo account `balance`, `currency`, `account_id`, and live open contracts (`portfolio` + `proposal_open_contract` with unrealized PnL, spot price, payout, and expiry).
- **Volume Profile Strategy**: Tracks Previous Week (`PW PoC/VaH/VaL`), Previous Session (`PS PoC`), and Current Week (`CW PoC/VaH/VaL`) levels.
- **Break & Retest Engine**: Automated state machine with volume threshold confirmation (`> 10,500` on XAUUSD), retest zones (`±$0.50`), invalidation bounds (`$2.00`), and projected SL/TP/RR per level.
- **Valid Deriv contract shapes**: Intraday `frxXAUUSD` trades as an at-the-money Rise/Fall contract with **no `barrier` field** (Deriv rejects any barrier with `InvalidBarrier`), with a signed-barrier re-proposal only if Deriv asks for one. Stakes below Deriv's minimum (`0.50` USD) are clamped up instead of failing. Evidence and how to reproduce: `ci/deriv_probe.py` → `ci/deriv/DERIV.md` (see [`node3-execution/README.md`](node3-execution/README.md#deriv-connection-notes)).
- **Risk Management**: Dynamic ATR-based Stop Loss (`200–300` pips) and Take Profit (`600–800` pips) targeting a `2:1` to `3:1` Risk-to-Reward ratio.
- **Multi-Venue Execution**: Supports Deriv MT5 **demo** (via the `mt5-bridge` service, see below), Deriv options Demo API, Chelsea Live MCP, or Node 1 Signal Mode. Venues are mutually exclusive and fail closed: configuring two credential sets is a startup error (even with `EXECUTION_VENUE` set — unset the credential you are leaving behind), and a configured-but-unavailable venue refuses trades instead of falling back to another one.
- **Deriv MT5 Demo Bridge**: `mt5-bridge/` runs next to the MT5 terminal, dials out to this service, and gives it full control of the demo account — place/modify/close/close-all, halt/resume kill switch, account/positions/closed-deal history, reconciliation and restart-safe history. Demo-only, broker-confirmed fills only, no live fallback. See [`docs/mt5/EXECUTION_ARCHITECTURE.md`](docs/mt5/EXECUTION_ARCHITECTURE.md).
- **HTTP Endpoints**: CORS-enabled `GET /health`, `GET /diagnostics`, `GET /scanning`, `GET /open-trades`, `GET /deriv`, `GET /mt5/account`, `GET /mt5/positions`, `GET /mt5/history`, `GET /mt5/status` and token-gated `POST /mt5/control` on port `10000`.

## Quick Start

### 1. Configuration

Copy the example environment file and configure your Node 1 endpoint and execution credentials:

```bash
cd node3-execution
cp .env.example .env
```

### 2. Run with Cargo

```bash
cargo run --release
```

### 3. Run with Docker

```bash
docker build -t node3-execution .
docker run --env-file .env -p 10000:10000 node3-execution
```

### 4. Endpoints (`PORT=10000`)

```bash
curl http://localhost:10000/health        # -> ok
curl http://localhost:10000/diagnostics   # -> Full JSON diagnostics snapshot
curl http://localhost:10000/scanning      # -> Current scanned VP levels & armed setups
curl http://localhost:10000/open-trades   # -> Node 3 open trades + Deriv open contracts
curl http://localhost:10000/deriv         # -> Deriv options Demo balance & open contracts

# Deriv MT5 demo (inert unless MT5_BRIDGE_TOKEN is set and the bridge dials in)
curl http://localhost:10000/mt5/account     # -> configured/connected/authorized/account_type/... 
curl http://localhost:10000/mt5/positions   # -> broker positions (volume, SL/TP, unrealized PnL)
curl http://localhost:10000/mt5/history     # -> closed deals with realized P&L
curl http://localhost:10000/mt5/status      # -> bridge/EA link state and counters

# Operator kill switch (403 unless MT5_CONTROL_TOKEN is set and sent)
curl -X POST http://localhost:10000/mt5/control \
     -H "X-Control-Token: $MT5_CONTROL_TOKEN" \
     -d '{"action":"halt","reason":"operator"}'   # halt | resume | close_all | close_position
```

### 5. Deriv credentials

Node 3 auto-detects the token shape from `DERIV_DEMO_API`:

- **Legacy `a1-...` token** — only `DERIV_DEMO_API` is needed.
- **Personal Access Token `pat_...`** — Deriv needs your App ID on every REST call,
  so `DERIV_APP_ID` must be set as well. Without it the account monitor answers
  `HTTP 401: Deriv-App-ID header is required for PAT tokens`.

Register a free app at <https://developers.deriv.com> (API dashboard) and set both
variables **in the environment of the deployed service** (e.g. Render → Environment),
not just in a local `.env`:

```env
DERIV_DEMO_API=pat_...
DERIV_APP_ID=12345
```

`GET /deriv` reports `token_kind`, `app_id_configured` and a `setup_hint` naming the
missing variable, so a misconfiguration is visible without reading the service logs.

### 6. Deriv MT5 demo venue (optional)

Node 3 needs one secret for this venue — `MT5_BRIDGE_TOKEN` — plus an explicit
venue selection when more than one venue is configured:

```env
EXECUTION_VENUE=deriv_mt5_demo
MT5_BRIDGE_TOKEN=<shared with the bridge>
MT5_CONTROL_TOKEN=<random; enables POST /mt5/control>
MT5_VOLUME_LOTS=0.01        # lots, NOT ORDER_SIZE (that is an options stake in USD)
```

`MT5_LOGIN` / `MT5_PASSWORD` are read **only** by `mt5-bridge`, on the MT5 host.
Build and run the bridge from [`mt5-bridge/README.md`](mt5-bridge/README.md); its
live-demo checklist lives in
[`docs/mt5/EXECUTION_ARCHITECTURE.md`](docs/mt5/EXECUTION_ARCHITECTURE.md).
