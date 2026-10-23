// layout.js — responsive layout decisions. Pure functions + CSS class
// names; the matching media queries live in styles.css and are verified by
// tests.

// Layout modes (mirrors styles.css media queries):
//   > 1400px   three columns: chart | node1 panels | node4 panels
//   1000-1400  two columns:  chart | panels (node1 above node4)
//   < 1000px    single column stack
export function layoutMode(width) {
  if (!Number.isFinite(width)) return "single-col";
  if (width > 1400) return "three-col";
  if (width >= 1000) return "two-col";
  return "single-col";
}

export function layoutClass(width) {
  return `layout layout--${layoutMode(width)}`;
}

export const BREAKPOINTS = { threeColMin: 1401, twoColMin: 1000 };
