/* Render only Node 3's published PW / PS / CW levels; no profile math. */
(function (App) {
  "use strict";
  const { $, escapeHtml } = App.utils;
  const state = { levels: [], lastGood: null, error: null };
  const fmt = (value) => value == null || !Number.isFinite(Number(value)) ? "—" : App.format.price(Number(value));
  function render() {
    const list = $("levels-list");
    const count = $("levels-count");
    if (!list) return;
    count.textContent = String(state.levels.length);
    if (!state.levels.length) {
      list.innerHTML = `<div class="empty">${state.error ? `Node 3 levels unavailable · last good ${state.lastGood ? App.format.time(state.lastGood) : "—"}` : "Awaiting Node 3 levels…"}</div>`;
      return;
    }
    list.innerHTML = `<div class="node3-level-head"><span>Window</span><span>PoC</span><span>VaH</span><span>VaL</span></div>${state.levels.map((level) => {
      const ps = level.window === "PS";
      return `<div class="level-row node3-level-row"><span class="level-label">${escapeHtml(level.window)}</span><span class="level-price">${fmt(level.poc)}</span>${ps ? `<span class="level-meta" style="grid-column:span 2">PoC only</span>` : `<span class="level-price">${fmt(level.vah)}</span><span class="level-price">${fmt(level.val)}</span>`}</div>`;
    }).join("")}`;
  }
  function apply(payload) { state.levels = Array.isArray(payload) ? payload : []; state.lastGood = Date.now(); state.error = null; render(); }
  async function fetchAll() {
    try { apply(await App.api.fetchLevels()); }
    catch (error) { state.error = error.message; render(); console.warn("Node 3 levels unavailable:", error); }
  }
  App.levels = { apply, fetchAll, render, getState: () => ({ ...state }) };
})((window.App = window.App || {}));
