// safejson.js — every external payload goes through safeJsonParse before it
// touches the rest of the app. A malformed frame must never crash a panel;
// it is reported as { ok: false } and dropped.
export function safeJsonParse(text) {
  try {
    const value = JSON.parse(text);
    return { ok: true, data: value };
  } catch (err) {
    return { ok: false, error: String(err && err.message ? err.message : err) };
  }
}

// Guard for constructors we do not control (e.g. a WebSocketImpl injected in
// tests).
export function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
