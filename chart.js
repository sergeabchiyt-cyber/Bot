/* ============================================================
 * XAUUSD Terminal — Node 2
 * lightweight-charts v4.2.0 API
 * Direct Binance kline WS + multi-exchange order flow
 * Bubbles rendered via custom series primitive (bubbles.js)
 * ============================================================ */

const BACKEND_WS = "wss://engine-southeastasia-sng-main.onrender.com/ws";
const BACKEND_REST = "https://engine-southeastasia-sng-main.onrender.com/levels";
const BINANCE_KLINE_WS = "wss://fstream.binance.com/market/ws/xauusdt@kline_15m";

// ---------- Level visibility policy ----------
// PW -> PoC + VaH + VaL
// PS -> PoC only
// CW -> PoC + VaH + VaL
function levelVisible(windowKey, kind) {
  if (windowKey === "PW") return true;
  if (windowKey === "PS") return kind === "poc";
  if (windowKey === "CW") return true;
  return false;
}

// ---------- Chart bootstrap ----------
const chartEl = document.getElementById("chart");

const chart = LightweightCharts.createChart(chartEl, {
  layout: {
    background: { color: "#0B0E11" },
    textColor: "#7D8590",
    fontFamily: "'Inter', sans-serif",
    fontSize: 11,
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
  },
});

const candleSeries = chart.addCandlestickSeries({
  upColor: "#26A69A",
  downColor: "#EF5350",
  borderUpColor: "#26A69A",
  borderDownColor: "#EF5350",
  wickUpColor: "#26A69A",
  wickDownColor: "#EF5350",
});

// ---------- Historical candles (one-time REST) ----------
async function loadHistory() {
  setStatus("loading history");
  try {
    const res = await fetch(
      "https://fapi.binance.com/fapi/v1/klines?symbol=XAUUSDT&interval=15m&limit=500"
    );
    if (!res.ok) throw new Error(`Binance REST ${res.status}`);
    const raw = await res.json();
    const data = raw.map((k) => ({
      time: Math.floor(k[0] / 1000),
      open: parseFloat(k[1]),
      high: parseFloat(k[2]),
      low: parseFloat(k[3]),
      close: parseFloat(k[4]),
    }));
    candleSeries.setData(data);
    chart.timeScale().fitContent();
    if (data.length) updateLastPrice(data[data.length - 1]);
    console.log(`Loaded ${data.length} historical candles`);
  } catch (e) {
    console.error("History load failed:", e);
    setStatus("history error");
  }
}

// ---------- Direct Binance kline WebSocket ----------
let binanceWs;
let binanceReconnect = 1000;

function connectBinanceKline() {
  binanceWs = new WebSocket(BINANCE_KLINE_WS);

  binanceWs.onopen = () => {
    console.log("Binance kline WS connected");
  };

  binanceWs.onmessage = (ev) => {
    let msg;
    try {
      msg = JSON.parse(ev.data);
    } catch {
      return;
    }
    if (msg.e !== "kline" || !msg.k) return;

    const k = msg.k;
    const candle = {
      time: Math.floor(k.t / 1000),
      open: parseFloat(k.o),
      high: parseFloat(k.h),
      low: parseFloat(k.l),
      close: parseFloat(k.c),
    };

    candleSeries.update(candle);
    updateLastPrice(candle);

    if (k.x === true) {
      console.log(`Candle closed @ ${candle.time}`, candle);
      fetchLevels();
    }
  };

  binanceWs.onclose = () => {
    console.log(`Binance kline WS closed, retrying in ${binanceReconnect}ms`);
    setTimeout(connectBinanceKline, binanceReconnect);
    binanceReconnect = Math.min(binanceReconnect * 2, 30000);
  };

  binanceWs.onerror = (e) => console.error("Binance kline WS error:", e);
}

// ---------- Price lines ----------
const priceLines = {};

function upsertPriceLine(key, price, color, title, dashed = false) {
  if (priceLines[key]) candleSeries.removePriceLine(priceLines[key]);
  priceLines[key] = candleSeries.createPriceLine({
    price,
    color,
    lineWidth: 1,
    lineStyle: dashed
      ? LightweightCharts.LineStyle.Dashed
      : LightweightCharts.LineStyle.Solid,
    axisLabelVisible: true,
    title,
  });
}

// Colour map per window + kind. CW uses purple for all three; dashed.
const LEVEL_STYLE = {
  PW: {
    poc: { color: "#F0B90B", title: "PW PoC", dashed: false },
    vah: { color: "#26A69A", title: "PW VaH", dashed: false },
    val: { color: "#EF5350", title: "PW VaL", dashed: false },
  },
  PS: {
    poc: { color: "#58A6FF", title: "PS PoC", dashed: false },
  },
  CW: {
    poc: { color: "#A371F7", title: "CW PoC", dashed: true },
    vah: { color: "#A371F7", title: "CW VaH", dashed: true },
    val: { color: "#A371F7", title: "CW VaL", dashed: true },
  },
};

// Draw every visible level from one window object.
function drawLevelsForWindow(l) {
  const styles = LEVEL_STYLE[l.window];
  if (!styles) return;
  const kinds = [
    ["poc", l.poc],
    ["vah", l.vah],
    ["val", l.val],
  ];
  for (const [kind, price] of kinds) {
    if (price === undefined || price === null) continue;
    if (!levelVisible(l.window, kind)) continue;
    const s = styles[kind];
    if (!s) continue;
    upsertPriceLine(`${l.window}-${kind}`, price, s.color, s.title, s.dashed);
  }
}

// ---------- Order flow bubbles (custom primitive) ----------
const EVENT_STYLE =
  typeof OrderFlowBubblesPrimitive !== "undefined"
    ? OrderFlowBubblesPrimitive.EVENT_STYLE
    : {
        BUY_BUBBLE:  { color: "#26A69A", label: "BUY"   },
        SELL_BUBBLE: { color: "#EF5350", label: "SELL"  },
        ABS_BUY:     { color: "#F0B90B", label: "ABS-B" },
        ABS_SELL:    { color: "#F0B90B", label: "ABS-S" },
      };

let bubblesPrimitive;
if (typeof OrderFlowBubblesPrimitive !== "undefined") {
  bubblesPrimitive = OrderFlowBubblesPrimitive.create();
  candleSeries.attachPrimitive(bubblesPrimitive);
} else {
  console.error("bubbles.js failed to load — chart bubbles disabled");
  bubblesPrimitive = { updateData() {}, maxStrength: 100 };
}

const bubbleEvents = [];          // rolling window for rendering
const MAX_BUBBLES_ON_CHART = 60;
const MAX_SCALE_CAP = 5000;

let maxStrengthSeen = 100;        // dynamic scale reference

// Decay the scale reference slowly so it recovers after a spike.
setInterval(() => {
  if (maxStrengthSeen > 100) {
    maxStrengthSeen = Math.max(100, maxStrengthSeen * 0.95);
    bubblesPrimitive.maxStrength = maxStrengthSeen;
  }
}, 30_000);

function addBubble(event) {
  bubbleEvents.push(event);
  while (bubbleEvents.length > MAX_BUBBLES_ON_CHART) {
    bubbleEvents.shift();
  }

  if (event.strength > maxStrengthSeen) {
    maxStrengthSeen = Math.min(MAX_SCALE_CAP, event.strength);
  }

  bubblesPrimitive.maxStrength = maxStrengthSeen;
  bubblesPrimitive.updateData(bubbleEvents);

  // Nudge the chart to force a repaint of the primitive layer.
  chart.applyOptions({});
  candleSeries.applyOptions({});

  renderBubbleRow(event);
}

function clearBubbles() {
  bubbleEvents.length = 0;
  bubblesPrimitive.updateData([]);

  bubbleRows.length = 0;
  const list = document.getElementById("bubbles-list");
  const count = document.getElementById("bubbles-count");
  if (list) list.innerHTML = '<div class="empty">No bubbles detected</div>';
  if (count) count.textContent = 0;

  chart.applyOptions({});
}

// ---------- Level state ----------
const levelsState = { PW: null, PS: null, CW: null };

function updateLevelState(l) {
  levelsState[l.window] = l;
  const arr = Object.values(levelsState).filter(Boolean);
  renderLevels(arr);
}

// Count only rows that will actually be rendered.
function visibleRowCount(levels) {
  let n = 0;
  for (const l of levels) {
    if (l.window === "PW") n += 3;
    else if (l.window === "PS") n += 1;
    else if (l.window === "CW") n += 3;
  }
  return n;
}

function renderLevels(levels) {
  const list = document.getElementById("levels-list");
  const count = document.getElementById("levels-count");
  count.textContent = visibleRowCount(levels);

  if (!levels.length) {
    list.innerHTML = '<div class="empty">Awaiting data…</div>';
    return;
  }

  list.innerHTML = levels
    .map((l) => {
      const rows = [];
      if (l.window === "PW") {
        rows.push(row("PW", "PoC", l.poc, "poc"));
        rows.push(row("PW", "VaH", l.vah, "vah"));
        rows.push(row("PW", "VaL", l.val, "val"));
      } else if (l.window === "PS") {
        rows.push(row("PS", "PoC", l.poc, "ps-poc"));
      } else if (l.window === "CW") {
        rows.push(row("CW", "PoC", l.poc, "cw-poc"));
        rows.push(row("CW", "VaH", l.vah, "cw-vah"));
        rows.push(row("CW", "VaL", l.val, "cw-val"));
      }
      return rows.join("");
    })
    .join("");
}

function row(windowLabel, type, price, cls) {
  if (price === undefined || price === null) return "";
  return `
    <div class="level-row">
      <div class="level-label"><i class="${cls}"></i>${windowLabel} ${type}</div>
      <div class="level-price">${Number(price).toFixed(2)}</div>
    </div>
  `;
}

// ---------- Bubbles sidebar ----------
const bubbleRows = [];

function renderBubbleRow(event) {
  const list = document.getElementById("bubbles-list");
  const count = document.getElementById("bubbles-count");
  if (bubbleRows.length === 0) list.innerHTML = "";

  const style = EVENT_STYLE[event.kind] || EVENT_STYLE.BUY_BUBBLE;
  const time = new Date(event.timestamp).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
  const src = (event.exchange || "binance").toUpperCase();

  bubbleRows.unshift({ event, time, style, src });
  if (bubbleRows.length > 40) bubbleRows.pop();

  list.innerHTML = bubbleRows
    .map((b) => `
      <div class="bubble-row">
        <span class="bubble-dir" style="color:${b.style.color}">
          ${b.style.label}
        </span>
        <span class="bubble-meta">${Number(b.event.level).toFixed(2)} · ${b.time} · <em>${b.src}</em></span>
      </div>
    `)
    .join("");

  count.textContent = bubbleRows.length;
}

// ---------- Calendar ----------
function renderCalendar(events) {
  const list = document.getElementById("calendar-list");
  const count = document.getElementById("calendar-count");
  count.textContent = events.length;

  if (!events.length) {
    list.innerHTML = '<div class="empty">No events loaded</div>';
    return;
  }

  list.innerHTML = events
    .slice(0, 10)
    .map((e) => {
      const impact = (e.impact || "low").toLowerCase();
      return `
        <div class="event-row">
          <div class="event-top">
            <span class="event-time">${e.time || ""}</span>
            <span class="event-impact impact-${impact}">${impact}</span>
          </div>
          <div class="event-name">${e.currency || "USD"} · ${e.event || ""}</div>
        </div>
      `;
    })
    .join("");
}

// ---------- Top bar ----------
function updateLastPrice(candle) {
  const priceEl = document.getElementById("last-price");
  if (priceEl) priceEl.textContent = Number(candle.close).toFixed(2);
}

function updateSentiment(frame) {
  const val = document.getElementById("sentiment-value");
  if (!val) return;
  const { hawkish = 0, dovish = 0 } = frame;
  const bias = hawkish - dovish;
  let label = "Neutral";
  let color = "var(--text-muted)";
  if (bias > 0.15) {
    label = `Hawkish ${(hawkish * 100).toFixed(0)}%`;
    color = "var(--teal)";
  } else if (bias < -0.15) {
    label = `Dovish ${(dovish * 100).toFixed(0)}%`;
    color = "var(--coral)";
  }
  val.textContent = label;
  val.style.color = color;
}

function setStatus(text) {
  const el = document.getElementById("status");
  if (el) el.textContent = text;
}

// ---------- Backend WebSocket ----------
let backendWs;
let backendReconnect = 1000;

function connectBackend() {
  backendWs = new WebSocket(BACKEND_WS);

  backendWs.onopen = () => {
    console.log("Backend WS connected");
    backendWs.send(
      JSON.stringify({
        type: "subscribe",
        topics: ["levels", "bubbles", "trades", "sentiment", "calendar"],
      })
    );
  };

  backendWs.onmessage = (ev) => {
    let frame;
    try {
      frame = JSON.parse(ev.data);
    } catch {
      return;
    }

    switch (frame.type) {
      case "levels": {
        const l = frame.data;
        drawLevelsForWindow(l);
        updateLevelState(l);
        break;
      }
      case "bubbles":
        addBubble(frame.data);
        break;
      case "sentiment":
        updateSentiment(frame.data);
        break;
      case "calendar":
        renderCalendar(frame.data);
        break;
    }
  };

  backendWs.onclose = () => {
    setStatus("reconnecting");
    setTimeout(connectBackend, backendReconnect);
    backendReconnect = Math.min(backendReconnect * 2, 30000);
  };

  backendWs.onerror = (e) => {
    setStatus("ws error");
    console.error("Backend WS error:", e);
  };
}

// ---------- REST levels fallback ----------
async function fetchLevels() {
  try {
    const res = await fetch(BACKEND_REST);
    if (!res.ok) throw new Error(`levels ${res.status}`);
    const levels = await res.json();
    for (const l of levels) {
      drawLevelsForWindow(l);
      updateLevelState(l);
    }
  } catch (e) {
    console.error("Levels fetch failed:", e);
  }
}

// ---------- Timeframe tabs ----------
document.querySelectorAll(".tf").forEach((btn) => {
  btn.addEventListener("click", () => {
    document.querySelectorAll(".tf").forEach((b) => b.classList.remove("active"));
    btn.classList.add("active");
    const tf = btn.dataset.tf;
    if (tf !== "15m") {
      document.querySelector('.tf[data-tf="15m"]').classList.add("active");
      btn.classList.remove("active");
      return;
    }
    Object.values(priceLines).forEach((l) => candleSeries.removePriceLine(l));
    Object.keys(priceLines).forEach((k) => delete priceLines[k]);
    clearBubbles();
    loadHistory();
    fetchLevels();
  });
});

// ---------- Boot ----------
setStatus("initializing");
loadHistory().then(() => {
  setStatus("live");
  connectBinanceKline();
  connectBackend();
  fetchLevels();
});

// ---------- Resize ----------
const ro = new ResizeObserver(() => {
  chart.applyOptions({ width: chartEl.clientWidth, height: chartEl.clientHeight });
});
ro.observe(chartEl);

console.log("XAUUSD terminal booted — direct Binance kline + multi-exchange order flow");