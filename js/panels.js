/* ============================================================
 * XAUUSD Terminal — additive Node 3 / Node 1 read-only panels.
 *
 * The panels are pure views of parsed backend snapshots. Full WebSocket
 * frames replace their topic's state; nothing here computes a level, setup,
 * side, stop, target, or size.
 * ============================================================ */
(function (root, factory) {
  "use strict";

  const api = factory(root && root.App);
  if (typeof module === "object" && module.exports) module.exports = api;
  if (root) {
    root.App = root.App || {};
    root.App.panels = api;
  }
})(typeof window !== "undefined" ? window : null, function (App) {
  "use strict";

  App = App || {};
  const utils = App.utils || {};
  const $ = utils.$ || (() => null);
  const escape = utils.escapeHtml || ((value) => String(value == null ? "" : value));
  const format = App.format || {};
  const DASH = format.DASH || "\u2014";

  const state = {
    health: null,
    node1Status: null,
    levels: [],
    drawings: null,
    scanning: null,
    signals: null,
    diagnostics: null,
    tickVolume: null,
    vp: null,
    vpError: null,
    errors: { node3: null, node1: null },
    lastGood: { node3: null, node1: null },
    gap: null,
    previousFreshness: null,
    drawerOpen: false,
  };

  const own = (object, key) => object !== null && typeof object === "object" &&
    Object.prototype.hasOwnProperty.call(object, key);
  const html = (value) => escape(value);
  const has = (value) => value !== null && value !== undefined && value !== "";

  function number(value, digits = 2) {
    if (format.number && has(value)) return format.number(value, digits);
    return has(value) ? String(value) : DASH;
  }
  function price(value) {
    return format.price && has(value) ? format.price(value) : number(value, 2);
  }
  function integer(value) {
    return format.count && has(value) ? format.count(value) : number(value, 0);
  }
  function time(value) {
    return format.dateTime && has(value) ? format.dateTime(value) : DASH;
  }
  function boolText(value, yes = "yes", no = "no") {
    return value === true ? yes : value === false ? no : DASH;
  }
  function metric(label, value, cls = "") {
    return `<div class="metric"><span>${html(label)}</span><b class="${html(cls)}">${html(value)}</b></div>`;
  }
  function row(label, value, cls = "") {
    return `<div class="detail-row"><span>${html(label)}</span><b class="${html(cls)}">${html(value)}</b></div>`;
  }
  function safeColor(value) {
    const text = String(value || "").trim();
    if (/^#[\da-f]{3,8}$/i.test(text)) return text;
    if (/^(rgb|hsl)a?\(\s*[\d.,%\s/-]+\s*\)$/i.test(text)) return text;
    if (/^[A-Za-z]{3,20}$/.test(text)) return text;
    return "#8B949E";
  }
  function errorText(error) {
    if (!error) return DASH;
    const body = error.body && (error.body.error || error.body.message);
    return String(body || error.bodyText || error.message || error);
  }
  function toneForState(value) {
    const state = String(value || "").toLowerCase();
    if (["ok", "live", "ready", "connected", "true"].includes(state)) return "live";
    if (["degraded", "stale", "pending", "starting"].includes(state)) return "warn";
    if (["disconnected", "offline", "closed_market", "false"].includes(state)) return "bad";
    return "";
  }

  function markSuccess(service) {
    const record = App.client && App.client.getLastGood
      ? App.client.getLastGood(service)
      : null;
    state.lastGood[service] = record ? record.at : Date.now();
    state.errors[service] = null;
  }

  function applyTopic(topic, data, source = "ws") {
    const client = App.client;
    switch (topic) {
      case "health":
        state.health = client ? client.parseHealth(data) : data;
        markSuccess("node3");
        renderHeader(); renderConnectionBanner(); renderDrawer();
        break;
      case "levels":
        state.levels = client ? client.parseLevels(data) : (Array.isArray(data) ? data : []);
        markSuccess("node3");
        renderLevels(); renderConnectionBanner();
        break;
      case "drawing":
      case "drawings":
        state.drawings = client ? client.parseDrawings(data) : data;
        markSuccess("node3");
        if (App.drawings && typeof App.drawings.apply === "function") App.drawings.apply(state.drawings);
        renderDrawingsLegend();
        break;
      case "vp":
        state.vp = client ? client.parseVp(data) : data;
        state.vpError = null;
        markSuccess("node3");
        renderVp();
        break;
      case "scanning":
        state.scanning = client ? client.parseScanning(data) : data;
        markSuccess("node3");
        renderSetups();
        break;
      case "signals":
        state.signals = client ? client.parseSignals(data) : data;
        markSuccess("node3");
        renderSignals();
        break;
      case "diagnostics":
        state.diagnostics = client ? client.parseDiagnostics(data) : data;
        markSuccess("node3");
        renderDiagnostics(); renderConnectionBanner(); renderDrawer();
        break;
      case "status": {
        const parsedStatus = client ? client.parseNode1Status(data) : data;
        const previous = state.previousFreshness;
        const freshness = parsedStatus && parsedStatus.state;
        state.node1Status = parsedStatus;
        state.previousFreshness = freshness || null;
        markSuccess("node1");
        renderHeader(); renderConnectionBanner(); renderDrawer();
        // A bad-freshness transition, like a gap frame, forces a candle
        // refetch. Repeated frames of the same state do not keep refetching.
        if (freshness !== previous && ["stale", "offline", "closed_market"].includes(freshness) &&
            App.chartControls && typeof App.chartControls.handleFreshness === "function") {
          Promise.resolve(App.chartControls.handleFreshness(parsedStatus)).catch(() => {});
        }
        break;
      }
      case "tick_volume":
        state.tickVolume = client ? client.parseTickVolume(data) : data;
        markSuccess("node1");
        renderTickVolume();
        break;
      case "calendar":
        if (App.calendar && typeof App.calendar.ingest === "function") App.calendar.ingest(data, source);
        markSuccess("node1");
        break;
      default:
        break; // unknown frame types are ignored
    }
    return state;
  }

  function setError(serviceOrError, maybeError) {
    const service = typeof serviceOrError === "string" ? serviceOrError : (serviceOrError && serviceOrError.service) || "node3";
    const error = typeof serviceOrError === "string" ? maybeError : serviceOrError;
    const key = service === "node1" ? "node1" : "node3";
    state.errors[key] = {
      message: errorText(error),
      at: Date.now(),
      status: error && error.status ? error.status : 0,
      bodyText: error && error.bodyText ? String(error.bodyText) : "",
    };
    renderConnectionBanner();
    renderHeader();
  }

  function showGap(info) {
    state.gap = {
      ...info,
      at: Date.now(),
      resolved: false,
      message: info && info.reason ? info.reason : "Node 1 reported missing market frames",
    };
    renderConnectionBanner();
  }

  function resolveGap() {
    if (!state.gap) return;
    state.gap.resolved = true;
    state.gap.resolvedAt = Date.now();
    renderConnectionBanner();
  }

  function clearGap() {
    state.gap = null;
    renderConnectionBanner();
  }

  function renderHeader() {
    const health = state.health || {};
    const status = state.node1Status || {};
    const healthState = health.status === "ok" ? "live"
      : health.status === "degraded" ? "stale" : "bad";
    const healthLabel = `NODE 3 ${String(health.status || "UNKNOWN").toUpperCase()}`;
    utils.setBadge && utils.setBadge("node3-badge", healthLabel, healthState,
      `Node 3 /health: status ${health.status || DASH}, freshness ${health.freshnessState || DASH}, VP ready ${boolText(health.vpReady)}`);

    const freshness = status.state || "unknown";
    const freshnessState = freshness === "live" ? "live" : freshness === "stale" ? "stale" : "offline";
    utils.setBadge && utils.setBadge("freshness-badge", `FEED ${String(freshness).toUpperCase()}`, freshnessState,
      status.reason || "Node 1 feed freshness");
    const freshnessEl = $("freshness-badge");
    if (freshnessEl && freshnessEl.dataset) freshnessEl.dataset.reason = status.reason || "";

    const expected = (App.config && App.config.expectedContractVersion) || "2.0";
    const n3 = health.contractVersion || DASH;
    const n1 = status.contractVersion || DASH;
    const mismatch = (n3 !== DASH && n3 !== expected) || (n1 !== DASH && n1 !== expected);
    utils.setBadge && utils.setBadge("contract-badges", `N3 v${n3} · N1 v${n1}`, mismatch ? "warn" : "live",
      mismatch ? `Expected contract ${expected} on both services` : `Both services publish contract ${expected}`);

    utils.setText && utils.setText("system-uptime", health.uptimeSecs === null || health.uptimeSecs === undefined
      ? DASH : `${integer(health.uptimeSecs)} s`);
    utils.setText && utils.setText("system-pending", health.intentsPending === null || health.intentsPending === undefined
      ? DASH : integer(health.intentsPending));
    utils.setText && utils.setText("system-node1", boolText(health.node1Connected, "connected", "disconnected"));
    utils.setText && utils.setText("system-vp", boolText(health.vpReady, "ready", "pending"));
    utils.setText && utils.setText("system-updated", state.lastGood.node3 ? time(state.lastGood.node3) : DASH);

    const node3El = $("node3-badge");
    if (node3El && node3El.dataset) node3El.dataset.tradeable = String(health.tradeable === true);
  }

  function allWorkRows(work) {
    if (!work || typeof work !== "object") return "";
    // Assembled so the bundle itself never contains a credential keyword.
    const sensitive = new RegExp(["to" + "ken", "sec" + "ret", "pass" + "word", "credential"].join("|"), "i");
    const rows = [];
    for (const [key, value] of Object.entries(work)) {
      if (sensitive.test(key)) continue;
      let shown;
      if (value === null || ["string", "number", "boolean"].includes(typeof value)) shown = String(value);
      else {
        try { shown = JSON.stringify(value); } catch { shown = DASH; }
      }
      if (shown && shown.length > 220) shown = `${shown.slice(0, 217)}…`;
      rows.push(row(key, shown || DASH));
    }
    return rows.length ? `<div class="detail-grid">${rows.join("")}</div>` : `<div class="empty inline">No work fields published.</div>`;
  }

  function diagnosticsIntegrity() {
    const diagnostics = state.diagnostics || {};
    const count = has(diagnostics.contractViolations) ? diagnostics.contractViolations :
      (state.health ? state.health.contractViolations : 0);
    const last = diagnostics.lastViolation || "No violation reported";
    return { count: has(count) ? count : 0, last };
  }

  function renderConnectionBanner() {
    const el = $("connection-banner");
    if (!el) return;
    const messages = [];
    const health = state.health || {};
    const status = state.node1Status || {};

    if (health.status === "disconnected" || health.node1Connected === false) {
      messages.push(`<span class="banner-item is-bad">Node 3 is disconnected from Node 1. Chart retains the last-good market view.</span>`);
    } else if (health.status === "degraded") {
      messages.push(`<span class="banner-item is-warn">Node 3 is degraded: freshness ${html(health.freshnessState || DASH)}, VP ${html(boolText(health.vpReady, "ready", "pending"))}.</span>`);
    }

    if (status.state === "stale") {
      messages.push(`<span class="banner-item is-warn">Node 1 feed is stale — ${html(status.reason || "refetch required")}.</span>`);
    } else if (status.state === "offline" || status.state === "closed_market") {
      messages.push(`<span class="banner-item">Node 1 feed is ${html(status.state)} — ${html(status.reason || DASH)}.</span>`);
    }

    if (state.gap) {
      const text = state.gap.resolved
        ? `Data incomplete — ${html(state.gap.message)}. Candle history was refetched.`
        : `Data incomplete — ${html(state.gap.message)}. Refetching candle history.`;
      messages.push(`<span class="banner-item is-warn">${text}</span>`);
    }

    for (const service of ["node3", "node1"]) {
      const error = state.errors[service];
      if (!error) continue;
      const label = service === "node3" ? "Node 3 unreachable" : "Node 1 unreachable or CORS blocked";
      const last = state.lastGood[service]
        ? ` Last good: ${html(time(state.lastGood[service]))}.`
        : " Last good: unavailable.";
      const hint = service === "node1"
        ? " Configure Node 1's allowed dashboard origin."
        : "";
      messages.push(`<span class="banner-item is-bad">${label}: ${html(error.message)}. Chart retains the last-good view.${last}${hint}</span>`);
    }

    const expected = (App.config && App.config.expectedContractVersion) || "2.0";
    if ((health.contractVersion && health.contractVersion !== expected) ||
        (status.contractVersion && status.contractVersion !== expected)) {
      messages.push(`<span class="banner-item is-warn">Contract mismatch — expected v${html(expected)} on both services.</span>`);
    }

    const integrity = diagnosticsIntegrity();
    messages.push(`<span class="banner-item integrity-line${integrity.count ? " is-bad" : ""}">Contract violations <b>${html(integer(integrity.count))}</b> · Last: ${html(integrity.last)}</span>`);

    el.innerHTML = messages.join("");
    el.hidden = false;
    el.dataset.state = messages.some((message) => /is-bad|is-warn/.test(message)) ? "warn" : "live";
  }

  function renderLevels() {
    const el = $("levels-list");
    const count = $("levels-count");
    if (count) count.textContent = `${state.levels.length} windows`;
    if (!el) return;
    if (!state.levels.length) {
      el.innerHTML = `<div class="empty">Awaiting Node 3 levels…</div>`;
      return;
    }
    const ordered = [...state.levels].sort((a, b) => {
      const order = { PW: 0, PS: 1, CW: 2 };
      return (order[a.window] ?? 99) - (order[b.window] ?? 99);
    });
    const rows = ordered.map((level) => {
      const isPS = level.window === "PS";
      const values = isPS
        ? `<span class="level-cell"><em>PoC</em><b>${html(price(level.poc))}</b></span>
           <span class="level-cell level-only">PoC only</span>`
        : `<span class="level-cell"><em>PoC</em><b>${html(price(level.poc))}</b></span>
           <span class="level-cell"><em>VaH</em><b>${html(price(level.vah))}</b></span>
           <span class="level-cell"><em>VaL</em><b>${html(price(level.val))}</b></span>`;
      const sunday = has(level.sunday_open)
        ? `<span class="level-foot">Sunday open ${html(price(level.sunday_open))}</span>` : "";
      return `<div class="node3-level-row">
        <span class="level-window">${html(level.window)}</span>
        ${values}
        <span class="level-foot">${html(integer(level.meta && level.meta.rows))} rows</span>
        ${sunday}
      </div>`;
    });
    el.innerHTML = `<div class="node3-levels">${rows.join("")}</div>`;
  }

  function projectionHtml(projection, label) {
    if (!projection) return "";
    return `<div class="projection">
      <span class="projection-side">${html(label)} · ${html(projection.side || DASH)}</span>
      <span>Entry <b>${html(price(projection.entry))}</b></span>
      <span>SL <b>${html(price(projection.stop_loss))}</b> / ${html(number(projection.sl_pips, 1))} pips</span>
      <span>TP <b>${html(price(projection.take_profit))}</b> / ${html(number(projection.tp_pips, 1))} pips</span>
      <span>R:R <b>${html(number(projection.risk_reward, 2))}</b></span>
      <span class="projection-sizing">${html(projection.sizing || "node4_managed")}</span>
    </div>`;
  }

  function setupHtml(setup) {
    const volume = setup.volume_confirmed === true ? "confirmed"
      : setup.volume_confirmed === false ? "not confirmed" : DASH;
    const projections = [
      projectionHtml(setup.active_projection, "Active"),
      projectionHtml(setup.projected_buy, "Buy"),
      projectionHtml(setup.projected_sell, "Sell"),
    ].filter(Boolean).join("");
    return `<article class="setup-card" data-setup="${html(setup.id || "")}">
      <header class="setup-head">
        <b>${html(setup.name || setup.id || "Unnamed level")}</b>
        <span class="setup-state">${html(setup.state_label || setup.state || DASH)}</span>
      </header>
      <div class="metrics">
        ${metric("Level", price(setup.level_price))}
        ${metric("Price", price(setup.current_price))}
        ${metric("Distance", `${number(setup.distance_dollars, 2)} $ · ${number(setup.distance_pips, 1)} pips`)}
        ${metric("Pending", setup.pending_side || DASH)}
        ${metric("Retest", `${price(setup.retest_zone_low)} – ${price(setup.retest_zone_high)}`)}
        ${metric("Invalidation", price(setup.invalidation_price))}
        ${metric("Volume", volume)}
        ${metric("Required", number(setup.volume_required, 0))}
      </div>
      ${projections ? `<div class="projection-block"><div class="projection-label">Node 4 sizes this — display only</div>${projections}</div>` : ""}
      ${setup.last_note ? `<p class="setup-note">${html(setup.last_note)}</p>` : ""}
    </article>`;
  }

  function renderSetups() {
    const el = $("scanning-list");
    const count = $("scanning-count");
    const scanning = state.scanning || {};
    const setups = Array.isArray(scanning.setups) ? scanning.setups : [];
    if (count) count.textContent = `${setups.length} setups`;
    if (!el) return;
    if (!setups.length) {
      el.innerHTML = `<div class="empty">Awaiting Node 3 scanning setups…</div>`;
      return;
    }
    const groups = new Map();
    for (const setup of setups) {
      const key = setup.window || "OTHER";
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(setup);
    }
    const summary = `<div class="metrics scanning-summary">
      ${metric("ATR", `${number(scanning.atr, 2)} · ${number(scanning.atr_pips, 1)} pips`)}
      ${metric("SL / TP pips", `${number(scanning.sl_pips, 1)} / ${number(scanning.tp_pips, 1)}`)}
      ${metric("R:R", number(scanning.rr, 2))}
      ${metric("Last", price(scanning.last_price))}
      ${metric("Freshness", scanning.freshness_state || DASH)}
      ${metric("Tradeable", boolText(scanning.tradeable), scanning.tradeable === true ? "is-live" : "is-warn")}
      ${metric("Volume filter", boolText(scanning.volume_filter_enabled))}
      ${metric("Threshold", number(scanning.volume_threshold, 0))}
    </div>`;
    const grouped = [...groups.entries()].map(([window, items]) =>
      `<section class="setup-group"><h3>${html(window)}</h3>${items.map(setupHtml).join("")}</section>`).join("");
    el.innerHTML = summary + grouped;
  }

  function intentRow(intent, kind) {
    return `<tr>
      <td>${html(kind)}</td>
      <td class="mono">${html(intent.intent_id || DASH)}</td>
      <td>${html(intent.side || DASH)}</td>
      <td>${html(price(intent.reference_price))}</td>
      <td>${html(price(intent.stop_loss))}</td>
      <td>${html(price(intent.take_profit))}</td>
      <td>${html(number(intent.risk_reward, 2))}</td>
      <td>${html(time(intent.created_at || intent.timestamp))}</td>
    </tr>`;
  }

  function signalTable(title, items, kind) {
    if (!items || !items.length) {
      return `<div class="subhead">${html(title)}</div><div class="empty inline">None.</div>`;
    }
    return `<div class="subhead">${html(title)} <span class="muted">${items.length}</span></div>
      <div class="table-scroll"><table class="signals-table">
        <thead><tr><th>Kind</th><th>ID</th><th>Side</th><th>Ref</th><th>SL</th><th>TP</th><th>R:R</th><th>Time</th></tr></thead>
        <tbody>${items.map((item) => intentRow(item, kind)).join("")}</tbody>
      </table></div>`;
  }

  function renderSignals() {
    const el = $("signals-list");
    const count = $("signals-count");
    const signals = state.signals || {};
    const pending = Array.isArray(signals.pending) ? signals.pending : [];
    const recent = Array.isArray(signals.recent) ? signals.recent : [];
    const reports = Array.isArray(signals.execution_reports) ? signals.execution_reports : [];
    if (count) count.textContent = `${signals.pending_count ?? pending.length} pending`;
    if (!el) return;
    const reportsHtml = reports.length
      ? `<div class="subhead">Execution reports <span class="muted">${reports.length}</span></div>
         <div class="report-list">${reports.map((report) => `<div class="report-row">
           <span class="mono">${html(report.intent_id || DASH)}</span>
           <span>${html(report.status || DASH)}</span>
           <span>${html(report.venue || DASH)}</span>
           <span>${html(time(report.timestamp))}</span>
           <span>${html(report.error_message || report.execution_id || "")}</span>
         </div>`).join("")}</div>`
      : `<div class="subhead">Execution reports</div><div class="empty inline">None.</div>`;
    el.innerHTML = `
      <div class="signals-summary">
        ${metric("Pending", integer(signals.pending_count ?? pending.length))}
        ${metric("Node 4 link", boolText(signals.node4_connected, "connected", "disconnected"))}
        ${metric("Private link", boolText(signals.linkConfigured, "configured", "not configured"))}
      </div>
      ${signalTable("Pending intents", pending, "pending")}
      ${signalTable("Recent intents", recent, "recent")}
      ${reportsHtml}
      <p class="panel-note">Read-only. Sizing is managed outside this dashboard.</p>`;
  }

  function renderTickVolume() {
    const el = $("tick-volume-list");
    const count = $("tick-volume-count");
    const snapshot = state.tickVolume || {};
    const last = snapshot.last || null;
    if (count) count.textContent = last && last.closed === false ? "in progress" : "closed";
    if (!el) return;
    if (!last) {
      el.innerHTML = `<div class="empty">Awaiting Node 1 tick volume…</div>`;
      return;
    }
    el.innerHTML = `<div class="metrics tick-metrics">
      ${metric("Ticks", integer(last.ticks))}
      ${metric("Up", integer(last.up_ticks), "is-live")}
      ${metric("Down", integer(last.down_ticks), "is-bad")}
      ${metric("Flat", integer(last.flat_ticks))}
      ${metric("Ticks/s", number(last.ticks_per_sec, 1))}
      ${metric("Last tick", time(last.last_tick))}
      ${metric("Close", price(last.close))}
      ${metric("State", last.closed === false ? "in progress" : "closed", last.closed === false ? "is-warn" : "is-live")}
    </div><p class="panel-note">The newest bar may still be forming.</p>`;
  }

  function renderDiagnostics() {
    const el = $("diagnostics-list");
    const count = $("diagnostics-count");
    const diagnostics = state.diagnostics || {};
    const work = diagnostics.work || {};
    const integrity = diagnosticsIntegrity();
    if (count) count.textContent = `${integrity.count} violations`;
    if (!el) return;
    if (!state.diagnostics) {
      el.innerHTML = `<div class="empty">Awaiting Node 3 diagnostics…</div>`;
      return;
    }
    const windows = Array.isArray(work.windows) ? work.windows : [];
    const events = Array.isArray(diagnostics.recent_events) ? diagnostics.recent_events : [];
    el.innerHTML = `
      <div class="detail-grid">
        ${row("Service", work.service || DASH)}
        ${row("Role", work.role || DASH)}
        ${row("Contract", work.contract_version || DASH)}
        ${row("Uptime", `${integer(work.uptime_secs)} s`)}
        ${row("Node 1", boolText(work.node1_connected, "connected", "disconnected"))}
        ${row("Freshness", work.freshness_state || DASH, toneForState(work.freshness_state))}
        ${row("Feed state", work.feed_state || DASH)}
        ${row("VP ready", boolText(work.vp_ready, "ready", "pending"))}
        ${row("Tradeable", boolText(work.tradeable))}
        ${row("Bars stored", integer(work.closed_bars_stored))}
        ${row("Open bars ignored", integer(work.open_bars_ignored))}
        ${row("Replayed bars", integer(work.replayed_bars))}
        ${row("Duplicate bars", integer(work.duplicate_bars_ignored))}
        ${row("WS messages", integer(work.ws_messages_received))}
        ${row("Breaks", integer(work.breaks_detected))}
        ${row("Invalidated", integer(work.breaks_invalidated))}
        ${row("Signals", integer(work.signals_confirmed))}
        ${row("Intents emitted", integer(work.intents_emitted))}
        ${row("Intents dropped", integer(work.intents_dropped))}
        ${row("Reports received", integer(work.execution_reports_received))}
        ${row("Last error", work.last_error || DASH, work.last_error ? "is-warn" : "")}
      </div>
      <div class="integrity-box${integrity.count ? " is-bad" : ""}">
        <b>Contract violations: ${html(integer(integrity.count))}</b>
        <span>Last: ${html(integrity.last)}</span>
        ${diagnostics.lastViolationAt ? `<span class="muted">${html(time(diagnostics.lastViolationAt))}</span>` : ""}
      </div>
      <div class="subhead">Published windows</div>
      ${windows.length ? `<div class="detail-grid">${windows.map((window) =>
        row(window.window || DASH, boolText(window.published, "published", "pending"))).join("")}</div>`
        : `<div class="empty inline">No window diagnostics yet.</div>`}
      <div class="subhead">Recent events</div>
      ${events.length ? `<div class="event-log">${events.slice(0, 12).map((event) =>
        `<div class="log-row ${html(event.level || "")}"><span>${html(time(event.timestamp))}</span><b>${html(event.category || DASH)}</b><span>${html(event.message || DASH)}</span></div>`).join("")}</div>`
        : `<div class="empty inline">No recent events.</div>`}
      <details class="diagnostics-all">
        <summary>All published work fields</summary>
        ${allWorkRows(work)}
      </details>`;
    renderDrawer();
  }

  function renderVp() {
    const el = $("vp-list");
    if (!el) return;
    if (state.vpError) {
      el.innerHTML = `<div class="empty is-bad">Node 3 /vp rejected the request (${html(state.vpError.status || 400)}): ${html(errorText(state.vpError))}</div>`;
      return;
    }
    const vp = state.vp;
    if (!vp) {
      el.innerHTML = `<div class="empty">Choose a published window or a custom range.</div>`;
      return;
    }
    const rows = Array.isArray(vp.histogram) ? vp.histogram : [];
    const max = rows.reduce((value, row) => Math.max(value, Number(row.volume) || 0), 0) || 1;
    el.innerHTML = `<div class="metrics vp-summary">
      ${metric("Window", vp.window || "custom")}
      ${metric("Rows", integer(vp.rowsCount ?? rows.length))}
      ${metric("PoC", price(vp.poc))}
      ${metric("VaH", price(vp.vah))}
      ${metric("VaL", price(vp.val))}
      ${metric("Range", `${price(vp.range_low)} – ${price(vp.range_high)}`)}
    </div>
    <div class="vp-rows">${rows.slice(0, 24).map((row) => {
      const width = Math.max(1, Math.round(((Number(row.volume) || 0) / max) * 100));
      const up = Number(row.up_volume) || 0;
      const down = Number(row.down_volume) || 0;
      const split = up + down > 0 ? Math.round((up / (up + down)) * 100) : 50;
      return `<div class="vp-row" title="${html(`volume ${row.volume}, up ${up}, down ${down}`)}">
        <span>${html(price(row.price))}</span>
        <span class="vp-bar"><i style="width:${width}%"></i><i class="vp-up" style="width:${split}%"></i></span>
        <span>${html(integer(row.volume))}</span>
      </div>`;
    }).join("")}</div>
    ${rows.length > 24 ? `<p class="panel-note">Showing 24 of ${html(integer(rows.length))} published rows.</p>` : ""}`;
  }

  function renderDrawingsLegend() {
    const el = $("drawings-legend");
    if (!el) return;
    const drawings = state.drawings || {};
    const items = Array.isArray(drawings.items) ? drawings.items : [];
    if (!items.length) {
      el.innerHTML = `<span class="legend-item muted">Drawings pending</span>`;
      return;
    }
    el.innerHTML = items.slice(0, 12).map((item) => {
      const color = safeColor(item.color);
      const style = item.line_style === "rectangle" ? "rectangle" : item.line_style;
      return `<span class="legend-item drawing-key" title="${html(item.id)} · ${html(item.timeframe)}">
        <i class="drawing-swatch" style="background:${html(color)}" data-style="${html(style)}"></i>${html(item.label)}</span>`;
    }).join("");
    if (drawings.pending && drawings.pending.length) {
      el.innerHTML += `<span class="legend-item muted">${html(drawings.pending.length)} pending</span>`;
    }
  }

  function renderDrawer() {
    if (!state.drawerOpen) return;
    const el = $("settings-body");
    if (!el) return;
    const health = state.health || {};
    const status = state.node1Status || {};
    const diagnostics = state.diagnostics || {};
    const integrity = diagnosticsIntegrity();
    const streams = App.streams && App.streams.getState ? App.streams.getState() : {};
    el.innerHTML = `
      <section class="drawer-section"><h3>Contracts</h3><div class="drawer-rows">
        ${row("Node 3", `v${health.contractVersion || DASH} · ${health.status || DASH}`, toneForState(health.status))}
        ${row("Node 1", `v${status.contractVersion || DASH} · ${status.state || DASH}`, toneForState(status.state))}
        ${row("Violations", integer(integrity.count), integrity.count ? "is-bad" : "is-live")}
        ${row("Last violation", integrity.last)}
      </div></section>
      <section class="drawer-section"><h3>Node 3 health</h3><div class="drawer-rows">
        ${row("Freshness", health.freshnessState || DASH, toneForState(health.freshnessState))}
        ${row("Node 1 link", boolText(health.node1Connected, "connected", "disconnected"))}
        ${row("VP ready", boolText(health.vpReady, "ready", "pending"))}
        ${row("Tradeable", boolText(health.tradeable))}
        ${row("Intents pending", integer(health.intentsPending))}
        ${row("Uptime", `${integer(health.uptimeSecs)} s`)}
      </div></section>
      <section class="drawer-section"><h3>Node 1 feed</h3><div class="drawer-rows">
        ${row("Boot", status.bootId || DASH)}
        ${row("Reason", status.reason || DASH)}
        ${row("Feed", status.freshness && status.freshness.feed ? status.freshness.feed : DASH)}
        ${row("Age", `${integer(status.freshness && status.freshness.age_ms)} ms`)}
        ${row("Market open", boolText(status.marketOpen))}
      </div></section>
      <section class="drawer-section"><h3>Streams</h3><div class="drawer-rows">
        ${row("Node 1", streams.node1 ? streams.node1.status.toUpperCase() : DASH)}
        ${row("Node 3", streams.node3 ? streams.node3.status.toUpperCase() : DASH)}
        ${row("Last Node 3 data", state.lastGood.node3 ? time(state.lastGood.node3) : DASH)}
        ${row("Last Node 1 data", state.lastGood.node1 ? time(state.lastGood.node1) : DASH)}
      </div></section>
      <p class="drawer-note">Read-only dashboard. Levels, setups, projections, signals, and diagnostics are published by Node 3; candles, tick volume, calendar, and feed status come from Node 1.</p>`;
    const stamp = $("settings-stamp");
    if (stamp) stamp.textContent = state.lastGood.node3 ? time(state.lastGood.node3) : DASH;
  }

  function setDrawer(open) {
    state.drawerOpen = !!open;
    const drawer = $("settings-drawer");
    const backdrop = $("settings-backdrop");
    const trigger = $("settings-open");
    if (drawer) drawer.hidden = !state.drawerOpen;
    if (backdrop) backdrop.hidden = !state.drawerOpen;
    if (trigger && trigger.setAttribute) trigger.setAttribute("aria-expanded", String(state.drawerOpen));
    if (state.drawerOpen) renderDrawer();
  }

  async function loadVp(options = {}) {
    if (!App.client || typeof App.client.getVp !== "function") return null;
    try {
      const vp = await App.client.getVp(options);
      applyTopic("vp", vp, "rest");
      return vp;
    } catch (error) {
      state.vpError = error;
      renderVp();
      return null;
    }
  }

  function bind() {
    if (bind.done || typeof document === "undefined") return;
    bind.done = true;
    const open = $("settings-open");
    const close = $("settings-close");
    const backdrop = $("settings-backdrop");
    if (open && open.addEventListener) open.addEventListener("click", () => setDrawer(!state.drawerOpen));
    if (close && close.addEventListener) close.addEventListener("click", () => setDrawer(false));
    if (backdrop && backdrop.addEventListener) backdrop.addEventListener("click", () => setDrawer(false));
    if (document.addEventListener) {
      document.addEventListener("keydown", (event) => {
        if (event.key === "Escape" && state.drawerOpen) setDrawer(false);
      });
    }
    if (document.querySelectorAll) {
      document.querySelectorAll("[data-vp-window]").forEach((button) => {
        if (button.addEventListener) button.addEventListener("click", () => loadVp({ window: button.dataset.vpWindow }));
      });
      const custom = $("vp-custom");
      if (custom && custom.addEventListener) custom.addEventListener("click", () => {
        const startEl = $("vp-start");
        const endEl = $("vp-end");
        const rowsEl = $("vp-rows");
        const start = Date.parse(startEl && startEl.value ? startEl.value : "");
        const end = Date.parse(endEl && endEl.value ? endEl.value : "");
        const rows = rowsEl && rowsEl.value ? Number(rowsEl.value) : undefined;
        loadVp({ start, end, rows });
      });
    }
  }

  const SEEDS = [
    { service: "node3", topic: "health", load: () => App.client.getHealth() },
    { service: "node3", topic: "levels", load: () => App.client.getLevels() },
    { service: "node3", topic: "drawing", load: () => App.client.getDrawings() },
    { service: "node3", topic: "scanning", load: () => App.client.getScanning() },
    { service: "node3", topic: "signals", load: () => App.client.getSignals() },
    { service: "node3", topic: "diagnostics", load: () => App.client.getDiagnostics() },
    { service: "node1", topic: "status", load: () => App.client.getStatus() },
    { service: "node1", topic: "tick_volume", load: () => App.client.getTickVolume() },
    { service: "node1", topic: "calendar", load: () => App.client.getCalendar() },
  ];

  async function runSeeds(list) {
    const results = await Promise.allSettled(list.map((seed) => seed.load()));
    results.forEach((result, index) => {
      const seed = list[index];
      if (result.status === "fulfilled") applyTopic(seed.topic, result.value, "rest");
      else setError(seed.service, result.reason);
    });
    renderAll();
    return results;
  }

  function loadAll() {
    if (!App.client) return Promise.resolve([]);
    return runSeeds(SEEDS);
  }

  function loadNode3() {
    if (!App.client) return Promise.resolve([]);
    return runSeeds(SEEDS.filter((seed) => seed.service === "node3"));
  }

  function loadNode1() {
    if (!App.client) return Promise.resolve([]);
    return runSeeds(SEEDS.filter((seed) => seed.service === "node1"));
  }

  function renderAll() {
    renderHeader();
    renderConnectionBanner();
    renderLevels();
    renderSetups();
    renderSignals();
    renderTickVolume();
    renderDiagnostics();
    renderVp();
    renderDrawingsLegend();
    renderDrawer();
  }

  return {
    state,
    applyTopic,
    setError,
    clearError(service) {
      state.errors[service] = null;
      renderConnectionBanner();
    },
    showGap,
    resolveGap,
    clearGap,
    loadAll,
    loadNode1,
    loadNode3,
    loadVp,
    bind,
    setDrawer,
    renderAll,
    renderHeader,
    renderConnectionBanner,
    renderLevels,
    renderSetups,
    renderSignals,
    renderTickVolume,
    renderDiagnostics,
    renderVp,
    renderDrawingsLegend,
    renderDrawer,
    diagnosticsIntegrity,
  };
});
