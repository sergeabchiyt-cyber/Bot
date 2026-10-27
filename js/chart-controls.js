/* ============================================================
 * XAUUSD Terminal — timeframe selection and data-integrity chart hooks.
 *
 * The chart renderer itself is unchanged. This controller only selects the
 * Node 1 interval, filters replay frames from other intervals, marks a forming
 * bar, and refetches history after a published gap.
 * ============================================================ */
(function (root, factory) {
  "use strict";

  const api = factory(root && root.App);
  if (typeof module === "object" && module.exports) module.exports = api;
  if (root) {
    root.App = root.App || {};
    root.App.chartControls = api;
  }
})(typeof window !== "undefined" ? window : null, function (App) {
  "use strict";

  App = App || {};
  const config = (App.config && App.config.chart) || {};
  const utils = App.utils || {};
  const intervals = config.intervals || ["1m", "15m", "1h"];
  const state = { interval: config.interval || "15m", forming: false, loading: false };

  function setText(id, value) {
    const el = utils.$ ? utils.$(id) : null;
    if (el) el.textContent = String(value == null ? "" : value);
  }

  function setBadge(id, text, badgeState) {
    const el = utils.$ ? utils.$(id) : null;
    if (!el) return;
    el.textContent = text;
    el.hidden = false;
    if (el.dataset) el.dataset.state = badgeState;
  }

  function updateTabs() {
    if (typeof document === "undefined" || !document.querySelectorAll) return;
    document.querySelectorAll("[data-interval]").forEach((button) => {
      const active = button.dataset.interval === state.interval;
      if (button.classList && button.classList.toggle) button.classList.toggle("active", active);
      if (button.setAttribute) {
        button.setAttribute("aria-pressed", String(active));
        if (active) button.setAttribute("aria-current", "true");
        else button.removeAttribute && button.removeAttribute("aria-current");
      }
    });
  }

  function updateForming() {
    const el = utils.$ ? utils.$("forming-badge") : null;
    if (!el) return;
    el.hidden = !state.forming;
    el.textContent = state.forming ? "FORMING BAR" : "";
  }

  function setCandles(candles) {
    state.forming = Array.isArray(candles) && candles.some((candle) => candle && candle.closed === false);
    updateForming();
  }

  function setCandleState(forming) {
    state.forming = !!forming;
    updateForming();
  }

  function acceptsCandle(candle) {
    if (!candle || !candle.interval) return true;
    return String(candle.interval).toLowerCase() === state.interval;
  }

  async function selectInterval(interval) {
    if (!intervals.includes(interval)) return false;
    state.interval = interval;
    setText("chart-interval-label", interval.toUpperCase());
    updateTabs();
    if (App.chart && typeof App.chart.setInterval === "function") App.chart.setInterval(interval);
    if (App.drawings && typeof App.drawings.requestUpdate === "function") App.drawings.requestUpdate();
    if (App.chart && typeof App.chart.loadHistory === "function") {
      state.loading = true;
      setBadge("socket-badge", "RELOADING", "connecting");
      try { await App.chart.loadHistory(); } finally { state.loading = false; }
    }
    return true;
  }

  async function handleGap(info) {
    if (App.panels && typeof App.panels.showGap === "function") App.panels.showGap(info);
    if (!App.chart || typeof App.chart.loadHistory !== "function") return false;
    state.loading = true;
    try {
      await App.chart.loadHistory();
      if (App.panels && typeof App.panels.resolveGap === "function") App.panels.resolveGap();
      return true;
    } finally {
      state.loading = false;
    }
  }

  /** A non-live freshness transition is a data-integrity signal too. */
  async function handleFreshness(status) {
    const freshness = status && status.state;
    if (!["stale", "offline", "closed_market"].includes(freshness)) return false;
    if (!App.chart || typeof App.chart.loadHistory !== "function") return false;
    state.loading = true;
    try {
      await App.chart.loadHistory();
      return true;
    } finally {
      state.loading = false;
    }
  }

  function bind() {
    if (bind.done || typeof document === "undefined" || !document.querySelectorAll) return;
    bind.done = true;
    document.querySelectorAll("[data-interval]").forEach((button) => {
      if (button.addEventListener) {
        button.addEventListener("click", () => selectInterval(button.dataset.interval));
      }
    });
    updateTabs();
    updateForming();
  }

  return {
    bind,
    state,
    getInterval: () => state.interval,
    selectInterval,
    acceptsCandle,
    setCandles,
    setCandleState,
    handleGap,
    handleFreshness,
  };
});
