/* ============================================================
 * Small schema-normalisation and safety helpers shared by the UI.
 * Dependency-free UMD so Node's built-in test runner can exercise the
 * exact helpers used in the browser.
 * ============================================================ */
(function (root, factory) {
  "use strict";

  const api = factory();
  if (typeof module === "object" && module.exports) module.exports = api;
  if (root) {
    root.App = root.App || {};
    root.App.normalize = api;
  }
})(typeof window !== "undefined" ? window : null, function () {
  "use strict";

  const ESCAPE = {
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  };
  const own = (value, key) => Object.prototype.hasOwnProperty.call(value, key);

  function isRecord(value) {
    return value !== null && typeof value === "object" && !Array.isArray(value);
  }

  /**
   * Unwrap `{type, data}` WebSocket frames and the common `{data: snapshot}`
   * REST wrapper. A direct REST snapshot is returned unchanged.
   */
  function unwrapFrame(value) {
    let current = isRecord(value) ? value : null;
    if (!current) return null;

    for (let depth = 0; depth < 4; depth += 1) {
      const typedFrame = typeof current.type === "string" && own(current, "data");
      const bareWrapper = !own(current, "type") && own(current, "data") &&
        Object.keys(current).every((key) => key === "data");
      if (!typedFrame && !bareWrapper) break;
      const next = current.data;
      if (!isRecord(next) && !Array.isArray(next)) break;
      current = next;
      if (Array.isArray(current)) break;
    }
    return current;
  }

  /** Preserve zero and reject blanks, booleans, NaN, and infinities. */
  function numberOrNull(value) {
    if (value === null || value === undefined || typeof value === "boolean") return null;
    if (typeof value === "string" && !value.trim()) return null;
    const number = typeof value === "number" ? value : Number(String(value).trim());
    return Number.isFinite(number) ? number : null;
  }

  /** Convert epoch seconds, epoch milliseconds, or an ISO timestamp to ms. */
  function timestampMs(value) {
    if (value === null || value === undefined || value === "") return null;
    let result;
    if (typeof value === "number" && Number.isFinite(value)) {
      result = Math.abs(value) < 100000000000 ? value * 1000 : value;
    } else if (typeof value === "string") {
      const text = value.trim();
      if (!text) return null;
      if (/^-?\d+(?:\.\d+)?$/.test(text)) {
        const number = Number(text);
        if (!Number.isFinite(number)) return null;
        result = Math.abs(number) < 100000000000 ? number * 1000 : number;
      } else {
        result = Date.parse(text);
      }
    } else {
      return null;
    }
    return Number.isFinite(result) && Math.abs(result) <= 8640000000000000 ? result : null;
  }

  function escapeHtml(value) {
    return String(value === null || value === undefined ? "" : value)
      .replace(/[&<>"']/g, (character) => ESCAPE[character]);
  }

  return {
    own,
    isRecord,
    unwrapFrame,
    numberOrNull,
    timestampMs,
    escapeHtml,
  };
});
