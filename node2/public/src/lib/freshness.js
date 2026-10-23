// freshness.js — pure classification of data freshness. Everything here is
// deterministic and testable; the UI just renders the resulting state.
//
// States used across the dashboard:
//   live          data is flowing within heartbeat tolerance
//   stale         connection up but data has stalled (missed heartbeats /
//                 candle edge lag beyond tolerance)
//   connecting    first connection attempt in progress
//   reconnecting  socket dropped, backoff wait before the next attempt
//   disconnected  socket closed, no attempt scheduled (e.g. fatal)
//   unknown       no data received yet

export const FRESHNESS = {
  LIVE: "live",
  STALE: "stale",
  CONNECTING: "connecting",
  RECONNECTING: "reconnecting",
  DISCONNECTED: "disconnected",
  UNKNOWN: "unknown",
};

const TONE = {
  [FRESHNESS.LIVE]: "good",
  [FRESHNESS.STALE]: "warn",
  [FRESHNESS.CONNECTING]: "info",
  [FRESHNESS.RECONNECTING]: "info",
  [FRESHNESS.DISCONNECTED]: "bad",
  [FRESHNESS.UNKNOWN]: "muted",
};

export function freshnessTone(state) {
  return TONE[state] || "muted";
}

export function freshnessLabel(state) {
  switch (state) {
    case FRESHNESS.LIVE:
      return "Live";
    case FRESHNESS.STALE:
      return "Stale";
    case FRESHNESS.CONNECTING:
      return "Connecting";
    case FRESHNESS.RECONNECTING:
      return "Reconnecting";
    case FRESHNESS.DISCONNECTED:
      return "Disconnected";
    default:
      return "Unknown";
  }
}

// "15m" / "5m" / "1h" / "4h" / "1d" -> minutes (null if unparseable).
export function intervalMinutes(interval) {
  if (typeof interval !== "string") return null;
  const m = interval.match(/^(\d+)([mhdw])$/);
  if (!m) return null;
  const n = Number(m[1]);
  switch (m[2]) {
    case "m":
      return n;
    case "h":
      return n * 60;
    case "d":
      return n * 60 * 24;
    case "w":
      return n * 60 * 24 * 7;
    default:
      return null;
  }
}

// Heartbeat liveness: no frame received within staleAfterMs (default 2.5 x
// heartbeat interval) marks the stream stale even though the socket is open.
export function classifyHeartbeat(lastMsgTs, nowMs, heartbeatMs, staleAfterMs) {
  if (!lastMsgTs || !Number.isFinite(lastMsgTs)) return FRESHNESS.UNKNOWN;
  const age = nowMs - lastMsgTs;
  const limit = staleAfterMs != null ? staleAfterMs : heartbeatMs * 2.5;
  if (age < 0) return FRESHNESS.UNKNOWN; // clock skew guard
  if (age > limit) return FRESHNESS.STALE;
  return FRESHNESS.LIVE;
}

// Socket-level class: combines transport state with heartbeat liveness.
export function classifySocket({ state, lastMsgTs, nowMs, heartbeatMs, staleAfterMs }) {
  if (state === "open") {
    return classifyHeartbeat(lastMsgTs, nowMs, heartbeatMs, staleAfterMs);
  }
  if (state === "connecting") return FRESHNESS.CONNECTING;
  if (state === "reconnecting") return FRESHNESS.RECONNECTING;
  return FRESHNESS.DISCONNECTED;
}

// Candle freshness. Two independent signals:
//  1. Node 1's own measurement: status.candles.edge_lag_minutes (how far the
//     newest candle is behind the current bar). The live feed documents a
//     ~27-minute edge lag at 15m — tolerance is a multiple of the interval.
//  2. Wall-clock age of the newest candle close vs now.
export function classifyCandles({ status, candles, nowMs }) {
  const intervalMin =
    status && status.candles ? intervalMinutes(status.candles.interval) : null;
  if (intervalMin == null) return { state: FRESHNESS.UNKNOWN, reason: "unknown interval" };

  if (!Array.isArray(candles) || candles.length === 0) {
    return { state: FRESHNESS.UNKNOWN, reason: "no candles", intervalMin };
  }

  const last = candles[candles.length - 1];
  const lastBarTs = Number.isFinite(last.time) ? last.time : null;

  // Signal 1: edge lag reported by Node 1 (minutes) — the parsed status
  // normalizes the wire field (edge_lag_minutes) to edgeLagMinutes.
  let edgeLagMin = null;
  if (status && status.candles && Number.isFinite(status.candles.edgeLagMinutes)) {
    edgeLagMin = status.candles.edgeLagMinutes;
  }
  // Signal 2: age of the newest bar close.
  let lastBarAgeSec = null;
  if (lastBarTs) {
    const ageMs = nowMs - lastBarTs * 1000;
    if (ageMs >= 0) lastBarAgeSec = ageMs / 1000;
  }

  // Stale if the reported edge lag exceeds 2x the interval (at 15m that is
  // 30 minutes) or the newest bar is older than 3x the interval.
  const lagLimitMin = Math.max(2 * intervalMin, intervalMin);
  const ageLimitSec = 3 * intervalMin * 60;

  if (edgeLagMin != null && edgeLagMin > lagLimitMin) {
    return {
      state: FRESHNESS.STALE,
      reason: `edge lag ${edgeLagMin}min > ${lagLimitMin}min`,
      edgeLagMin,
      lastBarAgeSec,
      intervalMin,
    };
  }
  if (lastBarAgeSec != null && lastBarAgeSec > ageLimitSec) {
    return {
      state: FRESHNESS.STALE,
      reason: `newest bar ${Math.round(lastBarAgeSec)}s old`,
      edgeLagMin,
      lastBarAgeSec,
      intervalMin,
    };
  }
  return {
    state: FRESHNESS.LIVE,
    reason: edgeLagMin != null ? `edge lag ${edgeLagMin}min` : "bars arriving",
    edgeLagMin,
    lastBarAgeSec,
    intervalMin,
  };
}

// Map a Node 1 feed state string to a tone for the status drawer.
export function classifyFeedTone(state) {
  if (state === "connected") return "good";
  if (state === "error") return "bad";
  if (state === "degraded") return "warn";
  // "off" and "idle" are expected offline states; unknown strings render muted.
  return "muted";
}

export function feedLabel(state, name) {
  if (state === "connected") return `${name} connected`;
  if (state === "error") return `${name} error`;
  if (state === "degraded") return `${name} degraded`;
  if (state === "idle") return `${name} idle`;
  if (state === "off") return `${name} off`;
  return `${name} ${state || "unknown"}`;
}
