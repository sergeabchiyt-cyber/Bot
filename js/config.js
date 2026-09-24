/* ============================================================
 * XAUUSD Terminal — configuration
 * All tunables and endpoint URLs live here.
 * ============================================================ */
(function (App) {
  "use strict";

  const BACKEND_HOST = "engine-southeastasia-sng-main.onrender.com";

  App.config = {
    endpoints: {
      ws: `wss://${BACKEND_HOST}/ws`,
      levels: `https://${BACKEND_HOST}/levels`,
      calendar: `https://${BACKEND_HOST}/calendar`,
      klines:
        "https://fapi.binance.com/fapi/v1/klines?symbol=XAUUSDT&interval=15m&limit=500",
    },

    ws: {
      topics: ["levels", "candle", "bubbles", "trades", "sentiment", "calendar"],
      reconnectMinMs: 1000,
      reconnectMaxMs: 30000,
    },

    calendar: {
      currency: "USD",            // only this currency is shown
      refreshMs: 5 * 60 * 1000,   // REST refresh (WS pushes also update)
      keepPastMs: 6 * 60 * 60 * 1000, // show events released in the last 6h
      impactRank: { high: 3, medium: 2, low: 1, holiday: 0 },
    },

    bubbles: {
      maxOnChart: 60,
      maxRows: 40,
      scaleCap: 5000,
      scaleFloor: 100,
      decayEveryMs: 30000,
      decayFactor: 0.95,
    },

    // Colour + label per level window/kind. PS shows PoC only.
    levels: {
      PW: {
        poc: { color: "#F0B90B", title: "PW PoC", cls: "poc", dashed: false },
        vah: { color: "#26A69A", title: "PW VaH", cls: "vah", dashed: false },
        val: { color: "#EF5350", title: "PW VaL", cls: "val", dashed: false },
      },
      PS: {
        poc: { color: "#58A6FF", title: "PS PoC", cls: "ps-poc", dashed: false },
      },
      CW: {
        poc: { color: "#A371F7", title: "CW PoC", cls: "cw-poc", dashed: true },
        vah: { color: "#F778BA", title: "CW VaH", cls: "cw-vah", dashed: true },
        val: { color: "#22D3EE", title: "CW VaL", cls: "cw-val", dashed: true },
      },
    },
    levelWindows: ["PW", "PS", "CW"],
    levelKinds: ["poc", "vah", "val"],
  };
})((window.App = window.App || {}));
