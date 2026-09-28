/* ============================================================
 * Frontend tests — optional CW handling in js/levels.js
 *
 * Runs with plain Node (no dependencies, no DOM):
 *
 *   node tests/levels.test.js
 *
 * Covers the required cases:
 *   1. /levels containing PW, PS, CW and swing levels
 *   2. /levels without CW
 *   3. WebSocket snapshot that removes CW after a week reset
 *   4. WebSocket update that adds CW after the first daily close
 *   5. No stale CW chart lines after CW disappears
 *   6. No exception when CW is missing
 *   7. Correct level count when CW is absent
 * ============================================================ */
"use strict";

const path = require("path");

/* ---------- tiny DOM + chart fakes ---------- */

function makeEl() {
  const el = {
    innerHTML: "",
    textContent: "",
    attrs: {},
    classes: new Set(),
    setAttribute(k, v) {
      el.attrs[k] = String(v);
    },
    removeAttribute(k) {
      delete el.attrs[k];
    },
    classList: {
      toggle(c, on) {
        if (on) el.classes.add(c);
        else el.classes.delete(c);
      },
      add(c) {
        el.classes.add(c);
      },
      remove(c) {
        el.classes.delete(c);
      },
      contains(c) {
        return el.classes.has(c);
      },
    },
  };
  return el;
}

const els = {
  "levels-list": makeEl(),
  "levels-count": makeEl(),
};
const legendItems = [makeEl(), makeEl(), makeEl()];
for (const item of legendItems) item.classes.add("legend-cw");

global.window = global;
global.document = {
  getElementById(id) {
    return els[id] || null;
  },
  querySelectorAll(sel) {
    return sel === ".legend-cw" ? legendItems : [];
  },
};

// Record every price line the chart is asked to draw/remove.
const chartLines = new Map();
window.App = {
  chart: {
    upsertPriceLine(key, price, style) {
      chartLines.set(key, { price, style });
    },
    removePriceLine(key) {
      chartLines.delete(key);
    },
    clearPriceLines() {
      chartLines.clear();
    },
  },
};

require(path.join(__dirname, "..", "js", "config.js"));
require(path.join(__dirname, "..", "js", "utils.js"));
require(path.join(__dirname, "..", "js", "levels.js"));

const App = window.App;
const listEl = els["levels-list"];
const countEl = els["levels-count"];

/* ---------- fixtures ---------- */

const PW = {
  window: "PW",
  poc: 4285.3,
  vah: 4339.55,
  val: 4252.55,
  direction: "neutral",
};
const PS = {
  window: "PS",
  poc: 4285.66,
  vah: 4286.46,
  val: 4285.46,
  direction: "neutral",
};
const CW = {
  window: "CW",
  poc: 4286.1,
  vah: 4287.25,
  val: 4284.35,
  direction: "neutral",
};
const SWING_BULL = {
  window: "SWING_BULL",
  poc: 4285.71,
  vah: 4286.46,
  val: 4285.46,
  direction: "bullish",
  swing_high: 4286.46,
  swing_low: 4285.46,
};

// Exact fixture from the task: /levels WITHOUT CW (Monday, pre-daily-close).
const FIXTURE_NO_CW = [
  {
    window: "PW",
    poc: 4285.3,
    vah: 4339.55,
    val: 4252.55,
    direction: "neutral",
  },
  {
    window: "PS",
    poc: 4285.66,
    vah: 4286.46,
    val: 4285.46,
    direction: "neutral",
  },
  {
    window: "SWING_BULL",
    poc: 4285.71,
    vah: 4286.46,
    val: 4285.46,
    direction: "bullish",
  },
];

const FIXTURE_WITH_CW = [PW, PS, CW, SWING_BULL];

/* ---------- helpers ---------- */

let passed = 0;
let failed = 0;

async function test(name, fn) {
  App.levels.reset();
  chartLines.clear();
  try {
    await fn();
    passed += 1;
    console.log(`ok   - ${name}`);
  } catch (e) {
    failed += 1;
    console.error(`FAIL - ${name}\n       ${e && e.message}`);
  }
}

function assert(cond, msg) {
  if (!cond) throw new Error(msg || "assertion failed");
}

function assertEqual(got, want, msg) {
  if (got !== want) {
    throw new Error(`${msg || "mismatch"}: expected ${JSON.stringify(want)}, got ${JSON.stringify(got)}`);
  }
}

function count() {
  return Number(countEl.textContent);
}

function html() {
  return listEl.innerHTML;
}

function cwKeys() {
  return [...chartLines.keys()].filter((k) => k.startsWith("CW-"));
}

function legendPending() {
  return legendItems.map((el) => el.classes.has("is-pending"));
}

function mockFetch(body, ok = true, status = 200) {
  global.fetch = async () => ({ ok, status, json: async () => body });
}

/* ---------- tests ---------- */

(async () => {
  await test("1. /levels with PW, PS, CW and swing renders every level", () => {
    App.levels.apply(FIXTURE_WITH_CW);
    assertEqual(count(), 10, "level count (PW 3 + PS 1 + CW 3 + swing 3)");
    assert(!html().includes("level-pending"), "no pending placeholder when CW exists");
    assert(html().includes("CW PoC"), "CW PoC row rendered");
    assert(html().includes("CW VaH"), "CW VaH row rendered");
    assert(html().includes("CW VaL"), "CW VaL row rendered");
    assert(html().includes("PW PoC") && html().includes("PS PoC"), "PW/PS rows rendered");
    assert(html().includes("Bull Swing PoC"), "swing rows rendered");
    assertEqual(cwKeys().length, 3, "three CW chart lines");
    assert(legendPending().every((p) => !p), "legend not pending");
  });

  await test("2. /levels without CW shows the pending placeholder", () => {
    App.levels.apply(FIXTURE_NO_CW); // exact task fixture
    assertEqual(count(), 7, "level count (PW 3 + PS 1 + swing 3)");
    assert(html().includes("level-pending"), "placeholder row rendered");
    assert(html().includes("Waiting for first daily close"), "placeholder text");
    assert(!html().includes("CW PoC"), "no CW rows");
    assertEqual(cwKeys().length, 0, "no CW chart lines");
    assert(html().includes("PW PoC") && html().includes("PS PoC"), "PW/PS still render");
    assert(html().includes("Bull Swing PoC"), "swing still renders");
    assert(legendPending().every((p) => p), "CW legend keys dimmed");
  });

  await test("3. WebSocket snapshot without CW removes CW after week reset", () => {
    App.levels.apply(FIXTURE_WITH_CW);
    assertEqual(cwKeys().length, 3, "CW lines present before reset");
    // WS `levels` frame whose data is a full snapshot without CW
    App.levels.apply(JSON.parse(JSON.stringify(FIXTURE_NO_CW)));
    assertEqual(cwKeys().length, 0, "stale CW lines removed");
    assert(html().includes("Waiting for first daily close"), "placeholder back");
    assertEqual(count(), 7, "count drops the missing CW levels");
    assert(html().includes("PW PoC") && html().includes("PS PoC"), "PW/PS independent of CW");
    assert(html().includes("Bull Swing PoC"), "swing independent of CW");
  });

  await test("4. WebSocket update adds CW after the first daily close", () => {
    App.levels.apply(FIXTURE_NO_CW);
    assertEqual(cwKeys().length, 0, "no CW before the close");
    // WS `levels` frame with a single CW object (backend per-window frame)
    App.levels.apply(CW);
    assertEqual(cwKeys().length, 3, "CW lines drawn");
    assert(!html().includes("level-pending"), "placeholder removed");
    assert(html().includes("CW PoC"), "CW rows rendered");
    assertEqual(count(), 10, "count includes CW again");
    assert(legendPending().every((p) => !p), "legend recovers");
    // and a snapshot containing CW works as well
    App.levels.reset();
    App.levels.apply(FIXTURE_NO_CW);
    App.levels.apply({ levels: FIXTURE_WITH_CW });
    assertEqual(cwKeys().length, 3, "snapshot adds CW too");
    assertEqual(count(), 10, "snapshot count includes CW");
  });

  await test("5. No stale CW chart lines after CW disappears (REST path)", async () => {
    mockFetch(FIXTURE_WITH_CW);
    await App.levels.fetchAll();
    assertEqual(cwKeys().length, 3, "CW lines from first fetch");
    mockFetch(JSON.parse(JSON.stringify(FIXTURE_NO_CW))); // week reset
    await App.levels.fetchAll();
    assertEqual(cwKeys().length, 0, "stale CW lines removed on reconcile");
    assert(html().includes("Waiting for first daily close"), "placeholder shown");
  });

  await test("6. No exception when CW is missing or malformed", () => {
    // none of these may throw
    App.levels.apply(null);
    App.levels.apply(undefined);
    App.levels.apply([]);
    App.levels.apply({});
    App.levels.apply({ levels: null });
    App.levels.apply({ window: "UNKNOWN_WINDOW", poc: 1 });
    App.levels.apply({ window: "CW" }); // CW entry without any prices
    App.levels.apply([{ window: "CW", poc: null, vah: null, val: null }]);
    App.levels.apply([null, 42, "x"]);
    assertEqual(cwKeys().length, 0, "nothing drawn for malformed CW");
    assertEqual(count(), 0, "nothing counted for malformed CW");
    // a price-less CW entry counts as pending while other windows exist
    App.levels.apply([PW, PS, { window: "CW", poc: null, vah: null, val: null }]);
    assert(html().includes("level-pending"), "CW entry without prices still counts as pending");
    assert(!html().includes("CW PoC"), "no CW rows without prices");
    assertEqual(count(), 4, "price-less CW contributes no levels");
  });

  await test("7. Level count is correct when CW is absent", () => {
    App.levels.apply([PW, PS]); // snapshot without any swing or CW
    assertEqual(count(), 4, "PW 3 + PS 1");
    assert(html().includes("level-pending"), "pending row shown (not counted)");
    assertEqual(count(), 4, "placeholder never inflates the count");
  });

  await test("A. single-frame upsert keeps windows independent", () => {
    App.levels.apply(FIXTURE_WITH_CW);
    App.levels.apply(CW); // repeated CW frame must not touch PW/PS/swing
    assert(html().includes("PW PoC") && html().includes("PS PoC"), "PW/PS untouched");
    assert(html().includes("Bull Swing PoC"), "swing untouched");
    assertEqual(count(), 10, "count stable across upserts");
    // swing flip still clears only the opposite swing
    App.levels.apply({
      window: "SWING_BEAR",
      poc: 4284.0,
      vah: 4285.0,
      val: 4283.0,
      direction: "bearish",
    });
    assert(!html().includes("Bull Swing"), "opposite swing cleared");
    assert(html().includes("Bear Swing PoC"), "new swing rendered");
    assert(html().includes("CW PoC"), "CW survives the swing flip");
  });

  await test("B. empty snapshot is authoritative (clears everything)", () => {
    App.levels.apply(FIXTURE_WITH_CW);
    App.levels.apply([]);
    assertEqual(cwKeys().length, 0, "CW cleared");
    assert(html().includes("Awaiting data"), "panel back to empty state");
    assertEqual(count(), 0, "count reset");
  });

  await test("C. malformed /levels body is ignored", async () => {
    App.levels.apply(FIXTURE_WITH_CW);
    mockFetch(null);
    await App.levels.fetchAll();
    assertEqual(count(), 10, "state untouched by malformed body");
    assertEqual(cwKeys().length, 3, "CW lines untouched");
  });

  await test("D. reset() clears lines, state and count", () => {
    App.levels.apply(FIXTURE_WITH_CW);
    App.levels.reset();
    assertEqual(chartLines.size, 0, "all price lines removed");
    assert(html().includes("Awaiting data"), "panel cleared");
    assertEqual(count(), 0, "count cleared");
  });

  await test("E. swing anchor metadata row renders without affecting count", () => {
    App.levels.apply(FIXTURE_WITH_CW);
    assert(html().includes("Bull Swing"), "anchor label rendered");
    assert(html().includes("4286.46") && html().includes("4285.46"), "anchor prices rendered");
    assertEqual(count(), 10, "anchor row is not a level");
  });

  /* ---------- summary ---------- */
  console.log(`\n${passed} passed, ${failed} failed`);
  if (failed > 0) process.exitCode = 1;
})();
