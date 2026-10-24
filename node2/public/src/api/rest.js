// rest.js — shared fetch helper for read-only REST resources.
//
// Every request is a GET with `Cache-Control: no-store` (the Node 1 contract
// says NOT cached — must be 200 every time). Failures never throw past this
// layer: they are returned as { ok:false, status } so panels can show
// "unavailable" states instead of crashing.

export async function fetchJson(url, { timeoutMs = 10000 } = {}) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const res = await fetch(url, {
      method: "GET",
      cache: "no-store",
      headers: { "Cache-Control": "no-store" },
      signal: controller.signal,
      credentials: "omit",
    });
    let data = null;
    const text = await res.text();
    try {
      data = text ? JSON.parse(text) : null;
    } catch (err) {
      data = null;
    }
    return { ok: res.ok, status: res.status, data };
  } catch (err) {
    return { ok: false, status: 0, error: String(err && err.message ? err.message : err) };
  } finally {
    clearTimeout(timer);
  }
}
