/* ============================================================
 * XAUUSD Terminal — bootstrap.
 *
 * One Node 1 WebSocket feeds every panel. On mount the REST snapshots
 * (`/strategy`, `/demo`, `/status`) seed the UI; while the socket is down a
 * 5 s REST poll keeps strategy + demo fresh and the header shows DELAYED.
 * Nothing on this page can place, modify or close an order.
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, setStatus } = App.utils;
  const cfg = App.config;

  App.runtime = { polling: false };

  // ---------- Frame routing ----------

  App.socket.on("levels", (data) => { if (App.levels) App.levels.apply(data); });
  App.socket.on("candle", (data) => { if (App.chart) App.chart.updateCandle(data); });
  App.socket.on("tick_volume", (data) => { if (App.chart) App.chart.updateTickVolume(data); });
  App.socket.on("calendar", (data) => { if (App.calendar) App.calendar.ingest(data); });
  App.socket.on("strategy", (data) => { if (App.strategy) App.strategy.applyDecision(data, "ws"); });
  App.socket.on("demo", (data) => { if (App.demo) App.demo.applySnapshot(data, "ws"); });
  App.socket.on("status", (data) => { if (App.status) App.status.ingest(data, "ws"); });

  // ---------- Polling fallback ----------

  let pollTimer = null;
  let pollGraceTimer = null;
  let lastStreamKey = null;

  function paintDelayed(active) {
    const badge = $("delayed-badge");
    if (!badge) return;
    badge.hidden = !active;
    badge.title = "Socket unavailable \u2014 /strategy and /demo refresh from REST every 5 s.";
  }

  function stopPolling() {
    if (pollGraceTimer !== null) { clearTimeout(pollGraceTimer); pollGraceTimer = null; }
    if (pollTimer !== null) { clearInterval(pollTimer); pollTimer = null; }
    App.runtime.polling = false;
    paintDelayed(false);
  }

  function startPolling() {
    if (pollTimer !== null) return;
    App.runtime.polling = true;
    paintDelayed(true);
    pollTimer = setInterval(() => {
      if (App.strategy) App.strategy.poll();
      if (App.demo) App.demo.poll();
    }, cfg.socket.pollFallbackMs);
  }

  function handleSocketState(state) {
    // publish() runs for every frame; only react when the state actually
    // changes so the aria-live line is not repeated every few seconds.
    const key = `${state.status}|${state.connected}|${state.endpointConfigured}`;
    if (key === lastStreamKey) return;
    lastStreamKey = key;

    if (state.connected) {
      stopPolling();
      setStatus("stream connected", "live");
      return;
    }
    setStatus(`stream ${state.status}`, "busy");
    if (!state.endpointConfigured) {
      stopPolling();
      return;
    }
    if (pollTimer !== null || pollGraceTimer !== null) return;
    // Brief grace so a normal first connect never flashes DELAYED.
    pollGraceTimer = setTimeout(() => {
      pollGraceTimer = null;
      startPolling();
      if (App.strategy) App.strategy.tick();
      if (App.demo) App.demo.load();
    }, 1500);
  }

  App.socket.onState(handleSocketState);

  // ---------- Boot ----------

  setStatus("dashboard starting", "busy");
  if (App.calendar) App.calendar.start();
  if (App.strategy) App.strategy.start();
  if (App.demo) App.demo.start();
  if (App.status) App.status.start();

  // Mount-time REST seeds run before the first socket frame.
  App.socket.start();
  if (App.chart) App.chart.loadHistory();
  if (App.levels) App.levels.fetchAll();

  if (cfg.levelsRefreshMs > 0 && App.levels) {
    setInterval(() => App.levels.fetchAll(), cfg.levelsRefreshMs);
  }
  if (cfg.candlesRefreshMs > 0 && App.chart) {
    setInterval(() => App.chart.loadHistory(), cfg.candlesRefreshMs);
  }
})((window.App = window.App || {}));
