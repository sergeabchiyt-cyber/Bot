// config.js — endpoint resolution for Node 1 / Node 4.
//
// Rules:
//   1. Default to SAME-ORIGIN relative paths (/api/node1, /api/node4) so the
//      browser never depends on cross-origin CORS and never targets
//      localhost. The dev server / reverse proxy maps them to the services.
//   2. Absolute overrides are supported via ?node1=<base> / ?node4=<base>
//      for direct cross-origin deployments — the Node 1 operator must add
//      the dashboard origin to CORS_ALLOWED_ORIGIN.
//   3. Absolute URLs pointing at localhost/loopback are REJECTED (defense in
//      depth for the "no localhost in browser code" rule) and reported via
//      the `rejected` flag.
//
// The dashboard never embeds tokens or credentials; authentication is
// entirely server-side.

function toWsUrl(httpUrl) {
  return httpUrl
    .replace(/^https:\/\//i, "wss://")
    .replace(/^http:\/\//i, "ws://")
    .replace(/\/+$/, "");
}

function isLoopbackHost(hostname) {
  if (!hostname) return false;
  return [
    "localhost",
    "127.0.0.1",
    "0.0.0.0",
    "::1",
    "[::1]",
  ].includes(hostname.toLowerCase());
}

export function resolveEndpoints(runtimeConfig = {}, search = "") {
  const params = new URLSearchParams(search);
  const rejected = [];

  function pick(base) {
    // Query override first, then runtime config, then default.
    const fromQuery = params.get(base.param);
    const fromRuntime = runtimeConfig[base.runtimeKey];
    const raw = fromQuery || fromRuntime || base.default;
    if (!raw) return base.default;
    const trimmed = String(raw).trim().replace(/\/+$/, "");
    if (/^https?:\/\//i.test(trimmed)) {
      let host;
      try {
        host = new URL(trimmed).hostname;
      } catch (err) {
        rejected.push({ key: base.param, value: raw, reason: "invalid URL" });
        return base.default;
      }
      if (isLoopbackHost(host)) {
        rejected.push({ key: base.param, value: raw, reason: "loopback host rejected" });
        return base.default;
      }
      return trimmed;
    }
    if (/^wss?:\/\//i.test(trimmed)) {
      const host = new URL(trimmed).hostname;
      if (isLoopbackHost(host)) {
        rejected.push({ key: base.param, value: raw, reason: "loopback host rejected" });
        return base.default;
      }
      return trimmed;
    }
    // Relative path (e.g. /api/node1) — always same-origin.
    return trimmed;
  }

  const node1Rest = pick({
    param: "node1",
    runtimeKey: "node1Rest",
    default: "/api/node1",
  });
  const node4Rest = pick({
    param: "node4",
    runtimeKey: "node4Rest",
    default: "/api/node4",
  });

  return {
    node1Rest,
    node4Rest,
    node1Ws: params.get("node1")
      ? toWsUrl(node1Rest) + "/ws"
      : (runtimeConfig.node1Ws || "/api/node1/ws"),
    node4Ws: params.get("node4")
      ? toWsUrl(node4Rest) + "/ws"
      : (runtimeConfig.node4Ws || "/api/node4/ws"),
    rejected,
    mock: runtimeConfig.mock === true || params.get("mock") === "1",
    demo: runtimeConfig.demo === true || params.get("demo") !== "0",
  };
}
