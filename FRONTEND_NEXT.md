# Frontend handoff — what to build next (Node 2 dashboard)

This is the instruction sheet for the frontend agent working on the **Node 2**
dashboard. It is written against the Node 1 branch that ships:

* **no order flow** — the `bubbles` WS topic, its four event kinds and every
  bubble/absorption renderer are gone;
* the **built-in strategy evaluator** at `GET /strategy` + WS `strategy`;
* the **demo (paper) account** at `GET /demo` + WS `demo`
  (XAUUSD, **1:1000 leverage, 0.10 spread, 0.01–1.00 lots**).

Everything below is a client-side change. Node 1 is already serving both
endpoints — do not add a backend, a proxy or a database to the dashboard.

---

## 0. Ground rules

1. **Read-only.** There is no `POST`/`PUT`/`PATCH`/`DELETE` anywhere in Node 1.
   Do not build order tickets, "buy/sell" buttons, lot inputs or a close
   button. The demo engine trades itself; the dashboard only *observes* it.
2. **Do not recreate order flow.** No `bubbles` subscription, no bubble
   overlays, no absorption markers, no delta/cumulative-delta panel. The topic
   is silent by design and the data no longer exists.
3. **One WebSocket.** Keep the existing `/ws` connection; add `strategy` and
   `demo` to the `subscribe` topic list and remove `bubbles`.
4. All money is **USD**, all prices are **XAUUSD**, lot sizes are **0.01-step**.

---

## 1. Breaking changes to remove first

| What | Where it usually lives | Action |
|---|---|---|
| `bubbles` topic in the subscribe payload | `useMarketSocket` / `socket.ts` | delete |
| `BUY_BUBBLE` / `SELL_BUBBLE` / `ABS_BUY` / `ABS_SELL` handling | chart overlay + legend | delete |
| Bubble/absorption legend ("four order-flow colors") | chart header | delete |
| Any "order flow", "delta", "absorption" label | panels, tooltips, README of the UI | delete |
| `bubbles` in WS type unions | `types/ws.ts` | delete |

Ship this first, on its own, so the dashboard stops rendering a dead feature
before the new panels land.

---

## 2. New REST endpoints

### `GET /strategy`

```ts
interface StrategyParams {
  enabled: boolean;
  min_confidence: number;        // 0..1, default 0.55
  risk_pct: number;              // % of equity per trade, default 0.5
  atr_period: number;            // 14
  atr_stop_mult: number;         // 1.5
  reward_risk: number;           // 2.0
  min_lots: number;              // 0.01
  max_lots: number;              // 1.00
  lot_step: number;              // 0.01
  fast_ema: number; slow_ema: number;   // 9 / 21
  momentum_bars: number; tick_pressure_bars: number;
  event_blackout_secs: number;   // 900
  leverage: number;              // 1000
  max_margin_pct: number;        // 50
}

interface SignalFactor {
  name: "trend" | "value_location" | "level_reaction" | "momentum"
      | "tick_pressure" | "swing_structure" | string;
  value: number;          // -1..1, positive = bullish
  weight: number;         // 0.10 .. 0.30
  contribution: number;   // value * weight
  detail: string;         // human-readable, e.g. "EMA9 3415.10 vs EMA21 3409.32"
}

interface StrategyDecision {
  ts: number;             // epoch ms
  symbol: "XAUUSD";
  action: "buy" | "sell" | "flat";
  score: number;          // -1..1
  confidence: number;     // 0..1
  price: number;
  atr: number;
  regime: "trend_up" | "trend_down" | "balance";
  volatility: "expanding" | "normal" | "contracting";
  stop_loss: number | null;
  take_profit: number | null;
  reward_risk: number | null;
  lots: number;           // 0.00 when action === "flat"
  risk_usd: number;
  reasons: SignalFactor[];
  notes: string[];        // e.g. "confidence 0.42 below threshold", blackout
  next_event_ts: number | null;
}

interface StrategySnapshot {
  symbol: "XAUUSD";
  enabled: boolean;
  params: StrategyParams;
  last: StrategyDecision | null;   // null until the first evaluation
  history: StrategyDecision[];     // up to 120, oldest first
}
```

### `GET /demo`

```ts
interface DemoQuote { bid: number; ask: number; mid: number; spread: number; ts: number; }

interface DemoAccount {
  currency: "USD";
  balance: number;
  equity: number;
  margin: number;
  free_margin: number;
  margin_level: number;     // equity / margin * 100, 0 when flat
  unrealized_pnl: number;
  peak_equity: number;
  max_drawdown_pct: number;
}

interface DemoPosition {
  id: number;
  side: "buy" | "sell";
  lots: number;             // 0.01..1.00
  entry: number;
  stop_loss: number | null;
  take_profit: number | null;
  opened_at: number;
  risk_usd: number;
  margin: number;
  unrealized_pnl: number;
  reason: string;           // "trend + value_location + momentum"
}

interface DemoTrade {
  id: number; side: "buy" | "sell"; lots: number;
  entry: number; exit: number; opened_at: number; closed_at: number;
  pnl: number; r_multiple: number;
  exit_reason: "signal_flip" | "stop_loss" | "take_profit" | "stop_out";
}

interface DemoSnapshot {
  enabled: boolean;
  mode: "demo";
  symbol: "XAUUSD";
  leverage: number;         // 1000
  quote: DemoQuote;
  account: DemoAccount;
  params: { start_balance: number; leverage: number; spread: number;
            min_lots: number; max_lots: number; lot_step: number;
            stop_out_pct: number; cooldown_secs: number; };
  position: DemoPosition | null;
  stats: {
    closed_trades: number; wins: number; losses: number; win_rate: number; // 0..100
    gross_profit: number; gross_loss: number; net_pnl: number;
    profit_factor: number; avg_r: number; max_drawdown_pct: number;
  };
  recent_trades: DemoTrade[];  // newest first, max 50
  note: string;                // last engine action, e.g. "opened buy 0.18 lot(s) ..."
  ts: number;
}
```

Both routes send `Cache-Control: no-store`. Fetch them on mount (so the UI has
state before the first frame) and then trust the WebSocket.

---

## 3. WebSocket changes

Subscribe:

```json
{"type":"subscribe","topics":["levels","candle","tick_volume","strategy","demo","calendar","status"]}
```

* `{"type":"strategy","data":StrategyDecision}` — one frame per evaluation
  (default every 15 s). Replaces the previous decision, and appends to the
  sparkline/history.
* `{"type":"demo","data":DemoSnapshot}` — account snapshots: on every open,
  close and stop-out, and once per evaluation. Replace the whole snapshot; the
  payload is complete, there is no patch semantics.
* On subscribe, the engine immediately replays the latest `strategy` frame
  (if a decision exists) and a `demo` snapshot, then a `status` frame.
* A `bubbles` subscription now yields **nothing**. Delete it rather than
  waiting for frames that never arrive.

Keep the existing heartbeat handling (30 s) and reconnect-with-backoff. On
reconnect, re-send the full subscribe payload — the replay is what reseeds the
panels.

Polling fallback: if the socket is down, poll `GET /strategy` and `GET /demo`
every 5 s and show a "delayed" badge.

---

## 4. UI work, in priority order

### P0 — remove order flow, keep the dashboard honest
- Delete the bubble/absorption layer, legend and any `bubbles` code path.
- Add `strategy` + `demo` to the subscribe list; keep the connection healthy.
- Ship it. Nothing below is blocked on a backend change.

### P1 — Strategy panel (top of the right rail, above the levels table)

Show, from the latest decision:

| Element | Source | Format |
|---|---|---|
| Action badge | `action` | `BUY` green / `SELL` red / `FLAT` grey |
| Confidence gauge | `confidence` | 0–100 %, threshold tick at `params.min_confidence * 100` |
| Score bar | `score` | −1 … +1, zero-centred, bullish right |
| Regime chip | `regime` | "Trend up" / "Trend down" / "Balance" |
| Volatility chip | `volatility` | "Expanding" / "Normal" / "Contracting" |
| Price / ATR | `price`, `atr` | 2 dp both |
| Stop / target | `stop_loss`, `take_profit` | 2 dp, "—" when null |
| Size | `lots`, `risk_usd` | `0.18 lots`, `$48.60 at risk` |
| Age | `ts` | "2 s ago" / "14 s ago", grey out beyond 60 s |

**Reasons list**: one row per `reasons[]` entry — name (humanised), a small
signed bar of `value`, the `weight` as a percentage, and `detail` as a tooltip
or muted second line. Sort by `|contribution|` descending. This is the panel
that explains the engine; do not hide it behind a click on desktop.

**Notes**: render `notes[]` as amber footnotes when the decision is `flat`
(blackout, "confidence 0.42 below threshold", "no edge"). When
`next_event_ts` is set and in the future, show "event in 12 min — standing
aside" until it passes.

**History strip**: last ~30 decisions as a colour-coded bar (buy/sell/flat)
with a confidence sparkline. `GET /strategy.history` seeds it; each WS frame
appends.

### P1 — Demo account header (replaces any account placeholder)

A compact strip showing:
`DEMO 1:1000 · spread 0.10` badge → `Balance` → `Equity` → `Free margin` →
`Margin level` → `Open PnL`.

* Values from `demo.account`; PnL in green/red with a leading `+`/`-`.
* `margin_level` as a percentage, amber under 200 %, red under 100 %.
* Show `mode: "demo"` prominently so nobody mistakes this for a live account.
* Tooltip: "Simulated fills — no broker, no real money."

### P1 — Open position card

When `demo.position` is non-null:

* `side`, `lots`, `entry`, `stop_loss`, `take_profit`, `unrealized_pnl`,
  `margin`, `risk_usd`, `reason`, `opened_at` (local time).
* A progress bar from entry to target with the stop on the other side
  (green zone = profit side), using `quote.bid` / `quote.ask` as the current
  marker (a long exits at the bid, a short at the ask).
* Empty state when flat: "No open position — the engine is waiting for a
  qualifying signal", plus the last `demo.note`.

### P2 — Trade history table

`demo.recent_trades`, newest first, columns:
`#`, `Side`, `Lots`, `Entry`, `Exit`, `P&L (USD)`, `R`, `Exit reason`, `Opened`, `Closed`.

* P&L and R coloured by sign; R shown as `+1.85R` / `−1.00R`.
* `exit_reason` as a chip: SL (red), TP (green), flip (blue), stop-out (orange).
* Cap the table at 50 rows (the payload caps it), virtualise if you keep more.
* Empty state: "No closed demo trades yet."

### P2 — Performance card

From `demo.stats`: `closed_trades`, `win_rate`, `net_pnl`, `profit_factor`,
`avg_r`, `max_drawdown_pct`. Two-column grid; `profit_factor` shows `—` when
`closed_trades === 0`, and cap the display at `99+` (the engine caps it at 99).

### P2 — Chart overlays

Using `/levels` + `strategy` + `demo`:

* horizontal `POC` / `VAH` / `VAL` lines (already there) — keep;
* entry line + SL/TP lines for the open position, labelled with price and lot size;
* a small marker on the candle of the last `action` change (buy ▲ / sell ▼);
* an optional shaded band around `price ± atr` to show the stop unit.

Do **not** draw anything for order flow — there is no order flow.

### P3 — Status & settings drawer

`GET /status` now includes `strategy` and `demo` blocks
(`enabled`, `eval_secs`, `min_confidence`, `risk_pct`; `enabled`, `mode`,
`leverage`, `spread`, `min_lots`, `max_lots`). Show them read-only in the
settings drawer, plus feed liveness from `status.feeds`. If
`demo.enabled === false`, dim the account strip and show "demo engine off".

---

## 5. Formatting rules (pin them in one `format.ts`)

| Value | Format |
|---|---|
| Price | 2 decimals, `toFixed(2)`, group thousands |
| Lots | 2 decimals (`0.18`, `1.00`) |
| Money | 2 decimals with `$`, signed for PnL (`+$12.40`, `−$61.00`) |
| Percent | 1 decimal, `%` |
| R multiple | 2 decimals, signed, `R` suffix |
| Time | local time, `HH:mm:ss`; dates `DD MMM` |
| Confidence | 0–100 % (multiply by 100) |

Never show raw floats (`0.30000000000000004`), never show a `null` — use `—`.
Prices are in price units, not points/pips; do not introduce a pip conversion.

---

## 6. Edge cases the UI must survive

* `last === null` (engine just booted): "Waiting for the first evaluation…".
* `action === "flat"` with `lots === 0`: show the action badge and `notes`,
  never a zero-lot position.
* `position === null` with trades in history: history still renders.
* `demo.note` can read `cooldown: no new position for another 42s` after a
  stop — show it verbatim in the empty-state, it is not an error.
* `margin_level === 0`: flat account — show `—`, not `0 %` or `∞`.
* Negative `equity` after a gap (possible on 1:1000): display it in red, do not
  clamp.
* `stop_loss`/`take_profit` null on an active decision (should not happen, but
  the type allows it): hide the SL/TP lines.
* Stale frames: grey the strategy panel after ~60 s without a frame and show
  the socket state.
* Reconnect replay: replace state, do not duplicate history entries (key on
  `decision.ts`).
* `stats.closed_trades === 0`: win rate and profit factor are `0` / `—`.

---

## 7. Acceptance checklist

- [ ] The dashboard never subscribes to `bubbles` and never renders a bubble.
- [ ] `/strategy` and `/demo` are fetched on mount and then kept live by the
      `strategy` / `demo` WS frames (no polling when the socket is healthy).
- [ ] Action badge, confidence gauge, score bar, reasons list and notes render
      for `buy`, `sell` and `flat`.
- [ ] The account strip shows `1:1000` and `spread 0.10` from the payload —
      not hardcoded — and reads `Balance`, `Equity`, `Free margin`,
      `Margin level`, `Open PnL`.
- [ ] An open position shows entry, SL, TP, lots, floating PnL and margin; a
      flat account shows the empty state.
- [ ] Trade history renders `pnl`, `r_multiple` and `exit_reason` correctly for
      all four exit reasons.
- [ ] No `POST`/`PUT`/`PATCH`/`DELETE` is ever called; no "trade now" button.
- [ ] `ts`-keyed updates do not duplicate history on reconnect.
- [ ] Currency, lot and price formatting comes from one shared formatter.
- [ ] The page boots with the API unreachable (skeleton states, no crash).

---

## 8. Suggested component layout

```
src/
  api/
    strategy.ts          // fetchStrategy(): StrategySnapshot
    demo.ts              // fetchDemo(): DemoSnapshot
  ws/
    types.ts             // StrategyDecision, DemoSnapshot, MarketFrame union (no bubbles)
    useMarketSocket.ts   // subscribe list, reconnect, dispatch
  features/
    strategy/
      StrategyPanel.tsx  // action, confidence, score, regime
      ReasonsList.tsx
      DecisionHistory.tsx
    demo/
      DemoAccountStrip.tsx
      PositionCard.tsx
      TradeHistoryTable.tsx
      PerformanceCard.tsx
    chart/
      LevelLines.tsx      // existing
      PositionLines.tsx   // entry / SL / TP + side marker
  lib/
    format.ts            // money, price, lots, percent, R, time
    strategyMath.ts      // confidence %, progress-to-target
```

State: keep one reducer per topic (`strategy`, `demo`) keyed by `ts`, exactly
like the existing `candle` upsert. The REST payload is only the seed.

---

## 9. What is *not* in scope

* Real order entry, broker connection, MT5, account credentials.
* New Node 1 endpoints. If a field is missing, ask for it — do not derive it
  client-side from price (e.g. do not recompute margin; use
  `position.margin`).
* Re-adding any order-flow visualisation.
* Server-side rendering or a new backend: Node 2 stays a static site calling
  Node 1 with `CORS_ALLOWED_ORIGIN` set to its own origin.
