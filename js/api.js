/* Typed read-only route facade; transports and payload validation live in node2-client.js. */
(function (App) {
  "use strict";
  const client = App.config.client;
  App.api = {
    fetchCandles: (interval) => client.node1.candles(interval),
    fetchTickVolume: () => client.node1.tickVolume(),
    fetchCalendar: () => client.node1.calendar(),
    fetchStatus: () => client.node1.status(),
    fetchHealth: () => client.node3.health(),
    fetchLevels: () => client.node3.levels(),
    fetchDrawings: () => client.node3.drawings(),
    fetchVp: (query) => client.node3.vp(query),
    fetchScanning: () => client.node3.scanning(),
    fetchSignals: () => client.node3.signals(),
    fetchDiagnostics: () => client.node3.diagnostics(),
  };
})((window.App = window.App || {}));
