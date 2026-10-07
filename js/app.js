/* ============================================================
 * XAUUSD Terminal — bootstrap and dashboard navigation.
 * Node 1 market, Node 3 strategy, and Node 4 execution start independently.
 * ============================================================ */
(function (App) {
  "use strict";

  const { setStatus } = App.utils;

  // Node 1 market frame routing. A missing chart library only disables chart
  // drawing; it does not prevent strategy, execution, or diagnostics startup.
  App.socket.on("heartbeat", () => {});
  if (App.levels) App.socket.on("levels", App.levels.apply);
  if (App.chart) {
    App.socket.on("candle", App.chart.updateCandle);
    App.socket.on("tick_volume", App.chart.updateTickVolume);
    App.socket.on("tickVolume", App.chart.updateTickVolume);
  }
  if (App.orderflow) App.socket.on("bubbles", App.orderflow.add);
  if (App.sentiment) App.socket.on("sentiment", App.sentiment.update);
  if (App.calendar) App.socket.on("calendar", App.calendar.ingest);

  // ---------- Dashboard tabs ----------
  const tabs = Array.from(document.querySelectorAll(".panel-tab"));
  const panels = Array.from(document.querySelectorAll(".panel"));
  tabs.forEach((tab, index) => {
    tab.addEventListener("click", () => {
      const target = tab.dataset.panel;
      tabs.forEach((item) => {
        const active = item === tab;
        item.setAttribute("aria-selected", String(active));
        item.setAttribute("tabindex", active ? "0" : "-1");
      });
      panels.forEach((panel) => {
        const active = panel.id === target;
        panel.classList.toggle("is-active", active);
        panel.setAttribute("aria-hidden", String(!active));
      });
      try { localStorage.setItem("xau.panel", target); } catch {}
    });
    tab.addEventListener("keydown", (event) => {
      let next = index;
      if (event.key === "ArrowRight" || event.key === "ArrowDown") next = (index + 1) % tabs.length;
      else if (event.key === "ArrowLeft" || event.key === "ArrowUp") next = (index - 1 + tabs.length) % tabs.length;
      else if (event.key === "Home") next = 0;
      else if (event.key === "End") next = tabs.length - 1;
      else return;
      event.preventDefault();
      tabs[next].focus();
      tabs[next].click();
    });
  });
  let saved = null;
  try { saved = localStorage.getItem("xau.panel"); } catch {}
  const initial = tabs.find((tab) => tab.dataset.panel === saved) || tabs[0];
  if (initial) initial.click();

  // ---------- Timeframe tabs (only 15M is currently provided by Node 1) ----------
  document.querySelectorAll(".tf").forEach((button) => {
    button.addEventListener("click", () => {
      if (button.dataset.tf !== "15m" || button.classList.contains("active")) return;
      if (App.levels) App.levels.reset();
      if (App.orderflow) App.orderflow.clear();
      if (App.chart) App.chart.loadHistory();
      if (App.levels) App.levels.fetchAll();
    });
  });

  function refreshMarketSnapshots() {
    const tasks = [];
    if (App.chart) tasks.push(App.chart.loadHistory());
    if (App.levels) tasks.push(App.levels.fetchAll());
    if (App.calendar) tasks.push(App.calendar.fetchAll());
    return Promise.allSettled(tasks);
  }

  App.socket.setSnapshotRefresh(refreshMarketSnapshots);

  // ---------- Boot ----------
  setStatus("dashboard starting", "busy");
  if (App.calendar) App.calendar.start();
  if (App.strategy) App.strategy.start();
  if (App.execution) App.execution.start();

  // Each service starts even if another service's REST endpoint is offline.
  App.socket.connect();
  if (App.chart) App.chart.loadHistory();
  if (App.levels) App.levels.fetchAll();

  if (App.config.levelsRefreshMs > 0 && App.levels) {
    setInterval(() => App.levels.fetchAll(), App.config.levelsRefreshMs);
  }
  if (App.config.marketSnapshotRefreshMs > 0 && App.chart) {
    setInterval(() => App.chart.loadHistory(), App.config.marketSnapshotRefreshMs);
  }
})((window.App = window.App || {}));
