/* ============================================================
 * Small schema-normalisation and safety helpers shared by the UI.
 * This file is a dependency-free UMD module so Node's built-in test runner
 * can exercise the exact helpers used in the browser.
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
   * Unwrap `{type, data}` frames and the common `{data: snapshot}` REST wrapper.
   * A direct REST snapshot is returned unchanged. The special Node 4
   * `bridge_event` envelope is handled by its event renderer, not as a snapshot.
   */
  function unwrapFrame(value) {
    let current = isRecord(value) ? value : null;
    if (!current) return null;

    for (let depth = 0; depth < 4; depth += 1) {
      if (current.type === "bridge_event") return current;
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

  /** Unknown or malformed execution outcomes fail safe to the prominent state. */
  function executionStatus(value) {
    const status = String(value === null || value === undefined ? "" : value)
      .trim()
      .toLowerCase();
    if (["accepted", "filled", "partial", "rejected", "unknown", "cancelled", "closed"].includes(status)) {
      return status;
    }
    if (["partially_filled", "partial_fill", "part-filled"].includes(status)) return "partial";
    if (["canceled", "cancel"].includes(status)) return "cancelled";
    return "unknown";
  }

  function reportId(report) {
    const item = isRecord(report) ? report : {};
    return [
      item.intent_id || "",
      executionStatus(item.status),
      timestampMs(item.timestamp) ?? "",
      item.execution_id || "",
      item.error_code || "",
    ].join("|");
  }

  function eventId(event) {
    const item = isRecord(event) ? event : {};
    return [
      timestampMs(item.timestamp ?? item.time) ?? "",
      item.category || item.event || "",
      item.message || "",
    ].join("|");
  }

  /**
   * Remove duplicate IDs while keeping the first (newest-first callers put
   * the freshest copy first) and cap the result to bound browser memory.
   */
  function dedupeBy(items, idFor, limit = 100) {
    const values = new Map();
    for (const item of Array.isArray(items) ? items : []) {
      if (!isRecord(item)) continue;
      const id = String(idFor(item) || "");
      if (!id) continue;
      if (!values.has(id)) values.set(id, item);
    }
    const capped = Math.max(0, Number.isFinite(Number(limit)) ? Number(limit) : 100);
    if (capped === 0) return [];
    return [...values.values()].slice(0, capped);
  }

  /** Per-domain revision guard used to prevent a late REST response replacing WS data. */
  function createRevisionGuard() {
    const revisions = new Map();
    const current = (key) => revisions.get(String(key)) || 0;
    return {
      current,
      capture: (keys) => Object.fromEntries((keys || []).map((key) => [String(key), current(key)])),
      isCurrent: (key, version) => current(key) === version,
      mark: (key) => {
        const name = String(key);
        const next = current(name) + 1;
        revisions.set(name, next);
        return next;
      },
      applyRest: (key, version, apply) => {
        if (current(key) !== version) return false;
        if (typeof apply === "function") apply();
        revisions.set(String(key), current(key) + 1);
        return true;
      },
    };
  }

  return {
    own,
    isRecord,
    unwrapFrame,
    numberOrNull,
    timestampMs,
    escapeHtml,
    executionStatus,
    reportId,
    eventId,
    dedupeBy,
    createRevisionGuard,
  };
});
