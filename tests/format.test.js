"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const format = require("../js/format.js");

const DASH = "\u2014";
const MINUS = "\u2212";

test("prices, lots and counts are fixed-decimal and grouped", () => {
  assert.equal(format.price(3415.1), "3,415.10");
  assert.equal(format.price(0), "0.00");
  assert.equal(format.price(null), DASH);
  assert.equal(format.price(""), DASH);
  assert.equal(format.lots(0.18), "0.18");
  assert.equal(format.lots(1), "1.00");
  assert.equal(format.lots(null), DASH);
  assert.equal(format.count(1000), "1,000");
  assert.equal(format.number(0.30000000000000004, 2), "0.30");
});

test("money is 2 dp with $ and PnL carries an explicit sign", () => {
  assert.equal(format.money(1000), "$1,000.00");
  assert.equal(format.money(0), "$0.00");
  assert.equal(format.money(-100), `${MINUS}$100.00`);
  assert.equal(format.signedMoney(12.4), "+$12.40");
  assert.equal(format.signedMoney(-61), `${MINUS}$61.00`);
  assert.equal(format.signedMoney(0), "$0.00");
  assert.equal(format.signedMoney(null), DASH);
});

test("dimensionless signed values use the same minus glyph and 2 dp", () => {
  assert.equal(format.signedNumber(0.34), "+0.34");
  assert.equal(format.signedNumber(-0.42), `${MINUS}0.42`);
  assert.equal(format.signedNumber(0), "0.00");
  assert.equal(format.signedNumber(null), DASH);
});

test("percent, confidence and R follow the pinned 1 dp / 2 dp rules", () => {
  assert.equal(format.percent(62), "62.0%");
  assert.equal(format.percent(15930.5), "15,930.5%");
  assert.equal(format.percent(null), DASH);
  assert.equal(format.confidence(0.62), "62.0%");
  assert.equal(format.confidence(0), "0.0%");
  assert.equal(format.confidence(null), DASH);
  assert.equal(format.rMultiple(1.85), "+1.85R");
  assert.equal(format.rMultiple(-1), `${MINUS}1.00R`);
  assert.equal(format.rMultiple(0), "0.00R");
  assert.equal(format.rMultiple(null), DASH);
});

test("local time and dates use HH:mm:ss and DD MMM", () => {
  const stamp = new Date(2026, 9, 23, 14, 3, 22).getTime();
  assert.equal(format.time(stamp), "14:03:22");
  assert.equal(format.clock(stamp), "14:03");
  assert.equal(format.date(stamp), "23 Oct");
  assert.equal(format.dateTime(stamp), "23 Oct 14:03:22");
  assert.equal(format.time(null), DASH);
  assert.equal(format.date("not a date"), DASH);
  assert.equal(format.dayLabel(stamp), "Today \u00b7 23 Oct");
});

test("ages read in seconds then minutes and never go negative", () => {
  const now = new Date(2026, 9, 23, 14, 3, 22).getTime();
  assert.equal(format.age(now - 2000, now), "2 s ago");
  assert.equal(format.age(now - 14000, now), "14 s ago");
  assert.equal(format.age(now - 65000, now), "1 min ago");
  assert.equal(format.age(now - 7200000, now), "2 h ago");
  assert.equal(format.age(now + 5000, now), "just now");
  assert.equal(format.age(null, now), DASH);
});

test("null, NaN and infinities always render as an em dash", () => {
  for (const value of [null, undefined, "", "  ", NaN, Infinity, -Infinity, true]) {
    assert.equal(format.price(value), DASH, String(value));
    assert.equal(format.money(value), DASH, String(value));
    assert.equal(format.percent(value), DASH, String(value));
    assert.equal(format.rMultiple(value), DASH, String(value));
    assert.equal(format.time(value), DASH, String(value));
  }
});
