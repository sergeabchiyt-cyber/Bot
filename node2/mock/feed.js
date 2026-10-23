// mock/feed.js — synthetic feeds that mirror the published Node 1 / Node 4
// contracts exactly (shapes taken from the live samples recorded by Node 1's
// ci/live snapshots and Node 4's FRONTEND_RESOURCES.md). Used by the dev
// server in mock mode and by tests.
//
// IMPORTANT: the mock is clearly labeled synthetic. It exists so the
// dashboard can be demonstrated and tested without the live services.

export function mulberry32(seed) {
  let a = seed >>> 0;
  return function () {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const BASE_PRICE = 2655.0;

export function makeCandles(nowSec, { count = 15, intervalSec = 900, seed = 1, lagBars = 2 } = {}) {
  const rand = mulberry32(seed);
  const bars = [];
  // End slightly behind the current bar boundary to mimic the ~27-minute
  // edge lag observed on the live feed.
  const lastBar = Math.floor(nowSec / intervalSec) * intervalSec - lagBars * intervalSec;
  let price = BASE_PRICE - 6 + rand() * 12;
  for (let i = 0; i < count; i++) {
    const t = lastBar - (count - 1 - i) * intervalSec;
    const open = price;
    const drift = (rand() - 0.5) * 3.2;
    const close = Math.max(100, open + drift);
    const hi = Math.max(open, close) + rand() * 1.8;
    const lo = Math.min(open, close) - rand() * 1.8;
    const volume = Math.round(60 + rand() * 140);
    bars.push({ time: t, open: +open.toFixed(2), high: +hi.toFixed(2), low: +lo.toFixed(2), close: +close.toFixed(2), volume, source: "sifting" });
    price = close;
  }
  return bars;
}

export function makeLevels(nowSec, price, { seed = 2 } = {}) {
  const rand = mulberry32(seed);
  const windows = ["PT", "SW", "PS", "CW", "CUSTOM"];
  return windows.map((window, idx) => {
    const half = 8 + idx * 6 + rand() * 14;
    const poc = price + (rand() - 0.5) * 2;
    return {
      window,
      poc: +poc.toFixed(2),
      vah: +(poc + half * 0.8).toFixed(2),
      val: +(poc - half * 0.8).toFixed(2),
      start: Math.floor(nowSec / 3600) * 3600 - 2 * 3600 * (5 - idx),
      end: Math.floor(nowSec / 3600) * 3600,
      timestamp: nowSec,
      direction: rand() > 0.5 ? "up" : "down",
      swing_high: +(poc + half).toFixed(2),
      swing_low: +(poc - half).toFixed(2),
      sunday_open: +(BASE_PRICE - 3.5).toFixed(2),
    };
  });
}

export function makeTickBars(nowSec, { count = 8, seed = 3 } = {}) {
  const rand = mulberry32(seed);
  const bars = [];
  for (let i = 0; i < count; i++) {
    const t = Math.floor(nowSec / 900) * 900 - (count - i) * 900;
    const ticks = Math.round(1200 + rand() * 4000);
    const upTicks = Math.round(ticks * (0.35 + rand() * 0.3));
    const downTicks = Math.round(ticks * (0.3 + rand() * 0.3));
    bars.push({
      time: t,
      ticks,
      up_ticks: upTicks,
      down_ticks: downTicks,
      flat_ticks: ticks - upTicks - downTicks,
      close: +(BASE_PRICE + (rand() - 0.5) * 6).toFixed(2),
      last_tick: Math.random() > 0.5 ? "B" : "S",
      ticks_per_sec: +(ticks / (30 + rand() * 30)).toFixed(1),
      closed: i < count - 1,
      source: "mt5",
    });
  }
  return bars;
}

export function makeCalendar(nowSec) {
  const today = Math.floor(nowSec / 86400) * 86400;
  const events = [
    { actual: "22B", currency: "USD", event: "CPI YoY", forecast: "3.1%", gold_relevant: true, impact: "High", previous: "3.2%", time: today + 13 * 3600, timestamp: today + 13 * 3600 },
    { actual: "1.4%", currency: "USD", event: "Core CPI YoY", forecast: "3.1%", gold_relevant: true, impact: "High", previous: "3.3%", time: today + 13 * 3600, timestamp: today + 13 * 3600 },
    { actual: "51.2", currency: "USD", event: "Nonfarm Payrolls", forecast: "180K", gold_relevant: true, impact: "High", previous: "254K", time: today + 13 * 3600 + 4 * 3600, timestamp: today + 13 * 3600 + 4 * 3600 },
    { actual: "4.3%", currency: "USD", event: "Unemployment Rate", forecast: "4.2%", gold_relevant: true, impact: "High", previous: "4.1%", time: today + 13 * 3600 + 4 * 3600, timestamp: today + 13 * 3600 + 4 * 3600 },
    { actual: "52", currency: "USD", event: "ISM Manufacturing", forecast: "50.5", gold_relevant: false, impact: "Medium", previous: "49.8", time: today + 14 * 3600, timestamp: today + 14 * 3600 },
  ];
  return { count: events.length, events };
}

export function makeStatus(nowSec, candles, { version = "0.1.0-mock" } = {}) {
  const lastBar = candles.length ? candles[candles.length - 1].time : nowSec - 1800;
  return {
    version,
    ts: nowSec,
    feeds: {
      sifting: { state: "connected", msgs: 10432, last_msg: nowSec - 4 },
      mt5_feed: { state: "off", msgs: 0, last_msg: null },
    },
    candles: {
      interval: "15m",
      bars: 2000,
      first_bar: lastBar - 1999 * 900,
      last_bar: lastBar,
      edge_lag_minutes: 27,
      seed_bars: 0,
    },
    volume_profile: {
      audit: "ok",
      bin_size: 0.5,
      input: "15m",
      row_mode: "pt",
      rows: 40,
      va_pct: 33.2,
    },
  };
}

export function makeStrategy(nowSec, price, { seed = 4 } = {}) {
  const rand = mulberry32(seed);
  const action = rand() > 0.35 ? "buy" : rand() > 0.15 ? "sell" : "flat";
  const atr = +(2.8 + rand() * 1.4).toFixed(2);
  const lots = +(0.4 + rand() * 0.8).toFixed(2);
  const stopLoss = +(price - (action === "buy" ? 1 : -1) * (3 + rand() * 2)).toFixed(2);
  const takeProfit = +(price + (action === "buy" ? 1 : -1) * (6 + rand() * 4)).toFixed(2);
  const riskUsd = +(lots * 100 * (action === "buy" ? price - stopLoss : stopLoss - price)).toFixed(2);
  const reasons = [
    { name: "range_position", value: `${Math.round((price - 2640) * 10) / 10} pts from range low`, weight: 0.35, contribution: 0.18, detail: "Price relative to the active 15m range" },
    { name: "poc_proximity", value: `${Math.abs(price - 2660).toFixed(1)} from POC`, weight: 0.25, contribution: 0.11, detail: "Distance to the point of control" },
    { name: "volume_delta", value: rand() > 0.5 ? "buy skew" : "sell skew", weight: 0.2, contribution: 0.07, detail: "Up vs down tick volume over the window" },
    { name: "regime_filter", value: "regime ok", weight: 0.2, contribution: 0.05, detail: "Regime gate passed" },
  ];
  const decision = {
    ts: nowSec,
    symbol: "XAUUSD",
    action,
    score: Math.round(40 + rand() * 55),
    confidence: Math.round((50 + rand() * 45) * 10) / 10,
    price: +price.toFixed(2),
    atr,
    regime: "range",
    volatility: "normal",
    stop_loss: stopLoss,
    take_profit: takeProfit,
    reward_risk: +((Math.abs(takeProfit - price) / Math.abs(price - stopLoss))).toFixed(1),
    lots,
    risk_usd: riskUsd,
    reasons,
    notes: ["Mock decision for dashboard demonstration."],
    next_event_ts: nowSec + 1800,
  };
  return { symbol: "XAUUSD", enabled: true, params: { atr_period: 14, risk_per_trade_pct: 0.5 }, last: decision, history: [decision] };
}

export function makeDemo(nowSec, price, { seed = 5, position } = {}) {
  const rand = mulberry32(seed);
  const balance = 10000 + Math.round(rand() * 800);
  const unrealized = position ? (position.side === "buy" ? position.entry - price : price - position.entry) * position.lots * 100 : 0;
  return {
    enabled: true,
    mode: "demo",
    symbol: "XAUUSD",
    leverage: 1000,
    quote: {
      bid: +(price - 0.05).toFixed(2),
      ask: +(price + 0.05).toFixed(2),
      mid: +price.toFixed(2),
      spread: 0.1,
      ts: nowSec,
    },
    account: {
      currency: "USD",
      balance,
      equity: +(balance + unrealized).toFixed(2),
      margin: +(balance / 1000).toFixed(2),
      free_margin: +(balance - balance / 1000 + unrealized).toFixed(2),
      margin_level: 999.0,
      unrealized_pnl: +unrealized.toFixed(2),
      peak_equity: balance + 120,
      max_drawdown_pct: 3.2,
    },
    params: { start_balance: 10000, leverage: 1000, spread: 0.1, min_lots: 0.1, max_lots: 3, lot_step: 0.01, stop_out_pct: 50, cooldown_secs: 60 },
    position: position || null,
    stats: {
      closed_trades: 12,
      wins: 7,
      losses: 5,
      win_rate: 58.3,
      gross_profit: 940.5,
      gross_loss: -520.2,
      net_pnl: 420.3,
      profit_factor: 1.81,
      avg_r: 0.62,
      max_drawdown_pct: 4.1,
    },
    recent_trades: [
      { id: 41, side: "buy", lots: 0.8, entry: +(price - 4.2).toFixed(2), exit: +(price - 1.1).toFixed(2), opened_at: nowSec - 3600, closed_at: nowSec - 2400, pnl: 254.4, r_multiple: 1.4, exit_reason: "tp" },
      { id: 40, side: "sell", lots: 0.6, entry: +(price + 2.1).toFixed(2), exit: +(price + 0.4).toFixed(2), opened_at: nowSec - 7200, closed_at: nowSec - 6000, pnl: -112.8, r_multiple: -0.7, exit_reason: "sl" },
      { id: 39, side: "buy", lots: 0.5, entry: +(price - 9.8).toFixed(2), exit: +(price - 3.0).toFixed(2), opened_at: nowSec - 10800, closed_at: nowSec - 9000, pnl: 340.0, r_multiple: 1.9, exit_reason: "tp" },
    ],
    note: "Simulated demo engine (1:1000). Not connected to any broker or real-money account.",
    ts: nowSec,
  };
}

// ---- Node 4 mock resources --------------------------------------------------

export function makeMt5Account(nowSec, { seed = 6 } = {}) {
  return {
    configured: true,
    connected: true,
    authorized: true,
    account_type: "demo",
    login: 11001001 + Math.floor(seed % 1000),
    server: "MetaQuotes-Demo",
    company: "Demo Ltd",
    currency: "USD",
    balance: 10000,
    equity: 10024.5,
    margin: 12.4,
    margin_free: 10012.1,
    leverage: "1:500",
    trade_allowed: true,
    halted: false,
    halt_reason: null,
    requested_symbol: "XAUUSD",
    broker_symbol: "GOLD",
    symbol_digits: 2,
    symbol_volume_min: 0.01,
    symbol_volume_max: 50,
    symbol_volume_step: 0.01,
    symbol_contract_size: 100,
    terminal_build: "5050",
    ea_version: "node1-demo-ea-0.1",
    bridge_version: "0.1.0",
    latency_ms: 8,
    last_heartbeat: nowSec - 2,
    last_updated: nowSec,
    error: null,
    setup_hint: null,
  };
}

export function makeMt5Positions(nowSec) {
  return {
    positions: [
      { ticket: 77001, symbol: "GOLD", side: "buy", volume: 0.5, price_open: 2651.2, sl: 2644.0, tp: 2664.0, profit: 115.0, swap: 0, comment: "NODE1", magic: 2024001, time_ms: nowSec - 1800, current_price: 2653.5, unrealized_pnl: 115.0 },
    ],
    count: 1,
    total_volume: 0.5,
    total_unrealized_pnl: 115.0,
    halted: false,
    halt_reason: null,
    account_login: 11001001,
    account_type: "demo",
    source: "mt5_history",
    timestamp: nowSec,
  };
}

export function makeMt5History(nowSec) {
  return {
    deals: [
      { ticket: 76990, order_ticket: 76989, position_ticket: 0, symbol: "GOLD", side: "buy", volume: 0.4, price: 2648.1, profit: 160.2, swap: 0, commission: -1.2, comment: "NODE1", magic: 2024001, time_ms: nowSec - 7200, entry: "in", reason: "OPEN" },
      { ticket: 76985, order_ticket: 0, position_ticket: 76980, symbol: "GOLD", side: "sell", volume: 0.3, price: 2645.0, profit: -90.5, swap: 0, commission: -0.9, comment: "NODE1", magic: 2024001, time_ms: nowSec - 14400, entry: "out", reason: "CLOSE" },
    ],
    count: 2,
    cursor: null,
    complete: true,
    total_realized_pnl: 69.7,
    first_ms: nowSec - 14400,
    last_ms: nowSec - 7200,
    source: "mt5_history",
    timestamp: nowSec,
  };
}

export function makeMt5Status(nowSec, { seed = 7 } = {}) {
  return {
    configured: true,
    connected: true,
    authorized: true,
    protocol: "json_v2",
    bridge_version: "0.1.0",
    node4_url: "https://execution.node4.example/ws",
    ea_connected: true,
    ea_write_enabled: false,
    ea_mode: "simulate",
    ea_login: 11001001,
    ea_server: "MetaQuotes-Demo",
    ea_last_heartbeat: nowSec - 2,
    halted: false,
    halt_reason: null,
    trading_enabled: false,
    orders_sent: 12,
    orders_filled: 10,
    orders_rejected: 1,
    orders_unknown: 1,
    positions_open: 1,
    history_deals: 37,
    last_error: null,
    uptime_secs: 86400 + Math.floor(seed * 137),
    timestamp: nowSec,
  };
}

export function makeDerivAccount(nowSec) {
  return {
    connected: true,
    trading_disabled: true,
    can_trade: false,
    balance: 5000,
    margin: 0,
    pnl: 0,
    currency: "USD",
    leverage: 1000,
    timestamp: nowSec,
  };
}

export function makeDiagnostics(nowSec) {
  return {
    timestamp: nowSec,
    node3: {
      degraded: false,
      last_heartbeat: nowSec - 1,
      latency_ms: 12,
      attempts: 990,
      errors: [],
    },
    work: {
      open_trades: 2,
      positions: 1,
      fills_attempted: 12,
      fills_confirmed: 10,
      fills_rejected: 1,
      fills_unknown: 1,
      cancels_sent: 1,
      cancels_accepted: 1,
    },
    ledger: {
      version: 1,
      entries: [
        { seq: 1, kind: "fill", status: "filled", intent_id: "int-0001", broker_symbol: "GOLD", side: "buy", requested_volume: 0.5, filled_volume: 0.5, ts: nowSec - 3600 },
        { seq: 2, kind: "reject", status: "rejected", intent_id: "int-0002", broker_symbol: "GOLD", side: "sell", requested_volume: 0.4, filled_volume: 0, ts: nowSec - 7200 },
      ],
    },
  };
}

export function makeOpenTrades(nowSec) {
  return [
    {
      id: "550e8400-e29b-41d4-a716-446655440001",
      lifecycle: "filled",
      intent_id: "int-0011",
      symbol: "XAUUSD",
      side: "buy",
      lots: 0.5,
      entry: 2651.2,
      mark: 2653.5,
      unrealized: 115.0,
      take_profit: 2664.0,
      stop_loss: 2644.0,
      exit_price: null,
      filled_volume: 0.5,
      opened_at: nowSec - 1800,
      mt5: { matched: true, ticket: 77001, symbol: "GOLD", status: "filled", filled_volume: 0.5, reason: null },
      note: null,
    },
    {
      id: "550e8400-e29b-41d4-a716-446655440002",
      lifecycle: "accepted",
      intent_id: "int-0012",
      symbol: "XAUUSD",
      side: "sell",
      lots: 0.3,
      entry: null,
      mark: 2653.5,
      unrealized: null,
      take_profit: null,
      stop_loss: null,
      exit_price: null,
      filled_volume: 0,
      opened_at: nowSec - 60,
      mt5: null,
      note: "Intent accepted by Node 4 — broker result pending, NOT a confirmed fill.",
    },
    {
      id: "550e8400-e29b-41d4-a716-446655440003",
      lifecycle: "unknown",
      intent_id: "int-0013",
      symbol: "XAUUSD",
      side: "buy",
      lots: 0.2,
      entry: null,
      mark: 2653.5,
      unrealized: null,
      take_profit: null,
      stop_loss: null,
      exit_price: null,
      filled_volume: 0,
      opened_at: nowSec - 120,
      mt5: null,
      note: "Outcome never arrived from the broker. Reconciled, never resent.",
    },
  ];
}

export function makeBridgeEvent(nowSec, kind = "order_filled") {
  if (kind === "order_filled") {
    return {
      type: "order_filled",
      ts: nowSec,
      outcome: {
        status: "filled",
        idempotency_key: "idem-" + Math.floor(nowSec),
        intent_id: "int-0011",
        requested_symbol: "XAUUSD",
        broker_symbol: "GOLD",
        side: "buy",
        requested_volume: 0.5,
        filled_volume: 0.5,
        price: 2651.2,
        sl: 2644.0,
        tp: 2664.0,
        order_ticket: 77000,
        deal_ticket: 77001,
        position_ticket: 77001,
        retcode: 10009,
        retcode_desc: "ORDER_TYPE_BUY_LIMIT",
        risk_amount: 250,
        risk_currency: "USD",
        notional: 132560,
        reconciled: true,
        latency_ms: 120,
        ts: nowSec - 1800,
        error: null,
      },
    };
  }
  if (kind === "order_unknown") {
    return {
      type: "order_unknown",
      ts: nowSec,
      outcome: {
        status: "unknown",
        idempotency_key: "idem-" + Math.floor(nowSec),
        intent_id: "int-0013",
        requested_symbol: "XAUUSD",
        broker_symbol: "GOLD",
        side: "buy",
        requested_volume: 0.2,
        filled_volume: 0,
        price: null,
        sl: null,
        tp: null,
        order_ticket: null,
        deal_ticket: null,
        position_ticket: null,
        retcode: null,
        retcode_desc: null,
        risk_amount: 100,
        risk_currency: "USD",
        notional: null,
        reconciled: true,
        latency_ms: null,
        ts: nowSec - 120,
        error: "no fill or reject received from broker",
      },
    };
  }
  return { type: "halted", ts: nowSec, halted: true, reason: "risk", auto: true, flattened: "1/1" };
}

// ---- WS sessions ------------------------------------------------------------

function send(ws, obj) {
  if (ws.readyState === 1) ws.send(JSON.stringify(obj));
}

// Node 1 mock session: replays the full snapshot set on subscribe, then
// streams updates + heartbeats.
export function mockNode1Session(ws, { intervalSec = 900 } = {}) {
  const timers = [];
  let running = true;
  const price = { v: BASE_PRICE };

  function now() {
    return Math.floor(Date.now() / 1000);
  }

  function snapshot() {
    const t = now();
    return {
      candles: makeCandles(t, { count: 15, intervalSec, lagBars: 2, seed: Math.floor(t / 30) }),
      levels: makeLevels(t, price.v, { seed: Math.floor(t / 60) }),
      tick_volume: makeTickBars(t, { count: 8, seed: Math.floor(t / 45) }),
      calendar: makeCalendar(t),
      strategy: makeStrategy(t, price.v, { seed: Math.floor(t / 60) }),
      demo: makeDemo(t, price.v, { seed: Math.floor(t / 30) }),
      status: makeStatus(t, makeCandles(t, { count: 15, intervalSec })),
    };
  }

  function replay() {
    const snap = snapshot();
    // Same replay order the real engine uses.
    send(ws, { type: "levels", frames: snap.levels });
    send(ws, { type: "candle", frames: snap.candles.slice(-15) });
    send(ws, { type: "tick_volume", frames: snap.tick_volume });
    send(ws, { type: "calendar", frames: snap.calendar });
    send(ws, { type: "strategy", frames: [snap.strategy.last] });
    send(ws, { type: "demo", frames: [snap.demo] });
    send(ws, { type: "status", status: snap.status });
  }

  ws.on("message", (data) => {
    let msg;
    try {
      msg = JSON.parse(data.toString());
    } catch (err) {
      return;
    }
    if (msg.type === "subscribe") replay();
  });

  // Stream updates.
  timers.push(setInterval(() => {
    if (!running) return;
    const t = now();
    price.v = +(price.v + (Math.random() - 0.5) * 2.4).toFixed(2);
    const candles = makeCandles(t, { count: 3, intervalSec, lagBars: 2, seed: Math.floor(t / 30) });
    send(ws, { type: "candle", frames: candles });
    send(ws, { type: "levels", frames: makeLevels(t, price.v, { seed: Math.floor(t / 60) }) });
    send(ws, { type: "tick_volume", frames: makeTickBars(t, { count: 2, seed: Math.floor(t / 45) }) });
  }, 15000));

  timers.push(setInterval(() => {
    if (!running) return;
    const t = now();
    send(ws, { type: "strategy", frames: [makeStrategy(t, price.v, { seed: Math.floor(t / 60) }).last] });
    send(ws, { type: "demo", frames: [makeDemo(t, price.v, { seed: Math.floor(t / 30) })] });
    send(ws, { type: "status", status: makeStatus(t, makeCandles(t, { count: 15, intervalSec })) });
  }, 30000));

  timers.push(setInterval(() => {
    if (!running) return;
    send(ws, { type: "heartbeat", ts: now() });
  }, 30000));

  ws.on("close", () => {
    stop();
  });

  function stop() {
    running = false;
    for (const t of timers) clearInterval(t);
    timers.length = 0;
  }
  return { stop };
}

// Node 4 mock session: 7 initial frames on subscribe, then refreshes.
export function mockNode4Session(ws) {
  const timers = [];
  let running = true;

  function now() {
    return Math.floor(Date.now() / 1000);
  }

  function replay() {
    const t = now();
    send(ws, { topic: "diagnostics", diagnostics: makeDiagnostics(t) });
    send(ws, { topic: "open_trades", open_trades: makeOpenTrades(t) });
    send(ws, { topic: "deriv_account", deriv_account: makeDerivAccount(t) });
    send(ws, { topic: "mt5_account", mt5_account: makeMt5Account(t) });
    send(ws, { topic: "mt5_positions", mt5_positions: makeMt5Positions(t) });
    send(ws, { topic: "mt5_history", mt5_history: makeMt5History(t) });
    send(ws, { topic: "bridge_status", mt5_status: makeMt5Status(t) });
  }

  ws.on("message", (data) => {
    let msg;
    try {
      msg = JSON.parse(data.toString());
    } catch (err) {
      return;
    }
    if (msg.type === "subscribe") replay();
  });

  timers.push(setInterval(() => {
    if (!running) return;
    const t = now();
    send(ws, { topic: "diagnostics", diagnostics: makeDiagnostics(t) });
    send(ws, { topic: "bridge_status", mt5_status: makeMt5Status(t) });
  }, 5000));

  timers.push(setInterval(() => {
    if (!running) return;
    const t = now();
    send(ws, { topic: "bridge_event", bridge_event: makeBridgeEvent(t, "order_filled") });
  }, 45000));

  timers.push(setInterval(() => {
    if (!running) return;
    send(ws, { topic: "heartbeat", ts: now() });
  }, 25000));

  ws.on("close", () => stop());

  function stop() {
    running = false;
    for (const t of timers) clearInterval(t);
    timers.length = 0;
  }
  return { stop };
}
