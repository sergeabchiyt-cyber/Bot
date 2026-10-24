// dom.js — minimal hyperscript helper. No framework, no build step.
// h("div", {class: "x"}, child, child2) -> HTMLElement

import { freshnessLabel, freshnessTone } from "../lib/freshness.js";

export function h(tag, attrs, ...children) {
  const el = document.createElement(tag);
  if (attrs && typeof attrs === "object") {
    for (const [key, value] of Object.entries(attrs)) {
      if (value == null || value === false) continue;
      if (key === "class") {
        el.className = value;
      } else if (key === "dataset") {
        for (const [dk, dv] of Object.entries(value)) {
          if (dv != null) el.dataset[dk] = String(dv);
        }
      } else if (key === "html") {
        el.innerHTML = value;
      } else if (key.startsWith("on") && typeof value === "function") {
        el.addEventListener(key.slice(2).toLowerCase(), value);
      } else if (value === true) {
        el.setAttribute(key, "");
      } else {
        el.setAttribute(key, String(value));
      }
    }
  }
  for (const child of children.flat()) {
    if (child == null || child === false) continue;
    if (typeof child === "string" || typeof child === "number") {
      el.appendChild(document.createTextNode(String(child)));
    } else {
      el.appendChild(child);
    }
  }
  return el;
}

export function clear(node) {
  while (node.firstChild) node.removeChild(node.firstChild);
}

// Tone classes used across badges: good=green, warn=amber, bad=red,
// info=blue, muted=grey.
export function toneClass(tone) {
  return `badge badge--${tone}`;
}

// Freshness pill: colored badge + optional reason text.
export function freshnessPill(state, { reason = "", label } = {}) {
  return h(
    "span",
    { class: `freshness freshness--${freshnessTone(state)}`, title: reason || undefined },
    h("span", { class: "freshness__dot" }),
    label || freshnessLabel(state),
  );
}

// Lifecycle badge for Node 4 trades/events.
export function lifecycleBadgeEl(badge) {
  return h(
    "span",
    { class: `lifecycle lifecycle--${badge.tone}`, title: badge.title },
    h("span", { class: "lifecycle__icon" }, badge.icon),
    badge.label,
  );
}

// Empty-state placeholder for panels with no data yet.
export function emptyState(text, { tone = "muted" } = {}) {
  return h("div", { class: `panel__empty panel__empty--${tone}` }, text);
}
