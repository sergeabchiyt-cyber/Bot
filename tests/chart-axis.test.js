/* ============================================================
 * Frontend tests — display time axis (js/axis.js)
 *
 * The backend's /candles window is a wall-clock window, not a contiguous
 * series: SiftingIO emits a bodyless filler bar (open === close) for every
 * closed bucket (weekend, 21:00-22:00 UTC daily break) and real buckets go
 * missing during upstream outages. Both used to render as blank columns —
 * the "gap" in the chart.
 *
 * These tests pin the compaction contract that removes them:
 *   - every drawn bar is exactly one 15m bucket after the previous one,
 *   - closed-market filler runs are dropped, short doji runs are kept,
 *   - the newest bar keeps its real timestamp (the live edge),
 *   - display time -> real time stays exact for the axis/crosshair labels,
 *   - a live bucket arriving after an outage stays visually contiguous.
 *
 * Runs with plain Node (no dependencies, no DOM):
 *
 *   node tests/chart-axis.test.js
 * ============================================================ */
"use strict";

const path = require("path");

global.window = global;
require(path.join(__dirname, "..", "js", "axis.js"));
const axis = window.App.axis;

const BUCKET = axis.BUCKET_SEC;
const MIN = 60;

let passed = 0;
let failed = 0;

function test(name, fn) {
  try {
    fn();
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

/* ---------- payload builders (SiftingIO shapes) ---------- */

/** Normal traded 15m bar. */
function bar(time, close) {
  return {
    time,
    open: close - 0.4,
    high: close + 0.9,
    low: close - 1.1,
    close,
    volume: 4200,
  };
}

/** Closed-market filler: the previous price repeated, no body. */
function filler(time, price) {
  return { time, open: price, high: price + 0.65, low: price, close: price, volume: 4600 };
}

/** `n` consecutive traded bars starting at `start` (exclusive), 900s apart. */
function session(start, n, price) {
  const out = [];
  for (let i = 0; i < n; i += 1) out.push(bar(start + (i + 1) * BUCKET, price + (i % 7) * 0.3));
  return out;
}

function stepsOf(bars) {
  const seen = new Set();
  for (let i = 1; i < bars.length; i += 1) seen.add(bars[i].time - bars[i - 1].time);
  return [...seen].sort((a, b) => a - b);
}

/* ---------- tests ---------- */

test("contiguous history is passed through untouched", () => {
  const real = session(0, 40, 4300);
  const a = axis.create();
  const out = a.compact(real);
  assertEqual(out.dropped, 0, "nothing dropped");
  assertEqual(out.bars.length, 40, "all bars drawn");
  assertEqual(a.length, 40, "axis length");
  assertEqual(stepsOf(out.bars).join(), String(BUCKET), "one bucket step");
  for (let i = 0; i < real.length; i += 1) {
    assertEqual(out.bars[i].time, real[i].time, `bar ${i} keeps its timestamp`);
  }
});

test("a weekend filler run is dropped and the chart stays contiguous", () => {
  const friday = session(0, 30, 4300);
  const last = friday[friday.length - 1].time;
  const weekend = [];
  for (let i = 1; i <= 96; i += 1) weekend.push(filler(last + i * BUCKET, 4290)); // 24h closed
  const monday = session(last + 96 * BUCKET, 30, 4280);

  const a = axis.create();
  const out = a.compact([...friday, ...weekend, ...monday]);

  assertEqual(out.dropped, 96, "weekend filler dropped");
  assertEqual(out.bars.length, 60, "only traded bars are drawn");
  assertEqual(stepsOf(out.bars).join(), String(BUCKET), "no blank column left between Friday and Monday");
  assertEqual(out.bars[out.bars.length - 1].time, monday[monday.length - 1].time, "live edge keeps its real timestamp");
});

test("the 21:00-22:00 UTC daily break block is dropped", () => {
  const before = session(0, 20, 4300);
  const last = before[before.length - 1].time;
  const brk = [];
  for (let i = 1; i <= 4; i += 1) brk.push(filler(last + i * BUCKET, 4295));
  const after = session(last + 4 * BUCKET, 20, 4290);

  const out = axis.create().compact([...before, ...brk, ...after]);
  assertEqual(out.dropped, 4, "daily break dropped");
  assertEqual(stepsOf(out.bars).join(), String(BUCKET), "contiguous across the break");
});

test("short bodyless runs (real dojis) are kept", () => {
  const a = axis.create();
  const bars = [
    bar(0, 4300),
    { time: BUCKET, open: 4300, high: 4300.2, low: 4299.8, close: 4300, volume: 900 }, // doji
    { time: 2 * BUCKET, open: 4300, high: 4300.1, low: 4299.9, close: 4300, volume: 800 }, // doji
    bar(3 * BUCKET, 4301),
  ];
  const out = a.compact(bars);
  assertEqual(out.dropped, 0, "a two-bar doji run is data, not a closed market");
  assertEqual(out.bars.length, 4, "all four bars drawn");
});

test("real holes in the payload are closed", () => {
  const head = session(0, 10, 4300);
  const lastHead = head[head.length - 1].time;
  const tail = session(lastHead + 20 * BUCKET, 10, 4290); // 19 buckets missing (outage)

  const a = axis.create();
  const out = a.compact([...head, ...tail]);
  assertEqual(out.dropped, 0, "nothing is a filler");
  assertEqual(stepsOf(out.bars).join(), String(BUCKET), "outage hole closed");
});

test("display time maps back to the exact real timestamp of every bar", () => {
  const head = session(0, 12, 4300);
  const lastHead = head[head.length - 1].time;
  const brk = [1, 2, 3].map((i) => filler(lastHead + i * BUCKET, 4300));
  const tail = session(lastHead + 3 * BUCKET + 6 * BUCKET, 12, 4290); // + outage

  const a = axis.create();
  const out = a.compact([...head, ...brk, ...tail]);

  // exact round trip against the source bars
  const source = [...head, ...tail];
  assertEqual(a.length, source.length, "bar count");
  for (let i = 0; i < source.length; i += 1) {
    assertEqual(a.realFor(out.bars[i].time), source[i].time, `bar ${i} real time`);
    assertEqual(a.displayFor(source[i].time), out.bars[i].time, `bar ${i} display time`);
  }
  // a display tick that lands between bars resolves to the previous real bar
  assertEqual(a.realFor(out.bars[3].time + 1), source[3].time, "tick between bars");
  // past the live edge the timeline continues at its offset (right whitespace)
  assertEqual(a.realFor(a.lastDisplay + 4 * BUCKET), a.lastReal + 4 * BUCKET, "right offset");
});

test("displayFor() answers only for buckets that are on the chart", () => {
  const head = session(0, 6, 4300);
  const lastHead = head[head.length - 1].time;
  const closed = [lastHead + BUCKET, lastHead + 2 * BUCKET]; // weekend filler
  const tail = session(lastHead + 2 * BUCKET, 6, 4290);

  const a = axis.create();
  a.compact([...head, ...closed, ...tail]);
  assertEqual(a.displayFor(head[0].time), head[0].time, "first traded bar is known");
  assertEqual(a.displayFor(closed[0]), null, "dropped filler is unknown");
  assertEqual(a.displayFor(tail[0].time) != null, true, "session bars after the break are known");
});

test("a live bucket that arrives after an outage stays contiguous", () => {
  const head = session(0, 8, 4300);
  const a = axis.create();
  a.compact(head);

  // 13:00 real, next live bucket is 18:30 real (5.5h outage): visually +15m
  const lastReal = head[head.length - 1].time;
  const gapBucket = lastReal + 22 * BUCKET;
  const slot = a.append(gapBucket);

  assertEqual(slot, a.lastDisplay, "append returns the new slot");
  assertEqual(slot - head[head.length - 1].time, BUCKET, "exactly one bucket after the last bar — no hole");
  assertEqual(a.realFor(slot), gapBucket, "the live bar still knows its real time");
});

test("an instant inside a bucket resolves to that candle's column", () => {
  const head = session(0, 10, 4300);
  const lastHead = head[head.length - 1].time;
  const tail = session(lastHead + 10 * BUCKET, 10, 4290); // outage between sessions

  const a = axis.create();
  a.compact([...head, ...tail]);

  const candleReal = tail[4].time;
  assertEqual(a.displayForReal(candleReal), a.displayFor(candleReal), "exact bucket start");
  assertEqual(a.displayForReal(candleReal + 7 * MIN + 33), a.displayFor(candleReal), "a trade at :37:33 lands on its candle");
  assertEqual(a.displayForReal(head[2].time + 60), a.displayFor(head[2].time), "older session too");

  // an instant in a dropped closed-market run has no column
  const brk = [];
  const last = tail[tail.length - 1].time;
  for (let i = 1; i <= 8; i += 1) brk.push(filler(last + i * BUCKET, 4290));
  const b = axis.create();
  b.compact([...head, ...tail, ...brk]);
  assertEqual(b.displayForReal(last + 9 * BUCKET), null, "closed market has no candle");
  assertEqual(b.displayForReal(NaN), null, "non-finite input");
});

test("empty or single-bar input cannot break the axis", () => {
  const a = axis.create();
  const empty = a.compact([]);
  assertEqual(empty.bars.length, 0, "no bars");
  assertEqual(a.length, 0, "no timeline");
  assertEqual(a.realFor(1234), 1234, "realFor passes unknown time through");

  const one = a.compact([bar(1000, 4300)]);
  assertEqual(one.bars.length, 1, "single bar drawn");
  assertEqual(one.bars[0].time, 1000, "single bar anchors on itself");
  assertEqual(stepsOf(one.bars).length, 0, "no steps to check");
});

test("full real-payload shape: filler runs + outages leave exactly one step", () => {
  // 2000 buckets of wall clock (23 days of 15m), weekend + daily breaks as
  // filler, and ~220 buckets missing like the live capture.
  const bars = [];
  const outages = [[300, 14], [700, 9], [1100, 12], [1500, 6]];
  const missing = new Set();
  for (const [start, n] of outages) for (let i = 0; i < n; i += 1) missing.add(start + i);

  let fillerCount = 0;
  let lastTraded = null;
  for (let i = 0; i < 2000; i += 1) {
    const t = i * BUCKET;
    if (missing.has(i)) continue;
    const dayOfWeek = Math.floor((i * BUCKET) / (24 * 3600)) % 7;
    const isWeekend = dayOfWeek === 5 || dayOfWeek === 6;
    const hourOfDay = ((i * BUCKET) / 3600) % 24;
    const isDailyBreak = hourOfDay >= 21 && hourOfDay < 22;
    if (isWeekend || isDailyBreak) {
      bars.push(filler(t, 4200));
      fillerCount += 1;
    } else {
      bars.push(bar(t, 4200 + Math.sin(i / 30) * 8));
      lastTraded = t;
    }
  }

  const a = axis.create();
  const out = a.compact(bars);
  const realSpan = bars[bars.length - 1].time - bars[0].time;

  assert(
    fillerCount > 200,
    `fixture should contain a real weekend/daily-break share, got ${fillerCount} filler bars`
  );
  assertEqual(out.dropped, fillerCount, "every closed-market filler bar is dropped");
  assertEqual(stepsOf(out.bars).join(), String(BUCKET), "every drawn bar is one bucket apart");
  // The fixture ends inside a closed market, so the newest *drawn* bar is the
  // newest traded one — and it must still carry its real timestamp.
  assertEqual(out.bars[out.bars.length - 1].time, lastTraded, "live edge is real time");

  const drawn = out.bars.length;
  const displaySpan = out.bars[drawn - 1].time - out.bars[0].time;
  assertEqual(displaySpan, (drawn - 1) * BUCKET, "display span matches the drawn bar count");
  assert(displaySpan < realSpan, "compaction removes the closed/absent time");
  assertEqual(
    realSpan - displaySpan,
    (2000 - 1) * BUCKET - (drawn - 1) * BUCKET,
    "the removed span is exactly the bars that were not drawn"
  );
});

/* ---------- summary ---------- */
console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) process.exitCode = 1;
