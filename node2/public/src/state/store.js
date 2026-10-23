// store.js — central application state and frame reducers.
//
// TS-keyed upsert semantics (Node 1 contract): candles and strategy history
// are keyed by their timestamp, so replayed frames on every reconnect do not
// duplicate rows. Caps keep memory bounded; the oldest entries drop out.

import {
  parseCandles,
  parseLevels,
  parseTickBars,
  parseCalendar,
  parseStatus,
  parseStrategy,
  parseDecision,
  parseDemo,
  parseDiagnostics,
  parseOpenTrades,
  parseMt5Account,
  parseMt5Positions,
  parseMt5History,
  parseMt5Status,
  parseBridgeEvent,
  parseExecutionSnapshot,
} from "../lib/parse.js";

const MAX_CANDLES = 2000; // Node 1 serves up to 2000 bars
const MAX_HISTORY = 100; // strategy decisions kept for the history strip
const MAX_EVENTS = 200; // Node 4 bridge events kept in the log
const MAX_TRADES = 100;

function emptyMarket() {
  return {
    candles: new Map(),
    candlesOrder: [], // insertion-ordered times for charting
    levels: [],
    tickBars: new Map(),
    tickBarsOrder: [],
    calendar: [],
    status: null,
  };
}

function emptyNode4() {
  return {
    diagnostics: null,
    openTrades: [],
    mt5: { account: null, positions: null, history: null, status: null },
    events: [],
    derivAccount: null,
  };
}

export function createStore() {
  let state = {
    market: emptyMarket(),
    strategy: { params: {}, last: null, history: new Map(), historyOrder: [] },
    demo: null,
    node4: emptyNode4(),
    conn: {
      node1: { state: "connecting", lastMsgTs: 0 },
      node4: { state: "connecting", lastMsgTs: 0 },
    },
    errors: {}, // endpoint -> error string (404s etc.)
    seeds: { node1: false, node4: false },
  };

  const listeners = new Set();
  function emit() {
    for (const l of listeners) {
      try {
        l();
      } catch (err) {
        console.error("store listener error", err);
      }
    }
  }

  function upsertKeyed(map, order, item, key, cap) {
    const k = item[key];
    if (k == null) return false;
    if (!map.has(k)) {
      map.set(k, item);
      order.push(k);
      while (order.length > cap) {
        const dropped = order.shift();
        map.delete(dropped);
      }
      return true;
    }
    // Same key: same entity (a bar with the same ts / a decision with the
    // same ts). Replace the stored value so updates win over stale replays.
    map.set(k, item);
    return false;
  }

  // ---- connection state -------------------------------------------------
  function setConn(service, patch) {
    const prev = state.conn[service] || { state: "connecting", lastMsgTs: 0 };
    state.conn[service] = { ...prev, ...patch };
    emit();
  }

  // ---- Node 1 frames ------------------------------------------------------
  function applyNode1Frame(frame) {
    if (!frame || typeof frame !== "object") return;
    switch (frame.type) {
      case "levels": {
        const { levels } = parseLevels(frame.frames);
        if (levels.length) {
          state.market.levels = levels;
          emit();
        }
        break;
      }
      case "candle": {
        const { bars } = parseCandles(frame.frames);
        let added = 0;
        for (const bar of bars) {
          if (upsertKeyed(state.market.candles, state.market.candlesOrder, bar, "time", MAX_CANDLES)) added++;
        }
        if (added) emit();
        break;
      }
      case "tick_volume": {
        const { bars } = parseTickBars(frame.frames);
        let added = 0;
        for (const bar of bars) {
          if (upsertKeyed(state.market.tickBars, state.market.tickBarsOrder, bar, "time", MAX_TRADES)) added++;
        }
        if (added) emit();
        break;
      }
      case "calendar": {
        const cal = parseCalendar(frame.frames);
        if (cal.events.length) {
          state.market.calendar = cal.events;
          emit();
        }
        break;
      }
      case "strategy": {
        // frames: [decision] — upsert by ts into history; "last" = newest.
        const decisions = Array.isArray(frame.frames) ? frame.frames : [];
        const decision = parseDecision(decisions[decisions.length - 1]);
        if (!decision) break;
        if (decision.ts != null) {
          upsertKeyed(state.strategy.history, state.strategy.historyOrder, decision, "ts", MAX_HISTORY);
        }
        state.strategy.last = decision;
        emit();
        break;
      }
      case "demo": {
        const frames = Array.isArray(frame.frames) ? frame.frames : [];
        const snap = parseDemo(frames[frames.length - 1]);
        if (snap.ok) {
          state.demo = snap;
          emit();
        }
        break;
      }
      case "status": {
        const parsed = parseStatus(frame.status);
        if (parsed.ok) {
          state.market.status = parsed;
          emit();
        }
        break;
      }
      default:
        break; // heartbeat / unknown frames are liveness signals only
    }
  }

  // ---- Node 4 frames ------------------------------------------------------
  function applyNode4Frame(frame) {
    if (!frame || typeof frame !== "object") return;
    const topic = typeof frame.topic === "string" ? frame.topic : null;
    if (!topic) return;
    switch (topic) {
      case "diagnostics": {
        const d = parseDiagnostics(frame.diagnostics);
        if (d.ok) {
          state.node4.diagnostics = d;
          emit();
        }
        break;
      }
      case "open_trades": {
        const { trades } = parseOpenTrades(frame.open_trades);
        state.node4.openTrades = trades.slice(0, MAX_TRADES);
        emit();
        break;
      }
      case "trades": {
        // Alternative topic name for the same resource.
        const { trades } = parseOpenTrades(frame.trades);
        state.node4.openTrades = trades.slice(0, MAX_TRADES);
        emit();
        break;
      }
      case "mt5_account": {
        const a = parseMt5Account(frame.mt5_account);
        if (a.ok) {
          state.node4.mt5.account = a;
          emit();
        }
        break;
      }
      case "mt5_positions": {
        const p = parseMt5Positions(frame.mt5_positions);
        if (p.ok) {
          state.node4.mt5.positions = p;
          emit();
        }
        break;
      }
      case "mt5_history": {
        const h = parseMt5History(frame.mt5_history);
        if (h.ok) {
          state.node4.mt5.history = h;
          emit();
        }
        break;
      }
      case "bridge_status": {
        const s = parseMt5Status(frame.mt5_status);
        if (s.ok) {
          state.node4.mt5.status = s;
          emit();
        }
        break;
      }
      case "bridge_event": {
        const e = parseBridgeEvent(frame.bridge_event);
        if (e) {
          state.node4.events.unshift(e);
          if (state.node4.events.length > MAX_EVENTS) {
            state.node4.events.length = MAX_EVENTS;
          }
          emit();
        }
        break;
      }
      case "deriv_account": {
        const da = frame.deriv_account;
        if (da && typeof da === "object") {
          state.node4.derivAccount = {
            connected: da.connected === true,
            tradingDisabled: da.trading_disabled === true,
            canTrade: da.can_trade === true,
            balance: Number.isFinite(da.balance) ? da.balance : null,
            margin: Number.isFinite(da.margin) ? da.margin : null,
            pnl: Number.isFinite(da.pnl) ? da.pnl : null,
            currency: typeof da.currency === "string" ? da.currency : null,
            leverage: Number.isFinite(da.leverage) ? da.leverage : null,
            timestamp: Number.isFinite(da.timestamp) ? da.timestamp : null,
          };
          emit();
        }
        break;
      }
      case "execution_snapshot": {
        // Optional full snapshot topic (handoff variant).
        const snap = parseExecutionSnapshot(frame.execution_snapshot);
        if (snap.ok) {
          applyNode4Frame({ topic: "diagnostics", diagnostics: { timestamp: snap.timestamp, node3: snap.diagnostics.node3, work: null } });
          state.node4.openTrades = snap.openTrades.trades.slice(0, MAX_TRADES);
          if (snap.mt5.account.ok) state.node4.mt5.account = snap.mt5.account;
          if (snap.mt5.positions.ok) state.node4.mt5.positions = snap.mt5.positions;
          if (snap.mt5.history.ok) state.node4.mt5.history = snap.mt5.history;
          if (snap.mt5.status.ok) state.node4.mt5.status = snap.mt5.status;
          if (snap.derivAccount) state.node4.derivAccount = snap.derivAccount;
          for (const e of snap.recentEvents) {
            state.node4.events.unshift(e);
          }
          if (state.node4.events.length > MAX_EVENTS) {
            state.node4.events.length = MAX_EVENTS;
          }
          emit();
        }
        break;
      }
      default:
        break;
    }
  }

  // ---- REST seeds -----------------------------------------------------------
  function seedNode1Rest(payload) {
    if (!payload || typeof payload !== "object") return;
    const { levels, candles, tick_volume, calendar, strategy, demo, status } = payload;
    if (Array.isArray(levels)) {
      const { levels: lv } = parseLevels(levels);
      if (lv.length) state.market.levels = lv;
    }
    if (Array.isArray(candles)) {
      const { bars } = parseCandles(candles);
      for (const bar of bars) {
        upsertKeyed(state.market.candles, state.market.candlesOrder, bar, "time", MAX_CANDLES);
      }
    }
    if (Array.isArray(tick_volume)) {
      const { bars } = parseTickBars(tick_volume);
      for (const bar of bars) {
        upsertKeyed(state.market.tickBars, state.market.tickBarsOrder, bar, "time", MAX_TRADES);
      }
    }
    if (calendar && typeof calendar === "object") {
      state.market.calendar = parseCalendar(calendar).events;
    }
    if (strategy && typeof strategy === "object") {
      const s = parseStrategy(strategy);
      if (s.ok) {
        state.strategy.params = s.params;
        state.strategy.last = s.last;
        for (const h of s.history) {
          upsertKeyed(state.strategy.history, state.strategy.historyOrder, h, "ts", MAX_HISTORY);
        }
      }
    }
    if (demo && typeof demo === "object") {
      const d = parseDemo(demo);
      if (d.ok) state.demo = d;
    }
    if (status && typeof status === "object") {
      const st = parseStatus(status);
      if (st.ok) state.market.status = st;
    }
    state.seeds.node1 = true;
    emit();
  }

  function seedNode4Rest(payload) {
    if (!payload || typeof payload !== "object") return;
    if (payload.diagnostics) {
      const d = parseDiagnostics(payload.diagnostics);
      if (d.ok) state.node4.diagnostics = d;
    }
    if (payload.open_trades) {
      const { trades } = parseOpenTrades(payload.open_trades);
      state.node4.openTrades = trades.slice(0, MAX_TRADES);
    }
    if (payload.mt5_account) {
      const a = parseMt5Account(payload.mt5_account);
      if (a.ok) state.node4.mt5.account = a;
    }
    if (payload.mt5_positions) {
      const p = parseMt5Positions(payload.mt5_positions);
      if (p.ok) state.node4.mt5.positions = p;
    }
    if (payload.mt5_history) {
      const h = parseMt5History(payload.mt5_history);
      if (h.ok) state.node4.mt5.history = h;
    }
    if (payload.mt5_status) {
      const s = parseMt5Status(payload.mt5_status);
      if (s.ok) state.node4.mt5.status = s;
    }
    state.seeds.node4 = true;
    emit();
  }

  function setEndpointError(endpoint, message) {
    const changed = state.errors[endpoint] !== message;
    if (changed) {
      state.errors = { ...state.errors, [endpoint]: message };
      emit();
    }
  }

  function clearEndpointError(endpoint) {
    if (state.errors[endpoint]) {
      state.errors = { ...state.errors, [endpoint]: undefined };
      delete state.errors[endpoint];
      emit();
    }
  }

  return {
    get state() {
      return state;
    },
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
    setConn,
    applyNode1Frame,
    applyNode4Frame,
    seedNode1Rest,
    seedNode4Rest,
    setEndpointError,
    clearEndpointError,
  };
}

// Derived helpers used by the UI (kept here so tests cover them too).
export function latestClose(store) {
  const order = store.state.market.candlesOrder;
  if (order.length === 0) return null;
  const bar = store.state.market.candles.get(order[order.length - 1]);
  return bar ? bar.close : null;
}

export function candleBars(store) {
  const { candles, candlesOrder } = store.state.market;
  const bars = [];
  for (const t of candlesOrder) {
    const bar = candles.get(t);
    if (bar) bars.push(bar);
  }
  return bars;
}
