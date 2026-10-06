# Node 3: Rust Automated Execution Service (XAUUSD)

High-performance, low-latency execution service built entirely in **Rust** to replace legacy AI/Python models. It directly interfaces with **Node 1 (`xauusd-engine`)** over low-latency WebSockets, automates trades based on the Volume Profile break-and-retest strategy, and serves a real-time **Diagnostics & Execution WebSocket stream** (`wss://execution-southeastasia-sng-main.onrender.com/ws`) for the frontend.

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
|   2. Break & Retest State Machine + Setup Scanner            |
|   3. Volume Filter (> 10,500 threshold on XAUUSD)            |
|   4. Dynamic ATR SL (200-300 pips) & TP (600-800 pips, 2-3RR)|
|   5. Live Deriv Demo Account Monitor (Balance & Open Trades) |
|   6. Outbound WSS (/ws) & REST (/diagnostics) for Frontend   |
|                                                              |
|   Routes to exactly one venue (mutually exclusive, fail closed): |
|   -> Deriv MT5 demo via mt5-bridge (if MT5_BRIDGE_TOKEN)     |
|   -> Deriv options Demo API (if DERIV_DEMO_API is set)       |
|   -> Chelsea Live MCP (if MCP_CHELSEA_URL is set)            |
|   -> Signal Mode (no venue credentials configured)           |
+--------------------------------------------------------------+
```

The Deriv MT5 demo venue executes through a separate service, `mt5-bridge/`,
that runs next to the MT5 terminal and **dials out** to this one — see
[`../docs/mt5/EXECUTION_ARCHITECTURE.md`](../docs/mt5/EXECUTION_ARCHITECTURE.md)
and [`../mt5-bridge/README.md`](../mt5-bridge/README.md). Node 3 never holds MT5
credentials, never opens an inbound connection to the terminal, never marks a
trade open without a broker-confirmed fill, and refuses to trade (instead of
falling back to another venue) when the bridge is disconnected, the account is
not demo, or the symbol/stops do not validate.

## Strategy Logic Implemented
1. **PW (Previous Week)**: Tracks Sunday 18:00 to Sunday 18:00 UTC-4 levels: `PW PoC`, `PW VaH`, `PW VaL`.
2. **PS (Previous Session)**: Tracks Friday close to Sunday 18:00 UTC-4 session: `PS PoC`.
3. **CW (Current Week)**: From Wednesday onward, tracks Monday open to Wednesday close: `CW PoC`, `CW VaH`, `CW VaL`.
4. **Trigger**: Clean candle break of a level, followed by a retest of that level (`±$0.50` proximity, invalidated beyond `$2.00`).
5. **Volume Confirmation**: Candle volume must exceed `10,500` on XAUUSD.
6. **Risk Management**: Dynamic ATR Stop Loss ($200 - 300$ pips) and Take Profit ($600 - 800$ pips), strictly bounding Risk-to-Reward between **2.0 and 3.0**.

## WebSocket & HTTP Endpoints (`PORT=10000`)

Production URL: `https://execution-southeastasia-sng-main.onrender.com`

| Endpoint | Protocol | Description |
|----------|----------|-------------|
| `wss://execution-southeastasia-sng-main.onrender.com/ws` | WSS | Live stream of `diagnostics`, `scanning`, `open_trades`, `deriv_account`, `trades`, and `diagnostic_event` frames. |
| `GET /health` | HTTPS | Plaintext `"ok"` for platform probes. |
| `GET /diagnostics` (or `/status`) | HTTPS | Full JSON `DiagnosticsSnapshot` (scanning setups, open trades, Deriv Demo account, engine metrics, and event log). |
| `GET /scanning` | HTTPS | JSON `ScanningSnapshot` of all tracked VP levels and break/retest state machine setups. |
| `GET /open-trades` (or `/trades`) | HTTPS | JSON `OpenTradesSnapshot` (`node3_open_trades`, `deriv_open_trades`, `recent_trades`). |
| `GET /deriv` (or `/account`) | HTTPS | JSON `DerivAccountSnapshot` (`balance`, `currency`, `account_id`, `open_trades`, `total_unrealized_pnl`). |
| `GET /mt5/account` | HTTPS | JSON `Mt5AccountSnapshot`: `configured`, `connected`, `authorized`, `account_type`, login/server, balance/equity/margin, symbol contract data, `halted`/`halt_reason`, `error`, `setup_hint`. |
| `GET /mt5/positions` | HTTPS | JSON `Mt5PositionsSnapshot`: broker positions (volume, SL/TP, current price, unrealized PnL). |
| `GET /mt5/history` | HTTPS | JSON `Mt5HistorySnapshot`: closed deals with profit/swap/commission and realized totals. |
| `GET /mt5/status` | HTTPS | JSON `Mt5BridgeStatus`: bridge/EA link state, protocol, counters, uptime, last error. |
| `POST /mt5/control` | HTTPS | Operator kill switch. `{"action":"halt"\|"resume"\|"close_all"\|"close_position"}`, requires `X-Control-Token: $MT5_CONTROL_TOKEN`; 403 when that token is unset. |

### WebSocket Topics & Client Commands

On connection to `/ws`, Node 3 immediately sends the current `diagnostics`, `scanning`, `open_trades`, `deriv_account`, `mt5_account`, `mt5_positions`, `mt5_history` and `bridge_status` snapshots, and streams updates in real time.

The same endpoint also accepts the **MT5 bridge** connection: if the first frame
is `{"type":"bridge_hello","token":"$MT5_BRIDGE_TOKEN",...}`, the socket becomes
the bridge session (validated token, snapshot ingestion, command correlation) and
is never treated as a browser client. `bridge_hello` / `bridge_hello_ack` /
`bridge_ack` frames are never forwarded to frontends.

Clients may optionally send:
- `{"type": "subscribe", "topics": ["diagnostics", "scanning", "open_trades", "deriv_account", "mt5_account", "mt5_positions", "mt5_history", "bridge_status", "trades", "diagnostic_event"]}`
- `{"type": "snapshot"}` (requests an immediate replay of all snapshots)
- `{"type": "heartbeat"}` (replies with `{"type": "heartbeat"}`)

## Configuration (`.env`)

```env
# Node 1 Connection
NODE1_WS_URL=wss://engine-southeastasia-sng-main.onrender.com/ws

# Local HTTP & WebSocket port — hosting platforms inject PORT
PORT=10000

# Execution Venues — EXACTLY ONE. Setting two without picking one is a startup
# error on purpose; selecting a venue whose credentials are missing fails too.
#   deriv_mt5_demo | deriv_demo | chelsea_live | none
EXECUTION_VENUE=
# PAT (pat_...) tokens REQUIRE DERIV_APP_ID; legacy a1-... tokens do not.
DERIV_DEMO_API=your_deriv_token
# Required whenever DERIV_DEMO_API is a PAT (pat_...) token — register a free
# app at https://developers.deriv.com (API dashboard) and paste its App ID here.
# Set it in the running service's environment (Render → Environment), not only in
# a local .env, otherwise Deriv answers HTTP 401:
#   "Deriv-App-ID header is required for PAT tokens".
DERIV_APP_ID=
# Optional override. Default: https://api.derivws.com (current REST + OTP API).
# Set to wss://ws.derivws.com/websockets/v3 to force the legacy flow.
DERIV_API_URL=https://api.derivws.com
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
# Stake (USD) sent to Deriv, and the minimum Deriv will price. Deriv refuses
# stakes below 0.50 for frxXAUUSD ("Please enter a stake amount that's at
# least 0.50.", subcode InvalidMinStake), so ORDER_SIZE below it is clamped
# up and a warning is logged.
ORDER_SIZE=0.50
DERIV_MIN_STAKE=0.50

# Deriv MT5 demo venue. MT5_LOGIN / MT5_PASSWORD live ONLY on the MT5 host, in
# the bridge's environment; Node 3 never sees them.
MT5_BRIDGE_TOKEN=            # required to enable the venue; absent => inert
MT5_CONTROL_TOKEN=            # enables POST /mt5/control; absent => 403
MT5_SYMBOL=XAUUSD             # requested instrument
MT5_SYMBOL_MAP=               # explicit broker mapping, e.g. XAUUSD=XAUUSD.a
MT5_VOLUME_LOTS=0.01          # lots — NOT ORDER_SIZE (that is an options stake)
MT5_MAX_RISK_PER_TRADE=       # optional pre-send risk cap in account currency
MT5_ORDER_TIMEOUT_MS=15000
MT5_HISTORY_PAGE_SIZE=100
```

## Running

```bash
cargo run --release
# or Docker:
docker build -t node3-execution . && docker run --env-file .env -p 10000:10000 node3-execution
```

## Deriv connection notes

- **Two token types, auto-detected.** Legacy API tokens (`a1-...`, created at
  `app.deriv.com` → account settings → API token) and current Personal Access
  Tokens (`pat_...`, created at `developers.deriv.com`) use different Deriv
  APIs. Node 3 tries the current REST + OTP API first and automatically falls
  back to the legacy WebSocket flow for legacy tokens — no `app_id` needed
  unless you use a PAT.
- **HTTP 520 failover.** Deriv's legacy `wss://ws.derivws.com/websockets/v3`
  endpoint sits behind Cloudflare and returns HTTP 520 from many cloud/VPS
  networks. Legacy connections therefore use browser-like handshake headers
  (`Origin`, `User-Agent`) and fail over across `ws.derivws.com`,
  `ws.binaryws.com`, and `wss.derivws.com`, with and without `app_id`.
- **RFC 6455 upgrade headers.** Every Deriv socket (OTP and legacy) is dialed
  through a request built from the URL via `IntoClientRequest`, which is the
  only conversion that writes `Host`, `Connection: Upgrade`,
  `Upgrade: websocket`, `Sec-WebSocket-Version: 13` and the random
  `Sec-WebSocket-Key`; the browser-like `Origin` / `User-Agent` are added on
  top. Hand-building the `http::Request` (as an earlier revision did) makes
  tungstenite abort the handshake locally with
  `WebSocket protocol error: Missing, duplicated or incorrect header
  sec-websocket-key` — the connection never reaches Deriv.
- **Demo only.** The Deriv venue refuses to trade on anything that is not a
  demo/virtual (`VRTC...`) account, on both the OTP and legacy flows.
- **Order shape (Rise/Fall, no barrier).** An intraday `CALL`/`PUT` on
  `frxXAUUSD` is Deriv's at-the-money *Rise/Fall* contract, so the `proposal`
  is sent **without** a `barrier` field. That is not a guess: `ci/deriv_probe.py`
  (run by the `deriv-probe` workflow, report in `ci/deriv/DERIV.md`) probes
  Deriv's live API, and Deriv prices the barrier-less shape while rejecting
  every barrier — relative or absolute, `+/-0.01` to `+/-15.00`, including the
  `"barrier": "+2.20"` example that `contracts_for` advertises on those
  entries — with
  `ContractBuyValidationError: Invalid barrier.` (`subcode InvalidBarrier`).
  An earlier revision attached `+/- |tp - sl| / 2` to every proposal, so every
  order failed with exactly that error.
- **Shape fallback.** If Deriv ever rejects the barrier-less proposal and says
  the problem is the barrier (a Higher/Lower style contract that wants one),
  Node 3 re-proposes once with a signed relative barrier derived from the
  take-profit distance and rounded to the symbol's pip size. Every order logs
  the shape that was sent, the `longcode` Deriv priced, `echo_req` on
  rejection, and the `contracts_for` snapshot of what Deriv currently offers —
  so a rejected order names what happened instead of leaving the raw error.
- **Minimum stake.** Deriv answers a stake below the contract minimum with
  `InvalidMinStake` (`Please enter a stake amount that's at least 0.50.` for
  `frxXAUUSD`); `ORDER_SIZE` below `DERIV_MIN_STAKE` (default `0.50`) is
  clamped up with a warning, so a too-small `ORDER_SIZE` cannot silently kill
  every trade.

### PAT credentials checklist

A `pat_...` token needs **two** variables in the service environment:

| Variable           | Value                                                                        |
| ------------------ | ---------------------------------------------------------------------------- |
| `DERIV_DEMO_API`   | the PAT itself (`pat_...`)                                                   |
| `DERIV_APP_ID`     | App ID of a free app registered at <https://developers.deriv.com> (API dashboard) |

Set them where Node 3 actually runs (e.g. Render → **Environment**) and restart
the service — editing a local `.env` alone does not change the deployed bot.

Verify without reading logs:

```bash
curl -s http://localhost:10000/deriv | jq '{token_kind, app_id_configured, connected, authorized, error, setup_hint}'
```

- `token_kind: "pat"` with `app_id_configured: false` → `DERIV_APP_ID` is missing;
  the response's `setup_hint` names the exact variable to set.
- `app_id_configured: true` and still unauthorized → the App ID value is wrong,
  or `DERIV_API_URL` was pointed at a legacy `wss://` endpoint (PATs only work on
  the default `https://api.derivws.com` REST + OTP API).
