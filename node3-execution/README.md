# Node 3: Rust Automated Execution Service (XAUUSD)

High-performance, low-latency execution service built entirely in **Rust** to replace legacy AI/Python models. It directly interfaces with **Node 1 (`xauusd-engine`)** over low-latency WebSockets and automates trades based on the Volume Profile break-and-retest strategy.

## Architecture

```
+--------------------------------------------------------------+
|                         Node 1 (Engine)                      |
|            Rust Market Data & Volume Profile Engine          |
|      (Serves live candles, PW/PS/CW levels over /ws)         |
+------------------------------+-------------------------------+
                               |
               Direct WSS Stream (topics: candle, levels, trades)
                               |
+------------------------------v-------------------------------+
|                      Node 3 (Execution Layer)                |
|            Full Rust Automated Execution Daemon              |
|                                                              |
|   1. Levels Tracker: PW (PoC/VaH/VaL), PS (PoC), CW (Wed+)   |
|   2. Break & Retest State Machine                            |
|   3. Volume Filter (> 10,500 threshold on XAUUSD)            |
|   4. Dynamic ATR SL (200-300 pips) & TP (600-800 pips, 2-3RR)|
|                                                              |
|   Routes to:                                                 |
|   -> Deriv Demo API (if DERIV_DEMO_API is set)               |
|   -> Chelsea Live MCP (if MCP_CHELSEA_URL is set)            |
|   -> Signal Mode (broadcasts TradeEvent to Node 1)           |
+--------------------------------------------------------------+
```

## Strategy Logic Implemented
1. **PW (Previous Week)**: Tracks Sunday 18:00 to Sunday 18:00 UTC-4 levels: `PW PoC`, `PW VaH`, `PW VaL`.
2. **PS (Previous Session)**: Tracks Friday close to Sunday 18:00 UTC-4 session: `PS PoC`.
3. **CW (Current Week)**: From Wednesday onward, tracks Monday open to Wednesday close: `CW PoC`, `CW VaH`, `CW VaL`.
4. **Trigger**: Clean candle break of a level, followed by a retest of that level.
5. **Volume Confirmation**: Candle volume must exceed `10,500` on XAUUSD.
6. **Risk Management**: Dynamic ATR Stop Loss ($200 - 300$ pips) and Take Profit ($600 - 800$ pips), strictly bounding Risk-to-Reward between **2.0 and 3.0**.

## Configuration (`.env`)

```env
# Node 1 Connection
NODE1_WS_URL=wss://engine-southeastasia-sng-main.onrender.com/ws

# Execution Venues (Only 1 minimum required)
DERIV_DEMO_API=your_deriv_token
DERIV_APP_ID=1089
# or
MCP_CHELSEA_URL=http://localhost:3002/mcp

# Strategy Parameters
VOLUME_THRESHOLD=10500
SL_MIN_PIPS=200
SL_MAX_PIPS=300
TP_MIN_PIPS=600
TP_MAX_PIPS=800
RR_MIN=2.0
RR_MAX=3.0
ORDER_SIZE=0.01
```

## Running

```bash
cargo run --release
# or Docker:
docker build -t node3-execution . && docker run --env-file .env node3-execution
```
