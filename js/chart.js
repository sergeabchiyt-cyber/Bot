/* ============================================================
 * XAUUSD Terminal — chart (lightweight-charts v4.2.0)
 * Candles, historical load, price lines.
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, setStatus, fmtPrice, toSec, numberOrNull, fetchPublicJson } = App.utils;
  const cfg = App.config;

  const el = $("chart");

  if (typeof LightweightCharts === "undefined") {
    console.error("lightweight-charts failed to load");
    setStatus("chart lib error", "error");
    return;
  }

  const size = () => ({
    width: Math.max(1, el.clientWidth),
    height: Math.max(1, el.clientHeight),
  });

  const isNarrow = () => window.matchMedia("(max-width: 720px)").matches;

  /* ---------- Display timeline ----------
   * /candles is a wall-clock window: it contains the weekend/daily-break
   * closed-market filler bars SiftingIO keeps emitting, and it has real holes
   * wherever the upstream feed dropped out. Both render as blank columns, so
   * the series is drawn on the compacted timeline built by App.axis while the
   * axis/crosshair labels still read the true timestamps. */

  /** Identity timeline: only used when axis.js did not load (stale index.html),
   *  so a missing module degrades to the previous behaviour, not to no chart. */
  function passthroughAxis() {
    console.error("axis.js missing — falling back to raw timestamps");
    return {
      compact: (bars) => ({ bars: bars.slice(), dropped: 0 }),
      displayFor: (real) => real,
      displayForReal: (real) => real,
      bucketStart: (real) => real,
      append: (real) => real,
      realFor: (t) => t,
      lastDisplay: null,
      lastReal: null,
      length: 0,
      bucket: 15 * 60,
    };
  }

  const axis = App.axis ? App.axis.create() : passthroughAxis();
  const isClosedMarketBar = App.axis
    ? App.axis.isClosedMarketBar
    : (bar) => !!bar && bar.open === bar.close;

  const MONTHS_SHORT = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  const pad2 = (n) => String(n).padStart(2, "0");
  /** Date whose local fields are `sec`'s UTC fields (what the axis shows). */
  const clock = (sec) => (App.axis ? App.axis.utcClock(sec) : new Date(sec * 1000));

  /** Format `sec` the way lightweight-charts formats UTC timestamps. */
  function fmtTick(time, tickMarkType) {
    const sec = axis.realFor(Number(time));
    if (!Number.isFinite(sec)) return null;
    const d = clock(sec);
    switch (tickMarkType) {
      case LightweightCharts.TickMarkType.Year:
        return String(d.getFullYear());
      case LightweightCharts.TickMarkType.Month:
        return `${MONTHS_SHORT[d.getMonth()]} '${pad2(d.getFullYear() % 100)}`;
      case LightweightCharts.TickMarkType.DayOfMonth:
        return String(d.getDate());
      case LightweightCharts.TickMarkType.TimeWithSeconds:
        return `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`;
      default: // TickMarkType.Time
        return `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
    }
  }

  /** Crosshair time label: real date + clock, same shape as the default. */
  function fmtTitle(time) {
    const sec = axis.realFor(Number(time));
    if (!Number.isFinite(sec)) return String(time);
    const d = clock(sec);
    return `${pad2(d.getDate())} ${MONTHS_SHORT[d.getMonth()]} '${pad2(d.getFullYear() % 100)}, ` +
           `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
  }

  const chart = LightweightCharts.createChart(el, {
    ...size(),
    layout: {
      background: { color: "#0B0E11" },
      textColor: "#7D8590",
      fontFamily: "'Inter', -apple-system, sans-serif",
      fontSize: isNarrow() ? 10 : 11,
    },
    grid: {
      vertLines: { color: "rgba(255,255,255,0.03)" },
      horzLines: { color: "rgba(255,255,255,0.03)" },
    },
    crosshair: {
      mode: LightweightCharts.CrosshairMode.Normal,
      vertLine: { color: "rgba(255,255,255,0.15)", width: 1, style: 2 },
      horzLine: { color: "rgba(255,255,255,0.15)", width: 1, style: 2 },
    },
    localization: {
      // The series runs on the compacted timeline; labels must not lie about it.
      timeFormatter: fmtTitle,
    },
    rightPriceScale: {
      borderColor: "rgba(255,255,255,0.06)",
      scaleMargins: {
        top: 0.1,
        bottom: 0.25,
      },
    },
    timeScale: {
      borderColor: "rgba(255,255,255,0.06)",
      timeVisible: true,
      secondsVisible: false,
      rightOffset: 4,
      // fitContent() must be able to frame the whole 2,000-bar payload on a
      // phone too, so the floor is well below one pixel per bar.
      minBarSpacing: 0.1,
      tickMarkFormatter: fmtTick,
    },
    handleScale: { axisPressedMouseMove: true, pinch: true },
    handleScroll: { horzTouchDrag: true, vertTouchDrag: false },
  });

  const series = chart.addCandlestickSeries({
    upColor: "#26A69A",
    downColor: "#EF5350",
    borderUpColor: "#26A69A",
    borderDownColor: "#EF5350",
    wickUpColor: "#26A69A",
    wickDownColor: "#EF5350",
  });

  const volumeSeries = chart.addHistogramSeries({
    priceFormat: {
      type: "volume",
    },
    priceScaleId: "", // Overlay scale: separate from candlestick price scale
  });

  volumeSeries.priceScale().applyOptions({
    scaleMargins: {
      top: 0.8,
      bottom: 0,
    },
  });

  const COLOR_UP = "#26A69A";
  const COLOR_DOWN = "#EF5350";

  // Cache candles by bucket time (sec) for color fallbacks. REST snapshots
  // capture these WS revisions and are discarded if newer market data arrived
  // while the request was in flight.
  const candlesByTime = new Map();
  let wsCandleRevision = 0;
  let wsTickVolumeRevision = 0;
  let historyFetchInProgress = false;

  // ---------- Top bar price & TPS ----------
  let lastClose = null;
  function updateLastPrice(close) {
    const price = numberOrNull(close);
    if (price === null) return;
    const priceEl = $("last-price");
    if (!priceEl) return;
    priceEl.textContent = fmtPrice(price);
    if (lastClose != null && price !== lastClose) {
      priceEl.dataset.dir = price > lastClose ? "up" : "down";
    }
    lastClose = price;
  }

  function updateTps(rate) {
    const val = numberOrNull(rate);
    if (val === null) return;
    const text = val.toFixed(1);
    const tpsSec = $("ticks-per-sec");
    if (tpsSec) tpsSec.textContent = text;
    const tpsVal = $("tps-value");
    if (tpsVal && !tpsSec) tpsVal.textContent = text;
    const tpsPill = $("tps");
    if (tpsPill) tpsPill.dataset.rate = text;
  }

  /**
   * Determine volume bar color:
   * 1. Green or red by whether up-ticks or down-ticks win.
   * 2. If tied (or neither up_ticks nor down_ticks are present):
   *    fall back to the candle's colour (green if close >= open, else red).
   */
  function getBarColor(bar, candle) {
    const up = bar ? numberOrNull(bar.up_ticks) : null;
    const down = bar ? numberOrNull(bar.down_ticks) : null;
    const barClose = bar ? numberOrNull(bar.close) : null;

    if (up !== null && down !== null && up !== down) {
      return up > down ? COLOR_UP : COLOR_DOWN;
    }

    if (candle && Number.isFinite(candle.close) && Number.isFinite(candle.open)) {
      return candle.close >= candle.open ? COLOR_UP : COLOR_DOWN;
    }

    if (barClose !== null && candle && Number.isFinite(candle.open)) {
      return barClose >= candle.open ? COLOR_UP : COLOR_DOWN;
    }

    return COLOR_UP;
  }

  // ---------- History ----------
  /**
   * Backend `/candles` returns SiftingIO candle objects (ms timestamps):
   *   { time, open, high, low, close, volume, source }
   * Binance REST is never called from the browser.
   */
  function parseCandles(raw) {
    if (!Array.isArray(raw)) return [];
    const byTime = new Map();
    for (const c of raw) {
      if (!c) continue;
      const time = toSec(c.time);
      const open = numberOrNull(c.open);
      const high = numberOrNull(c.high);
      const low = numberOrNull(c.low);
      const close = numberOrNull(c.close);
      const candle = {
        time,
        open,
        high,
        low,
        close,
        volume: numberOrNull(c.volume) ?? 0,
      };
      if (time == null || open === null || high === null || low === null || close === null) {
        continue;
      }
      byTime.set(time, candle); // de-dupe: last candle for a bucket wins
    }
    return [...byTime.values()].sort((a, b) => a.time - b.time);
  }

  /**
   * Display slot for a real bucket: the known mapping, a fresh contiguous slot
   * for a live bar, or null when the bar must not be drawn at all — the venue's
   * closed-market placeholder (a bodyless bar, i.e. the dead flat slab while
   * the market is shut) or a bucket older than the loaded window (which the
   * chart cannot place without a full reload).
   *
   * `bar` is the bar being drawn, not the cached one: when the market reopens,
   * the first real bar replaces the placeholder of the *same* bucket and must
   * be drawn.
   */
  function displaySlotFor(realSec, bar) {
    const known = axis.displayFor(realSec);
    if (known != null) return known;
    if (isClosedMarketBar(bar)) return null;
    if (axis.lastReal != null && realSec < axis.lastReal) return null;
    return axis.append(realSec);
  }

  async function loadHistory() {
    if (historyFetchInProgress || !cfg.endpoints.candles) return;
    historyFetchInProgress = true;
    const requestCandleRevision = wsCandleRevision;
    const requestTickVolumeRevision = wsTickVolumeRevision;
    setStatus("loading", "busy");
    try {
      const raw = await fetchPublicJson(cfg.endpoints.candles);
      const data = parseCandles(raw);
      if (!data.length) throw new Error("market candles: empty payload");

      // A newer candle frame makes this whole REST snapshot stale. Do not reset
      // the series, timeline, or last price with a slower response.
      if (wsCandleRevision === requestCandleRevision) {
        candlesByTime.clear();
        for (const c of data) candlesByTime.set(c.time, c);

        const view = axis.compact(data);
        const volumeData = view.bars.map((c) => ({
          time: c.time,
          value: c.volume,
          color: c.close >= c.open ? COLOR_UP : COLOR_DOWN,
        }));

        series.setData(view.bars);
        if (wsTickVolumeRevision === requestTickVolumeRevision) volumeSeries.setData(volumeData);
        chart.timeScale().fitContent();
        const newest = view.bars.length ? view.bars[view.bars.length - 1] : data[data.length - 1];
        updateLastPrice(newest.close);

        if (view.dropped) {
          console.info(`chart: dropped ${view.dropped} closed-market filler bars`);
        }

        if (cfg.endpoints.tickVolume && wsTickVolumeRevision === requestTickVolumeRevision) {
          try {
            const tvBars = await fetchPublicJson(cfg.endpoints.tickVolume);
            if (wsCandleRevision === requestCandleRevision &&
                wsTickVolumeRevision === requestTickVolumeRevision && Array.isArray(tvBars)) {
              for (const bar of tvBars) updateTickVolume(bar, "rest");
            }
          } catch {
            // Non-fatal: the market WebSocket remains the primary feed.
          }
        }
      }
      setStatus("ready", "busy");
    } catch (error) {
      console.error("Market history load failed:", error);
      setStatus("history error", "error");
    } finally {
      historyFetchInProgress = false;
    }
  }

  /** Live candle from the backend WebSocket (same shape as `/candles`). */
  function updateCandle(c) {
    if (!c) return;
    const time = toSec(c.time);
    if (time == null) return;
    const open = numberOrNull(c.open);
    const high = numberOrNull(c.high);
    const low = numberOrNull(c.low);
    const close = numberOrNull(c.close);
    const candle = {
      time,
      open,
      high,
      low,
      close,
      volume: numberOrNull(c.volume) ?? 0,
    };
    if (open === null || high === null || low === null || close === null) {
      return;
    }
    wsCandleRevision += 1;
    const prev = candlesByTime.get(time);
    if (prev && isClosedMarketBar(prev) && !isClosedMarketBar(candle)) {
      // The market just reopened inside this bucket: its placeholder must not
      // leak its synthetic high/low into the first real candle.
      candlesByTime.set(time, candle);
    } else if (prev) {
      prev.high = Math.max(prev.high, candle.high);
      prev.low = Math.min(prev.low, candle.low);
      prev.close = candle.close;
      if (c.volume != null) prev.volume = candle.volume;
    } else {
      candlesByTime.set(time, candle);
    }

    const displaySec = displaySlotFor(time, candle);
    if (displaySec == null) return; // closed-market placeholder: nothing to draw

    series.update({ ...candle, time: displaySec });
    updateLastPrice(candle.close);
  }

  /** Live tick volume bar from Node 1, or a guarded /tick-volume snapshot. */
  function updateTickVolume(bar, source = "ws") {
    if (!bar) return;
    const time = toSec(bar.time);
    if (time == null) return;

    const ticks = numberOrNull(bar.ticks) ?? numberOrNull(bar.volume) ?? 0;
    if (source !== "rest") wsTickVolumeRevision += 1;

    const candle = candlesByTime.get(time);
    const barClose = numberOrNull(bar.close);
    if (candle && barClose !== null) candle.close = barClose;

    // Volume follows its candle onto the display timeline. A bucket that is
    // not on the chart yet may only extend it when its candle is drawable
    // (the backend sends "candle" before "tick_volume"), so a stranded bar
    // can never stretch the timeline past the live edge.
    let displaySec = axis.displayFor(time);
    if (displaySec == null) {
      if (!candle) return;
      displaySec = displaySlotFor(time, candle);
      if (displaySec == null) return; // closed-market filler or stale bucket
    }

    const color = getBarColor(bar, candle);

    volumeSeries.update({
      time: displaySec,
      value: ticks,
      color,
    });

    if (bar.ticks_per_sec != null) {
      updateTps(bar.ticks_per_sec);
    }

    if (bar.close != null) {
      updateLastPrice(bar.close);
    }
  }

  // ---------- Price lines ----------
  const priceLines = {};

  function upsertPriceLine(key, price, style) {
    const value = Number(price);
    if (!Number.isFinite(value) || !style) return;
    removePriceLine(key);
    priceLines[key] = series.createPriceLine({
      price: value,
      color: style.color,
      lineWidth: 1,
      lineStyle: style.dashed
        ? LightweightCharts.LineStyle.Dashed
        : LightweightCharts.LineStyle.Solid,
      axisLabelVisible: true,
      title: isNarrow() ? "" : style.title, // axis label only on mobile
    });
  }

  /** Drop a single price line, e.g. the swing profile that just went stale. */
  function removePriceLine(key) {
    if (!priceLines[key]) return;
    series.removePriceLine(priceLines[key]);
    delete priceLines[key];
  }

  function clearPriceLines() {
    for (const k of Object.keys(priceLines)) {
      series.removePriceLine(priceLines[k]);
      delete priceLines[k];
    }
  }

  /** Force the primitive layer to repaint. */
  function repaint() {
    chart.applyOptions({});
    series.applyOptions({});
    volumeSeries.applyOptions({});
  }

  new ResizeObserver(() => chart.applyOptions(size())).observe(el);

  App.chart = {
    chart,
    series,
    volumeSeries,
    axis,
    loadHistory,
    updateCandle,
    updateTickVolume,
    updateTps,
    upsertPriceLine,
    removePriceLine,
    clearPriceLines,
    repaint,
  };
})((window.App = window.App || {}));
