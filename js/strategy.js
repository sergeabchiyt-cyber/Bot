/* ============================================================
 * Node 3 strategy dashboard.
 * Strategy intents/projections live here; nothing in this module places or
 * controls trades or participates in the private intent/report link.
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, escapeHtml, numberOrNull, fmtNumber, fmtPrice, fmtDateTime, fetchPublicJson } = App.utils;
  const Normalise = App.normalize;
  const endpoints = App.config.endpoints;
  const settings = App.config.strategy;
  const MAX_ROWS = 100;
  const revisions = Normalise.createRevisionGuard();
  let started = false;
  let refreshTimer = null;
  let countdownTimer = null;
  let refreshInProgress = false;

  const state = {
    scanning: null,
    signals: null,
    work: null,
    events: [],
    timestamp: null,
    snapshotError: "",
  };

  const record = (value) => Normalise.isRecord(value) ? value : null;
  const has = (value, key) => record(value) !== null && Normalise.own(value, key);
  const text = (value, fallback = "—") =>
    value === null || value === undefined || value === "" ? fallback : String(value);
  const numeric = (value, digits = 0) => fmtNumber(value, digits);
  const price = (value) => fmtPrice(value, 2);
  const timestamp = (value) => fmtDateTime(value, true);
  const sourceData = (payload) => {
    if (Array.isArray(payload)) return payload;
    return Normalise.unwrapFrame(payload);
  };

  function sortNewest(items, getTimestamp) {
    return items.slice().sort((left, right) => {
      const a = Normalise.timestampMs(getTimestamp(left));
      const b = Normalise.timestampMs(getTimestamp(right));
      return (b ?? 0) - (a ?? 0);
    });
  }

  function replaceEvents(events) {
    const list = (Array.isArray(events) ? events : []).map(record).filter(Boolean);
    state.events = Normalise.dedupeBy(
      sortNewest(list, (event) => event.timestamp),
      Normalise.eventId,
      MAX_ROWS,
    );
  }

  function prependEvent(event) {
    const item = record(event);
    if (!item) return;
    state.events = Normalise.dedupeBy(
      [item, ...state.events],
      Normalise.eventId,
      MAX_ROWS,
    );
  }

  function applyValue(key, value, source, token) {
    if (source === "rest") {
      return revisions.applyRest(key, token, () => { state[key] = value; });
    }
    revisions.mark(key);
    state[key] = value;
    return true;
  }

  function applyEvents(events, source, token) {
    if (!Array.isArray(events)) return false;
    const apply = () => replaceEvents(events);
    if (source === "rest") return revisions.applyRest("events", token, apply);
    revisions.mark("events");
    apply();
    return true;
  }

  function applyDiagnostics(payload, source = "ws", tokens = {}) {
    const data = sourceData(payload);
    if (!record(data)) return false;
    let changed = false;

    for (const key of ["scanning", "signals", "work"]) {
      if (!has(data, key)) continue;
      changed = applyValue(key, data[key], source, tokens[key]) || changed;
      if (key === "signals" && (source !== "rest" || revisions.current(key) !== tokens[key])) {
        notifyReportConsumer(state.signals);
      }
    }
    if (has(data, "recent_events")) {
      changed = applyEvents(data.recent_events, source, tokens.events) || changed;
    }
    if (has(data, "timestamp")) {
      changed = applyValue("timestamp", data.timestamp, source, tokens.timestamp) || changed;
    }
    if (changed) {
      state.snapshotError = "";
      render();
    }
    return changed;
  }

  function notifyReportConsumer(signals) {
    if (App.execution && typeof App.execution.setStrategySignals === "function") {
      App.execution.setStrategySignals(signals);
    }
  }

  function applyScanning(payload, source = "ws", token) {
    let data = sourceData(payload);
    if (record(data) && has(data, "scanning")) data = data.scanning;
    if (!record(data)) return false;
    const changed = applyValue("scanning", data, source, token);
    if (changed) render();
    return changed;
  }

  function applySignals(payload, source = "ws", token) {
    let data = sourceData(payload);
    if (record(data) && has(data, "signals")) data = data.signals;
    if (!record(data)) return false;
    const changed = applyValue("signals", data, source, token);
    if (changed) {
      notifyReportConsumer(state.signals);
      render();
    }
    return changed;
  }

  function applyFrame(frame) {
    if (!record(frame) || typeof frame.type !== "string") return;
    switch (frame.type) {
      case "diagnostics":
        applyDiagnostics(frame, "ws");
        break;
      case "scanning":
        applyScanning(frame, "ws");
        break;
      case "signals":
        applySignals(frame, "ws");
        break;
      case "diagnostic_event":
        prependEvent(sourceData(frame));
        revisions.mark("events");
        render();
        break;
      case "heartbeat":
        break;
      default:
        // The public Node 3 /ws contract intentionally has no execution intent
        // or report transport frames.
        break;
    }
  }

  async function requestJson(url) {
    return fetchPublicJson(url, 12000);
  }

  async function refreshSnapshots() {
    if (refreshInProgress) return;
    const configured = !!endpoints.strategyDiagnostics || !!endpoints.strategyScanning || !!endpoints.strategySignals;
    if (!configured) {
      state.snapshotError = "Node 3 public endpoint is not configured.";
      render();
      return;
    }
    refreshInProgress = true;
    const diagTokens = revisions.capture(["scanning", "signals", "work", "events", "timestamp"]);
    let diagnosticsOk = false;
    try {
      const snapshot = await requestJson(endpoints.strategyDiagnostics);
      diagnosticsOk = !!record(sourceData(snapshot));
      if (diagnosticsOk) {
        applyDiagnostics(snapshot, "rest", diagTokens);
        state.snapshotError = "";
      } else {
        state.snapshotError = "Node 3 diagnostics snapshot returned no compatible data";
      }
    } catch (error) {
      state.snapshotError = error && error.message ? error.message : "Node 3 diagnostics snapshot unavailable";
    }

    const fallbacks = [];
    // A WebSocket frame can arrive while the diagnostics request is in flight.
    // Do not start an older REST fallback for a domain that is now populated;
    // if it arrives after a fallback starts, the revision guard rejects it.
    if (!state.scanning) {
      fallbacks.push({ key: "scanning", url: endpoints.strategyScanning, apply: applyScanning });
    }
    if (!state.signals) {
      fallbacks.push({ key: "signals", url: endpoints.strategySignals, apply: applySignals });
    }

    await Promise.all(fallbacks.map(async ({ key, url, apply }) => {
      if (!url) return;
      const token = revisions.current(key);
      try {
        const payload = await requestJson(url);
        if (!record(sourceData(payload))) throw new Error(`Node 3 ${key} snapshot returned no compatible data`);
        apply(payload, "rest", token);
        state.snapshotError = "";
      } catch (error) {
        state.snapshotError = error && error.message ? error.message : `Node 3 ${key} snapshot unavailable`;
      }
    }));
    refreshInProgress = false;
    render();
  }

  function metric(label, value, tone = "") {
    return `<div class="dash-stat">
      <span class="dash-stat-label">${escapeHtml(label)}</span>
      <strong class="dash-stat-value ${tone}">${value}</strong>
    </div>`;
  }

  function badge(label, tone = "muted", extraClass = "") {
    const safeTone = ["live", "blue", "amber", "warn", "muted", "unknown", "complete"].includes(tone)
      ? tone : "muted";
    return `<span class="state-badge is-${safeTone}${extraClass ? ` ${extraClass}` : ""}">${escapeHtml(label)}</span>`;
  }

  function renderServiceHealth(work) {
    const streamState = App.strategy.stream.getState();
    const node1Connected = work.node1_connected === true;
    const node1State = text(work.node1_state, node1Connected ? "connected" : "offline");
    const node4Connected = work.node4_connected === true;
    const tokenConfigured = work.node4_token_configured === true;
    const statusTone = node1Connected ? "live" : node1State === "connecting" || node1State === "reconnecting" ? "amber" : "warn";
    const node4Tone = node4Connected ? "live" : tokenConfigured ? "warn" : "muted";
    const error = work.last_error || streamState.lastError || state.snapshotError;
    const emptyState = !state.work && !state.scanning && !state.signals
      ? `<div class="empty">${streamState.status === "not-configured" ? "Node 3 is not configured. Set STRATEGY_HOST in the runtime override." : "Waiting for Node 3 strategy diagnostics; REST snapshots will fill this panel when available."}</div>`
      : "";

    return `<section class="dash-section">
      <header class="dash-section-head">
        <div><span class="dash-kicker">Node 3</span><h2>Strategy service health</h2></div>
        ${badge(streamState.status.toUpperCase(), streamState.status === "live" ? "live" : streamState.status === "stale" ? "amber" : "muted")}
      </header>
      ${emptyState}
      <div class="dash-grid">
        ${metric("Node 1 market link", `${badge(node1State, statusTone)}`)}
        ${metric("Node 4 execution link", `${badge(node4Connected ? "connected" : "disconnected", node4Tone)}`)}
        ${metric("Node 4 link configured", tokenConfigured ? "Yes" : "No")}
        ${metric("Service uptime", escapeHtml(formatUptime(work.uptime_secs)))}
        ${metric("Candles / levels received", `${numeric(work.candles_received)} / ${numeric(work.levels_received)}`)}
        ${metric("Node 1 reconnects", numeric(work.node1_reconnect_count))}
        ${metric("Last candle", escapeHtml(timestamp(work.last_candle_ts)))}
        ${metric("Last levels", escapeHtml(timestamp(work.last_levels_ts)))}
      </div>
      ${error ? `<p class="inline-error"><b>Last error</b> · ${escapeHtml(error)}</p>` : ""}
    </section>`;
  }

  function formatUptime(value) {
    const seconds = numberOrNull(value);
    if (seconds === null) return "—";
    const total = Math.floor(seconds);
    const hours = Math.floor(total / 3600);
    const minutes = Math.floor((total % 3600) / 60);
    return hours ? `${hours}h ${String(minutes).padStart(2, "0")}m` : `${minutes}m ${String(total % 60).padStart(2, "0")}s`;
  }

  function renderScanner(scanning, work) {
    if (!scanning) {
      return `<section class="dash-section">
        <header class="dash-section-head"><div><span class="dash-kicker">Node 3</span><h2>Scanner summary</h2></div></header>
        <div class="empty">No scanner snapshot yet. Check the STRATEGY service connection and public /scanning endpoint.</div>
      </section>`;
    }
    const activeCount = numberOrNull(scanning.active_setups_count);
    const currentVolume = numberOrNull(scanning.current_volume) ?? numberOrNull(work.last_candle_volume);
    const volumeThreshold = numberOrNull(scanning.volume_threshold) ?? numberOrNull(work.volume_threshold);
    const volumeConfirmed = scanning.volume_confirmed === true;
    const projection = [
      `SL ${numeric(work.current_sl_pips, 0)} pips`,
      `TP ${numeric(work.current_tp_pips, 0)} pips`,
      `${numeric(work.current_rr, 2)}R`,
    ].join(" · ");
    return `<section class="dash-section">
      <header class="dash-section-head">
        <div><span class="dash-kicker">Node 3</span><h2>Scanner summary</h2></div>
        ${badge(`${numeric(activeCount)} armed`, activeCount > 0 ? "amber" : "muted")}
      </header>
      <div class="dash-grid">
        ${metric("Active armed setups", numeric(activeCount))}
        ${metric("Tracked levels", numeric(scanning.total_levels_tracked))}
        ${metric("Current price", escapeHtml(price(scanning.last_price)))}
        ${metric("Current volume", numeric(currentVolume))}
        ${metric("Volume threshold", numeric(volumeThreshold))}
        ${metric("Volume confirmation", badge(volumeConfirmed ? "confirmed" : "waiting", volumeConfirmed ? "live" : "muted"))}
        ${metric("ATR", `${numeric(scanning.atr, 2)} · ${numeric(scanning.atr_pips, 0)} pips`)}
        ${metric("Projected risk / reward", escapeHtml(projection))}
      </div>
      <p class="dash-footnote">Scanner values and projections are strategy observations only; they are not fills or broker positions.</p>
    </section>`;
  }

  function projectionMarkup(label, order, active) {
    if (!record(order)) return `<div class="projection-line"><b>${escapeHtml(label)}</b><span>Not available</span></div>`;
    const side = text(order.side, label.toLowerCase());
    const sideClass = String(side).toLowerCase() === "buy" ? "is-buy" : String(side).toLowerCase() === "sell" ? "is-sell" : "";
    return `<div class="projection-line ${active ? "is-active-projection" : ""}">
      <b>${escapeHtml(label)} ${badge(side, sideClass === "is-buy" ? "live" : sideClass === "is-sell" ? "warn" : "muted")}</b>
      <span class="tnum">Entry ${escapeHtml(price(order.entry))} · SL ${escapeHtml(price(order.stop_loss))} · TP ${escapeHtml(price(order.take_profit))} · RR ${escapeHtml(numeric(order.risk_reward, 2))}</span>
    </div>`;
  }

  function renderSetup(setup) {
    const stateLabel = text(setup.state_label, text(setup.state, "unknown").replace(/_/g, " "));
    const pendingSide = text(setup.pending_side, "none");
    const armed = ["broken_above", "broken_below"].includes(String(setup.state || "").toLowerCase());
    const active = record(setup.active_projection);
    const activeSide = active ? String(active.side || "").toLowerCase() : "";
    const breakTime = setup.broken_at === null || setup.broken_at === undefined ? "—" : timestamp(setup.broken_at);
    const confirmed = setup.volume_confirmed === true;

    return `<article class="setup-card${armed ? " is-armed" : ""}">
      <header class="setup-card-head">
        <div><strong>${escapeHtml(text(setup.name, text(setup.id, "Strategy level")))}</strong><span class="setup-window">${escapeHtml(text(setup.window))}</span></div>
        ${badge(stateLabel, armed ? "amber" : "muted")}
      </header>
      <div class="setup-primary tnum">Level ${escapeHtml(price(setup.level_price))} · Pending side ${escapeHtml(pendingSide)}</div>
      <div class="setup-facts">
        <span>Distance <b>${escapeHtml(price(setup.distance_dollars))} dollars / ${escapeHtml(numeric(setup.distance_pips, 1))} pips</b></span>
        <span>Retest zone <b>${escapeHtml(price(setup.retest_zone_low))} – ${escapeHtml(price(setup.retest_zone_high))}</b></span>
        <span>Invalidation <b>${escapeHtml(price(setup.invalidation_price))}</b></span>
        <span>Volume required / current <b>${numeric(setup.volume_required)} / ${numeric(setup.current_volume)}</b> ${badge(confirmed ? "confirmed" : "waiting", confirmed ? "live" : "muted")}</span>
        <span>Break time / price <b>${escapeHtml(breakTime)} / ${escapeHtml(price(setup.broken_price))}</b></span>
      </div>
      <div class="projection-list">
        ${projectionMarkup("Projected buy", setup.projected_buy, activeSide === "buy")}
        ${projectionMarkup("Projected sell", setup.projected_sell, activeSide === "sell")}
        <div class="active-projection-label">Active projection: ${active ? escapeHtml(activeSide || "available") : "none"}</div>
      </div>
      <p class="setup-note"><b>Latest note</b> · ${escapeHtml(text(setup.last_note, "No note yet"))}</p>
    </article>`;
  }

  function renderSetups(scanning) {
    const setups = scanning && Array.isArray(scanning.setups)
      ? scanning.setups.map(record).filter(Boolean).slice(0, MAX_ROWS)
      : [];
    return `<section class="dash-section">
      <header class="dash-section-head">
        <div><span class="dash-kicker">Node 3</span><h2>Break / retest setups</h2></div>
        ${badge(`${setups.length} shown`, "muted")}
      </header>
      ${setups.length ? setups.map(renderSetup).join("") : '<div class="empty">No strategy setup yet. Waiting for Node 1 levels and candle volume.</div>'}
    </section>`;
  }

  function intentExpiry(intent) {
    const expires = Normalise.timestampMs(intent.expires_at);
    if (expires === null) return { text: "expiry unavailable", tone: "muted", expired: false };
    if (expires <= Date.now()) return { text: "expired", tone: "warn", expired: true };
    const remaining = Math.max(0, Math.ceil((expires - Date.now()) / 1000));
    const minutes = Math.floor(remaining / 60);
    const seconds = remaining % 60;
    return { text: `expires in ${minutes}:${String(seconds).padStart(2, "0")}`, tone: "amber", expired: false };
  }

  function renderIntent(intent, pendingIds) {
    const id = text(intent.intent_id, "intent ID unavailable");
    const side = text(intent.side, "unknown").toUpperCase();
    const sideTone = String(intent.side || "").toLowerCase() === "buy" ? "live" : String(intent.side || "").toLowerCase() === "sell" ? "warn" : "muted";
    const expiry = intentExpiry(intent);
    const delivery = pendingIds.has(String(intent.intent_id || ""))
      ? badge("pending Node 4 acknowledgement", "amber")
      : badge("strategy intent", "blue");
    return `<article class="intent-card${expiry.expired ? " is-expired" : ""}">
      <header class="intent-card-head">
        <div><span class="intent-side">${badge(side, sideTone)}</span><strong>${escapeHtml(id)}</strong></div>
        ${badge(expiry.text, expiry.tone, "intent-expiry")}
      </header>
      <div class="intent-facts">
        <span>Strategy <b>${escapeHtml(text(intent.strategy))}</b></span>
        <span>Symbol <b>${escapeHtml(text(intent.symbol))}</b></span>
        <span>Reference <b>${escapeHtml(price(intent.reference_price))}</b></span>
        <span>Stop loss <b>${escapeHtml(price(intent.stop_loss))}</b></span>
        <span>Take profit <b>${escapeHtml(price(intent.take_profit))}</b></span>
        <span>Risk / reward <b>${escapeHtml(numeric(intent.risk_reward, 2))}R</b></span>
        <span>Source level <b>${escapeHtml(text(intent.level_name))}</b></span>
        <span>Created <b>${escapeHtml(timestamp(intent.created_at))}</b></span>
        <span>Expires <b>${escapeHtml(timestamp(intent.expires_at))}</b></span>
      </div>
      <footer class="intent-card-foot">${delivery}<span>Node 3 intent only — not an execution or fill.</span></footer>
    </article>`;
  }

  function renderSignals(signals) {
    if (!signals) {
      return `<section class="dash-section">
        <header class="dash-section-head"><div><span class="dash-kicker">Node 3</span><h2>Pending and recent intents</h2></div></header>
        <div class="empty">No strategy signal snapshot yet.</div>
      </section>`;
    }
    const pending = Array.isArray(signals.pending) ? signals.pending.map(record).filter(Boolean).slice(0, MAX_ROWS) : [];
    const recent = Array.isArray(signals.recent) ? signals.recent.map(record).filter(Boolean).slice(0, 12) : [];
    const pendingIds = new Set(pending.map((intent) => String(intent.intent_id || "")));
    const reports = Array.isArray(signals.execution_reports) ? signals.execution_reports.length : 0;
    const pendingCount = numberOrNull(signals.pending_count) ?? pending.length;
    return `<section class="dash-section">
      <header class="dash-section-head">
        <div><span class="dash-kicker">Node 3</span><h2>Pending and recent intents</h2></div>
        ${badge(`${numeric(pendingCount)} pending`, pendingCount > 0 ? "amber" : "muted")}
      </header>
      <div class="dash-callout is-neutral">Intent records are strategy output. Broker-authoritative execution outcomes are shown separately in the Node 4 panel.</div>
      <div class="dash-grid compact-grid">
        ${metric("Node 4 connected", signals.node4_connected === true ? "Yes" : "No")}
        ${metric("Node 4 link configured", signals.node4_token_configured === true ? "Yes" : "No")}
        ${metric("Reports mirrored from Node 4", numeric(reports))}
        ${metric("Snapshot time", escapeHtml(timestamp(signals.timestamp)))}
      </div>
      <h3 class="dash-subtitle">Pending delivery · ${numeric(pendingCount)}</h3>
      ${pending.length ? pending.map((intent) => renderIntent(intent, pendingIds)).join("") : '<div class="empty">No pending strategy intent.</div>'}
      <h3 class="dash-subtitle">Recent strategy intents · not fills</h3>
      ${recent.length ? recent.map((intent) => renderIntent(intent, pendingIds)).join("") : '<div class="empty">No strategy signal yet.</div>'}
    </section>`;
  }

  function eventTone(level) {
    const value = String(level || "").toLowerCase();
    if (value === "error" || value === "warn" || value === "warning") return "warn";
    if (value === "signal" || value === "success") return "live";
    return "muted";
  }

  function renderActivity(events) {
    const rows = Array.isArray(events) ? events.slice(0, 30) : [];
    return `<section class="dash-section">
      <header class="dash-section-head"><div><span class="dash-kicker">Node 3</span><h2>Strategy activity log</h2></div>${badge(`${rows.length} recent`, "muted")}</header>
      ${rows.length ? `<div class="activity-list">${rows.map((event) => `
        <article class="activity-row">
          <time>${escapeHtml(timestamp(event.timestamp))}</time>
          ${badge(text(event.category, text(event.level, "event")), eventTone(event.level))}
          <span>${escapeHtml(text(event.message, ""))}</span>
        </article>`).join("")}</div>` : '<div class="empty">No scanner, signal, queue, or connection events yet.</div>'}
    </section>`;
  }

  function render() {
    const body = $("strategy-body");
    if (!body) return;
    const scanning = record(state.scanning);
    const signals = record(state.signals);
    const work = record(state.work) || {};
    const setupCount = scanning && Array.isArray(scanning.setups) ? scanning.setups.length : 0;
    const pendingCount = signals && numberOrNull(signals.pending_count) !== null
      ? numberOrNull(signals.pending_count)
      : signals && Array.isArray(signals.pending) ? signals.pending.length : 0;
    const count = $("strategy-count");
    if (count) count.textContent = String(setupCount + pendingCount);
    body.innerHTML = [
      renderServiceHealth(work),
      renderScanner(scanning, work),
      renderSetups(scanning),
      renderSignals(signals),
      renderActivity(state.events),
    ].join("");
  }

  const stream = App.createServiceStream({
    service: "strategy",
    endpoint: endpoints.strategyWs,
    topics: settings.topics,
    reconnectMinMs: settings.reconnectMinMs,
    reconnectMaxMs: settings.reconnectMaxMs,
    staleAfterMs: settings.staleAfterMs,
    heartbeatIntervalMs: settings.heartbeatIntervalMs,
    heartbeatTimeoutMs: settings.heartbeatTimeoutMs,
    refreshSnapshots,
    onFrame: applyFrame,
    onState: render,
  });

  function start() {
    if (started) return;
    started = true;
    render();
    stream.start();
    refreshTimer = setInterval(refreshSnapshots, settings.snapshotRefreshMs);
    countdownTimer = setInterval(render, 1000);
    if (!state.signals && App.execution && typeof App.execution.setStrategySignals === "function") {
      App.execution.setStrategySignals(null);
    }
  }

  App.strategy = {
    start,
    stop() {
      stream.stop();
      if (refreshTimer !== null) clearInterval(refreshTimer);
      if (countdownTimer !== null) clearInterval(countdownTimer);
      refreshTimer = null;
      countdownTimer = null;
      started = false;
    },
    applyFrame,
    applyDiagnostics,
    applyScanning,
    applySignals,
    refreshSnapshots,
    render,
    getSignals: () => state.signals,
    getState: () => ({
      scanning: state.scanning,
      signals: state.signals,
      work: state.work,
      events: state.events.slice(),
      timestamp: state.timestamp,
      snapshotError: state.snapshotError,
    }),
    stream,
  };
})((window.App = window.App || {}));
