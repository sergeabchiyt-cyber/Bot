// lifecycle.js — broker-authoritative order lifecycle semantics.
//
// The authoritative order state machine is owned by Node 4 (mt5/intent.rs).
// This module mirrors its rendering rules ONLY — the dashboard never changes
// state, never resends, and never infers a fill that the broker did not
// confirm:
//
//   accepted    intent accepted by Node 4; broker result pending.
//               NEVER rendered as a confirmed fill.
//   filled      broker confirmed the full requested volume.
//   partial     broker confirmed a partial fill (filled_volume > 0).
//               A confirmed fill only when filled_volume > 0.
//   rejected    broker rejected the order.
//   unknown     outcome never arrived from the broker. Surfaced as Unknown —
//               NEVER rendered as filled, and the intent is NOT resent.
//   cancelled   intent cancelled.
//   closed      position/order closed.
//
// isConfirmedFill mirrors Mt5OrderOutcome::is_confirmed_fill:
// filled|partial AND filled_volume > 0.

export const LIFECYCLE_STATUSES = [
  "accepted",
  "filled",
  "partial",
  "rejected",
  "unknown",
  "cancelled",
  "closed",
];

export const LIFECYCLE = {
  accepted: {
    label: "Accepted",
    tone: "warn",
    icon: "⏳",
    brokerConfirmed: false,
    title: "Node 4 accepted the intent; broker result pending. Not a confirmed fill.",
  },
  filled: {
    label: "Filled",
    tone: "good",
    icon: "✔",
    brokerConfirmed: true,
    title: "Broker confirmed the full requested volume.",
  },
  partial: {
    label: "Partial",
    tone: "info",
    icon: "◖",
    brokerConfirmed: true,
    title: "Broker confirmed a partial fill — see filled volume.",
  },
  rejected: {
    label: "Rejected",
    tone: "bad",
    icon: "✕",
    brokerConfirmed: false,
    title: "The broker rejected the order.",
  },
  unknown: {
    label: "Unknown",
    tone: "bad",
    icon: "?",
    brokerConfirmed: false,
    title:
      "The broker never confirmed this outcome. It is reconciled and never resent. It is NOT a confirmed fill.",
  },
  cancelled: {
    label: "Cancelled",
    tone: "muted",
    icon: "⊘",
    brokerConfirmed: false,
    title: "The intent was cancelled before a broker outcome.",
  },
  closed: {
    label: "Closed",
    tone: "muted",
    icon: "⊟",
    brokerConfirmed: false,
    title: "The position/order is closed.",
  },
};

const FALLBACK = LIFECYCLE.unknown;

// Normalize an arbitrary incoming status string to a canonical lifecycle
// descriptor. Unknown or garbage statuses normalize to "unknown" — they are
// never presented as any other lifecycle state, and never as a fill.
export function lifecycleBadge(status) {
  const key = typeof status === "string" && status.length ? status.toLowerCase() : "";
  const known = Object.prototype.hasOwnProperty.call(LIFECYCLE, key);
  const meta = known ? LIFECYCLE[key] : FALLBACK;
  return {
    status: known ? key : "unknown",
    label: meta.label,
    tone: meta.tone,
    icon: meta.icon,
    brokerConfirmed: meta.brokerConfirmed,
    title: meta.title,
  };
}

// Mirrors Mt5OrderOutcome::is_confirmed_fill exactly:
// only filled|partial with a positive filled_volume is a confirmed fill.
export function isConfirmedFill(status, filledVolume) {
  if (status !== "filled" && status !== "partial") return false;
  return Number.isFinite(filledVolume) && filledVolume > 0;
}

// Filled vs open distinction used by the trades table. A trade is "open"
// unless a broker-confirmed fill exists for it.
export function tradeStatusKind(status, filledVolume) {
  if (isConfirmedFill(status, filledVolume)) return "open";
  if (status === "filled") return "filled";
  if (status === "accepted") return "accepted";
  if (status === "unknown") return "unknown";
  if (status === "rejected") return "rejected";
  return "other";
}

export function isTerminalStatus(status) {
  return ["filled", "rejected", "unknown", "cancelled", "closed"].includes(status);
}
