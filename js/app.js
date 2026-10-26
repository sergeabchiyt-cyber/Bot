/* Original dashboard bootstrap with market and Node 3 read-only topics only. */
(function (App) {
  "use strict";
  const { setStatus } = App.utils;
  App.socket.on("candle", (data) => App.chart && App.chart.updateCandle(data));
  App.socket.on("tick_volume", (data) => App.chart && App.chart.updateTickVolume(data));
  App.socket.on("calendar", (data) => App.calendar && App.calendar.ingest(data));
  App.socket.on("status", (data) => App.status && App.status.ingest(data));
  App.socket.on("gap", () => { App.node3 && App.node3.gap(); App.chart && App.chart.loadHistory(); });
  App.socket.onState((snapshot) => { if (snapshot.connected) setStatus("market stream connected", "live"); });
  setStatus("dashboard starting", "busy");
  App.calendar.start();
  App.status.start();
  App.node3.start();
  App.socket.start();
  App.chart.loadHistory();
  setInterval(() => App.chart.loadHistory(), App.config.candlesRefreshMs);
})((window.App = window.App || {}));
