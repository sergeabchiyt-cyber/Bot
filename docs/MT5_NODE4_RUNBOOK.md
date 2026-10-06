# Node 4 MT5 execution handoff and runbook

Status: **migration specification; the existing MT5 implementation has not yet
passed a live Deriv MT5 demo integration gate. Trading must remain disabled
until the gate below passes.**

All MT5 code and docs currently embedded in Node 1 belong on the new Node 4
branch. Node 3 is strategy-only and must not host the MT5 bridge session,
account monitor, control endpoints, login, password, lot sizing, or broker
writes.

## Target topology

```text
Render/public                              Windows or persistent Wine host
┌──────────────────────────────┐           ┌───────────────────────────────┐
│ Node 3 strategy              │           │ mt5-bridge (Rust)             │
│ WS /execution                │           │ loopback TCP :5055            │
└──────────────┬───────────────┘           │          ▲                    │
               │ authenticated intent      │          │ EA client socket   │
               ▼                           │ Mt5BridgeEA.mq5 in terminal    │
┌──────────────────────────────┐ outbound  │          │                    │
│ Node 4 execution             │◄──────────┤ outbound WSS to Node 4        │
│ idempotency + venue policy   │ bridge    │ Deriv MT5 DEMO account        │
│ broker REST/WS + /mt5/*      │ session   └───────────────────────────────┘
└──────────────────────────────┘
```

MQL5 sockets are client sockets: the EA uses `SocketCreate`/`SocketConnect` to
connect to the colocated Rust bridge. The terminal does not expose an inbound
server. The bridge then connects outbound to Node 4, so no public inbound port
is required on the terminal host.

Official references:

- MQL5 network functions: <https://www.mql5.com/en/docs/network>
- `SocketCreate`: <https://www.mql5.com/en/docs/network/socketcreate>
- `SocketConnect`: <https://www.mql5.com/en/docs/network/socketconnect>
- account trade mode: <https://www.mql5.com/en/docs/constants/environment_state/accountinformation#enum_account_trade_mode>
- trade return codes: <https://www.mql5.com/en/docs/constants/errorswarnings/enum_trade_return_codes>

## Files to move from Node 1 to Node 4

Move these paths with history, update all Node 3 naming to Node 4, and delete
the originals from Node 1 only after Node 4 builds:

```text
mt5-bridge/**
docs/mt5/EXECUTION_ARCHITECTURE.md
docs/mt5/FRONTEND_RESOURCES.md
ci/mt5/protocol_lint.py
node3-execution/src/execution_mt5.rs
```

The MT5 side of the execution manager, shared snapshot types, HTTP resources,
and protocol tests embedded under Node 1's `node3-execution/**` must become
`node4-execution/**`. Do not copy strategy state or Node 1 feed logic with it.

Required renames include:

| Old | New |
|---|---|
| `node3-execution` | `node4-execution` |
| `NODE3_WS_URL` in bridge | `NODE4_WS_URL` |
| bridge ↔ Node 3 | bridge ↔ Node 4 |
| Node 3 MT5 endpoints | Node 4 MT5 endpoints |
| Node 3 execution venue | Node 4 execution venue |

The bridge-to-Node-4 protocol is distinct from the Node-3-to-Node-4 intent
protocol. Use separate secrets: `NODE4_SHARED_TOKEN` for strategy handoff and
`MT5_BRIDGE_TOKEN` for bridge authentication.

## Responsibility split inside Node 4

### Node 4 execution service

- authenticated client of Node 3 `/execution`
- durable intent/idempotency ledger
- intent TTL/schema/symbol/side/price validation
- exactly one selected venue, fail closed
- account-aware sizing and optional max-risk cap
- bridge session registry and command correlation
- broker-authoritative account, positions, history, and execution reports
- operator controls (halt, resume, flatten, close), token-gated and never CORS
  exposed to Node 2

### `mt5-bridge` on terminal host

- loopback server the EA dials
- demo-account guard and expected-login check
- live symbol/quote/volume/stops validation
- idempotency ledger and unknown-outcome reconciliation
- append-only JSONL audit/history
- outbound authenticated WSS to Node 4
- halt on EA disconnect/account-mode change

### `Mt5BridgeEA.mq5`

- refuses writes unless `ACCOUNT_TRADE_MODE_DEMO`
- reports account/link/trade-allowed state on hello/heartbeat
- translates typed bridge requests to MT5 operations
- returns broker retcode, order/deal/position IDs, fill price, and volume
- never logs or transmits account password

## Configuration boundary

### Node 3

Only:

```env
NODE4_SHARED_TOKEN=...
```

No MT5 variables.

### Node 4

```env
NODE3_WS_URL=wss://<node3-host>/execution
NODE4_SHARED_TOKEN=<same value as Node 3>
EXECUTION_VENUE=deriv_mt5_demo
MT5_BRIDGE_TOKEN=<separate bridge secret>
MT5_CONTROL_TOKEN=<separate operator control secret>
MT5_SYMBOL=XAUUSD
MT5_VOLUME_LOTS=0.01
MT5_ORDER_TIMEOUT_MS=15000
MT5_HISTORY_PAGE_SIZE=100
MT5_MAX_RISK_PER_TRADE=
```

Exactly one venue credential set may be present. Selecting MT5 while Deriv
options or Chelsea credentials are also present is a startup error.

### Terminal-host bridge

```env
NODE4_WS_URL=wss://<node4-host>/mt5/bridge
MT5_BRIDGE_TOKEN=<same bridge secret as Node 4>
MT5_EA_BIND_ADDR=127.0.0.1
MT5_EA_PORT=5055
MT5_EA_TOKEN=<EA-to-bridge secret>
MT5_LOGIN=<expected demo login>
MT5_PASSWORD=<presence/terminal validation only; never transmitted>
MT5_SYMBOL=XAUUSD
MT5_SYMBOL_MAP=XAUUSD=XAUUSD.a
MT5_VOLUME_LOTS=0.01
MT5_TRADING_ENABLED=0
MT5_HALT_ON_EA_DISCONNECT=1
```

Broker suffixes must be mapped explicitly. Never guess `XAUUSD.a` or another
symbol. `MT5_PASSWORD` must not appear in a payload, log, snapshot, browser,
Node 3 environment, or Node 4 intent.

## Fail-closed order path

1. Node 4 durably records the Node 3 `intent_id` before a broker write.
2. Reject expired or unsupported intents and report `rejected` to Node 3.
3. Select exactly one configured venue; there is no fallback.
4. Verify bridge session, EA heartbeat, demo account, expected login, trading
   permission, account freshness, and quote freshness.
5. Validate exact/mapped symbol, lot min/max/step, stop direction/distance,
   deviation, and max risk.
6. Send one `ORDER_SEND` carrying `intent_id` as idempotency key/comment.
7. Treat only broker-confirmed retcodes and positive fill volume as filled.
8. On timeout return `unknown`, query positions/deals by the intent comment,
   and never resend the order as a new write.
9. Push broker-authoritative status back to Node 3 as `execution_report`.
10. Persist intent, command, answer, and reconciliation result in the audit
    ledger without secrets.

## Demo guards

Every layer independently refuses non-demo execution:

| Layer | Required check |
|---|---|
| EA | `ACCOUNT_TRADE_MODE == ACCOUNT_TRADE_MODE_DEMO` before every write |
| EA | terminal and symbol trading allowed |
| bridge | hello/heartbeat mode remains demo; expected login matches |
| bridge | fresh account and quote; not halted |
| Node 4 | bridge account snapshot is connected, authorized, and demo |

An account-mode change after startup halts execution and requires explicit
operator recovery. It must never auto-resume.

## Public and control resources on Node 4

Read-only resources for Node 2/operators:

```text
GET /health
GET /diagnostics
GET /open-trades
GET /account
GET /mt5/account
GET /mt5/positions
GET /mt5/history
GET /mt5/status
WS  /ws
```

Write controls are server/operator only:

```text
POST /mt5/control
```

Require `X-Control-Token: $MT5_CONTROL_TOKEN`, return 403 when the token is
unset, apply no permissive CORS policy, and never put the token in Node 2.

## Integration gate before enabling MT5

Run against the smallest permitted order on a Deriv MT5 **demo** account:

- [ ] Node 4 and bridge reject a real/contest account.
- [ ] Wrong bridge token and wrong EA token fail closed without secret logging.
- [ ] Node 4 receives a Node 3 intent, persists its ID, and acknowledges it.
- [ ] Exact or explicitly mapped XAUUSD symbol resolves; unknown suffix fails.
- [ ] Volume is normalized to broker min/max/step and reported in lots.
- [ ] Invalid or too-close SL/TP is rejected before `ORDER_SEND`.
- [ ] One market order returns broker retcode plus order/deal/position IDs.
- [ ] Duplicate `intent_id` returns the recorded result and creates no order.
- [ ] A forced timeout becomes `unknown`; reconciliation finds the outcome and
      no second order is sent.
- [ ] Position snapshots and unrealized PnL match the terminal.
- [ ] Modify SL/TP, close one, and close-all are confirmed by the broker.
- [ ] Closed-deal history includes profit, swap, and commission and survives
      Node 4/bridge restart.
- [ ] EA disconnect halts new orders while monitoring/control behavior remains
      explicit.
- [ ] Halt and flatten work; resume fails while any demo/link guard is unhealthy.
- [ ] Node 2 can read state but cannot invoke control or access a token.
- [ ] `ci/mt5/protocol_lint.py`, bridge contract tests, Node 4 tests, and build
      all pass on the Node 4 branch.

Only after every item has evidence should `MT5_TRADING_ENABLED` become `1`.
