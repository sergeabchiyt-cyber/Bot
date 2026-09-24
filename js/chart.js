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
    rightPriceScale: { borderColor: "rgba(255,255,255,0.06)" },
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

  // ---------- Top bar price ----------
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
      series.setData(data);
      chart.timeScale().fitContent();
      updateLastPrice(data[data.length - 1].close);
      setStatus("ready", "busy"); // WS flips this to "live" on open
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
    };
    if (
      !Number.isFinite(candle.open) ||
      !Number.isFinite(candle.high) ||
      !Number.isFinite(candle.low) ||
      !Number.isFinite(candle.close)
    ) {
      return;
    }
    series.update(candle);
    updateLastPrice(candle.close);
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
  }

  new ResizeObserver(() => chart.applyOptions(size())).observe(el);

  App.chart = {
    chart, series, loadHistory, updateCandle,
    upsertPriceLine, removePriceLine, clearPriceLines, repaint,
  };
})((window.App = window.App || {}));
