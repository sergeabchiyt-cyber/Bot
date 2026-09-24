/* ============================================================
 * XAUUSD Terminal — Fed sentiment pill
 * ============================================================ */
(function (App) {
  "use strict";

  const { $ } = App.utils;

  function update(frame) {
    const el = $("sentiment-value");
    if (!el) return;
    const { hawkish = 0, dovish = 0 } = frame || {};
    const bias = hawkish - dovish;
    let label = "Neutral";
    let state = "neutral";
    if (bias > 0.15) {
      label = `Hawkish ${(hawkish * 100).toFixed(0)}%`;
      state = "hawkish";
    } else if (bias < -0.15) {
      label = `Dovish ${(dovish * 100).toFixed(0)}%`;
      state = "dovish";
    }
    el.textContent = label;
    el.dataset.state = state;
  }

  App.sentiment = { update };
})((window.App = window.App || {}));
