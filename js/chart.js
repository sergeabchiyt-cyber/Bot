/* ============================================================
 * XAUUSD Terminal — chart (lightweight-charts v4.2.0)
 * Candles, historical load, price lines.
 * ============================================================ */
(function (App) {
  "use strict";

  const { $, setStatus, fmtPrice, toSec } = App.utils;
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

  // Cache candles by bucket time (sec) for color fallbacks
  const candlesByTime = new Map();

  // ---------- Top bar price & TPS ----------
  let lastClose = null;
  function updateLastPrice(close) {
    const price = Number(close);
    if (!Number.isFinite(price)) return;
    const priceEl = $("last-price");
    if (!priceEl) return;
    priceEl.textContent = fmtPrice(price);
    if (lastClose != null && price !== lastClose) {
      priceEl.dataset.dir = price > lastClose ? "up" : "down";
    }
    lastClose = price;
  }

  function updateTps(rate) {
    const val = Number(rate);
    if (!Number.isFinite(val)) return;
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
    const up = bar && bar.up_ticks != null ? Number(bar.up_ticks) : null;
    const down = bar && bar.down_ticks != null ? Number(bar.down_ticks) : null;

    if (up != null && down != null && Number.isFinite(up) && Number.isFinite(down) && up !== down) {
      return up > down ? COLOR_UP : COLOR_DOWN;
    }

    if (candle && Number.isFinite(candle.close) && Number.isFinite(candle.open)) {
      return candle.close >= candle.open ? COLOR_UP : COLOR_DOWN;
    }

    if (bar && Number.isFinite(Number(bar.close)) && candle && Number.isFinite(candle.open)) {
      return Number(bar.close) >= candle.open ? COLOR_UP : COLOR_DOWN;
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
      const candle = {
        time,
        open: Number(c.open),
        high: Number(c.high),
        low: Number(c.low),
        close: Number(c.close),
        volume: Number(c.volume) || 0,
      };
      if (
        time == null ||
        !Number.isFinite(candle.open) ||
        !Number.isFinite(candle.high) ||
        !Number.isFinite(candle.low) ||
        !Number.isFinite(candle.close)
      ) {
        continue;
      }
      byTime.set(time, candle); // de-dupe: last candle for a bucket wins
    }
    return [...byTime.values()].sort((a, b) => a.time - b.time);
  }

  async function loadHistory() {
    setStatus("loading", "busy");
    try {
      const res = await fetch(cfg.endpoints.candles);
      if (!res.ok) throw new Error(`backend candles ${res.status}`);
      const raw = await res.json();
      const data = parseCandles(raw);
      if (!data.length) throw new Error("backend candles: empty payload");

      candlesByTime.clear();
      const volumeData = [];
      for (const c of data) {
        candlesByTime.set(c.time, c);
        volumeData.push({
          time: c.time,
          value: c.volume,
          color: c.close >= c.open ? COLOR_UP : COLOR_DOWN,
        });
      }

      series.setData(data);
      volumeSeries.setData(volumeData);
      chart.timeScale().fitContent();
      updateLastPrice(data[data.length - 1].close);
      setStatus("ready", "busy"); // WS flips this to "live" on open

      if (cfg.endpoints.tickVolume) {
        try {
          const tvRes = await fetch(cfg.endpoints.tickVolume);
          if (tvRes.ok) {
            const tvBars = await tvRes.json();
            if (Array.isArray(tvBars)) {
              for (const bar of tvBars) {
                updateTickVolume(bar);
              }
            }
          }
        } catch {
          // Non-fatal: WS tick_volume replay handles live backfill
        }
      }
    } catch (e) {
      console.error("History load failed:", e);
      setStatus("history error", "error");
    }
  }

  /** Live candle from the backend WebSocket (same shape as `/candles`). */
  function updateCandle(c) {
    if (!c) return;
    const time = toSec(c.time);
    if (time == null) return;
    const candle = {
      time,
      open: Number(c.open),
      high: Number(c.high),
      low: Number(c.low),
      close: Number(c.close),
      volume: c.volume != null ? Number(c.volume) : 0,
    };
    if (
      !Number.isFinite(candle.open) ||
      !Number.isFinite(candle.high) ||
      !Number.isFinite(candle.low) ||
      !Number.isFinite(candle.close)
    ) {
      return;
    }
    const prev = candlesByTime.get(time);
    if (prev) {
      prev.high = Math.max(prev.high, candle.high);
      prev.low = Math.min(prev.low, candle.low);
      prev.close = candle.close;
      if (c.volume != null) prev.volume = candle.volume;
    } else {
      candlesByTime.set(time, candle);
    }
    series.update(candle);
    updateLastPrice(candle.close);
  }

  /** Live tick volume bar from the backend WebSocket or /tick-volume. */
  function updateTickVolume(bar) {
    if (!bar) return;
    const time = toSec(bar.time);
    if (time == null) return;

    const ticks = bar.ticks != null
      ? Number(bar.ticks)
      : (bar.volume != null ? Number(bar.volume) : 0);
    if (!Number.isFinite(ticks)) return;

    const candle = candlesByTime.get(time);
    if (candle && bar.close != null && Number.isFinite(Number(bar.close))) {
      candle.close = Number(bar.close);
    }

    const color = getBarColor(bar, candle);

    volumeSeries.update({
      time,
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
