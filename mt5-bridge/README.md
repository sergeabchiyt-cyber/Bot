# mt5-bridge — Deriv MT5 **demo** execution bridge

A separate Rust service that runs **on the machine with the MT5 terminal** and
gives Node 3 full control of a Deriv MT5 **demo** account: place orders, modify
stops, close positions, close everything, halt/resume, and read the account,
positions and closed-deal history.

Read [`../docs/mt5/EXECUTION_ARCHITECTURE.md`](../docs/mt5/EXECUTION_ARCHITECTURE.md)
first — it explains the topology (why the bridge must be the TCP server and why
it cannot run on the Node 3 host), the venue rules and the integration gate.

> **Demo only.** The bridge refuses to start on a configuration that can reach a
> live account, refuses every write while the demo guard fails, and never reports
> a fill the broker did not confirm. `MT5_PASSWORD` is used only to validate the
> terminal session; it is never sent to Node 3 or written to any payload, log or
> snapshot.

---

## 1. Pieces

| File | Role |
|---|---|
| `src/config.rs` | env configuration + fail-closed validation |
| `src/proto.rs` | EA ⇄ bridge line protocol (framing, percent codec, parse) |
| `src/ea_link.rs` | loopback TCP server the terminal's EA dials into |
| `src/terminal.rs` | typed client, symbol/volume/stop validation, normalization |
| `src/bridge.rs` | order flow, idempotency ledger, demo guard, halt, reconciliation, JSONL history |
| `src/node3.rs` | outbound WSS session to Node 3 (snapshots + commands) |
| `src/snapshot.rs` | the JSON contract shared with Node 3 |
| `src/fake.rs` | in-memory terminal for `MT5_SIM_TERMINAL=1` and tests |
| `mql5/Mt5BridgeEA.mq5` | the Expert Advisor that runs inside the terminal |
| `tests/contract.rs` | fake-terminal contract tests (fills, rejects, duplicates, disconnects, stale quotes, mode mismatch) |

Our own MQL5 note: MQL5 can only create **client** sockets, so the EA dials
`MT5_EA_BIND_ADDR:MT5_EA_PORT` and keeps the connection open; the bridge sends
requests and the EA answers. See `src/proto.rs`.

---

## 2. Install

### 2.1 Terminal side

1. Copy `mql5/Mt5BridgeEA.mq5` to
   `<terminal data folder>/MQL5/Experts/` (MetaEditor → File → Open Data Folder).
2. Compile it in MetaEditor (F7). It uses no external includes.
3. Attach it to **one** chart of a Deriv MT5 **demo** account, with:
   * *Allow Algo Trading* enabled in the terminal and in the EA properties,
   * inputs:
     | Input | Meaning |
     |---|---|
     | `InpBridgeHost` | `127.0.0.1` (or the bridge host on a private network) |
     | `InpBridgePort` | `5055` — must equal `MT5_EA_PORT` |
     | `InpToken` | must equal the bridge's `MT5_EA_TOKEN`; without it the bridge is read-only |
     | `InpAllowedSymbols` | optional comma list, e.g. `XAUUSD,XAUUSD.a` (empty = any) |
     | `InpHeartbeatSecs` | heartbeat interval, default 5 |
     | `InpMagicFilter` | optional; only used for display/close-all scoping |
     | `InpVerbose` | extra terminal-side logging |
4. The EA logs `connected to bridge …` and `bridge accepted the session`.

The EA itself refuses every write method unless the terminal is logged into a
demo account — a second layer behind the bridge's own demo guard.

### 2.2 Bridge side

```bash
cd mt5-bridge
cp .env.example .env      # fill it in; never commit it
cargo run --release
```

For protocol work without a terminal (no broker, no orders):

```bash
MT5_SIM_TERMINAL=1 MT5_BRIDGE_TOKEN=dev-token cargo run --release
```

### 2.3 Node 3 side

Node 3 needs exactly one thing from the bridge: the shared secret.

```bash
MT5_BRIDGE_TOKEN=<same value as the bridge>
EXECUTION_VENUE=deriv_mt5_demo          # optional when it is the only venue configured
MT5_CONTROL_TOKEN=<random>              # enables POST /mt5/control
MT5_SYMBOL=XAUUSD
MT5_VOLUME_LOTS=0.01
MT5_ORDER_TIMEOUT_MS=15000
```

---

## 3. Configuration reference

Every key is read by the bridge, never by Node 3. Defaults in parentheses.

### Node 3 link (outbound)

| Key | Notes |
|---|---|
| `NODE3_WS_URL` | `wss://execution-southeastasia-sng-main.onrender.com/ws` — the service the bridge dials into |
| `MT5_BRIDGE_TOKEN` | **required**; must equal Node 3's `MT5_BRIDGE_TOKEN`, sent in `bridge_hello`. Missing ⇒ the bridge refuses to start |

### Terminal link (loopback, EA dials in)

| Key | Notes |
|---|---|
| `MT5_EA_BIND_ADDR` (`127.0.0.1`) | bind address; keep loopback unless the EA is on another host |
| `MT5_EA_PORT` (`5055`) | must match the EA's `InpBridgePort` |
| `MT5_EA_TOKEN` | must match `InpToken`. **Unset ⇒ read-only bridge**: account/positions/history still work, every order/modify/close is refused |
| `MT5_LOGIN` | expected terminal login; a mismatch refuses trading |
| `MT5_PASSWORD` | presence-checked only (login/password pairing); never transmitted or logged |

### Instrument and sizing

| Key | Notes |
|---|---|
| `MT5_SYMBOL` (`XAUUSD`) | the strategy's instrument; any other name needs `MT5_ALLOW_OTHER_SYMBOL=1` |
| `MT5_SYMBOL_MAP` | explicit `requested=broker` pairs, e.g. `XAUUSD=XAUUSD.a`. Broker suffixes are **never** guessed: without a mapping, a missing symbol is an error |
| `MT5_ALLOW_OTHER_SYMBOL` (false) | allow a non-XAUUSD `MT5_SYMBOL` |
| `MT5_VOLUME_LOTS` (`0.01`) | order size in lots. **Not** `ORDER_SIZE` (that is a Deriv options stake in USD) |
| `MT5_MAX_RISK_PER_TRADE` | optional pre-send cap on `|entry − SL| × volume × contract size`, in account currency |
| `MT5_MAX_DEVIATION_POINTS` (`20`) | max slippage accepted on market orders |
| `MT5_MAGIC` (`330033`) | magic number on every position; used for ownership and reconciliation |

### Timeouts, freshness, pacing

| Key | Notes |
|---|---|
| `MT5_ORDER_TIMEOUT_MS` (`15000`) | `ORDER_SEND` timeout. On timeout the outcome is **unknown**, never "filled" |
| `MT5_REQUEST_TIMEOUT_MS` (`5000`) | timeout for read methods (account, symbol, quote, positions, history) |
| `MT5_ACCOUNT_MAX_AGE_MS` (`5000`) | an order is refused when the account snapshot is older than this |
| `MT5_MAX_QUOTE_AGE_MS` (`3000`) | an order is refused when the quote is older than this |
| `MT5_PUSH_INTERVAL_MS` (`2000`) | snapshot push cadence to Node 3 |

### History, control, safety

| Key | Notes |
|---|---|
| `MT5_HISTORY_FILE` (`data/mt5_history.jsonl`) | append-only closed-deal mirror; survives restarts |
| `MT5_HISTORY_PAGE_SIZE` (`100`) | page size for history queries |
| `MT5_TRADING_ENABLED` (true) | `0` starts the bridge **halted**: monitoring only, no orders |
| `MT5_HALT_ON_EA_DISCONNECT` (true) | halt trading when the EA link drops |
| `MT5_SIM_TERMINAL` (false) | run against the in-process fake terminal (protocol work only) |

Validation is fail-closed at startup: a non-`wss` Node 3 URL, a missing bridge
token, a password without a login, out-of-bounds volume/timeout/page-size/magic,
a malformed symbol map, a non-XAUUSD symbol without the opt-in, or a non-demo
expected account type all abort the process with the reason printed.

---

## 4. Wire protocols

### 4.1 EA ⇄ bridge (loopback, line based)

```text
EA     -> bridge : HELLO token=.. build=.. login=.. server=.. mode=demo company=.. currency=USD ea=1.0.0
bridge -> EA     : HELLOOK proto=1
bridge -> EA     : REQ 7 ORDER_SEND intent=N3-1730000000000-1 symbol=XAUUSD side=buy volume=0.01 sl=2647.5 tp=2656.0 deviation=20 magic=330033 comment=N3-1730000000000-1
EA     -> bridge : RESP 7 OK status=filled retcode=10009 order=123 deal=456 position=789 price=2650.12 volume=0.01
EA     -> bridge : ITEM 7 ticket=789 symbol=XAUUSD side=buy volume=0.01        (list payloads)
EA     -> bridge : END 7 count=1
EA     -> bridge : HB mode=demo connected=1 trade_allowed=1 login=123456 ts=1700000000000
EA     -> bridge : EVT TRADE kind=deal_in position=789 symbol=XAUUSD volume=0.01 price=2650.12 profit=0
```

Values are percent-encoded, one message per `\n`, ASCII only. Methods:
`PING`, `ACCOUNT`, `SYMBOL`, `QUOTE`, `POSITIONS`, `ORDERS`, `HISTORY`, `FIND`,
`ORDER_SEND`, `POS_MODIFY`, `POS_CLOSE`, `CLOSE_ALL`, `ORDER_CANCEL`.
`ci/mt5/protocol_lint.py` fails the build if the Rust side and the EA stop
agreeing about a method, a request parameter or a response field.

### 4.2 Bridge ⇄ Node 3 (`/ws`, JSON)

Bridge → Node 3: `bridge_hello` (authenticates with `MT5_BRIDGE_TOKEN`),
`mt5_account`, `mt5_positions`, `mt5_history`, `bridge_status`, `bridge_ack`,
`bridge_event`, `heartbeat`.

Node 3 → bridge: `bridge_hello_ack`, `mt5_order`, `mt5_modify`, `mt5_close`,
`mt5_close_all`, `mt5_halt` (optional `flatten`), `mt5_resume`,
`mt5_snapshot_request`, `mt5_ping`.

A command is answered with `bridge_ack { req_id, ok, data|error }`; Node 3
correlates by `req_id` and applies its own timeout. The same frame and struct
contract is asserted by the protocol lint against
`node3-execution/src/types.rs`.

---

## 5. Failure behaviour

| Situation | Behaviour |
|---|---|
| Bridge cannot start (config error) | process exits with the reasons printed |
| Node 3 unreachable | reconnect with backoff; monitoring and the EA link stay up; orders from Node 3 cannot arrive while offline |
| Node 3 rejects the handshake | close, log, retry — a wrong token is not retried in a tight loop |
| EA not connected | `connected=false`, orders refused (`bridge_not_connected`), halt if `MT5_HALT_ON_EA_DISCONNECT` |
| Account flips to real/contest | immediate halt, `halt_reason` set, no automatic resume |
| `ORDER_SEND` times out | outcome `unknown`, reconciled by intent comment (`FIND`), **never** re-sent |
| Broker rejects | `status=rejected` with `retcode`/`retcode_desc`; no retry |
| Duplicate `idempotency_key` | recorded outcome returned; no second order |
| Bridge restarts mid-position | startup reconciliation adopts magic-number positions, mirrors history from the broker |

## 6. Tests

```bash
cd mt5-bridge
cargo test --all-targets -- --nocapture     # unit + contract tests (fake terminal)
cargo check --all-targets

cd ..
python3 ci/mt5/protocol_lint.py            # EA ⇄ bridge ⇄ Node 3 contract lint
```

Manual dry run against the fake terminal (no broker involved):

```bash
MT5_SIM_TERMINAL=1 MT5_BRIDGE_TOKEN=dev-token MT5_EA_TOKEN=dev cargo run --release
```

The live-derivative checklist (smallest permitted order, ticket/fill/SL/TP
verification, position events, close + history P&L, restarts, real-account
refusal) lives in
[`docs/mt5/EXECUTION_ARCHITECTURE.md` §7](../docs/mt5/EXECUTION_ARCHITECTURE.md).
It has **not** been run yet.
