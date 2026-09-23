/* ============================================================
 * XAUUSD Terminal — volume-profile levels (PW / PS / CW)
 * Draws price lines and renders the LEVELS panel.
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, fmtPrice } = App.utils;
  const cfg = App.config;

  const state = { PW: null, PS: null, CW: null };

  /** Yields [window, kind, price, style] for every visible level. */
  function* visibleLevels(l) {
    const styles = cfg.levels[l.window];
    if (!styles) return;
    for (const kind of cfg.levelKinds) {
      const price = l[kind];
      const style = styles[kind];           // PS has only "poc"
      if (price == null || !style) continue;
      yield [l.window, kind, price, style];
    }
  }

  function render() {
    const list = $("levels-list");
    const count = $("levels-count");
    const rows = [];
    for (const w of cfg.levelWindows) {
      if (!state[w]) continue;
      for (const [, , price, s] of visibleLevels(state[w])) {
        rows.push({ price, s });
      }
    }
    count.textContent = rows.length;

    if (!rows.length) {
      list.innerHTML = '<div class="empty">Awaiting data…</div>';
      return;
    }
    list.innerHTML = rows
      .map(({ price, s }) => `
        <div class="level-row">
          <span class="level-label"><i class="swatch ${s.cls}"></i>${s.title}</span>
          <span class="level-price">${fmtPrice(price)}</span>
        </div>`)
      .join("");
  }

  function apply(l) {
    if (!l || !cfg.levels[l.window]) return;
    for (const [w, kind, price, style] of visibleLevels(l)) {
      App.chart.upsertPriceLine(`${w}-${kind}`, price, style);
    }
    state[l.window] = l;
    render();
  }

  async function fetchAll() {
    try {
      const res = await fetch(cfg.endpoints.levels);
      if (!res.ok) throw new Error(`levels ${res.status}`);
      const levels = await res.json();
      (Array.isArray(levels) ? levels : []).forEach(apply);
    } catch (e) {
      console.error("Levels fetch failed:", e);
    }
  }

  function reset() {
    App.chart.clearPriceLines();
  }

  App.levels = { apply, fetchAll, reset };
})((window.App = window.App || {}));
