/* ============================================================
 * XAUUSD Terminal — configuration
 * All tunables and endpoint URLs live here.
 *
 * The browser talks to the backend API only. Candles and volume-profile
 * levels are SiftingIO-backed and computed by the backend; Binance is used
 * there for order flow (aggTrade) exclusively and must never be called
 * directly from the frontend. No API keys belong in this file.
 * ============================================================ */
(function (App) {
  "use strict";

  const BACKEND_HOST = "engine-southeastasia-sng-main.onrender.com";

  App.config = {
    endpoints: {
      ws: `wss://${BACKEND_HOST}/ws`,
      candles: `https://${BACKEND_HOST}/candles`,
      tickVolume: `https://${BACKEND_HOST}/tick-volume`,
      tick_volume: `https://${BACKEND_HOST}/tick-volume`,
      levels: `https://${BACKEND_HOST}/levels`,
      calendar: `https://${BACKEND_HOST}/calendar`,
    },

    ws: {
      topics: ["levels", "candle", "bubbles", "trades", "calendar", "tick_volume"],
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

      // Only ONE swing profile is active at a time. The inactive window's
      // price lines are dropped whenever the backend flips direction.
      SWING_BULL: {
        poc: { color: "#26A69A", title: "Bull Swing PoC", cls: "swing-bull-poc", dashed: false },
        vah: { color: "#22C55E", title: "Bull Swing VaH", cls: "swing-bull-vah", dashed: true },
        val: { color: "#86EFAC", title: "Bull Swing VaL", cls: "swing-bull-val", dashed: true },
      },
      SWING_BEAR: {
        poc: { color: "#EF5350", title: "Bear Swing PoC", cls: "swing-bear-poc", dashed: false },
        vah: { color: "#F97316", title: "Bear Swing VaH", cls: "swing-bear-vah", dashed: true },
        val: { color: "#FDBA74", title: "Bear Swing VaL", cls: "swing-bear-val", dashed: true },
      },
    },
    levelWindows: ["PW", "PS", "CW", "SWING_BULL", "SWING_BEAR"],

    // Calendar windows plus the mutually exclusive swing profiles.
    swingWindows: ["SWING_BULL", "SWING_BEAR"],

    // Human labels for the swing anchor row (metadata only).
    swingLabels: {
      SWING_BULL: "Bull Swing",
      SWING_BEAR: "Bear Swing",
    },
    levelKinds: ["poc", "vah", "val"],
  };
})((window.App = window.App || {}));
