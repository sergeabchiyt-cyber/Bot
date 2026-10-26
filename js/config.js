/* Central dashboard configuration. Service origins live in node2-client.js. */
(function (App) {
  "use strict";
  App.config = {
    client: window.Node2Client.createClient(window.XAUUSD_CONFIG),
    socket: {
      topics: ["candle", "tick_volume", "calendar", "status"],
      reconnectMinMs: 1000, reconnectMaxMs: 30000, heartbeatEveryMs: 30000,
      heartbeatTimeoutMs: 75000, staleAfterMs: 90000,
    },
    calendar: { currency: "USD", refreshMs: 300000, keepPastMs: 21600000,
      impactRank: { high: 3, medium: 2, low: 1, holiday: 0 } },
    levelsRefreshMs: 60000, candlesRefreshMs: 60000, statusRefreshMs: 30000,
    levelWindows: ["PW", "PS", "CW"], levelKinds: ["poc", "vah", "val"],
    levels: {
      PW: { poc: { title: "PW PoC", cls: "poc" }, vah: { title: "PW VaH", cls: "vah" }, val: { title: "PW VaL", cls: "val" } },
      PS: { poc: { title: "PS PoC", cls: "ps-poc" } },
      CW: { poc: { title: "CW PoC", cls: "cw-poc" }, vah: { title: "CW VaH", cls: "cw-vah" }, val: { title: "CW VaL", cls: "cw-val" } },
    },
  };
})((window.App = window.App || {}));
