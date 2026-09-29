/* ============================================================
 * XAUUSD Terminal — display time axis
 *
 * The backend's /candles window is NOT contiguous, and that is by design:
 *   1. The venue is shut every weekend and for the 21:00-22:00 UTC daily
 *      break, and SiftingIO keeps emitting a *bodyless* bar (open === close)
 *      for every closed bucket — a dead flat slab at the last traded price.
 *   2. Upstream outages leave buckets with no bar at all, so the payload has
 *      real holes in it (e.g. 13:00 -> 18:30 UTC on a live capture).
 *
 * lightweight-charts lays every series out by timestamp, so both show up as
 * blank columns on the chart: the "gap" that makes the 2,000-bar view look
 * broken, plus long flat shelves where the market was closed.
 *
 * This module maps the payload onto a *display* timeline where consecutive
 * bars are exactly one bucket apart, so candles are always contiguous no
 * matter what the feed does — and drops the closed-market filler runs.
 *
 * The newest bar keeps its real timestamp (the timeline is anchored at the
 * live edge), so the part of the chart the user actually watches stays on the
 * real clock. Older bars are shifted by the closed/absent time that was
 * removed, and `realFor()` recovers the true timestamp of any display time so
 * the axis and crosshair labels can still show real dates and times.
 * ============================================================ */
(function (App) {
  "use strict";

  const BUCKET_SEC = 15 * 60;  // candle duration (15m)
  const MIN_FILLER_RUN = 3;    // closed-market runs shorter than this are kept

  /**
   * SiftingIO's signature for a bucket it could not trade in: the previous
   * price repeated with no body. A single bodyless bar can be genuine price
   * action (a doji), so callers drop only *runs* of them.
   */
  function isClosedMarketBar(bar) {
    return !!bar && bar.open === bar.close;
  }

  /**
   * Remove every maximal run of `minRun` or more bodyless bars.
   * @returns {{bars: Array, dropped: number}}
   */
  function dropClosedMarketRuns(bars, minRun) {
    const keep = [];
    const limit = minRun || MIN_FILLER_RUN;
    let dropped = 0;
    let i = 0;
    while (i < bars.length) {
      if (!isClosedMarketBar(bars[i])) {
        keep.push(bars[i]);
        i += 1;
        continue;
      }
      let j = i;
      while (j < bars.length && isClosedMarketBar(bars[j])) j += 1;
      if (j - i >= limit) {
        dropped += j - i; // market was closed: no candle to draw
      } else {
        for (; i < j; i += 1) keep.push(bars[i]); // short run: real dojis
      }
      i = j;
    }
    return { bars: keep, dropped };
  }

  /** UTC wall-clock fields of `sec` on a local Date, the way
   *  lightweight-charts' own formatters read timestamps. */
  function utcClock(sec) {
    const d = new Date(sec * 1000);
    return new Date(
      d.getUTCFullYear(),
      d.getUTCMonth(),
      d.getUTCDate(),
      d.getUTCHours(),
      d.getUTCMinutes(),
      d.getUTCSeconds(),
      d.getUTCMilliseconds()
    );
  }

  /**
   * Create the mapping between real payload timestamps and the compacted
   * display timeline used by the chart series.
   */
  function create(options) {
    const opts = options || {};
    const bucket = opts.bucket || BUCKET_SEC;
    const minRun = opts.minRun || MIN_FILLER_RUN;

    let displayTimes = [];      // ascending display seconds of every drawn bar
    let realTimes = [];         // true timestamp of each drawn bar (same index)
    const byReal = new Map();   // real sec  -> display sec
    let lastDisplay = null;
    let lastReal = null;

    /** Index of the newest drawn bar at or before `displaySec` (or 0). */
    function floorIndex(displaySec) {
      let lo = 0;
      let hi = displayTimes.length - 1;
      if (hi < 0) return -1;
      if (displaySec <= displayTimes[0]) return 0;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (displayTimes[mid] <= displaySec) lo = mid;
        else hi = mid - 1;
      }
      return lo;
    }

    /**
     * Rebuild the timeline from a history payload (ascending, de-duplicated,
     * real timestamps). Returns the bars to draw, re-timed onto the display
     * timeline; the input array is not modified.
     */
    function compact(bars) {
      const filtered = dropClosedMarketRuns(bars, minRun);
      const kept = filtered.bars;

      displayTimes = [];
      realTimes = [];
      byReal.clear();
      lastDisplay = null;
      lastReal = null;

      if (!kept.length) return { bars: [], dropped: filtered.dropped };

      // Anchor on the newest bar: display(n) === real(n). Every earlier bar is
      // exactly one bucket before it, which closes every hole in the payload.
      const firstDisplay = kept[kept.length - 1].time - (kept.length - 1) * bucket;
      const out = new Array(kept.length);
      for (let i = 0; i < kept.length; i += 1) {
        const bar = kept[i];
        const display = firstDisplay + i * bucket;
        const view = {
          time: display,
          open: bar.open,
          high: bar.high,
          low: bar.low,
          close: bar.close,
          volume: bar.volume,
        };
        out[i] = view;
        displayTimes.push(display);
        realTimes.push(bar.time);
        byReal.set(bar.time, display);
        lastDisplay = display;
        lastReal = bar.time;
      }
      return { bars: out, dropped: filtered.dropped };
    }

    /** Display time of a known real bucket, or null when it is not on the chart. */
    function displayFor(realSec) {
      return byReal.has(realSec) ? byReal.get(realSec) : null;
    }

    /** Start of the bucket `realSec` belongs to. */
    function bucketStart(realSec) {
      return Math.floor(realSec / bucket) * bucket;
    }

    /**
     * Display time of the candle a real instant belongs to, in seconds.
     * Timestamps inside a bucket (order-flow events, a trade at 12:37) resolve
     * to that candle's column; instants inside a dropped closed-market run, or
     * older than the loaded window, resolve to null.
     */
    function displayForReal(realSec) {
      const t = Number(realSec);
      if (!Number.isFinite(t)) return null;
      return displayFor(bucketStart(t));
    }

    /**
     * Reserve the next display slot for a live bucket that is newer than the
     * loaded window. The slot is one bucket after the newest bar, so a gap
     * that opens up live (an outage across a session break) still cannot
     * punch a hole in the chart.
     */
    function append(realSec) {
      if (lastDisplay == null) {
        lastDisplay = realSec;
      } else {
        lastDisplay += bucket;
      }
      lastReal = realSec;
      displayTimes.push(lastDisplay);
      realTimes.push(realSec);
      byReal.set(realSec, lastDisplay);
      return lastDisplay;
    }

    /**
     * True timestamp behind a display time. Ticks can land between bars, so
     * the nearest drawn bar at or before it wins; past the live edge the
     * timeline continues at its real offset (the "future" whitespace).
     */
    function realFor(displaySec) {
      const i = floorIndex(displaySec);
      if (i < 0) return displaySec;
      if (lastDisplay != null && displaySec > lastDisplay) {
        return lastReal + (displaySec - lastDisplay);
      }
      return realTimes[i];
    }

    return {
      compact: compact,
      displayFor: displayFor,
      displayForReal: displayForReal,
      bucketStart: bucketStart,
      append: append,
      realFor: realFor,
      get lastDisplay() { return lastDisplay; },
      get lastReal() { return lastReal; },
      get length() { return displayTimes.length; },
      bucket: bucket,
    };
  }

  App.axis = {
    BUCKET_SEC: BUCKET_SEC,
    MIN_FILLER_RUN: MIN_FILLER_RUN,
    create: create,
    isClosedMarketBar: isClosedMarketBar,
    dropClosedMarketRuns: dropClosedMarketRuns,
    utcClock: utcClock,
  };
})((window.App = window.App || {}));
