/* ============================================================
 * XAUUSD Terminal — volume-profile levels (PW / PS / CW / SWING_*)
 * Draws price lines and renders the LEVELS panel.
 *
 * Levels are computed by the backend from SiftingIO candles. This module
 * only renders what it is given — no VP math happens in the browser.
 *
 * CW is OPTIONAL. The backend only emits CW after the first 17:00
 * America/New_York daily close of the trading week, and clears it at the
 * Sunday 18:00 New York week boundary. While CW is absent the module
 * draws no CW lines and shows a stable "pending" placeholder instead —
 * it must never assume `levels.find(l => l.window === "CW")` exists.
 *
 * Two update shapes are accepted:
 *   - a single window object  → targeted upsert (WS per-window frames)
 *   - an array / {levels:[…]} → authoritative snapshot: windows missing
 *     from the snapshot are removed, so a week reset that drops CW clears
 *     stale CW lines (and a swing flip clears the opposite swing).
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, escapeHtml, fmtPrice, numberOrNull, fetchPublicJson } = App.utils;
  const cfg = App.config;

  const swingWindows = cfg.swingWindows || ["SWING_BULL", "SWING_BEAR"];
  const swingLabels = cfg.swingLabels || {};
  const isSwing = (w) => swingWindows.indexOf(w) !== -1;

  const CW_PENDING_TITLE = "CW pending first daily close";

  const state = {};
  for (const w of cfg.levelWindows) state[w] = null;
  let wsRevision = 0;
  let fetchInProgress = false;

  /** [window, kind, price, style] for every plotted level of one window. */
  function* visibleLevels(l) {
    const styles = cfg.levels[l.window];
    if (!styles) return;
    for (const kind of cfg.levelKinds) {
      const raw = l[kind];
      const style = styles[kind];           // PS has only "poc"
      if (raw == null || raw === "" || !style) continue;
      const price = numberOrNull(raw);
      if (price === null) continue;
      yield [l.window, kind, price, style];
    }
  }

  /** True when the window has at least one plottable poc/vah/val price. */
  function hasVisibleLevels(l) {
    if (!l || !cfg.levels[l.window]) return false;
    for (const entry of visibleLevels(l)) {
      if (entry) return true;
    }
    return false;
  }

  /** Remove every price line + cached payload for one window. */
  function clearWindow(w) {
    if (App.chart && typeof App.chart.removePriceLine === "function") {
      for (const kind of cfg.levelKinds) {
        App.chart.removePriceLine(`${w}-${kind}`);
      }
    }
    delete state[w];
  }

  /**
   * Optional metadata row: the structural anchors the swing profile used.
   * Purely informational — the plotted levels remain poc / vah / val.
   */
  function anchorRow(l) {
    const high = numberOrNull(l.swing_high);
    const low = numberOrNull(l.swing_low);
    if (high === null || low === null) return null;
    const label =
      swingLabels[l.window] ||
      (l.direction === "bearish" ? "Bear Swing" : l.direction === "bullish" ? "Bull Swing" : l.window);
    return { label, high, low };
  }

  /** Dim the CW legend keys while CW is pending — no layout change. */
  function syncLegendPending(pending) {
    if (typeof document === "undefined" || !document.querySelectorAll) return;
    document.querySelectorAll(".legend-cw").forEach((el) => {
      if (el.classList) el.classList.toggle("is-pending", !!pending);
      if (pending && el.setAttribute) el.setAttribute("title", CW_PENDING_TITLE);
      else if (el.removeAttribute) el.removeAttribute("title");
    });
  }

  function cwPendingRow() {
    return `
          <div class="level-row level-pending">
            <span class="level-label">CW</span>
            <span class="level-meta">Waiting for first daily close</span>
          </div>`;
  }

  function render() {
    const list = $("levels-list");
    const count = $("levels-count");

    const cwUsable = hasVisibleLevels(state.CW);
    const anyData = cfg.levelWindows.some((w) => {
      const l = state[w];
      return !!l && (hasVisibleLevels(l) || (isSwing(w) && !!anchorRow(l)));
    });

    // CW is shown exactly when the backend has sent a usable CW profile.
    // Until then the panel and legend carry a stable pending state.
    const cwPending = anyData && !cwUsable;
    syncLegendPending(cwPending);

    if (!anyData) {
      if (count) count.textContent = 0;
      if (list) list.innerHTML = '<div class="empty">Awaiting data…</div>';
      return;
    }

    const rows = [];
    let levels = 0;

    for (const w of cfg.levelWindows) {
      if (w === "CW" && cwPending) {
        rows.push(cwPendingRow());
        continue;
      }
      const l = state[w];
      if (!l) continue;                 // inactive window → not rendered
      const anchor = isSwing(w) ? anchorRow(l) : null;
      if (anchor) {
        rows.push(`
          <div class="level-row level-anchor">
            <span class="level-label">${escapeHtml(anchor.label)}</span>
            <span class="level-meta">${escapeHtml(fmtPrice(anchor.high))} &rarr; ${escapeHtml(fmtPrice(anchor.low))}</span>
          </div>`);
      }
      for (const [, , price, s] of visibleLevels(l)) {
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

  /** Upsert one window's lines + payload. Never touches the other windows
   *  (except the mutually exclusive opposite swing profile). */
  function applyOne(l) {
    if (!l || typeof l !== "object" || !cfg.levels[l.window]) return;

    // Only one swing profile is active: drop the opposite direction's lines
    // so a BULL → BEAR flip (or vice versa) cannot leave stale levels behind.
    if (isSwing(l.window)) {
      for (const w of swingWindows) {
        if (w !== l.window) clearWindow(w);
      }
    }

    for (const [w, kind, price, style] of visibleLevels(l)) {
      if (App.chart && typeof App.chart.upsertPriceLine === "function") {
        App.chart.upsertPriceLine(`${w}-${kind}`, price, style);
      }
    }
    state[l.window] = l;
  }

  /**
   * Authoritative snapshot: apply every window present, then REMOVE the
   * known windows that are absent. A snapshot without CW therefore clears
   * last week's CW lines and state instead of retaining them.
   */
  function applySnapshot(items) {
    const seen = new Set();
    for (const l of items || []) {
      if (!l || typeof l !== "object" || !cfg.levels[l.window]) continue;
      seen.add(l.window);
      applyOne(l);
    }
    for (const w of cfg.levelWindows) {
      if (!seen.has(w)) clearWindow(w);
    }
    render();
  }

  /**
   * Entry point for both REST and WebSocket payloads:
   *   - [ {...}, … ] or { levels: [ … ] } → snapshot (see applySnapshot)
   *   - { window: "CW", … }               → single-window upsert
   *   - anything else                     → ignored, never throws
   */
  function apply(payload, source = "ws") {
    if (source !== "rest") wsRevision += 1;
    if (Array.isArray(payload)) {
      applySnapshot(payload);
      return;
    }
    if (payload && Array.isArray(payload.levels)) {
      applySnapshot(payload.levels);
      return;
    }
    if (payload && typeof payload === "object" && payload.window != null) {
      applyOne(payload);
      render();
    }
  }

  /** Normalise a `/levels` response body into a snapshot list (or null). */
  function toSnapshotList(payload) {
    if (Array.isArray(payload)) return payload;
    if (payload && Array.isArray(payload.levels)) return payload.levels;
    if (payload && typeof payload === "object" && payload.window != null) return [payload];
    return null; // malformed body → keep current state
  }

  async function fetchAll() {
    if (fetchInProgress || !cfg.endpoints.levels) return;
    fetchInProgress = true;
    const requestRevision = wsRevision;
    try {
      const payload = await fetchPublicJson(cfg.endpoints.levels);
      const list = toSnapshotList(payload);
      if (list && wsRevision === requestRevision) applySnapshot(list);
    } catch (e) {
      console.error("Market levels snapshot failed:", e);
    } finally {
      fetchInProgress = false;
    }
  }

  function reset() {
    if (App.chart && typeof App.chart.clearPriceLines === "function") App.chart.clearPriceLines();
    for (const w of Object.keys(state)) delete state[w];
    render();
  }

  App.levels = { apply, fetchAll, reset };
})((window.App = window.App || {}));
