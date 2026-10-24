// parse.js — tolerant normalization of every external payload.
//
// Contract source: Node 1 FRONTEND_NEXT.md (market data) and Node 4
// FRONTEND_RESOURCES.md (execution). The published shapes are the ONLY
// shapes this module understands; anything else is reported and dropped.
// Normalization never throws on shape problems: missing numeric fields become
// null (rendered as "—"), unknown status strings fall back to "unknown", and
// malformed arrays are filtered. A panel must degrade, not crash.

import { lifecycleBadge } from "./lifecycle.js";
import { intervalMinutes } from "./freshness.js";

export function num(value) {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim() !== "") {
    const n = Number(value);
    if (Number.isFinite(n)) return n;
  }
  return null;
}

function objects(value) {
  if (!Array.isArray(value)) return [];
  return value.filter((v) => v && typeof v === "object");
}

// ---- Node 1: candles ------------------------------------------------------
// [{ time, open, high, low, close, volume, source }]  (source: "sifting")
export function parseCandles(json) {
  if (!json || typeof json !== "object") return { bars: [], error: "not an object" };
  const bars = objects(json).map((c) => ({
    time: num(c.time),
    open: num(c.open),
    high: num(c.high),
    low: num(c.low),
    close: num(c.close),
    volume: num(c.volume),
    source: typeof c.source === "string" ? c.source : null,
  }))
  .filter((b) => b.time != null && b.open != null && b.close != null);
  return { bars, error: bars.length === 0 && !Array.isArray(json) ? "no usable bars" : null };
}

// ---- Node 1: levels (VP clusters) ----------------------------------------
// [{ window, poc, vah, val, start, end, timestamp, direction, swing_high,
//    swing_low, sunday_open, meta }]
export function parseLevels(json) {
  if (!json || typeof json !== "object") return { levels: [], error: "not an object" };
  const levels = objects(json).map((l) => ({
    window: typeof l.window === "string" ? l.window : null,
    poc: num(l.poc),
    vah: num(l.vah),
    val: num(l.val),
    start: num(l.start),
    end: num(l.end),
    timestamp: num(l.timestamp),
    direction: typeof l.direction === "string" ? l.direction : null,
    swingHigh: num(l.swing_high),
    swingLow: num(l.swing_low),
    sundayOpen: num(l.sunday_open),
  }));
  return { levels, error: null };
}

// ---- Node 1: tick volume --------------------------------------------------
// [{ time, ticks, up_ticks, down_ticks, flat_ticks, close, last_tick,
//    ticks_per_sec, closed, source }]
export function parseTickBars(json) {
  if (!json || typeof json !== "object") return { bars: [], error: "not an object" };
  const bars = objects(json).map((t) => ({
    time: num(t.time),
    ticks: num(t.ticks),
    upTicks: num(t.up_ticks),
    downTicks: num(t.down_ticks),
    flatTicks: num(t.flat_ticks),
    close: num(t.close),
    lastTick: num(t.last_tick),
    ticksPerSec: num(t.ticks_per_sec),
    closed: t.closed === true,
    source: typeof t.source === "string" ? t.source : null,
  })).filter((b) => b.time != null);
  return { bars, error: null };
}

// ---- Node 1: economic calendar -------------------------------------------
// { count, events: [{ actual, currency, event, forecast, gold_relevant,
//    impact, previous, time, timestamp }] }
export function parseCalendar(json) {
  if (!json || typeof json !== "object") return { count: 0, events: [] };
  const events = objects(json.events).map((e) => ({
    actual: e.actual == null ? null : String(e.actual),
    currency: typeof e.currency === "string" ? e.currency : null,
    event: typeof e.event === "string" ? e.event : null,
    forecast: e.forecast == null ? null : String(e.forecast),
    goldRelevant: e.gold_relevant === true,
    impact: typeof e.impact === "string" ? e.impact : null,
    previous: e.previous == null ? null : String(e.previous),
    time: num(e.time),
    timestamp: num(e.timestamp),
  }));
  return { count: num(json.count) ?? events.length, events };
}

// ---- Node 1: engine status ------------------------------------------------
// { version, ts, feeds: { name: { state, msgs, last_msg } }, candles: {
//   interval, bars, first_bar, last_bar, edge_lag_minutes, seed_bars },
//   volume_profile }
export function parseStatus(json) {
  if (!json || typeof json !== "object") return { ok: false };
  const candles = json.candles && typeof json.candles === "object" ? json.candles : {};
  const feeds = {};
  if (json.feeds && typeof json.feeds === "object") {
    for (const [name, f] of Object.entries(json.feeds)) {
      if (f && typeof f === "object") {
        feeds[name] = {
          state: typeof f.state === "string" ? f.state : null,
          msgs: num(f.msgs),
          lastMsg: num(f.last_msg),
        };
      }
    }
  }
  const intervalMin = intervalMinutes(candles.interval);
  return {
    ok: true,
    version: json.version == null ? null : String(json.version),
    ts: num(json.ts),
    feeds,
    candles: {
      interval: typeof candles.interval === "string" ? candles.interval : null,
      intervalMin,
      bars: num(candles.bars),
      firstBar: num(candles.first_bar),
      lastBar: num(candles.last_bar),
      edgeLagMinutes: num(candles.edge_lag_minutes),
      seedBars: num(candles.seed_bars),
    },
    volumeProfile: json.volume_profile && typeof json.volume_profile === "object"
      ? {
          input: json.volume_profile.input ?? null,
          rowMode: json.volume_profile.row_mode ?? null,
          rows: num(json.volume_profile.rows),
          vaPct: num(json.volume_profile.va_pct),
        }
      : null,
  };
}

// ---- Node 1: strategy snapshot -------------------------------------------
// { symbol, enabled, params, last, history: [Decision] }
// Decision: { ts, symbol, action, score, confidence, price, atr, regime,
//   volatility, stop_loss, take_profit, reward_risk, lots, risk_usd,
//   reasons: [{ name, value, weight, contribution, detail }], notes,
//   next_event_ts }
export function parseDecision(json) {
  if (!json || typeof json !== "object") return null;
  return {
    ts: num(json.ts),
    symbol: typeof json.symbol === "string" ? json.symbol : null,
    action: ["buy", "sell", "flat"].includes(json.action) ? json.action : "unknown",
    score: num(json.score),
    confidence: num(json.confidence),
    price: num(json.price),
    atr: num(json.atr),
    regime: json.regime == null ? null : String(json.regime),
    volatility: json.volatility == null ? null : String(json.volatility),
    stopLoss: num(json.stop_loss),
    takeProfit: num(json.take_profit),
    rewardRisk: num(json.reward_risk),
    lots: num(json.lots),
    riskUsd: num(json.risk_usd),
    reasons: objects(json.reasons).map((r) => ({
      name: typeof r.name === "string" ? r.name : null,
      value: r.value == null ? null : String(r.value),
      weight: num(r.weight),
      contribution: num(r.contribution),
      detail: r.detail == null ? null : String(r.detail),
    })),
    notes: Array.isArray(json.notes)
      ? json.notes.map((n) => (n == null ? "" : String(n))).filter(Boolean)
      : [],
    nextEventTs: num(json.next_event_ts),
  };
}

export function parseStrategy(json) {
  if (!json || typeof json !== "object") return { ok: false };
  return {
    ok: true,
    symbol: typeof json.symbol === "string" ? json.symbol : null,
    enabled: json.enabled === true,
    params: json.params && typeof json.params === "object" ? json.params : {},
    last: parseDecision(json.last),
    history: objects(json.history).map(parseDecision).filter(Boolean),
  };
}

// ---- Node 1: demo account snapshot ---------------------------------------
// { enabled, mode, symbol, leverage, quote{bid,ask,mid,spread,ts}, account{
//   currency, balance, equity, margin, free_margin, margin_level,
//   unrealized_pnl, peak_equity, max_drawdown_pct }, params{...},
//   position|null, stats, recent_trades, note, ts }
export function parseDemo(json) {
  if (!json || typeof json !== "object") return { ok: false };
  const account = json.account && typeof json.account === "object" ? json.account : {};
  const quote = json.quote && typeof json.quote === "object" ? json.quote : {};
  const position = json.position && typeof json.position === "object" ? json.position : null;
  const recentTrades = objects(json.recent_trades).map((t) => ({
    id: t.id == null ? null : String(t.id),
    side: t.side === "buy" || t.side === "sell" ? t.side : null,
    lots: num(t.lots),
    entry: num(t.entry),
    exit: num(t.exit),
    openedAt: num(t.opened_at),
    closedAt: num(t.closed_at),
    pnl: num(t.pnl),
    rMultiple: num(t.r_multiple),
    exitReason: t.exit_reason == null ? null : String(t.exit_reason),
  }));
  return {
    ok: true,
    enabled: json.enabled === true,
    mode: typeof json.mode === "string" ? json.mode : null,
    symbol: typeof json.symbol === "string" ? json.symbol : null,
    leverage: num(json.leverage),
    note: typeof json.note === "string" ? json.note : null,
    ts: num(json.ts),
    quote: {
      bid: num(quote.bid),
      ask: num(quote.ask),
      mid: num(quote.mid),
      spread: num(quote.spread),
      ts: num(quote.ts),
    },
    account: {
      currency: typeof account.currency === "string" ? account.currency : null,
      balance: num(account.balance),
      equity: num(account.equity),
      margin: num(account.margin),
      freeMargin: num(account.free_margin),
      marginLevel: num(account.margin_level),
      unrealizedPnl: num(account.unrealized_pnl),
      peakEquity: num(account.peak_equity),
      maxDrawdownPct: num(account.max_drawdown_pct),
    },
    params: json.params && typeof json.params === "object" ? json.params : {},
    position: position
      ? {
          id: position.id == null ? null : String(position.id),
          side: position.side === "buy" || position.side === "sell" ? position.side : null,
          lots: num(position.lots),
          entry: num(position.entry),
          stopLoss: num(position.stop_loss),
          takeProfit: num(position.take_profit),
          openedAt: num(position.opened_at),
          riskUsd: num(position.risk_usd),
          margin: num(position.margin),
          unrealizedPnl: num(position.unrealized_pnl),
          reason: position.reason == null ? null : String(position.reason),
        }
      : null,
    stats: json.stats && typeof json.stats === "object" ? {
      closedTrades: num(json.stats.closed_trades),
      wins: num(json.stats.wins),
      losses: num(json.stats.losses),
      winRate: num(json.stats.win_rate),
      grossProfit: num(json.stats.gross_profit),
      grossLoss: num(json.stats.gross_loss),
      netPnl: num(json.stats.net_pnl),
      profitFactor: num(json.stats.profit_factor),
      avgR: num(json.stats.avg_r),
      maxDrawdownPct: num(json.stats.max_drawdown_pct),
    } : null,
    recentTrades,
  };
}

// ---- Node 4: execution diagnostics ----------------------------------------
// { timestamp, node3: {degraded, last_heartbeat, latency_ms, attempts,
//   errors[]}, work: {open_trades, positions, fills_attempted, fills_confirmed,
//   fills_rejected, fills_unknown, cancels_sent, cancels_accepted}, ledger}
export function parseDiagnostics(json) {
  if (!json || typeof json !== "object") return { ok: false };
  const node3 = json.node3 && typeof json.node3 === "object" ? json.node3 : {};
  const work = json.work && typeof json.work === "object" ? json.work : {};
  return {
    ok: true,
    timestamp: num(json.timestamp),
    node3: {
      degraded: node3.degraded === true,
      lastHeartbeat: num(node3.last_heartbeat),
      latencyMs: num(node3.latency_ms),
      attempts: num(node3.attempts),
      errors: Array.isArray(node3.errors) ? node3.errors.map((e) => String(e)) : [],
    },
    work: {
      openTrades: num(work.open_trades),
      positions: num(work.positions),
      fillsAttempted: num(work.fills_attempted),
      fillsConfirmed: num(work.fills_confirmed),
      fillsRejected: num(work.fills_rejected),
      fillsUnknown: num(work.fills_unknown),
      cancelsSent: num(work.cancels_sent),
      cancelsAccepted: num(work.cancels_accepted),
    },
    ledger: json.ledger && typeof json.ledger === "object" ? json.ledger : null,
  };
}

// ---- Node 4: open trades (deriv) -------------------------------------------
// [{ id, lifecycle, intent_id, symbol, side, lots, entry, mark, unrealized,
//   take_profit, stop_loss, exit_price, filled_volume, opened_at, mt5{...},
//   note }]
export function parseOpenTrades(json) {
  if (!Array.isArray(json)) return { trades: [] };
  const trades = objects(json).map((t) => ({
    id: t.id == null ? null : String(t.id),
    lifecycle: lifecycleBadge(t.lifecycle).status, // unknown -> badge fallback
    intentId: t.intent_id == null ? null : String(t.intent_id),
    symbol: typeof t.symbol === "string" ? t.symbol : null,
    side: t.side === "buy" || t.side === "sell" ? t.side : null,
    lots: num(t.lots),
    entry: num(t.entry),
    mark: num(t.mark),
    unrealized: num(t.unrealized),
    takeProfit: num(t.take_profit),
    stopLoss: num(t.stop_loss),
    exitPrice: num(t.exit_price),
    filledVolume: num(t.filled_volume),
    openedAt: num(t.opened_at),
    mt5: t.mt5 && typeof t.mt5 === "object"
      ? {
          matched: t.mt5.matched === true,
          ticket: num(t.mt5.ticket),
          symbol: typeof t.mt5.symbol === "string" ? t.mt5.symbol : null,
          status: t.mt5.status == null ? null : String(t.mt5.status),
          filledVolume: num(t.mt5.filled_volume),
          reason: t.mt5.reason == null ? null : String(t.mt5.reason),
        }
      : null,
    note: t.note == null ? null : String(t.note),
  }));
  return { trades };
}

// ---- Node 4: MT5 account snapshot -----------------------------------------
export function parseMt5Account(json) {
  if (!json || typeof json !== "object") return { ok: false };
  return {
    ok: true,
    configured: json.configured === true,
    connected: json.connected === true,
    authorized: json.authorized === true,
    accountType: typeof json.account_type === "string" ? json.account_type : null,
    login: json.login == null ? null : String(json.login),
    server: typeof json.server === "string" ? json.server : null,
    company: json.company == null ? null : String(json.company),
    currency: typeof json.currency === "string" ? json.currency : null,
    balance: num(json.balance),
    equity: num(json.equity),
    margin: num(json.margin),
    marginFree: num(json.margin_free),
    leverage: json.leverage == null ? null : String(json.leverage),
    tradeAllowed: json.trade_allowed === true,
    halted: json.halted === true,
    haltReason: json.halt_reason == null ? null : String(json.halt_reason),
    requestedSymbol: json.requested_symbol == null ? null : String(json.requested_symbol),
    brokerSymbol: json.broker_symbol == null ? null : String(json.broker_symbol),
    symbolDigits: num(json.symbol_digits),
    symbolVolumeMin: num(json.symbol_volume_min),
    symbolVolumeMax: num(json.symbol_volume_max),
    symbolVolumeStep: num(json.symbol_volume_step),
    symbolContractSize: num(json.symbol_contract_size),
    terminalBuild: json.terminal_build == null ? null : String(json.terminal_build),
    eaVersion: json.ea_version == null ? null : String(json.ea_version),
    bridgeVersion: json.bridge_version == null ? null : String(json.bridge_version),
    latencyMs: num(json.latency_ms),
    lastHeartbeat: num(json.last_heartbeat),
    lastUpdated: num(json.last_updated),
    error: json.error == null ? null : String(json.error),
    setupHint: json.setup_hint == null ? null : String(json.setup_hint),
  };
}

// ---- Node 4: MT5 positions -----------------------------------------------
export function parseMt5Positions(json) {
  if (!json || typeof json !== "object") return { ok: false };
  const positions = objects(json.positions).map((p) => ({
    ticket: num(p.ticket),
    symbol: typeof p.symbol === "string" ? p.symbol : null,
    side: p.side === "buy" || p.side === "sell" ? p.side : null,
    volume: num(p.volume),
    priceOpen: num(p.price_open),
    sl: num(p.sl),
    tp: num(p.tp),
    profit: num(p.profit),
    swap: num(p.swap),
    comment: p.comment == null ? null : String(p.comment),
    magic: p.magic == null ? null : String(p.magic),
    timeMs: num(p.time_ms),
    currentPrice: num(p.current_price),
    unrealizedPnl: num(p.unrealized_pnl),
  }));
  return {
    ok: true,
    positions,
    count: num(json.count) ?? positions.length,
    totalVolume: num(json.total_volume),
    totalUnrealizedPnl: num(json.total_unrealized_pnl),
    halted: json.halted === true,
    haltReason: json.halt_reason == null ? null : String(json.halt_reason),
    accountLogin: json.account_login == null ? null : String(json.account_login),
    accountType: typeof json.account_type === "string" ? json.account_type : null,
    source: typeof json.source === "string" ? json.source : null,
    timestamp: num(json.timestamp),
  };
}

// ---- Node 4: MT5 history (deals) ------------------------------------------
export function parseMt5History(json) {
  if (!json || typeof json !== "object") return { ok: false };
  const deals = objects(json.deals).map((d) => ({
    ticket: num(d.ticket),
    orderTicket: num(d.order_ticket),
    positionTicket: num(d.position_ticket),
    symbol: typeof d.symbol === "string" ? d.symbol : null,
    side: d.side === "buy" || d.side === "sell" ? d.side : null,
    volume: num(d.volume),
    price: num(d.price),
    profit: num(d.profit),
    swap: num(d.swap),
    commission: num(d.commission),
    comment: d.comment == null ? null : String(d.comment),
    magic: d.magic == null ? null : String(d.magic),
    timeMs: num(d.time_ms),
    entry: d.entry == null ? null : String(d.entry),
    reason: d.reason == null ? null : String(d.reason),
  }));
  return {
    ok: true,
    deals,
    count: num(json.count) ?? deals.length,
    cursor: json.cursor == null ? null : String(json.cursor),
    complete: json.complete === true,
    totalRealizedPnl: num(json.total_realized_pnl),
    firstMs: num(json.first_ms),
    lastMs: num(json.last_ms),
    source: typeof json.source === "string" ? json.source : null,
    timestamp: num(json.timestamp),
  };
}

// ---- Node 4: MT5 bridge status ---------------------------------------------
export function parseMt5Status(json) {
  if (!json || typeof json !== "object") return { ok: false };
  return {
    ok: true,
    configured: json.configured === true,
    connected: json.connected === true,
    authorized: json.authorized === true,
    protocol: json.protocol == null ? null : String(json.protocol),
    bridgeVersion: json.bridge_version == null ? null : String(json.bridge_version),
    node4Url: json.node4_url == null ? null : String(json.node4_url),
    eaConnected: json.ea_connected === true,
    eaWriteEnabled: json.ea_write_enabled === true,
    eaMode: json.ea_mode == null ? null : String(json.ea_mode),
    eaLogin: json.ea_login == null ? null : String(json.ea_login),
    eaServer: json.ea_server == null ? null : String(json.ea_server),
    eaLastHeartbeat: num(json.ea_last_heartbeat),
    halted: json.halted === true,
    haltReason: json.halt_reason == null ? null : String(json.halt_reason),
    tradingEnabled: json.trading_enabled === true,
    ordersSent: num(json.orders_sent),
    ordersFilled: num(json.orders_filled),
    ordersRejected: num(json.orders_rejected),
    ordersUnknown: num(json.orders_unknown),
    positionsOpen: num(json.positions_open),
    historyDeals: num(json.history_deals),
    lastError: json.last_error == null ? null : String(json.last_error),
    uptimeSecs: num(json.uptime_secs),
    timestamp: num(json.timestamp),
  };
}

// ---- Node 4: bridge event (WS frame) --------------------------------------
// { type: "order_filled" | "order_rejected" | "order_unknown" | "halted" |
//   "resumed" | "ea_link_disconnected" | "trade_transaction", ts, ... }
export function parseBridgeEvent(json) {
  if (!json || typeof json !== "object") return null;
  const type = typeof json.type === "string" ? json.type : "unknown";
  if (type === "order_filled" || type === "order_rejected" || type === "order_unknown") {
    const outcome = json.outcome && typeof json.outcome === "object" ? json.outcome : {};
    return {
      type,
      ts: num(json.ts),
      outcome: {
        status: lifecycleBadge(outcome.status).status,
        idempotencyKey: outcome.idempotency_key == null ? null : String(outcome.idempotency_key),
        intentId: outcome.intent_id == null ? null : String(outcome.intent_id),
        requestedSymbol: outcome.requested_symbol == null ? null : String(outcome.requested_symbol),
        brokerSymbol: outcome.broker_symbol == null ? null : String(outcome.broker_symbol),
        side: outcome.side === "buy" || outcome.side === "sell" ? outcome.side : null,
        requestedVolume: num(outcome.requested_volume),
        filledVolume: num(outcome.filled_volume),
        price: num(outcome.price),
        sl: num(outcome.sl),
        tp: num(outcome.tp),
        orderTicket: num(outcome.order_ticket),
        dealTicket: num(outcome.deal_ticket),
        positionTicket: num(outcome.position_ticket),
        retcode: num(outcome.retcode),
        retcodeDesc: outcome.retcode_desc == null ? null : String(outcome.retcode_desc),
        riskAmount: num(outcome.risk_amount),
        riskCurrency: outcome.risk_currency == null ? null : String(outcome.risk_currency),
        notional: num(outcome.notional),
        reconciled: outcome.reconciled === true,
        latencyMs: num(outcome.latency_ms),
        ts: num(outcome.ts),
        error: outcome.error == null ? null : String(outcome.error),
      },
    };
  }
  if (type === "halted") {
    return {
      type,
      ts: num(json.ts),
      halted: json.halted === true,
      reason: json.reason == null ? null : String(json.reason),
      auto: json.auto == null ? null : String(json.auto),
      flattened: json.flattened == null ? null : String(json.flattened),
    };
  }
  if (type === "resumed") {
    // A resume means the venue is no longer halted; preserve the boolean
    // (only an explicit true means halted).
    return { type, ts: num(json.ts), halted: json.halted === true };
  }
  if (type === "ea_link_disconnected") {
    return { type, ts: num(json.ts), reason: json.reason == null ? null : String(json.reason) };
  }
  if (type === "trade_transaction") {
    return { type, ts: num(json.ts) };
  }
  return { type, ts: num(json.ts) };
}

// ---- Node 4: full execution snapshot ---------------------------------------
// { timestamp, node3, work, open_trades, deriv_account, mt5{account,
//   positions, history, status}, recent_events }
export function parseExecutionSnapshot(json) {
  if (!json || typeof json !== "object") return { ok: false };
  const mt5 = json.mt5 && typeof json.mt5 === "object" ? json.mt5 : {};
  const events = objects(json.recent_events).map(parseBridgeEvent).filter(Boolean);
  return {
    ok: true,
    timestamp: num(json.timestamp),
    diagnostics: parseDiagnostics({
      timestamp: json.timestamp,
      node3: json.node3,
      work: json.work,
    }),
    openTrades: parseOpenTrades(json.open_trades),
    derivAccount: json.deriv_account && typeof json.deriv_account === "object"
      ? {
          connected: json.deriv_account.connected === true,
          tradingDisabled: json.deriv_account.trading_disabled === true,
          canTrade: json.deriv_account.can_trade === true,
          balance: num(json.deriv_account.balance),
          margin: num(json.deriv_account.margin),
          pnl: num(json.deriv_account.pnl),
          currency: typeof json.deriv_account.currency === "string"
            ? json.deriv_account.currency : null,
          leverage: num(json.deriv_account.leverage),
          timestamp: num(json.deriv_account.timestamp),
        }
      : null,
    mt5: {
      account: parseMt5Account(mt5.account),
      positions: parseMt5Positions(mt5.positions),
      history: parseMt5History(mt5.history),
      status: parseMt5Status(mt5.status),
    },
    recentEvents: events,
  };
}
