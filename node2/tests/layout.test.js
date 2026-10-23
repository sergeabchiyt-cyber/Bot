// layout.test.js — responsive layout decisions + CSS contract.

import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { layoutMode, layoutClass, BREAKPOINTS } from "../public/src/render/layout.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const CSS = fs.readFileSync(path.join(__dirname, "../public/styles.css"), "utf8");
const HTML = fs.readFileSync(path.join(__dirname, "../public/index.html"), "utf8");

test("layoutMode: breakpoint boundaries", () => {
  assert.equal(layoutMode(1500), "three-col");
  assert.equal(layoutMode(1401), "three-col");
  assert.equal(layoutMode(1400), "two-col");
  assert.equal(layoutMode(1200), "two-col");
  assert.equal(layoutMode(1000), "two-col");
  assert.equal(layoutMode(999), "single-col");
  assert.equal(layoutMode(600), "single-col");
  assert.equal(layoutMode(320), "single-col");
  assert.equal(layoutMode(NaN), "single-col");
  assert.equal(layoutMode(undefined), "single-col");
});

test("layoutClass: maps mode to class name", () => {
  assert.match(layoutClass(1500), /layout--three-col/);
  assert.match(layoutClass(1200), /layout--two-col/);
  assert.match(layoutClass(500), /layout--single-col/);
});

test("styles.css: defines the matching media queries and grid classes", () => {
  // The JS and CSS must agree on breakpoints.
  assert.ok(CSS.includes("@media (min-width: 1401px)"), "three-col media query");
  assert.ok(CSS.includes("@media (min-width: 1000px)"), "two-col media query");
  assert.ok(CSS.includes(".layout--three-col"), "three-col class");
  assert.ok(CSS.includes(".layout--two-col"), "two-col class");
  assert.ok(CSS.includes(".layout--single-col"), "single-col class");
  assert.ok(CSS.includes("grid-template-columns"), "grid columns");
});

test("styles.css: chart height adapts on small screens", () => {
  assert.ok(CSS.includes("@media (max-width: 640px)"), "mobile breakpoint");
});

test("index.html: viewport meta present, module script", () => {
  assert.match(HTML, /viewport/);
  assert.match(HTML, /type="module"/);
  assert.match(HTML, /runtime-config\.js/);
});

test("breakpoints constant documented", () => {
  assert.equal(BREAKPOINTS.threeColMin, 1401);
  assert.equal(BREAKPOINTS.twoColMin, 1000);
});
