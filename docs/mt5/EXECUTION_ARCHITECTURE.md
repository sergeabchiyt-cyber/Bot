# Deriv MT5 demo execution architecture

Status: **implemented, not yet verified against the live Deriv MT5 demo server.**
The integration gate at the bottom of this file must pass before the venue is
enabled anywhere that matters.

Scope: let Node 3 execute the XAUUSD volume-profile strategy on a **Deriv MT5
demo** account, with full operator control (open, modify, close, close-all,
halt/kill switch) and broker-authoritative state (account, positions, closed
deal history) — while keeping the existing Deriv *options* venue untouched.

---

## 1. Topology

Two facts drive the whole design:

1. **MQL5 sockets are client-only.** `SocketCreate` + `SocketConnect` exist;
   there is no `SocketBind` / `SocketListen` / `SocketAccept`. A process can
   therefore never connect *to* the terminal. The terminal must connect *out*.
2. **The MT5 terminal cannot run on the Node 3 host.** It is a Windows GUI
   application (or a Wine container with a desktop), it needs a persistent
   profile, and Render's containers are ephemeral. The bridge therefore runs on
   the same machine as the terminal, and the terminal-side EA dials into the
   bridge on loopback.

```text
  ┌──────────────────────────── Render (public) ─────────────────────────────┐
  │  Node 3  (node3-execution)                                               │
  │    strategy ──► ExecutionManager ──► MT5 venue (execution_mt5.rs)         │
  │    HTTP  GET /mt5/account|positions|history|status                       │
  │    POST  /mt5/control        (X-Control-Token)                           │
  │    WS    /ws   ◄── frontends      ◄── the bridge dials in here           │
  └──────────────────────────────────▲───────────────────────────────────────┘
                                     │ outbound WSS, `bridge_hello {token}`
                                     │ (no inbound port on the terminal host)
  ┌────────────────── MT5 host (Windows / Wine) ────────────────────────────┐
  │  mt5-bridge (Rust)                                                      │
  │    • listens on MT5_EA_BIND_ADDR:MT5_EA_PORT (default 127.0.0.1:5055)    │
  │    • demo guard, validation, idempotency, reconciliation, JSONL history  │
  │         ▲ line protocol over loopback TCP                               │
  │         │ (EA is the client)                                            │
  │    Mt5BridgeEA.mq5  inside the terminal                                 │
  │         │                                                              │
  │         ▼                                                              │
  │  Deriv MT5 **demo** server                                             │
  └────────────────────────────────────────────────────────────────────────┘
```

* Node 3 never connects to the bridge and never holds MT5 credentials. It
  authenticates the bridge instead (`MT5_BRIDGE_TOKEN`).
* The bridge never exposes a public port. `MT5_EA_BIND_ADDR` defaults to
  loopback; if the EA runs on another machine, bind a private interface and set
  `MT5_EA_TOKEN`, and treat that link as a credential (no token ⇒ read-only).
* `MT5_LOGIN` / `MT5_PASSWORD` are read only by the bridge, only to validate the
  terminal session, and are never sent to Node 3, never logged, and never part
  of a trade payload.

The design doc's original sketch ("bridge hosted on Render, reach the terminal
from there") is **not realizable**: it would require the terminal side to accept
inbound connections, which MQL5 cannot do. The corrected shape above is what is
implemented.

---

## 2. Execution venue rules (fail closed)

`EXECUTION_VENUE` ∈ `deriv_mt5_demo | deriv_demo | chelsea_live | none`.

* The three real venues are **mutually exclusive**. With more than one
  credential set and no explicit `EXECUTION_VENUE`, startup is an error; the
  service never picks one silently.
* An explicit `EXECUTION_VENUE` does **not** paper over a second configured
  venue: it must still be the only one, so a strategy can never trade an account
  it was not pointed at. Unset the other credential (e.g. `DERIV_DEMO_API` when
  switching to MT5) rather than relying on the override.
* Selecting a venue whose credentials are missing is an error.
* If MT5 demo is configured but unavailable (bridge disconnected, EA
  disconnected, wrong account mode, symbol missing), the strategy **refuses**
  the trade. There is no fallback to live, to Deriv options, or to signal-only
  execution while the venue says `deriv_mt5_demo`.
* Signal-only mode exists only as the explicit `none` venue (or when no venue
  credentials are configured at all).

Demo-only guards, in order, each independently sufficient to refuse a trade:

| Layer | Check | Failure mode |
|---|---|---|
| EA (terminal) | `ACCOUNT_TRADE_MODE == DEMO` before every write method | `RESP ERR 10017` |
| EA | `TERMINAL_TRADE_ALLOWED`, symbol `TRADE_MODE == FULL` | `RESP ERR 10017` |
| Bridge | `MODE == demo` from the hello **and** every heartbeat | order refused, `halt` |
| Bridge | configured login == terminal login | order refused |
| Bridge | `trade_allowed` true, account snapshot fresh (`MT5_ACCOUNT_MAX_AGE_MS`) | order refused |
| Node 3 | `mt5_account.account_type == "demo"` and `authorized == true` | order refused |

If the demo guard fails **after** the bridge has been running, the bridge halts
itself (`halt_reason` set, exposed through `/mt5/status` and `bridge_status`) and
does not resume automatically.

---

## 3. Order path

1. Node 3 builds an intent: `side`, XAUUSD, `MT5_VOLUME_LOTS`, ATR-derived
   SL/TP, level name, and a deterministic `idempotency_key`
   (`n3-<ts>-<seq>-<side>-<level>`).
2. Node 3 sends `mt5_order` over the bridge session and waits for the
   `bridge_ack` (bounded by `MT5_ORDER_TIMEOUT_MS`).
3. The bridge validates against live symbol data: symbol exists (exact name or
   explicit `MT5_SYMBOL_MAP` entry — suffixes are never guessed), quote exists
   and is fresh, volume is on the broker's min/max/step grid, stops are on the
   correct side and outside the broker's stops level, and risk
   (`|entry − SL| × volume × contract size`) is within the optional
   `MT5_MAX_RISK_PER_TRADE` cap.
4. The bridge sends `ORDER_SEND` to the EA **once**. Writes are never retried.
5. The broker answer is normalized into an `OrderOutcome`. `status` may only be
   `filled`/`partial` if the broker reported `retcode ∈ {10009, 10008, 10010}`
   **and** `volume > 0`.
6. Node 3 turns a confirmed outcome into a `TradeEvent` with `venue:
   "DerivMt5Demo"`. A timeout/unclear answer becomes `status: "unknown"` — never
   "open".
7. An unknown outcome is reconciled by looking up the order comment (the intent
   id, ≤31 chars) among current positions and recent deals (`FIND`), not by
   re-sending.
8. Every intent, outcome and reconciliation is appended to the audit records and
   the JSONL history store, so the ledger survives a bridge or Node 3 restart.

The bridge keeps a small idempotency ledger keyed by `idempotency_key`: a repeat
of the same intent returns the recorded outcome instead of placing a second
order, even across a Node 3 restart.

---

## 4. Broker data monitor

* Account, positions and closed deals are polled on `MT5_PUSH_INTERVAL_MS`
  (default 2 s) and pushed to Node 3 as `mt5_account`, `mt5_positions`,
  `mt5_history`, plus a `bridge_status` frame; unsolicited terminal events
  (`OnTradeTransaction`) arrive as `bridge_event`.
* Closed deals are mirrored into `MT5_HISTORY_FILE`
  (default `data/mt5_history.jsonl`) with a cursor, so history survives a
  restart of **both** processes and can be replayed to Node 3.
* Reconciliation on startup adopts broker positions carrying the bridge's magic
  number that are missing from the local ledger, and reports them; strategy-side
  candle simulations are never counted as broker positions.
* `OpenTradesSnapshot` carries `mt5_open_positions` / `mt5_open_count` next to
  the Node 3 and Deriv options ones, so a dashboard cannot confuse them.

## 5. Frontend resources

| Resource | Method | Purpose |
|---|---|---|
| `/mt5/account` | GET | `Mt5AccountSnapshot`: `configured`, `connected`, `authorized`, `account_type`, login/server, balance/equity/margin, symbol + contract details, `halted`/`halt_reason`, `error`, `setup_hint` |
| `/mt5/positions` | GET | `Mt5PositionsSnapshot`: broker positions with volume, SL/TP, current price, unrealized PnL |
| `/mt5/history` | GET | `Mt5HistorySnapshot`: closed deals with profit/swap/commission and realized totals |
| `/mt5/status` | GET | `Mt5BridgeStatus`: bridge/EA link state, protocol, counters, uptime, last error |
| `/mt5/control` | POST | `{"action": "halt"\|"resume"\|"close_all"\|"close_position", ...}` — requires `X-Control-Token: $MT5_CONTROL_TOKEN`; 403 when the token is unset. Bridge-only, token-gated; never exposed to browsers without the token |

Field-by-field payloads for every resource and frame are in
[`FRONTEND_RESOURCES.md`](FRONTEND_RESOURCES.md).

WebSocket frames on `/ws`: `mt5_account`, `mt5_positions`, `mt5_history`,
`bridge_status`, `bridge_event` (plus the existing Deriv options and diagnostics
frames). `bridge_hello_ack`, `bridge_ack` and `bridge_hello` are bridge-only and
are never forwarded to browser clients — `topic_matches` returns `false` for them
regardless of subscription.

The existing Deriv options resources (`deriv_account`, `/deriv`, `/account`)
stay as they are until deliberately retired.

> `FRONTEND_HANDOFF.md` is referenced by the original design but is **not** in
> this repository. The shapes above are modelled on `DerivAccountSnapshot` /
> `OpenTradesSnapshot`; if the handoff file defines different field names, the
> snapshot structs in `mt5-bridge/src/snapshot.rs` and
> `node3-execution/src/types.rs` must be changed together — CI's
> `ci/mt5/protocol_lint.py` fails if they drift apart.

---

## 6. Sizing and contract data (XAUUSD)

`ORDER_SIZE` is a Deriv **options stake in USD** and must never be used as a
MetaTrader volume. The MT5 venue sizes in lots (`MT5_VOLUME_LOTS`, default
`0.01`).

On Deriv MT5, XAUUSD is typically 100 oz per lot with 3 digits, so:

| Lots | Ounces | Value of a $1.00 move |
|---|---|---|
| 0.01 | 1 oz | $1.00 |
| 0.10 | 10 oz | $10.00 |
| 1.00 | 100 oz | $100.00 |

The strategy's SL is 200–300 pips, i.e. `200–300 × 0.01 = $2.00–$3.00` on
XAUUSD at these digits, so 0.01 lots risks roughly **$2–$3** per trade. Confirm
the real `contract_size`, `tick_size`, `tick_value`, `digits`, `volume_min`,
`volume_step` and `stops_level` from `/mt5/account` on the actual demo server
during the integration gate; the bridge refuses to trade if it cannot read them.

---

## 7. Integration gate (must pass before enabling the venue)

Unit / contract (runnable in CI and locally, no terminal needed):

- [x] venue exclusivity and fail-closed selection (`node3-execution` config tests)
- [x] intent validation, idempotency keys, volume/stop normalization, stale quote
      rejection, demo-guard refusal, error/timeout classification (`mt5-bridge`
      unit tests, `node3-execution` unit tests)
- [x] fake-terminal contract tests: accepted fill, rejected order, duplicate
      request id, bridge disconnect, stale quote, account-mode mismatch
      (`mt5-bridge/tests/contract.rs`, `MT5_SIM_TERMINAL=1`)
- [x] protocol lint: EA methods/params/response fields, the Node 3 frame +
      struct contract, and the bridge-event names documented for the frontend
      (`ci/mt5/protocol_lint.py`, run in CI)
- [x] both `build` and `bridge` jobs green on `arena/c2a01e42-bot`

Live demo (manual, on the real Deriv MT5 demo account):

- [ ] confirm the broker's XAUUSD symbol name and contract data
- [ ] place the smallest permitted order with SL/TP; verify ticket, fill price,
      SL/TP on the position, and the resulting `/mt5/positions` entry
- [ ] observe `bridge_event` position updates, then close the position and
      confirm the deal appears in `/mt5/history` with the right P&L
- [ ] disconnect the terminal (or the bridge) mid-flight and confirm: no trade is
      marked open, the intent is reconciled from the broker, and the bridge
      reports `halted`
- [ ] restart Node 3 and the bridge; confirm reconciliation and history
      persistence (same deals, no duplicates)
- [ ] point the bridge at a **real** account and confirm it refuses to start
      trading (and refuses every write method)
- [ ] only then: deploy with `EXECUTION_VENUE=deriv_mt5_demo`, and verify the
      reported account mode and symbol in `/mt5/account` and `/diagnostics`

---

## 8. Operator runbook

```bash
# state
curl -s $NODE3/mt5/account   | jq '{configured,connected,authorized,account_type,halted,error}'
curl -s $NODE3/mt5/status    | jq '{ea_connected,ea_mode,orders_sent,orders_filled,orders_unknown}'
curl -s $NODE3/mt5/positions | jq '.positions[] | {ticket,symbol,volume,price_open,sl,tp,unrealized_pnl}'

# stop trading now, keep positions
curl -sX POST $NODE3/mt5/control -H "X-Control-Token: $MT5_CONTROL_TOKEN" \
     -d '{"action":"halt","reason":"operator"}'

# flatten everything (also implies halt when sent as a halt with flatten)
curl -sX POST $NODE3/mt5/control -H "X-Control-Token: $MT5_CONTROL_TOKEN" \
     -d '{"action":"close_all","reason":"operator flatten"}'

# close one position
curl -sX POST $NODE3/mt5/control -H "X-Control-Token: $MT5_CONTROL_TOKEN" \
     -d '{"action":"close_position","position_ticket":123456789}'

# resume (only when the demo guard and link are healthy)
curl -sX POST $NODE3/mt5/control -H "X-Control-Token: $MT5_CONTROL_TOKEN" \
     -d '{"action":"resume"}'
```

Bridge-side kill switch: stop the bridge process (trading goes away with it) or
start it with `MT5_TRADING_ENABLED=0` (read-only monitoring, no orders).

Configuration reference: [`mt5-bridge/README.md`](../../mt5-bridge/README.md).
