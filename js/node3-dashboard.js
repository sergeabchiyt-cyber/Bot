/* Node 3 REST + public WebSocket state, rendered with the original dashboard panels. */
(function (App) {
  "use strict";
  const { $, escapeHtml } = App.utils;
  const state = { health: null, levels: [], drawings: [], scanning: null, signals: null, diagnostics: null, vp: null, lastGood: {}, errors: {}, node1Errors: {}, gap: false };
  const fmt = (v, digits = 2) => v == null || !Number.isFinite(Number(v)) ? "—" : Number(v).toLocaleString(undefined, { maximumFractionDigits: digits, minimumFractionDigits: digits });
  const stamp = (t) => t == null ? "—" : App.format.time(typeof t === "number" && t > 1e12 ? t : Number(t) * 1000);
  const safe = (v) => escapeHtml(v == null ? "—" : v);
  function good(key) { state.lastGood[key] = Date.now(); delete state.errors[key]; }
  function warning() {
    const h = state.health;
    const violations = Number(h && h.contract_violations) || 0;
    const diag = state.diagnostics || {};
    const last = diag.last_violation || diag.last_contract_violation || diag.contract_violation_text || (Array.isArray(diag.violations) ? diag.violations.at(-1) : null);
    const messages = [];
    if (state.errors.health || !h) messages.push(`Node 3 unreachable · last good ${state.lastGood.health ? stamp(state.lastGood.health) : "—"}`);
    else {
      if (h.status === "disconnected") messages.push("Node 3 reports disconnected");
      if (!h.node1_connected) messages.push("Node 3 reports Node 1 disconnected");
      if (["stale", "offline"].includes(String(h.freshness_state).toLowerCase())) messages.push(`Node 3 freshness ${h.freshness_state}`);
    }
    const n1 = App.status && App.status.getState ? App.status.getState() : null;
    if (n1 && n1.error) messages.push(`Node 1 status unavailable · last good ${n1.stamp ? stamp(n1.stamp) : "—"} · check Node 1 CORS configuration`);
    for (const [key,message] of Object.entries(state.node1Errors)) messages.push(`Node 1 ${key} unavailable · ${message} · check Node 1 CORS configuration`);
    if (n1 && n1.node1 && ["stale", "offline", "closed_market"].includes(n1.node1.freshness && n1.node1.freshness.state)) messages.push(`Node 1 freshness ${n1.node1.freshness.state}`);
    if (violations) messages.push(`Contract violations ${violations}${last ? ` · ${typeof last === "object" ? JSON.stringify(last) : last}` : ""}`);
    for (const key of ["levels","drawings","scanning","signals","diagnostics"]) if (state.errors[key]) messages.push(`Node 3 ${key} unavailable · last good ${state.lastGood[key] ? stamp(state.lastGood[key]) : "—"}`);
    if (state.gap) messages.push("Data incomplete — refetching Node 1 candles");
    const el = $("connection-banner");
    if (!el) return;
    el.hidden = messages.length === 0;
    el.textContent = messages.join(" · ");
  }
  function renderHealth() {
    const h = state.health;
    const badge = $("node3-badge"), label = $("node3-status");
    if (badge) { badge.dataset.state = state.errors.health || !h || h.status === "disconnected" ? "disconnected" : h.status === "ok" ? "live" : "stale"; badge.title = state.errors.health || `Node 3 ${h && h.status || "unreachable"}`; }
    if (label) label.textContent = state.errors.health ? "NODE 3 UNREACHABLE" : `NODE 3 ${String(h && h.status || "—").toUpperCase()}${h && h.contract_version ? ` · ${h.contract_version}` : ""}`;
    $("node3-contract").textContent = `N3 contract ${h && h.contract_version || "—"}`;
    $("node3-summary-state").textContent = h ? `${h.tradeable ? "TRADEABLE" : "MONITOR"} · ${h.freshness_state || "—"}` : "UNREACHABLE";
    warning();
  }
  function renderSummary() {
    const s = state.scanning;
    if (!s) return;
    $("scan-summary").innerHTML = [["ATR", s.atr], ["ATR pips", s.atr_pips], ["SL pips", s.sl_pips], ["TP pips", s.tp_pips], ["RR", s.rr]]
      .map(([k,v]) => `<div class="demo-stat"><dt>${k}</dt><dd class="tnum">${fmt(v, k === "RR" ? 2 : 1)}</dd></div>`).join("");
  }
  function projection(p, side) {
    if (!p) return "";
    return `<div class="n3-projection"><b>Projected ${safe(side)} · display only</b><span>Entry ${fmt(p.entry)} · SL ${fmt(p.stop_loss)} · TP ${fmt(p.take_profit)}</span><span>SL ${fmt(p.sl_pips,1)} pips · TP ${fmt(p.tp_pips,1)} pips · RR ${fmt(p.risk_reward)}</span><small>${safe(p.sizing || "node4_managed")} · Node 4 sizes this</small></div>`;
  }
  function renderScanning() {
    const s = state.scanning; if (!s) return;
    renderSummary();
    $("scanner-state").textContent = `${s.setups.length} setups`;
    const groups = ["PW", "PS", "CW"].map((window) => {
      const items = s.setups.filter((setup) => setup.window === window);
      if (!items.length) return "";
      return `<div class="n3-group"><b>${window}</b>${items.map((x) => {
        const active = x.active_projection;
        const project = active ? projection(active, active.side) : `${projection(x.projected_buy, "buy")}${projection(x.projected_sell, "sell")}`;
        return `<article class="n3-setup"><div class="n3-setup-head"><strong>${safe(x.name || x.id)}</strong><span class="feed-chip">${safe(x.state_label || x.state)}</span></div><div class="n3-facts"><span>Side <b>${safe(x.pending_side)}</b></span><span>Level <b>${fmt(x.level_price)}</b></span><span>Distance <b>$${fmt(x.distance_dollars)} / ${fmt(x.distance_pips,1)} pips</b></span><span>Retest <b>${fmt(x.retest_zone_low)} – ${fmt(x.retest_zone_high)}</b></span><span>Invalidation <b>${fmt(x.invalidation_price)}</b></span><span>Volume <b>${x.volume_confirmed ? "confirmed" : x.volume_required ? "required" : "not required"}</b></span></div>${project}${x.last_note ? `<small>${safe(x.last_note)}</small>` : ""}</article>`;
      }).join("")}</div>`;
    }).join("");
    $("setups-body").innerHTML = groups || `<div class="empty">No published setups · last good ${stamp(state.lastGood.scanning)}</div>`;
  }
  function rows(list, kind) {
    if (!list || !list.length) return `<div class="empty inline">No ${kind}.</div>`;
    return list.map((x) => `<div class="drawer-row"><span class="drawer-key">${safe(x.name || x.id || x.intent_id || x.signal_id || x.type || kind)}${x.side ? ` · ${safe(x.side)}` : ""}</span><b class="drawer-val">${safe(x.state_label || x.state || x.status || x.report || "—")}</b></div>`).join("");
  }
  function renderSignals() {
    const x = state.signals; if (!x) return;
    $("signals-count").textContent = `${x.pending_count ?? x.pending.length} pending`;
    $("signals-body").innerHTML = `<h4>Pending</h4>${rows(x.pending,"pending intents")}<h4>Recent</h4>${rows(x.recent,"recent intents")}<h4>Execution reports · read only</h4>${rows(x.execution_reports,"reports")}`;
  }
  function renderDiagnostics() {
    const h = state.health, d = state.diagnostics || {};
    if (!h && !state.diagnostics) return;
    const last = d.last_violation || d.last_contract_violation || d.contract_violation_text || (Array.isArray(d.violations) ? d.violations.at(-1) : null);
    const fields = [["Health",h && h.status],["Freshness",h && h.freshness_state],["Tradeable",h && (h.tradeable ? "yes" : "no")],["Node 1",h && (h.node1_connected ? "connected" : "disconnected")],["VP ready",h && (h.vp_ready ? "yes" : "no")],["Contract",h && h.contract_version],["Violations",h && h.contract_violations],["Pending",h && h.intents_pending],["Uptime",h && `${fmt(h.uptime_secs,0)} s`],["Last violation",last && (typeof last === "object" ? JSON.stringify(last) : last)]];
    $("diagnostics-state").textContent = h ? `${h.contract_violations || 0} violations` : "—";
    $("diagnostics-body").innerHTML = `<div class="drawer-rows">${fields.map(([k,v]) => `<div class="drawer-row"><span class="drawer-key">${safe(k)}</span><b class="drawer-val">${safe(v)}</b></div>`).join("")}</div>${state.diagnostics ? `<details class="n3-diagnostic-details"><summary>Full diagnostics snapshot</summary><pre>${safe(JSON.stringify(state.diagnostics,null,2))}</pre></details>` : ""}`;
    renderHealth();
  }
  function renderLegend() {
    const seen = new Set();
    for (const item of state.drawings) {
      if (item.kind !== "level" || !item.label) continue;
      const key = `${item.label}|${item.timeframe || ""}`;
      if (seen.has(key)) continue; seen.add(key);
      document.querySelectorAll(".legend-item").forEach((legend) => {
        if (legend.textContent.trim() !== item.label) return;
        const swatch = legend.querySelector(".swatch");
        if (swatch) { swatch.style.backgroundColor = item.color; swatch.style.borderColor = item.color; swatch.style.borderStyle = item.line_style === "dotted" ? "dotted" : "solid"; }
      });
    }
  }
  function apply(topic, data) {
    if (topic === "levels") { state.levels = window.Node2Client.parseLevels(data); App.levels.apply(state.levels); good("levels"); }
    else if (topic === "drawing") { state.drawings = window.Node2Client.parseDrawings(data); App.chart.renderDrawings(state.drawings); renderLegend(); good("drawings"); }
    else if (topic === "scanning") { state.scanning = window.Node2Client.parseScanning(data); renderScanning(); good("scanning"); }
    else if (topic === "signals") { state.signals = window.Node2Client.parseSignals(data); renderSignals(); good("signals"); }
    else if (topic === "diagnostics") { state.diagnostics = window.Node2Client.parseDiagnostics(data); renderDiagnostics(); good("diagnostics"); }
    else if (topic === "vp") { state.vp = window.Node2Client.parseVp(data); good("vp"); }
    warning();
  }
  async function refresh(key, fn, handler) {
    try { handler(await fn()); state.lastGood[key] = Date.now(); delete state.errors[key]; }
    catch (error) { state.errors[key] = error.message; console.warn(`Node 3 ${key} unavailable`, error); }
    renderHealth();
  }
  async function loadAll() {
    refresh("health", () => App.api.fetchHealth(), (data) => { state.health = data; renderHealth(); renderDiagnostics(); });
    refresh("levels", () => App.api.fetchLevels(), (data) => { state.levels = data; App.levels.apply(data); });
    refresh("drawings", () => App.api.fetchDrawings(), (data) => { state.drawings = data; App.chart.renderDrawings(data); renderLegend(); });
    refresh("scanning", () => App.api.fetchScanning(), (data) => { state.scanning = data; renderScanning(); });
    refresh("signals", () => App.api.fetchSignals(), (data) => { state.signals = data; renderSignals(); });
    refresh("diagnostics", () => App.api.fetchDiagnostics(), (data) => { state.diagnostics = data; renderDiagnostics(); });
  }
  let ws, reconnect;
  function connect() {
    const url = App.config.client.websocketUrl("node3");
    if (!url || typeof WebSocket !== "function") return;
    try { ws = new WebSocket(url); } catch { reconnect = setTimeout(connect, 3000); return; }
    ws.addEventListener("open", () => { ws.send(JSON.stringify({ type: "subscribe", topics: ["levels","drawing","vp","scanning","signals","diagnostics"] })); });
    ws.addEventListener("message", (event) => {
      let frame; try { frame = JSON.parse(event.data); } catch { return; }
      if (frame.type === "heartbeat") return;
      if (frame.type === "gap") { warning(); return; }
      if (!["levels","drawing","vp","scanning","signals","diagnostics"].includes(frame.type)) return;
      try { apply(frame.type, Object.prototype.hasOwnProperty.call(frame,"data") ? frame.data : frame); } catch (error) { console.warn("Node 3 frame rejected", error); }
    });
    ws.addEventListener("close", () => { reconnect = setTimeout(connect, 3000); });
    ws.addEventListener("error", () => { try { ws.close(); } catch {} });
  }
  function start() { loadAll(); connect(); setInterval(loadAll, 30000); setInterval(warning, 5000); setInterval(() => { if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ type: "heartbeat" })); }, 30000); }
  function gap() { state.gap = true; warning(); }
  function clearGap() { state.gap = false; warning(); }
  function reportNode1Error(key, message) { state.node1Errors[key] = message || "request failed"; warning(); }
  function clearNode1Error(key) { delete state.node1Errors[key]; warning(); }
  App.node3 = { start, apply, gap, clearGap, reportNode1Error, clearNode1Error, updateWarnings: warning, getState: () => ({ ...state }) };
})((window.App = window.App || {}));
