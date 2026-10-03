# XAUUSD Node 3 — Rust Automated Execution Service

This repository branch (`Node3`) hosts the standalone automated execution service built in **Rust** for the XAUUSD trading system. It directly interfaces with **Node 1 (`xauusd-engine`)** over low-latency WebSockets and executes trades based on the Volume Profile break-and-retest strategy, while streaming live execution diagnostics, scanned setups, open trades, and Deriv Demo account state over its own WebSocket server.

For detailed architecture, configuration parameters, WebSocket wire schema, and strategy details, see **[`node3-execution/README.md`](node3-execution/README.md)**.

## Features

- **Direct WSS Integration**: Subscribes directly to live candle, level, and trade streams from Node 1.
- **Live Execution & Diagnostics WSS Stream (`/ws`)**: Broadcasts real-time `diagnostics`, `scanning`, `open_trades`, `deriv_account`, `trades`, and `diagnostic_event` frames to connected dashboards (e.g., Node 2 frontend at `wss://execution-southeastasia-sng-main.onrender.com/ws`).
- **Live Deriv Demo Account Monitor**: Streams real-time Deriv Demo account `balance`, `currency`, `account_id`, and live open contracts (`portfolio` + `proposal_open_contract` with unrealized PnL, spot price, payout, and expiry).
- **Volume Profile Strategy**: Tracks Previous Week (`PW PoC/VaH/VaL`), Previous Session (`PS PoC`), and Current Week (`CW PoC/VaH/VaL`) levels.
- **Break & Retest Engine**: Automated state machine with volume threshold confirmation (`> 10,500` on XAUUSD), retest zones (`±$0.50`), invalidation bounds (`$2.00`), and projected SL/TP/RR per level.
- **Risk Management**: Dynamic ATR-based Stop Loss (`200–300` pips) and Take Profit (`600–800` pips) targeting a `2:1` to `3:1` Risk-to-Reward ratio.
- **Multi-Venue Execution**: Supports Deriv Demo API, Chelsea Live MCP, or Node 1 Signal Mode.
- **HTTP Endpoints**: CORS-enabled `GET /health`, `GET /diagnostics`, `GET /scanning`, `GET /open-trades`, and `GET /deriv` on port `10000`.

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
curl http://localhost:10000/deriv         # -> Deriv Demo balance & open contracts
```
