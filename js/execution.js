/* ============================================================
 * XAUUSD Terminal — Node 3 Execution & Diagnostics dashboard
 * Streams Node 3 diagnostics over WebSocket, with REST snapshots as
 * an initial/fallback source. No credentials are stored in the frontend.
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, escapeHtml, fmtClock, toMs } = App.utils;
  const endpoints = App.config.endpoints;
  const reconnectMin = () => App.config.ws.reconnectMinMs || 1000;
  const reconnectMax = () => App.config.ws.reconnectMaxMs || 30000;
  const MAX_EVENTS = 50;
  const own = (obj, key) => Object.prototype.hasOwnProperty.call(obj, key);

  let ws = null;
  let delay = reconnectMin();
  let timer = null;
  let started = false;
  let wsConnected = false;
  let connectionState = "connecting";
  let streamRevision = 0;
  let visibilityHandlerAdded = false;
  const topicRevision = {
    scanning: 0,
    open_trades: 0,
    deriv_account: 0,
    work: 0,
    recent_events: 0,
  };

  const state = {
    scanning: null,
    open_trades: null,
    deriv_account: null,
    work: null,
    recent_events: [],
    timestamp: null,
  };

  function objectOrNull(value) {
    return value && typeof value === "object" && !Array.isArray(value) ? value : null;
  }

  function unwrap(payload) {
    let value = objectOrNull(payload);
    if (!value) return null;
    if (value.type && value.data && typeof value.data === "object") {
      value = objectOrNull(value.data);
    } else if (value.data && typeof value.data === "object" && !own(value, "scanning") && !own(value, "work")) {
      value = objectOrNull(value.data);
    }
    return value;
  }

  function numberOrNull(value) {
    if (value === null || value === undefined || value === "") return null;
    const number = Number(value);
    return Number.isFinite(number) ? number : null;
  }

  function formatNumber(value, digits = 0) {
    const number = numberOrNull(value);
    if (number === null) return "—";
    return number.toLocaleString("en-US", {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    });
  }

  function formatPrice(value) {
    const number = numberOrNull(value);
    return number === null ? "—" : number.toFixed(2);
  }

  function formatMoney(value) {
    const number = numberOrNull(value);
    return number === null ? "—" : `$${formatNumber(number, 2)}`;
  }

  function formatPnl(value) {
    const number = numberOrNull(value);
    if (number === null) return "—";
    const prefix = number > 0 ? "+" : number < 0 ? "−" : "";
    return `${prefix}$${formatNumber(Math.abs(number), 2)}`;
  }

  function formatPercent(value) {
    const number = numberOrNull(value);
    if (number === null) return "—";
    const prefix = number > 0 ? "+" : "";
    return `${prefix}${number.toFixed(1)}%`;
  }

  function pnlClass(value) {
    const number = numberOrNull(value);
    return number > 0 ? "is-positive" : number < 0 ? "is-negative" : "";
  }

  function normaliseTimestamp(value) {
    if (typeof value === "number" && Number.isFinite(value)) return toMs(value);
    if (typeof value === "string") {
      const parsed = Date.parse(value);
      return Number.isFinite(parsed) ? parsed : null;
    }
    return null;
  }

  function formatClock(value) {
    const timestamp = normaliseTimestamp(value);
    return timestamp === null ? "—" : fmtClock(timestamp, true);
  }

  function eventKey(event) {
    return [event.timestamp || event.time || "", event.category || "", event.message || ""].join("|");
  }

  function prependEvent(value) {
    const event = objectOrNull(value);
    if (!event) return;
    const key = eventKey(event);
    state.recent_events = [event, ...state.recent_events.filter((item) => eventKey(item) !== key)].slice(0, MAX_EVENTS);
  }

  function applyDiagnostics(snapshot) {
    const data = unwrap(snapshot);
    if (!data) return;

    ["scanning", "open_trades", "deriv_account", "work"].forEach((key) => {
      if (own(data, key)) state[key] = data[key];
    });
    if (Array.isArray(data.recent_events)) {
      state.recent_events = data.recent_events.map(objectOrNull).filter(Boolean).slice(0, MAX_EVENTS);
    }
    if (own(data, "timestamp")) state.timestamp = data.timestamp;
    render();
  }

  function applyFrame(frame) {
    if (!frame || typeof frame !== "object" || !frame.type) return;
    streamRevision += 1;
    const revision = streamRevision;
    const data = own(frame, "data") ? frame.data : frame.payload;

    switch (frame.type) {
      case "diagnostics": {
        const snapshot = unwrap(data);
        if (!snapshot) return;
        ["scanning", "open_trades", "deriv_account", "work", "recent_events"].forEach((key) => {
          if (own(snapshot, key)) topicRevision[key] = revision;
        });
        applyDiagnostics(snapshot);
        return;
      }
      case "scanning": {
        const payload = unwrap(data);
        state.scanning = payload && own(payload, "scanning") ? payload.scanning : payload;
        topicRevision.scanning = revision;
        break;
      }
      case "open_trades": {
        const payload = unwrap(data);
        state.open_trades = payload && own(payload, "open_trades") ? payload.open_trades : payload;
        topicRevision.open_trades = revision;
        break;
      }
      case "deriv_account": {
        const payload = unwrap(data);
        state.deriv_account = payload && own(payload, "deriv_account") ? payload.deriv_account : payload;
        topicRevision.deriv_account = revision;
        break;
      }
      case "diagnostic_event":
        prependEvent(data && data.event ? data.event : data);
        topicRevision.recent_events = revision;
        break;
      case "trades":
        // The complete open/recent trade state is also provided by diagnostics.
        // A trades topic is subscribed to for compatibility with Node 3 updates.
        if (data && typeof data === "object" && own(data, "open_trades")) {
          state.open_trades = data.open_trades;
          topicRevision.open_trades = revision;
        }
        break;
      case "heartbeat":
        return;
      default:
        return;
    }
    render();
  }

  function renderTopbarPill() {
    const valueEl = $("deriv-balance-value");
    if (!valueEl) return;
    const pill = $("deriv-pill");
    const deriv = state.deriv_account;
    let label;
    let pillState = "muted";

    if (!deriv) {
      const pending = connectionState === "connecting" || connectionState === "reconnecting";
      label = wsConnected || pending ? "sync" : "offline";
      pillState = wsConnected ? "live" : pending ? "muted" : "warn";
    } else {
      const balance = numberOrNull(deriv.balance);
      if (balance !== null) {
        const pending = connectionState === "connecting" || connectionState === "reconnecting";
        label = `$${formatNumber(balance, 2)}`;
        pillState = deriv.error || (!wsConnected && !pending)
          ? "warn"
          : deriv.authorized && wsConnected ? "live" : "muted";
      } else if (deriv.configured === false) {
        label = "sim";
        pillState = "muted";
      } else if (deriv.connected) {
        label = "auth…";
        pillState = wsConnected ? "live" : connectionState === "connecting" || connectionState === "reconnecting" ? "muted" : "warn";
      } else {
        const pending = connectionState === "connecting" || connectionState === "reconnecting";
        label = deriv.error || (!wsConnected && !pending) ? "offline" : "sync";
        pillState = deriv.error || (!wsConnected && !pending) ? "warn" : "muted";
      }
    }

    valueEl.textContent = label;
    valueEl.dataset.state = pillState;
    if (pill) {
      pill.dataset.state = pillState;
      const stream = wsConnected
        ? "Node 3 stream live"
        : connectionState === "connecting" || connectionState === "reconnecting"
          ? "Node 3 stream connecting"
          : "Node 3 stream offline";
      pill.title = deriv && deriv.account_id
        ? `Deriv Demo ${deriv.account_id} · ${stream}`
        : `Deriv Demo Balance & Node 3 Status · ${stream}`;
    }
  }

  function streamBadge() {
    const label = wsConnected ? "LIVE" : connectionState === "reconnecting" ? "RECONNECTING" : connectionState.toUpperCase();
    const cls = wsConnected ? "is-live" : connectionState === "reconnecting" || connectionState === "error" ? "is-warn" : "is-muted";
    return `<span class="exec-badge ${cls}">${escapeHtml(label)}</span>`;
  }

  function accountStatus(deriv) {
    if (deriv.authorized) return { label: "AUTHORIZED", cls: "is-live" };
    if (deriv.error) return { label: "ERROR", cls: "is-warn" };
    if (deriv.connected) return { label: "AUTHORIZING", cls: "is-warn" };
    if (deriv.configured === false) return { label: "SIGNAL MODE", cls: "is-muted" };
    return { label: deriv.configured ? "CONNECTING" : "OFFLINE", cls: "is-muted" };
  }

  function renderAccount(deriv, derivOpen) {
    const status = accountStatus(deriv);
    const balance = numberOrNull(deriv.balance);
    const pnl = numberOrNull(deriv.total_unrealized_pnl);
    const openCount = Math.max(
      derivOpen.length,
      numberOrNull(deriv.open_trades_count) || 0,
    );
    const balanceText = balance !== null
      ? `${formatMoney(balance)} ${escapeHtml(deriv.currency || "USD")}`
      : deriv.configured === false ? "Signal Mode (No Token)" : "Connecting…";
    const accountId = deriv.account_id || (deriv.configured === false ? "Not configured" : "—");
    const pnlText = pnl === null ? "—" : formatPnl(pnl);

    return `
      <section class="exec-section">
        <div class="panel-head">
          <span class="panel-title">Deriv Demo Account</span>
          <span class="exec-badge ${status.cls}">${status.label}</span>
        </div>
        <div class="exec-account-id">
          <span class="exec-label">Account ID</span>
          <span class="tnum">${escapeHtml(accountId)}</span>
          <span class="exec-stream-status">Node 3 ${streamBadge()}</span>
        </div>
        <div class="exec-grid">
          <div class="exec-stat">
            <span class="exec-label">Balance</span>
            <span class="exec-val tnum">${balanceText}</span>
          </div>
          <div class="exec-stat">
            <span class="exec-label">Open Contracts</span>
            <span class="exec-val tnum">${formatNumber(openCount, 0)}</span>
          </div>
          <div class="exec-stat">
            <span class="exec-label">Open Stake</span>
            <span class="exec-val tnum">${formatMoney(deriv.total_open_stake)}</span>
          </div>
          <div class="exec-stat">
            <span class="exec-label">Unrealized P&amp;L</span>
            <span class="exec-val tnum ${pnlClass(pnl)}">${pnlText}</span>
          </div>
        </div>
        ${deriv.error ? `<div class="exec-note is-warn">${escapeHtml(deriv.error)}</div>` : ""}
      </section>`;
  }

  function normaliseSide(value, fallback = "") {
    const side = String(value || fallback).toLowerCase();
    if (side === "buy" || side === "call") return { label: "BUY", cls: "is-buy" };
    if (side === "sell" || side === "put") return { label: "SELL", cls: "is-sell" };
    return { label: "TRADE", cls: "" };
  }

  function renderDerivTrade(contract) {
    const side = normaliseSide(contract.side, contract.contract_type === "CALL" ? "buy" : contract.contract_type === "PUT" ? "sell" : "");
    const type = contract.contract_type ? ` (${escapeHtml(String(contract.contract_type).toUpperCase())})` : "";
    const profit = numberOrNull(contract.profit);
    const profitPct = numberOrNull(contract.profit_pct);
    const symbol = contract.display_symbol || contract.symbol || "XAUUSD";
    const currentSpot = contract.current_spot ?? contract.entry_spot;

    return `
      <article class="exec-card">
        <div class="exec-card-row">
          <span class="exec-side ${side.cls}">${side.label}${type} · ${escapeHtml(symbol)}</span>
          <span class="tnum ${pnlClass(profit)}">${formatPnl(profit)}${profitPct === null ? "" : ` <small>(${formatPercent(profitPct)})</small>`}</span>
        </div>
        <div class="exec-card-meta tnum">
          #${escapeHtml(contract.contract_id || "—")} · Stake ${formatMoney(contract.buy_price)} · Spot ${formatPrice(currentSpot)}
        </div>
        <div class="exec-card-meta tnum">
          Bid ${formatMoney(contract.bid_price)} · Payout ${formatMoney(contract.payout)} · ${escapeHtml(contract.status || "open")}
        </div>
      </article>`;
  }

  function renderNode3Trade(trade) {
    const side = normaliseSide(trade.side);
    const pnl = numberOrNull(trade.unrealized_pnl);
    const rr = numberOrNull(trade.rr);
    const current = trade.current_price == null ? "" : ` · Now ${formatPrice(trade.current_price)}`;

    return `
      <article class="exec-card">
        <div class="exec-card-row">
          <span class="exec-side ${side.cls}">${side.label} · ${escapeHtml(trade.symbol || "XAUUSD")}</span>
          <span class="tnum ${pnlClass(pnl)}">${formatPnl(pnl)}</span>
        </div>
        <div class="exec-card-meta tnum">
          ${escapeHtml(trade.level_name || "VP level")} · Entry ${formatPrice(trade.entry)} · SL ${formatPrice(trade.sl)} · TP ${formatPrice(trade.tp)}
        </div>
        <div class="exec-card-meta tnum">
          RR ${rr === null ? "—" : `${formatNumber(rr, 2)}R`} · Size ${formatNumber(trade.size, 2)}${current}
        </div>
      </article>`;
  }

  function renderTrades(openTrades, deriv, derivOpen, node3Open) {
    const rows = [];
    const reportedTotal = numberOrNull(openTrades.total_open_count);
    const total = reportedTotal === null ? derivOpen.length + node3Open.length : Math.max(reportedTotal, derivOpen.length + node3Open.length);

    rows.push(`
      <section class="exec-section">
        <div class="panel-head">
          <span class="panel-title">Open Trades</span>
          <span class="exec-badge is-muted">${formatNumber(total, 0)} TOTAL</span>
        </div>
        <div class="exec-subhead">DERIV DEMO CONTRACTS <span>${formatNumber(derivOpen.length, 0)}</span></div>
        ${derivOpen.length ? derivOpen.map(renderDerivTrade).join("") : '<div class="empty exec-empty">No open Deriv contracts</div>'}
        <div class="exec-subhead">NODE 3 STRATEGY TRADES <span>${formatNumber(node3Open.length, 0)}</span></div>
        ${node3Open.length ? node3Open.map(renderNode3Trade).join("") : '<div class="empty exec-empty">No Node 3 strategy trades</div>'}
      </section>`);
    return rows.join("");
  }

  function setupIsArmed(setup) {
    const setupState = String(setup && setup.state || "").toLowerCase();
    return setupState === "broken_above" || setupState === "broken_below";
  }

  function setupDistance(setup) {
    const pips = numberOrNull(setup.distance_pips);
    if (pips !== null) return Math.abs(pips);
    const dollars = numberOrNull(setup.distance_dollars);
    return dollars === null ? Number.POSITIVE_INFINITY : Math.abs(dollars * 100);
  }

  function projectedOrder(setup) {
    if (setup.active_order && typeof setup.active_order === "object") return setup.active_order;
    const side = String(setup.pending_side || "").toLowerCase();
    const setupState = String(setup.state || "").toLowerCase();
    if ((side === "buy" || setupState === "broken_above") && setup.projected_buy) return setup.projected_buy;
    if ((side === "sell" || setupState === "broken_below") && setup.projected_sell) return setup.projected_sell;
    const dollars = numberOrNull(setup.distance_dollars);
    if (dollars !== null) {
      if (dollars >= 0 && setup.projected_buy) return setup.projected_buy;
      if (dollars < 0 && setup.projected_sell) return setup.projected_sell;
    }
    return setup.projected_buy || setup.projected_sell || null;
  }

  function renderSetup(setup, scanning) {
    const armed = setupIsArmed(setup);
    const order = projectedOrder(setup);
    const stateText = setup.state_label || String(setup.state || "scanning").replace(/_/g, " ");
    const required = numberOrNull(setup.volume_required) ?? numberOrNull(scanning.volume_threshold);
    const currentVolume = numberOrNull(setup.current_volume) ?? numberOrNull(scanning.current_volume);
    const volumeConfirmed = typeof setup.volume_confirmed === "boolean"
      ? setup.volume_confirmed
      : required !== null && currentVolume !== null && currentVolume >= required;
    const volumeText = `${formatNumber(currentVolume, 0)} / ${formatNumber(required, 0)}`;
    const pct = required && required > 0 && currentVolume !== null
      ? Math.max(0, Math.min(100, (currentVolume / required) * 100))
      : 0;
    const distance = numberOrNull(setup.distance_pips);
    const distanceText = distance === null
      ? numberOrNull(setup.distance_dollars) === null ? "—" : `${formatNumber(Math.abs(Number(setup.distance_dollars) * 100), 0)} pips`
      : `${formatNumber(distance, 0)} pips`;
    const orderSide = order ? normaliseSide(order.side || setup.pending_side || (setup.state === "broken_below" ? "sell" : "buy")) : null;
    const orderText = order ? `
      <div class="exec-projection tnum">
        <span class="exec-projection-side ${orderSide.cls}">${orderSide.label} PLAN</span>
        Entry ${formatPrice(order.entry)} · SL ${formatPrice(order.sl)} · TP ${formatPrice(order.tp)} · ${numberOrNull(order.rr) === null ? "—" : `${formatNumber(order.rr, 2)}R`}
      </div>` : "";
    const note = setup.last_note || setup.state_label || "";

    return `
      <article class="exec-card exec-setup${armed ? " is-armed" : ""}">
        <div class="exec-card-row">
          <strong>${escapeHtml(setup.name || setup.id || "VP level")} <span class="tnum">@ ${formatPrice(setup.level_price)}</span></strong>
          <span class="exec-badge ${armed ? "is-armed" : "is-muted"}">${escapeHtml(stateText)}</span>
        </div>
        <div class="exec-setup-meta tnum">
          <span>Distance <b>${escapeHtml(distanceText)}</b></span>
          <span>Retest <b>${formatPrice(setup.retest_zone_low)}–${formatPrice(setup.retest_zone_high)}</b></span>
        </div>
        <div class="exec-volume-row">
          <span class="exec-label">Volume</span>
          <span class="tnum">${escapeHtml(volumeText)}</span>
          <span class="exec-badge ${volumeConfirmed ? "is-live" : "is-muted"}">${volumeConfirmed ? "CONFIRMED" : "WAITING"}</span>
        </div>
        <div class="exec-volume-track" role="progressbar" aria-label="Volume confirmation progress" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${Math.round(pct)}">
          <span style="width:${pct.toFixed(1)}%"></span>
        </div>
        ${orderText}
        ${note ? `<div class="exec-note">${escapeHtml(note)}</div>` : ""}
      </article>`;
  }

  function renderScanning(scanning) {
    const input = Array.isArray(scanning.setups) ? scanning.setups.map(objectOrNull).filter(Boolean) : [];
    const setups = input.slice().sort((a, b) => {
      const armedSort = Number(setupIsArmed(b)) - Number(setupIsArmed(a));
      return armedSort || setupDistance(a) - setupDistance(b);
    });
    const armedCount = Math.max(
      numberOrNull(scanning.active_setups_count) || 0,
      setups.filter(setupIsArmed).length,
    );
    const totalLevels = numberOrNull(scanning.total_levels_tracked) ?? setups.length;
    const setupRows = setups.map((setup) => renderSetup(setup, scanning));

    return `
      <section class="exec-section">
        <div class="panel-head">
          <span class="panel-title">Scanning for Trades</span>
          <span class="exec-badge ${armedCount ? "is-armed" : "is-muted"}">${formatNumber(armedCount, 0)} ARMED / ${formatNumber(totalLevels, 0)} LEVELS</span>
        </div>
        <div class="exec-scan-summary tnum">
          <span>Price <b>${formatPrice(scanning.last_price)}</b></span>
          <span>ATR <b>${formatNumber(scanning.atr_pips, 0)} pips</b></span>
          <span>Vol gate <b>${formatNumber(scanning.volume_threshold, 0)}</b></span>
        </div>
        ${setupRows.length ? setupRows.join("") : '<div class="empty">Awaiting VP levels from Node 1…</div>'}
      </section>`;
  }

  function metric(label, value, valueClass = "") {
    return `<div class="exec-stat"><span class="exec-label">${label}</span><span class="exec-val tnum ${valueClass}">${value}</span></div>`;
  }

  function formatUptime(value) {
    const seconds = numberOrNull(value);
    if (seconds === null) return "—";
    const mins = Math.floor(seconds / 60);
    const hours = Math.floor(mins / 60);
    if (hours) return `${hours}h ${String(mins % 60).padStart(2, "0")}m`;
    if (mins) return `${mins}m ${String(Math.floor(seconds % 60)).padStart(2, "0")}s`;
    return `${Math.floor(seconds)}s`;
  }

  function eventTone(event) {
    const level = String(event.level || "").toLowerCase();
    if (level === "error" || level === "warn" || level === "warning") return "is-negative";
    if (level === "signal" || level === "success") return "is-positive";
    return "";
  }

  function renderEvents(events) {
    if (!events.length) return '<div class="empty exec-empty">No execution events yet</div>';
    return `<div class="exec-events">${events.slice(0, 12).map((event) => `
      <div class="exec-event-row">
        <span class="tnum exec-event-time">${formatClock(event.timestamp || event.time)}</span>
        <span class="exec-event-tag ${eventTone(event)}">${escapeHtml(event.category || event.level || "engine")}</span>
        <span class="exec-event-msg">${escapeHtml(event.message || event.event || "")}</span>
      </div>`).join("")}</div>`;
  }

  function renderDiagnostics(work, events) {
    const nodeState = work.node1_state || (work.node1_connected ? "connected" : "offline");
    const nodeClass = work.node1_connected ? "is-live" : "is-warn";
    const ratio = numberOrNull(work.volume_ratio);
    const volumeRatio = ratio === null ? "—" : `${formatNumber(ratio, 2)}x`;
    const currentVolume = numberOrNull(work.last_candle_volume);
    const threshold = numberOrNull(work.volume_threshold);
    const volumeLabel = currentVolume === null
      ? "—"
      : `${formatNumber(currentVolume, 0)} / ${formatNumber(threshold, 0)} (${volumeRatio})`;
    const stops = `SL ${formatNumber(work.current_sl_pips, 0)}p · TP ${formatNumber(work.current_tp_pips, 0)}p`;
    const rr = numberOrNull(work.current_rr);
    const engineMetrics = [
      metric("Node 1 Upstream", escapeHtml(nodeState), work.node1_connected ? "is-positive" : "is-negative"),
      metric("ATR", `${formatNumber(work.atr_pips, 0)} pips`),
      metric("Dynamic Stops", stops),
      metric("Target R:R", rr === null ? "—" : `${formatNumber(rr, 2)}R`),
      metric("Candle Volume", volumeLabel),
      metric("Breaks / Invalid", `${formatNumber(work.breaks_detected, 0)} / ${formatNumber(work.breaks_invalidated, 0)}`),
      metric("Retests Rejected · Low Vol", formatNumber(work.retests_rejected_low_volume, 0)),
      metric("Signals Confirmed", formatNumber(work.signals_confirmed, 0)),
      metric("Trades Executed / Failed", `${formatNumber(work.trades_executed, 0)} / ${formatNumber(work.trades_failed, 0)}`),
      metric("Candles / Levels", `${formatNumber(work.candles_received, 0)} / ${formatNumber(work.levels_received, 0)}`),
      metric("Uptime", formatUptime(work.uptime_secs)),
    ].join("");

    return `
      <section class="exec-section">
        <div class="panel-head">
          <span class="panel-title">Node 3 Diagnostics</span>
          <span class="exec-badge ${nodeClass}">NODE 1: ${escapeHtml(nodeState)}</span>
        </div>
        <div class="exec-grid exec-diagnostics-grid">${engineMetrics}</div>
        <div class="exec-subhead">ACTIVITY LOG <span>${formatNumber(events.length, 0)}</span></div>
        ${renderEvents(events)}
      </section>`;
  }

  function getOpenLists(openTrades, deriv) {
    const derivFromTrades = Array.isArray(openTrades.deriv_open_trades)
      ? openTrades.deriv_open_trades.map(objectOrNull).filter(Boolean)
      : null;
    const derivFromAccount = Array.isArray(deriv.open_trades)
      ? deriv.open_trades.map(objectOrNull).filter(Boolean)
      : [];
    const derivOpen = derivFromTrades && derivFromTrades.length
      ? derivFromTrades
      : derivFromAccount.length ? derivFromAccount : derivFromTrades || [];
    const node3Open = Array.isArray(openTrades.node3_open_trades)
      ? openTrades.node3_open_trades.map(objectOrNull).filter(Boolean)
      : [];
    return { derivOpen, node3Open };
  }

  function render() {
    renderTopbarPill();
    const body = $("execution-body");
    const countBadge = $("execution-count");
    if (!body) return;

    const scanning = objectOrNull(state.scanning) || {};
    const deriv = objectOrNull(state.deriv_account) || {};
    const openTrades = objectOrNull(state.open_trades) || {};
    const work = objectOrNull(state.work) || {};
    const events = Array.isArray(state.recent_events) ? state.recent_events.map(objectOrNull).filter(Boolean) : [];
    const setups = Array.isArray(scanning.setups) ? scanning.setups.map(objectOrNull).filter(Boolean) : [];
    const { derivOpen, node3Open } = getOpenLists(openTrades, deriv);
    const armedCount = Math.max(
      numberOrNull(scanning.active_setups_count) || 0,
      setups.filter(setupIsArmed).length,
    );
    if (countBadge) countBadge.textContent = String(derivOpen.length + node3Open.length + armedCount);

    body.innerHTML = [
      renderAccount(deriv, derivOpen),
      renderTrades(openTrades, deriv, derivOpen, node3Open),
      renderScanning(scanning),
      renderDiagnostics(work, events),
    ].join("");
  }

  async function requestJson(url) {
    if (!url || typeof fetch !== "function") throw new Error("Snapshot endpoint unavailable");
    const response = await fetch(url, {
      method: "GET",
      cache: "no-store",
      headers: { Accept: "application/json" },
    });
    if (!response.ok) throw new Error(`Snapshot request failed (${response.status})`);
    return response.json();
  }

  function revisionsAtStart() {
    return { ...topicRevision };
  }

  function applyRestDiagnostics(snapshot, revisions) {
    const data = unwrap(snapshot);
    if (!data) return;
    const safeSnapshot = {};
    ["scanning", "open_trades", "deriv_account", "work", "recent_events", "timestamp"].forEach((key) => {
      if (!own(data, key)) return;
      if (key === "timestamp" || topicRevision[key] === revisions[key]) safeSnapshot[key] = data[key];
    });
    if (Object.keys(safeSnapshot).length) applyDiagnostics(safeSnapshot);
  }

  async function fetchSnapshot() {
    const revisions = revisionsAtStart();
    let snapshot = null;
    try {
      snapshot = await requestJson(endpoints.executionDiagnostics);
    } catch (error) {
      console.warn("Node 3 diagnostics snapshot failed; trying individual snapshots:", error);
    }

    if (snapshot) applyRestDiagnostics(snapshot, revisions);

    const missing = [];
    if (!state.scanning) missing.push(["scanning", endpoints.executionScanning]);
    if (!state.open_trades) missing.push(["open_trades", endpoints.executionOpenTrades]);
    if (!state.deriv_account) missing.push(["deriv_account", endpoints.executionDeriv]);
    if (!missing.length) return;

    const results = await Promise.all(missing.map(async ([key, url]) => {
      try {
        return { key, payload: await requestJson(url) };
      } catch (error) {
        console.warn(`Node 3 ${key} snapshot failed:`, error);
        return { key, payload: null };
      }
    }));

    let changed = false;
    results.forEach(({ key, payload }) => {
      if (topicRevision[key] !== revisions[key]) return;
      const data = unwrap(payload);
      if (!data) return;
      if (key === "scanning") state.scanning = own(data, "scanning") ? data.scanning : data;
      if (key === "open_trades") state.open_trades = own(data, "open_trades") ? data.open_trades : data;
      if (key === "deriv_account") state.deriv_account = own(data, "deriv_account") ? data.deriv_account : data;
      changed = true;
    });
    if (changed) render();
  }

  function scheduleReconnect() {
    if (!started) return;
    connectionState = "reconnecting";
    render();
    if (timer) return;
    timer = setTimeout(() => {
      timer = null;
      connect();
    }, delay);
    delay = Math.min(delay * 2, reconnectMax());
  }

  function connect() {
    if (!endpoints.executionWs) {
      connectionState = "offline";
      render();
      return;
    }
    if (typeof WebSocket === "undefined") {
      connectionState = "unsupported";
      render();
      return;
    }
    if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;

    connectionState = "connecting";
    render();
    let socket;
    try {
      socket = new WebSocket(endpoints.executionWs);
    } catch (error) {
      connectionState = "error";
      console.warn("Node 3 WebSocket connection failed:", error);
      render();
      scheduleReconnect();
      return;
    }
    ws = socket;

    socket.onopen = () => {
      if (ws !== socket) return;
      wsConnected = true;
      connectionState = "live";
      delay = reconnectMin();
      if (timer) {
        clearTimeout(timer);
        timer = null;
      }
      try {
        socket.send(JSON.stringify({
          type: "subscribe",
          topics: ["diagnostics", "scanning", "open_trades", "deriv_account", "trades", "diagnostic_event"],
        }));
      } catch (error) {
        console.warn("Node 3 subscribe request failed:", error);
      }
      render();
    };

    socket.onmessage = (event) => {
      if (ws !== socket) return;
      let frame;
      try {
        frame = JSON.parse(event.data);
      } catch {
        return;
      }
      applyFrame(frame);
    };

    socket.onclose = () => {
      if (ws !== socket) return;
      wsConnected = false;
      connectionState = "offline";
      render();
      scheduleReconnect();
    };

    socket.onerror = () => {
      if (ws !== socket) return;
      wsConnected = false;
      connectionState = "error";
      render();
      scheduleReconnect();
      try { socket.close(); } catch {}
    };
  }

  function start() {
    if (started) return;
    started = true;
    render();
    fetchSnapshot();
    connect();

    if (!visibilityHandlerAdded && typeof document !== "undefined") {
      visibilityHandlerAdded = true;
      document.addEventListener("visibilitychange", () => {
        const socketClosed = !ws || (typeof WebSocket !== "undefined" && ws.readyState === WebSocket.CLOSED);
        if (document.visibilityState === "visible" && socketClosed) {
          if (timer) {
            clearTimeout(timer);
            timer = null;
          }
          delay = reconnectMin();
          connect();
        }
      });
    }
  }

  App.execution = {
    start,
    applyDiagnostics,
    applyFrame,
  };
})((window.App = window.App || {}));
