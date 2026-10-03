# XAUUSD Node 3 — Rust Automated Execution Service

This repository branch (`Node3`) hosts the standalone automated execution service built in **Rust** for the XAUUSD trading system. It replaces legacy AI/Python models by directly interfacing with **Node 1 (`xauusd-engine`)** over low-latency WebSockets and executing trades based on the Volume Profile break-and-retest strategy.

For detailed architecture, configuration parameters, and strategy details, see **[`node3-execution/README.md`](node3-execution/README.md)**.

## Features

- **Direct WSS Integration**: Subscribes directly to live candle, level, and trade streams from Node 1.
- **Volume Profile Strategy**: Tracks Previous Week (PW PoC/VaH/VaL), Previous Session (PS PoC), and Current Week (CW PoC/VaH/VaL) levels.
- **Break & Retest Engine**: Automated state machine with volume threshold confirmation (> 10,500 on XAUUSD).
- **Risk Management**: Dynamic ATR-based Stop Loss (200–300 pips) and Take Profit (600–800 pips) targeting a 2:1 to 3:1 Risk-to-Reward ratio.
- **Multi-Venue Execution**: Supports Deriv Demo API, Chelsea Live MCP, or Node 1 Signal Mode.
- **Health Endpoint**: Dependency-free `GET /health` on port `10000` for platform health checks.

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

### 4. Health Check

The service exposes `GET /health` on port `10000` (override with `PORT`):

```bash
curl http://localhost:10000/health   # -> ok
```
