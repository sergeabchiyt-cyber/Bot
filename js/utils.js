/* Shared DOM and formatting helpers; network access is exclusively in node2-client.js. */
(function (App) {
  "use strict";
  const normalise = App.normalize;
  const $ = (id) => document.getElementById(id);
  const escapeHtml = (value) => normalise.escapeHtml(value);
  const toMs = (value) => normalise.timestampMs(value);
  const toSec = (value) => { const ms = toMs(value); return ms === null ? null : Math.floor(ms / 1000); };
  const numberOrNull = (value) => normalise.numberOrNull(value);
  function setStatus(text, state) {
    const el = $("status");
    if (!el) return;
    el.textContent = String(text == null ? "" : text);
    el.dataset.state = state || "";
  }
  App.utils = { $, escapeHtml, toMs, toSec, numberOrNull, setStatus };
})((window.App = window.App || {}));
