/* ============================================================
 * XAUUSD Terminal — shared, dependency-free browser helpers.
 *
 * All backend transport lives in js/client.js. The compatibility helper below
 * delegates to that client; no other module constructs a request.
 * ============================================================ */
(function (root, factory) {
  "use strict";

  const api = factory(root && root.App);
  if (typeof module === "object" && module.exports) module.exports = api;
  if (root) {
    root.App = root.App || {};
    root.App.utils = api;
  }
})(typeof window !== "undefined" ? window : null, function (App) {
  "use strict";

  const normalise = (App && App.normalize) || (
    typeof require === "function" ? require("./normalize.js") : null
  );

  const $ = (id) => (typeof document === "undefined" || !document.getElementById
    ? null : document.getElementById(id));
  const escapeHtml = (value) => normalise.escapeHtml(value);
  const toMs = (value) => normalise.timestampMs(value);
  const numberOrNull = (value) => normalise.numberOrNull(value);
  const toSec = (value) => {
    const ms = toMs(value);
    return ms === null ? null : Math.floor(ms / 1000);
  };

  /** Legacy callers route through the single typed client. */
  function fetchPublicJson(url, timeoutMs = 12000) {
    const client = App && App.client;
    if (!client || typeof client.requestUrl !== "function") {
      return Promise.reject(new Error("typed backend client is not loaded"));
    }
    return client.requestUrl(url, timeoutMs);
  }

  function setStatus(text, state) {
    const el = $("status");
    if (!el) return;
    el.textContent = String(text == null ? "" : text);
    el.dataset.state = state || "";
  }

  function setText(id, value) {
    const el = $(id);
    if (el) el.textContent = String(value == null ? "" : value);
  }

  function setBadge(id, text, state, title) {
    const el = $(id);
    if (!el) return;
    if (text != null) el.textContent = String(text);
    if (state != null && el.dataset) el.dataset.state = state;
    if (title != null) el.title = String(title);
  }

  return { $, escapeHtml, toMs, toSec, numberOrNull, fetchPublicJson, setStatus, setText, setBadge };
});
