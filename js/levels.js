/* ============================================================
 * XAUUSD Terminal — volume-profile levels (PW / PS / CW / SWING_*)
 * Draws price lines and renders the LEVELS panel.
 *
 * Levels are computed by the backend from SiftingIO candles. This module
 * only renders what it is given — no VP math happens in the browser.
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, escapeHtml, fmtPrice } = App.utils;
  const cfg = App.config;

  const swingWindows = cfg.swingWindows || ["SWING_BULL", "SWING_BEAR"];
  const swingLabels = cfg.swingLabels || {};
  const isSwing = (w) => swingWindows.indexOf(w) !== -1;

  const state = {};
  for (const w of cfg.levelWindows) state[w] = null;

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

  /** Remove every price line + cached payload for one window. */
  function clearWindow(w) {
    for (const kind of cfg.levelKinds) {
      App.chart.removePriceLine(`${w}-${kind}`);
    }
    delete state[w];
  }

  /**
   * Optional metadata row: the structural anchors the swing profile used.
   * Purely informational — the plotted levels remain poc / vah / val.
   */
  function anchorRow(l) {
    const high = Number(l.swing_high);
    const low = Number(l.swing_low);
    if (!Number.isFinite(high) || !Number.isFinite(low)) return null;
    const label =
      swingLabels[l.window] ||
      (l.direction === "bearish" ? "Bear Swing" : l.direction === "bullish" ? "Bull Swing" : l.window);
    return { label, high, low };
  }

  function render() {
    const list = $("levels-list");
    const count = $("levels-count");
    if (!list) return;

    const rows = [];
    let levels = 0;

    for (const w of cfg.levelWindows) {
      if (!state[w]) continue;                 // inactive window → not rendered
      const anchor = isSwing(w) ? anchorRow(state[w]) : null;
      if (anchor) {
        rows.push(`
          <div class="level-row level-anchor">
            <span class="level-label">${escapeHtml(anchor.label)}</span>
            <span class="level-meta">${escapeHtml(fmtPrice(anchor.high))} &rarr; ${escapeHtml(fmtPrice(anchor.low))}</span>
          </div>`);
      }
      for (const [, , price, s] of visibleLevels(state[w])) {
        levels += 1;
        rows.push(`
          <div class="level-row">
            <span class="level-label"><i class="swatch ${escapeHtml(s.cls)}"></i>${escapeHtml(s.title)}</span>
            <span class="level-price">${escapeHtml(fmtPrice(price))}</span>
          </div>`);
      }
    }

    if (count) count.textContent = levels;

    if (!rows.length) {
      list.innerHTML = '<div class="empty">Awaiting data…</div>';
      return;
    }
    list.innerHTML = rows.join("");
  }

  function apply(l) {
    if (!l || !cfg.levels[l.window]) return;

    // Only one swing profile is active: drop the opposite direction's lines
    // so a BULL → BEAR flip (or vice versa) cannot leave stale levels behind.
    if (isSwing(l.window)) {
      for (const w of swingWindows) {
        if (w !== l.window) clearWindow(w);
      }
    }

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
      const payload = await res.json();
      const levels = Array.isArray(payload)
        ? payload
        : Array.isArray(payload && payload.levels)
          ? payload.levels
          : payload
            ? [payload]
            : [];
      levels.forEach(apply);
    } catch (e) {
      console.error("Levels fetch failed:", e);
    }
  }

  function reset() {
    App.chart.clearPriceLines();
    for (const w of Object.keys(state)) delete state[w];
    render();
  }

  App.levels = { apply, fetchAll, reset };
})((window.App = window.App || {}));
