# Deriv MT5 demo execution architecture

Status: **implemented, not yet verified against the live Deriv MT5 demo server.**
The integration gate at the bottom of this file must pass before the venue is
enabled anywhere that matters.

Scope: let **Node 4** execute the intents Node 3 produces on a **Deriv MT5
demo** account, with full operator control (open, modify, close, close-all,
halt/kill switch) and broker-authoritative state (account, positions, closed
deal history) — while keeping the Deriv *options* venue in the same service.

Node 4 owns execution; Node 3 owns strategy. Node 4 receives a complete intent
(side, stop, target, risk-reward, `intent_id`) and either executes it or refuses
it — it never derives, adjusts or re-interprets the strategy's prices.

---

## 1. Topology

Two facts drive the whole design:

1. **MQL5 sockets are client-only.** `SocketCreate` + `SocketConnect` exist;
   there is no `SocketBind` / `SocketListen` / `SocketAccept`. A process can
   therefore never connect *to* the terminal. The terminal must connect *out*.
2. **The MT5 terminal cannot run inside the Node 4 process.** It is a Windows
   GUI application, it needs its own Wine prefix and login session, and it must
   not share the execution service's memory budget. The bridge therefore runs on
   the same *machine* as the terminal, and the terminal-side EA dials into the
   bridge on loopback — which is also why the terminal host is a separate
   service rather than another endpoint of this one. `mt5-host`
   (`../mt5-host/README.md`) is that service: a Linux container that installs
   the terminal under Wine, attaches the EA and supervises the bridge, with no
   desktop and no inbound control port. §1.1 covers what it changes.

```text
  ┌──────────────────────────── Render (public) ─────────────────────────────┐
  │  Node 4  (node4-execution)                                               │
  │    Node 3 intents ──► ExecutionManager ──► MT5 venue (execution_mt5.rs)   │
  │    HTTP  GET /mt5/account|positions|history|status                       │
  │    POST  /mt5/control        (X-Control-Token)                           │
  │    WS    /ws          ◄── frontends (read-only)                          │
  │    WS    /mt5/bridge  ◄── the bridge dials in here                       │
  └──────────────────────────────────▲───────────────────────────────────────┘
                                     │ outbound WSS, `bridge_hello {token}`
                                     │ (no inbound port on the terminal host)
  ┌──────── MT5 host (Linux + wine + Xvfb, or Windows) ─────────────────────┐
  │  mt5-host (Rust)            supervisor: Xvfb, prefix, install, EA,      │
  │                             startup ini, terminal, /health /readyz      │
  │  mt5-bridge (Rust)          started and restarted by mt5-host           │
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

* Node 4 never connects to the bridge and never holds MT5 credentials. It
  authenticates the bridge instead (`MT5_BRIDGE_TOKEN`, a secret separate from
  the Node 3 `NODE4_SHARED_TOKEN` and the operator `MT5_CONTROL_TOKEN`).
* The bridge never exposes a public port. `MT5_EA_BIND_ADDR` defaults to
  loopback; if the EA runs on another machine, bind a private interface and set
  `MT5_EA_TOKEN`, and treat that link as a credential (no token ⇒ read-only).
* `MT5_LOGIN` / `MT5_PASSWORD` are read only by the bridge, only to validate the
  terminal session, and are never sent to Node 4, never logged, and never part
  of a trade payload.

The design doc's original sketch ("bridge hosted on Render, reach the terminal
from there") is **not realizable**: it would require the terminal side to accept
inbound connections, which MQL5 cannot do. The corrected shape above is what is
implemented.

### 1.1 The terminal host on Linux (supersedes the Windows/Wine-desktop assumption)

The MT5 host is no longer assumed to be a Windows box (or a Wine container with
a desktop someone can look at). `mt5-host` runs the same topology inside one
headless Linux container:

* **Xvfb** provides a virtual display, so the terminal's GUI has a screen that
  nothing ever renders to. There is no VNC, no RDP and no open graphics port.
* **Wine** runs `terminal64.exe`, installed unattended from `mt5setup.exe /auto`
  during the container's boot stages.
* **The EA is attached by configuration**, not by hand: the host writes the MT5
  startup ini (`[StartUp] Expert=Node4\Mt5BridgeEA`) and compiles the EA with
  `metaeditor64 /compile`, so a fresh container reaches the same state a
  hand-configured Windows terminal would.
* **The bridge is a child process** of the host, inheriting the operator's
  environment plus the defaults the host owns (`MT5_TRADING_ENABLED=0`,
  `MT5_HISTORY_FILE`, `MT5_EA_PORT`).
* `GET /health`, `/readyz` and `/diagnostics` report the terminal side; there is
  still no inbound control port, and the host has no order path of its own.

The parts of the deployment that this does *not* fix are the platform's, and
they are documented honestly in `mt5-host/README.md` §*Free-plan realities*: an
ephemeral filesystem (the prefix is rebuilt from the image on every deploy), a
free instance that spins down after 15 minutes without inbound traffic (taking
the login session with it), and 0.1 CPU for an unloved Windows application. The
Windows deployment remains supported — nothing in the bridge, the EA or the
protocol changes — it is simply no longer the only shape.

---

## 1.2 Where credentials live on the Linux host

The startup ini is the only file that ever holds the terminal password. It is
written mode 0600 into the state directory and deleted as soon as the EA
connects, because from then on the terminal owns its session:

| secret | where it lives | where it must never appear |
|---|---|---|
| `MT5_PASSWORD` | the container's environment, then `mt5-start.ini` (0600, deleted after attach) | logs, `/diagnostics`, the bridge, Node 4, a prepared prefix |
| `MT5_LOGIN` / `MT5_SERVER` | same as above; they are identifiers, not secrets | — |
| `MT5_BRIDGE_TOKEN` | environment of Node 4 and of the bridge | the prepared prefix image/archive (`MT5_HOST_PREPARE_ONLY=1` refuses to run with it set) |
| `MT5_EA_TOKEN` | environment of the bridge and the EA's input | any non-loopback listener (unset ⇒ the bridge is read-only) |

`ci/mt5/protocol_lint.py` fails the build if the bundle image could bake a
credential into a published prefix, and unit tests in `mt5-host/src` assert that
no credential-shaped field can reach `/diagnostics`.

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
  disconnected, wrong account mode, symbol missing), Node 4 **refuses** the
  intent and reports `rejected`. There is no fallback to live, to Deriv options,
  or to signal-only execution while the venue says `deriv_mt5_demo`.
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
| Node 4 | `mt5_account.account_type == "demo"`, `authorized == true`, not halted, and a bridge frame younger than 15 s | intent refused (`rejected`) |

If the demo guard fails **after** the bridge has been running, the bridge halts
itself (`halt_reason` set, exposed through `/mt5/status` and `bridge_status`) and
does not resume automatically.

---

## 3. Intent input (Node 3 → Node 4) and reports (Node 4 → Node 3)

The MT5 order path starts with the protocol v1 intent, not with an order:

```json
{"type":"trade_intent","data":{
  "schema_version":1,
  "intent_id":"n3-xauusd-1700000000000-buy-pw-poc",
  "strategy":"vp_break_retest_v1","symbol":"XAUUSD","side":"buy",
  "order_type":"market","reference_price":2650.25,
  "stop_loss":2647.45,"take_profit":2656.25,"risk_reward":2.93,
  "level_name":"PW PoC","source_candle_time":1700000000000,
  "created_at":1700000000000,"expires_at":1700000000120
}}
```

The intent carries **no** venue, account, stake, lot size or credential — those
belong to Node 4. Node 4 validates it (schema version, `intent_id` shape and
uniqueness, symbol supported *and* explicitly mapped, side exactly
`buy`/`sell`, order type supported, all prices/RR finite and positive, stop and
target on the correct side of the entry, `now < expires_at`), refuses an expired
or unsafe intent, and only then durably records the `intent_id`.

Every answer back to Node 3 is an `execution_report` with the same `intent_id`:

* `accepted` — Node 4 durably owns the ID (sent **before** any broker write);
* `filled` / `partial` — broker-confirmed, with ticket/price/volume;
* `rejected` — nothing reached the broker (`error_code` explains why);
* `unknown` — a write may have reached the broker: reconciled by
  `intent_id`/comment/order/deal/position, then published as `filled`/`closed`;
  it is **never** re-sent;
* `cancelled` / `closed` — lifecycle completion;
* a repeated `intent_id` replays the recorded report with `duplicate: true`.

---

## 4. Order path inside the MT5 venue

1. Node 4 sends `mt5_order` with `idempotency_key = intent_id` over the bridge
   session and waits for the `bridge_ack` (bounded by `MT5_ORDER_TIMEOUT_MS`).
2. The bridge validates against live symbol data: symbol exists (exact name or
   explicit `MT5_SYMBOL_MAP` entry — suffixes are never guessed), quote exists
   and is fresh, volume is on the broker's min/max/step grid, stops are on the
   correct side and outside the broker's stops level, and risk
   (`|entry − SL| × volume × contract size`) is within the optional
   `MT5_MAX_RISK_PER_TRADE` cap.
3. The bridge sends `ORDER_SEND` to the EA **once**. Writes are never retried.
4. The broker answer is normalized into an `OrderOutcome`. `status` may only be
   `filled`/`partial` if the broker reported `retcode ∈ {10009, 10008, 10010}`
   **and** `volume > 0`.
5. Node 4 turns a confirmed outcome into an `ExecutionTrade` with `venue:
   "deriv_mt5_demo"` and reports `filled`/`partial` to Node 3. A timeout/unclear
   answer becomes `unknown` — never "open".
6. An unknown outcome is reconciled by looking up the order comment (the intent
   id, ≤31 chars) among current positions and recent deals (`FIND`), not by
   re-sending.
7. Every intent, broker command, outcome and reconciliation is appended to Node
   4's ledger (`EXECUTION_LEDGER_FILE`) and the bridge's JSONL history store, so
   the audit survives a restart of either process.

The bridge keeps a small idempotency ledger keyed by `idempotency_key`: a repeat
of the same intent returns the recorded outcome instead of placing a second
order, even across a Node 4 **or** bridge restart.

---

## 5. Broker data monitor

* Account, positions and closed deals are polled on `MT5_PUSH_INTERVAL_MS`
  (default 2 s) and pushed to Node 4 as `mt5_account`, `mt5_positions`,
  `mt5_history`, plus a `bridge_status` frame; unsolicited terminal events
  (`OnTradeTransaction`) arrive as `bridge_event`.
* Closed deals are mirrored into `MT5_HISTORY_FILE`
  (default `data/mt5_history.jsonl`) with a cursor, so history survives a
  restart of **both** processes and can be replayed to Node 4.
* Reconciliation on startup adopts broker positions carrying the bridge's magic
  number that are missing from the local ledger, and reports them; strategy-side
  candle simulations are never counted as broker positions.
* `OpenTradesSnapshot` carries `mt5_open_positions` / `mt5_open_count` next to
  Node 4's own `node4_open_trades` and the Deriv options ones, so a dashboard
  cannot confuse them.

## 6. Frontend resources

| Resource | Method | Purpose |
|---|---|---|
| `/mt5/account` | GET | `Mt5AccountSnapshot`: `configured`, `connected`, `authorized`, `account_type`, login/server, balance/equity/margin, symbol + contract details, `halted`/`halt_reason`, `error`, `setup_hint` |
| `/mt5/positions` | GET | `Mt5PositionsSnapshot`: broker positions with volume, SL/TP, current price, unrealized PnL |
| `/mt5/history` | GET | `Mt5HistorySnapshot`: closed deals with profit/swap/commission and realized totals |
| `/mt5/status` | GET | `Mt5BridgeStatus`: bridge/EA link state, protocol, counters, uptime, last error |
| `/mt5/bridge` | WS | The bridge's private session: `bridge_hello { token: $MT5_BRIDGE_TOKEN }` then snapshots/commands. Never a browser endpoint. |
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
> `node4-execution/src/types.rs` must be changed together — CI's
> `ci/mt5/protocol_lint.py` fails if they drift apart.

---

## 7. Sizing and contract data (XAUUSD)

`ORDER_SIZE` is a Deriv **options stake in USD** and must never be used as a
MetaTrader volume. The MT5 venue sizes in lots (`MT5_VOLUME_LOTS`, default
`0.01`).

On Deriv MT5, XAUUSD is typically 100 oz per lot with 3 digits, so:

| Lots | Ounces | Value of a $1.00 move |
|---|---|---|
| 0.01 | 1 oz | $1.00 |
| 0.10 | 10 oz | $10.00 |
| 1.00 | 100 oz | $100.00 |

Node 3 currently sends stops of roughly 200–300 pips, i.e.
`200–300 × 0.01 = $2.00–$3.00` on XAUUSD at these digits, so 0.01 lots risks
roughly **$2–$3** per trade; the size of the stop is Node 3's decision and Node 4
never recomputes it (it only refuses an order that violates its own cap). Confirm
the real `contract_size`, `tick_size`, `tick_value`, `digits`, `volume_min`,
`volume_step` and `stops_level` from `/mt5/account` on the actual demo server
during the integration gate; the bridge refuses to trade if it cannot read them.

---

## 8. Integration gate (must pass before enabling the venue)

Unit / contract (runnable in CI and locally, no terminal needed):

- [x] venue exclusivity and fail-closed selection (`node4-execution` config tests)
- [x] intent validation, idempotency keys, volume/stop normalization, stale quote
      rejection, demo-guard refusal, error/timeout classification (`mt5-bridge`
      unit tests, `node4-execution` unit tests)
- [x] fake-terminal contract tests: accepted fill, rejected order, duplicate
      request id, bridge disconnect, stale quote, account-mode mismatch
      (`mt5-bridge/tests/contract.rs`, `MT5_SIM_TERMINAL=1`)
- [x] protocol lint: EA methods/params/response fields, the Node 4 frame +
      struct contract, and the bridge-event names documented for the frontend
      (`ci/mt5/protocol_lint.py`, run in CI)
- [x] both the `node4` and `bridge` CI jobs green (`.github/workflows/build.yml`)

Live demo (manual, on the real Deriv MT5 demo account):

- [ ] confirm the broker's XAUUSD symbol name and contract data
- [ ] place the smallest permitted order with SL/TP; verify ticket, fill price,
      SL/TP on the position, and the resulting `/mt5/positions` entry
- [ ] observe `bridge_event` position updates, then close the position and
      confirm the deal appears in `/mt5/history` with the right P&L
- [ ] disconnect the terminal (or the bridge) mid-flight and confirm: no trade is
      marked open, the intent is reconciled from the broker, and the bridge
      reports `halted`
- [ ] restart Node 4 and the bridge; confirm reconciliation and history
      persistence (same deals, no duplicates)
- [ ] point the bridge at a **real** account and confirm it refuses to start
      trading (and refuses every write method)
- [ ] only then: deploy with `EXECUTION_VENUE=deriv_mt5_demo`, and verify the
      reported account mode and symbol in `/mt5/account` and `/diagnostics`

---

## 9. Operator runbook

```bash
# state
curl -s $NODE4/mt5/account   | jq '{configured,connected,authorized,account_type,halted,error}'
curl -s $NODE4/mt5/status    | jq '{ea_connected,ea_mode,orders_sent,orders_filled,orders_unknown}'
curl -s $NODE4/mt5/positions | jq '.positions[] | {ticket,symbol,volume,price_open,sl,tp,unrealized_pnl}'

# stop trading now, keep positions
curl -sX POST $NODE4/mt5/control -H "X-Control-Token: $MT5_CONTROL_TOKEN" \
     -d '{"action":"halt","reason":"operator"}'

# flatten everything (also implies halt when sent as a halt with flatten)
curl -sX POST $NODE4/mt5/control -H "X-Control-Token: $MT5_CONTROL_TOKEN" \
     -d '{"action":"close_all","reason":"operator flatten"}'

# close one position
curl -sX POST $NODE4/mt5/control -H "X-Control-Token: $MT5_CONTROL_TOKEN" \
     -d '{"action":"close_position","position_ticket":123456789}'

# resume (only when the demo guard and link are healthy)
curl -sX POST $NODE4/mt5/control -H "X-Control-Token: $MT5_CONTROL_TOKEN" \
     -d '{"action":"resume"}'
```

Bridge-side kill switch: stop the bridge process (trading goes away with it) or
start it with `MT5_TRADING_ENABLED=0` (read-only monitoring, no orders). On the
Linux host, `MT5_TRADING_ENABLED=0` is the default and the host passes it to the
bridge unless the operator set it explicitly.

### 9.1 When the MT5 link looks wrong

Check the terminal host before Node 4 — it knows whether the terminal, its login
and the EA are actually up:

```bash
curl -s $MT5_HOST/health      # 200 while the process is alive, whatever else is wrong
curl -s $MT5_HOST/readyz      # 200 only when display + prefix + terminal + EA + bridge agree
curl -s $MT5_HOST/diagnostics | jq '{stage,last_error,processes,node4,memory_mb}'
```

`/readyz` answers `503` with one reason per line, so a monitor can alert on the
first line instead of guessing, and `/diagnostics.node4` shows Node 4's own view
of this bridge (connected, authorized, `ea_connected`, `ea_mode`, halted) next to
the local one — which is usually enough to tell a dead terminal from a rejected
token. See `mt5-host/README.md` §*Troubleshooting* for the reason-by-reason
table.

Configuration reference: [`mt5-bridge/README.md`](../../mt5-bridge/README.md)
and [`mt5-host/README.md`](../../mt5-host/README.md).
