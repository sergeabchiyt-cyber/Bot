/* ============================================================
 * XAUUSD Terminal — bootstrap
 * Wires modules together, panel tabs, TF tabs.
 * ============================================================ */
(function (App) {
  "use strict";

  const { setStatus } = App.utils;
  if (!App.chart) return; // chart lib failed; nothing else can work

  // ---------- WS routing ----------
  App.socket.on("heartbeat", () => {});
  App.socket.on("levels", App.levels.apply);
  App.socket.on("candle", App.chart.updateCandle);
  App.socket.on("tick_volume", App.chart.updateTickVolume);
  App.socket.on("tickVolume", App.chart.updateTickVolume);
  App.socket.on("bubbles", App.orderflow.add);
  App.socket.on("sentiment", App.sentiment.update);
  App.socket.on("calendar", App.calendar.ingest);

  // ---------- Panel tabs ----------
  const tabs = document.querySelectorAll(".panel-tab");
  const panels = document.querySelectorAll(".panel");
  tabs.forEach((tab) => {
    tab.addEventListener("click", () => {
      const target = tab.dataset.panel;
      tabs.forEach((t) => t.setAttribute("aria-selected", String(t === tab)));
      panels.forEach((p) => p.classList.toggle("is-active", p.id === target));
      try { localStorage.setItem("xau.panel", target); } catch {}
    });
  });
  let saved = null;
  try { saved = localStorage.getItem("xau.panel"); } catch {}
  const initial = document.querySelector(`.panel-tab[data-panel="${saved}"]`) || tabs[0];
  if (initial) initial.click();

  // ---------- Timeframe tabs (only 15M wired to backend today) ----------
  document.querySelectorAll(".tf").forEach((btn) => {
    btn.addEventListener("click", () => {
      if (btn.dataset.tf !== "15m" || btn.classList.contains("active")) return;
      App.levels.reset();
      App.orderflow.clear();
      App.chart.loadHistory();
      App.levels.fetchAll();
    });
  });

  // ---------- Boot ----------
  setStatus("init", "busy");
  App.calendar.start();
  App.chart.loadHistory().then(() => {
    App.socket.connect();
    App.levels.fetchAll();
  });
})((window.App = window.App || {}));
