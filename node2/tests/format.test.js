// format.test.js — display formatting helpers.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  NA, fmtPrice, fmtMoney, fmtLots, fmtPercent, fmtR, fmtConfidence,
  fmtTime, fmtTimeShort, fmtDuration, fmtCount,
} from "../public/src/lib/format.js";

test("fmtPrice: two decimals with thousands grouping", () => {
  assert.equal(fmtPrice(2661.99), "2,661.99");
  assert.equal(fmtPrice(1234.5), "1,234.50");
  assert.equal(fmtPrice(1234567.8), "1,234,567.80");
});

test("fmtPrice: null/NaN/undefined render as —, never 0", () => {
  assert.equal(fmtPrice(null), NA);
  assert.equal(fmtPrice(undefined), NA);
  assert.equal(fmtPrice(NaN), NA);
  assert.equal(fmtPrice(Infinity), NA);
  assert.equal(fmtPrice("not a number"), NA);
});

test("fmtMoney: signed rendering with plus/minus signs", () => {
  assert.equal(fmtMoney(10000), "10,000.00");
  assert.equal(fmtMoney(10000, { signed: true }), "+10,000.00");
  assert.equal(fmtMoney(-1234.5, { signed: true }), "−1,234.50");
  assert.equal(fmtMoney(254.4, { signed: true }), "+254.40");
  assert.equal(fmtMoney(0, { signed: true }), "0.00");
  assert.equal(fmtMoney(-1234.5), "-1,234.50"); // unsigned keeps the minus
  assert.equal(fmtMoney(null), NA);
});

test("fmtLots: four decimals", () => {
  assert.equal(fmtLots(0.5), "0.5000");
  assert.equal(fmtLots(1.51), "1.5100");
  assert.equal(fmtLots(0), "0.0000");
  assert.equal(fmtLots(null), NA);
});

test("fmtPercent: one decimal, signed", () => {
  assert.equal(fmtPercent(58.3), "58.3%");
  assert.equal(fmtPercent(-4.1, { signed: true }), "−4.1%");
  assert.equal(fmtPercent(4.1, { signed: true }), "+4.1%");
  assert.equal(fmtPercent(null), NA);
});

test("fmtR: signed reward-risk", () => {
  assert.equal(fmtR(1.8), "+1.8");
  assert.equal(fmtR(-0.7), "−0.7");
  assert.equal(fmtR(0), "+0.0");
  assert.equal(fmtR(null), NA);
});

test("fmtConfidence: percent with one decimal", () => {
  assert.equal(fmtConfidence(71.4), "71.4%");
  assert.equal(fmtConfidence(null), NA);
});

test("fmtTime: UTC formatting", () => {
  const ts = Date.UTC(2024, 8, 30, 8, 15, 0); // 2024-09-30T08:15:00Z
  assert.equal(fmtTime(ts), "08:15:00 UTC");
  assert.equal(fmtTimeShort(ts), "08:15 UTC");
  assert.equal(fmtTime(null), NA);
  assert.equal(fmtTime(NaN), NA);
});

test("fmtDuration: seconds/minutes", () => {
  assert.equal(fmtDuration(45000), "45s");
  assert.equal(fmtDuration(125000), "2m05s");
  assert.equal(fmtDuration(null), NA);
});

test("fmtCount", () => {
  assert.equal(fmtCount(2000), "2000");
  assert.equal(fmtCount(null), NA);
});
