/* ============================================================
 * XAUUSD Terminal — dashboard bootstrap.
 *
 * One typed client owns every request. REST seeds paint the panels first;
 * Node 1 and Node 3 public sockets then replay and replace whole topics.
 * A disconnected stream uses a clearly labelled REST poll instead of leaving
 * silent stale data. There is no order entry or broker/account UI.
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, setStatus, setBadge } = App.utils;
  const config = App.config;
  const pollFallbackMs = (config.stream && config.stream.pollFallbackMs) || 5000;
  const refreshMs = config.refreshMs || 30000;

  App.runtime = {
    polling: { node1: false, node3: false },
    startedAt: Date.now(),
  };

  const pollTimers = { node1: null, node3: null };
  const graceTimers = { node1: null, node3: null };

  function paintDelayed() {
    const active = App.runtime.polling.node1 || App.runtime.polling.node3;
    const badge = $("delayed-badge");
    if (!badge) return;
    badge.hidden = !active;
    badge.title = "A public stream is unavailable — read-only REST polling is delayed.";
  }

  function stopPolling(service) {
    if (graceTimers[service] !== null) {
      clearTimeout(graceTimers[service]);
      graceTimers[service] = null;
    }
    if (pollTimers[service] !== null) {
      clearInterval(pollTimers[service]);
      pollTimers[service] = null;
    }
    App.runtime.polling[service] = false;
    paintDelayed();
  }

  function startPolling(service) {
    if (pollTimers[service] !== null) return;
    App.runtime.polling[service] = true;
    paintDelayed();
    const load = service === "node3" ? () => App.panels.loadNode3() : () => App.panels.loadNode1();
    load();
    pollTimers[service] = setInterval(load, pollFallbackMs);
  }

  function handleStreamState(service, streamState) {
    if (!streamState) return;
    if (streamState.connected) {
      stopPolling(service);
      if (service === "node1") setStatus("market stream connected", "live");
      return;
    }
    if (!streamState.endpointConfigured) {
      stopPolling(service);
      return;
    }
    if (pollTimers[service] !== null || graceTimers[service] !== null) return;
    graceTimers[service] = setTimeout(() => {
      graceTimers[service] = null;
      startPolling(service);
    }, 1500);
  }

  function bindStatusDrawer() {
    if (App.panels && typeof App.panels.bind === "function") App.panels.bind();
  }

  function bindPeriodicRefresh() {
    setInterval(() => {
      App.client.getHealth()
        .then((health) => App.panels.applyTopic("health", health, "rest"))
        .catch((error) => App.panels.setError("node3", error));
      App.client.getStatus()
        .then((status) => App.panels.applyTopic("status", status, "rest"))
        .catch((error) => App.panels.setError("node1", error));
    }, refreshMs);
  }

  async function boot() {
    setStatus("dashboard starting", "busy");
    setBadge("node3-stream-badge", "NODE 3 STREAM CONNECTING", "stale", "Waiting for the Node 3 public stream");
    bindStatusDrawer();
    if (App.chartControls) App.chartControls.bind();
    if (App.calendar) App.calendar.start({ initial: false });
    App.streams.onState(handleStreamState);
    App.panels.renderAll();

    // The initial GETs keep the chart and panels populated even if CORS or a
    // WebSocket is unavailable. Failures retain prior DOM and show a banner.
    await App.panels.loadAll();
    if (App.chart && typeof App.chart.loadHistory === "function") App.chart.loadHistory();
    App.panels.loadVp({ window: "PW" });

    App.streams.start();
    bindPeriodicRefresh();
    setStatus("dashboard ready", "live");
  }

  if (typeof window !== "undefined" && window.addEventListener) {
    window.addEventListener("beforeunload", () => {
      if (App.streams && typeof App.streams.stop === "function") App.streams.stop();
    });
  }

  boot().catch((error) => {
    console.error("Dashboard boot failed:", error);
    setStatus("dashboard boot error", "error");
    App.panels.setError("node3", error);
  });
})((window.App = window.App || {}));
