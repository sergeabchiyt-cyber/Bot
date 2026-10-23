/* ============================================================
 * XAUUSD Terminal — read-only status & settings drawer.
 *
 * Surfaces `GET /status` (and the `status` WS frame): connection truth,
 * the strategy / demo blocks, `/strategy` and `/demo` parameters, and feed
 * liveness. Every row is read-only — the dashboard has no controls to send.
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, escapeHtml } = App.utils;
  const format = App.format;
  const normalise = App.normalize;
  const math = App.strategyMath;

  const html = (value) => escapeHtml(value);
  const finite = (value) => math.finite(value);

  let status = null;
  let lastFetchAt = null;
  let fetchInProgress = false;
  let open = false;

  function normaliseStatus(raw) {
    if (!normalise.isRecord(raw)) return null;
    return {
      ts: normalise.timestampMs(raw.ts ?? raw.timestamp),
      strategy: normalise.isRecord(raw.strategy) ? raw.strategy : null,
      demo: normalise.isRecord(raw.demo) ? raw.demo : null,
      feeds: normalise.isRecord(raw.feeds) ? raw.feeds : null,
    };
  }

  function ingest(raw, source = "ws") {
    const next = normaliseStatus(raw);
    if (!next) return false;
    status = next;
    if (source !== "rest") lastFetchAt = Date.now();
    if (open) render();
    return true;
  }

  async function load() {
    if (fetchInProgress) return false;
    fetchInProgress = true;
    try {
      const payload = await App.api.fetchStatus();
      ingest(payload, "rest");
      lastFetchAt = Date.now();
      return true;
    } catch (error) {
      console.warn("Status snapshot unavailable:", error);
      return false;
    } finally {
      fetchInProgress = false;
    }
  }

  function row(label, value, tone = "") {
    return `
      <div class="drawer-row">
        <span class="drawer-key">${html(label)}</span>
        <b class="drawer-val ${tone}">${html(value)}</b>
      </div>`;
  }

  function boolText(value) {
    if (value === true) return "Yes";
    if (value === false) return "No";
    return format.DASH;
  }

  function percentText(value) {
    const number = finite(value);
    // `min_confidence` and `risk_pct` are both percent-unit values here.
    return number === null ? format.DASH : format.percent(number, 1);
  }

  /** 0..1 ratio → percent text; null stays a dash (not 0.0%). */
  function ratioText(value) {
    const number = finite(value);
    return number === null ? format.DASH : format.percent(number * 100, 1);
  }

  function strategyRows() {
    const live = status && status.strategy ? status.strategy : {};
    const params = (App.strategy && App.strategy.getState().params) || {};
    const merged = Object.assign({}, params, live);
    const enabled = live.enabled !== undefined ? live.enabled : params.enabled;
    const atrPeriod = finite(merged.atr_period);
    const atrMult = finite(merged.atr_stop_mult);
    const lotsRange = finite(merged.min_lots) !== null || finite(merged.max_lots) !== null
      ? `${format.lots(merged.min_lots)} \u2013 ${format.lots(merged.max_lots)} (step ${format.lots(merged.lot_step)})`
      : format.DASH;
    return [
      row("Enabled", boolText(enabled), enabled === true ? "is-live" : enabled === false ? "is-warn" : ""),
      row("Evaluation", finite(live.eval_secs) === null ? format.DASH : `every ${format.count(live.eval_secs)} s`),
      row("Min confidence", ratioText(merged.min_confidence)),
      row("Risk per trade", percentText(merged.risk_pct)),
      row("ATR stop", atrPeriod === null && atrMult === null ? format.DASH : `${format.number(atrPeriod, 0)} period \u00d7 ${format.number(atrMult, 2)}`),
      row("Reward : risk", finite(merged.reward_risk) === null ? format.DASH : `${format.number(merged.reward_risk, 2)} : 1`),
      row("Lots", lotsRange),
      row("EMAs", finite(merged.fast_ema) === null && finite(merged.slow_ema) === null
        ? format.DASH : `${format.count(merged.fast_ema)} / ${format.count(merged.slow_ema)}`),
      row("Event blackout", finite(merged.event_blackout_secs) === null ? format.DASH : `${format.count(merged.event_blackout_secs / 60)} min`),
      row("Leverage", finite(merged.leverage) === null ? format.DASH : `1:${String(finite(merged.leverage))}`),
      row("Max margin", percentText(merged.max_margin_pct)),
    ].join("");
  }

  function demoRows() {
    const live = status && status.demo ? status.demo : {};
    const demoState = App.demo && App.demo.getState().snapshot;
    const params = demoState ? demoState.params : {};
    const enabled = live.enabled !== undefined ? live.enabled : (demoState ? demoState.enabled : undefined);
    const minLots = format.lots(live.min_lots ?? params.min_lots);
    const maxLots = format.lots(live.max_lots ?? params.max_lots);
    const step = format.lots(params.lot_step);
    const range = minLots === format.DASH && maxLots === format.DASH
      ? format.DASH
      : `${minLots} \u2013 ${maxLots} (step ${step})`;
    return [
      row("Enabled", boolText(enabled), enabled === true ? "is-live" : enabled === false ? "is-warn" : ""),
      row("Mode", live.mode ? String(live.mode).toUpperCase() : demoState ? String(demoState.mode).toUpperCase() : format.DASH),
      row("Leverage", finite(live.leverage ?? params.leverage) === null ? format.DASH : `1:${String(finite(live.leverage ?? params.leverage))}`),
      row("Spread", format.price(live.spread ?? params.spread)),
      row("Lots", range),
      row("Start balance", format.money(params.start_balance)),
      row("Stop-out", params.stop_out_pct === null || params.stop_out_pct === undefined ? format.DASH : format.percent(params.stop_out_pct)),
    ].join("");
  }

  function feedTone(label) {
    const text = String(label || "").toLowerCase();
    if (/down|offline|disconnect|error|fail|false/.test(text)) return "is-bad";
    if (/stale|degrad|delay|warn|reconnect|pending/.test(text)) return "is-warn";
    if (/live|ok|connect|up|healthy|open|ready/.test(text)) return "is-live";
    return "";
  }

  function feedRow(name, value) {
    let label = "";
    let detail = "";
    if (typeof value === "boolean") {
      label = value ? "ok" : "down";
    } else if (typeof value === "string" || typeof value === "number") {
      label = String(value);
    } else if (normalise.isRecord(value)) {
      label = value.state || value.status ||
        (value.connected === true ? "connected" : value.connected === false ? "disconnected" :
          value.ok === true ? "ok" : value.ok === false ? "down" : "");
      const ts = normalise.timestampMs(
        value.last_update ?? value.last_message_ts ?? value.lastMessageAt ?? value.ts ?? value.timestamp
      );
      if (ts !== null) detail = format.time(ts);
    }
    return `
      <div class="drawer-row">
        <span class="drawer-key">${html(name)}</span>
        <b class="drawer-val">
          ${detail ? `<span class="muted">${html(detail)} </span>` : ""}
          <span class="feed-chip ${feedTone(label)}">${html(label || format.DASH)}</span>
        </b>
      </div>`;
  }

  function feedsRows() {
    const feeds = status && status.feeds ? status.feeds : null;
    if (!feeds || !Object.keys(feeds).length) {
      return `<div class="empty inline">No feed telemetry in /status yet.</div>`;
    }
    return Object.keys(feeds).map((name) => feedRow(name, feeds[name])).join("");
  }

  function connectionRows() {
    const socket = App.socket && App.socket.getState ? App.socket.getState() : null;
    const polling = !!(App.runtime && App.runtime.polling);
    const stream = socket ? socket.status.toUpperCase() : "OFFLINE";
    const streamTone = socket && socket.status === "live" ? "is-live"
      : socket && socket.status === "stale" ? "is-warn"
        : socket && socket.status === "not-configured" ? "is-bad" : "is-warn";
    const endpoint = socket && socket.endpoint ? socket.endpoint : format.DASH;
    return [
      row("Stream", stream, streamTone),
      row("Endpoint", endpoint),
      row("Last data", socket && socket.lastDataAt ? format.time(socket.lastDataAt) : format.DASH),
      row("Frames", socket ? format.count(socket.receivedFrames) : format.DASH),
      row("Fallback", polling ? "REST poll every 5 s (delayed)" : "Idle \u2014 socket healthy",
        polling ? "is-warn" : "is-live"),
    ].join("");
  }

  function render() {
    const body = $("settings-body");
    if (!body) return;
    body.innerHTML = `
      <section class="drawer-section">
        <h3>Connection</h3>
        <div class="drawer-rows">${connectionRows()}</div>
      </section>
      <section class="drawer-section">
        <h3>Strategy</h3>
        <div class="drawer-rows">${strategyRows()}</div>
      </section>
      <section class="drawer-section">
        <h3>Demo engine</h3>
        <div class="drawer-rows">${demoRows()}</div>
      </section>
      <section class="drawer-section">
        <h3>Feeds</h3>
        <div class="drawer-rows">${feedsRows()}</div>
      </section>
      <p class="drawer-note">
        Read-only dashboard. All values come from Node 1; there is no order entry,
        no sizing input and no broker credential in this page.
      </p>`;
    const stamp = $("settings-stamp");
    if (stamp) {
      stamp.textContent = lastFetchAt === null ? format.DASH : `status ${format.time(lastFetchAt)}`;
    }
  }

  function setOpen(next) {
    open = !!next;
    const drawer = $("settings-drawer");
    const backdrop = $("settings-backdrop");
    const trigger = $("settings-open");
    if (!drawer) return;
    drawer.hidden = !open;
    if (backdrop) backdrop.hidden = !open;
    if (trigger && trigger.setAttribute) trigger.setAttribute("aria-expanded", String(open));
    if (open) {
      render();
      if (lastFetchAt === null || Date.now() - lastFetchAt > App.config.statusRefreshMs) load();
      const close = $("settings-close");
      if (close && close.focus) close.focus();
    }
  }

  function bind() {
    const trigger = $("settings-open");
    const close = $("settings-close");
    const backdrop = $("settings-backdrop");
    if (trigger) trigger.addEventListener("click", () => setOpen(!open));
    if (close) close.addEventListener("click", () => setOpen(false));
    if (backdrop) backdrop.addEventListener("click", () => setOpen(false));
    if (typeof document !== "undefined" && document.addEventListener) {
      document.addEventListener("keydown", (event) => {
        if (event.key === "Escape" && open) setOpen(false);
      });
    }
  }

  function start() {
    bind();
    load();
  }

  App.status = {
    start,
    load,
    ingest,
    render,
    isOpen: () => open,
    setOpen,
    getState: () => status,
  };
})((window.App = window.App || {}));
