/* ============================================================
 * XAUUSD Terminal — Node 2
 * lightweight-charts v4.2.0 API
 * ============================================================ */

const WS_URL = "wss://engine-southeastasia-sng-main.onrender.com/ws";
const REST_LEVELS_URL = "https://engine-southeastasia-sng-main.onrender.com/levels";

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

// ---------- Load historical candles from Binance REST ----------
async function loadHistory(interval = "15m") {
  setStatus("loading history");
  try {
    const res = await fetch(
      `https://fapi.binance.com/fapi/v1/klines?symbol=XAUUSDT&interval=${interval}&limit=500`
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
    console.log(`Loaded ${data.length} candles (${interval})`);
  } catch (e) {
    console.error("History load failed:", e);
    setStatus("history error");
  }
}

// ---------- Price lines ----------
const priceLines = {};

function upsertPriceLine(key, price, color, title) {
  if (priceLines[key]) candleSeries.removePriceLine(priceLines[key]);
  priceLines[key] = candleSeries.createPriceLine({
    price,
    color,
    lineWidth: 1,
    lineStyle: LightweightCharts.LineStyle.Solid,
    axisLabelVisible: true,
    title,
  });
}

// ---------- Bubble markers (v4 API) ----------
let markers = [];

function addBubble(bubble) {
  const isBuy = bubble.direction === "ABS_BUY";
  markers.push({
    time: Math.floor(bubble.timestamp / 1000),
    position: isBuy ? "belowBar" : "aboveBar",
    color: isBuy ? "#26A69A" : "#EF5350",
    shape: "circle",
    text: isBuy ? "ABS BUY" : "ABS SELL",
    size: Math.min(3, Math.max(1, bubble.strength / 100)),
  });
  candleSeries.setMarkers(markers);   // v4 API
  renderBubbleRow(bubble);
}

function clearMarkers() {
  markers = [];
  candleSeries.setMarkers(markers);
}

// ---------- Sidebar renderers ----------
function renderLevels(levels) {
  const list = document.getElementById("levels-list");
  const count = document.getElementById("levels-count");
  count.textContent = levels.length;

  if (!levels.length) {
    list.innerHTML = '<div class="empty">Awaiting data…</div>';
    return;
  }

  const html = levels
    .map((l) => {
      const rows = [];
      if (l.window === "PW") {
        rows.push(row("PW", "PoC", l.poc, "poc"));
        rows.push(row("PW", "VaH", l.vah, "vah"));
        rows.push(row("PW", "VaL", l.val, "val"));
      } else if (l.window === "PS") {
        rows.push(row("PS", "PoC", l.poc, "ps-poc"));
      }
      return rows.join("");
    })
    .join("");

  list.innerHTML = html;
}

function row(windowLabel, type, price, cls) {
  return `
    <div class="level-row">
      <div class="level-label"><i class="${cls}"></i>${windowLabel} ${type}</div>
      <div class="level-price">${Number(price).toFixed(2)}</div>
    </div>
  `;
}

const bubbleRows = [];
function renderBubbleRow(bubble) {
  const list = document.getElementById("bubbles-list");
  const count = document.getElementById("bubbles-count");

  if (bubbleRows.length === 0) list.innerHTML = "";

  const isBuy = bubble.direction === "ABS_BUY";
  const time = new Date(bubble.timestamp).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });

  bubbleRows.unshift({ bubble, time, isBuy });
  if (bubbleRows.length > 20) bubbleRows.pop();

  list.innerHTML = bubbleRows
    .map(
      (b) => `
      <div class="bubble-row">
        <span class="bubble-dir ${b.isBuy ? "buy" : "sell"}">
          ${b.isBuy ? "▲ ABS BUY" : "▼ ABS SELL"}
        </span>
        <span class="bubble-meta">${Number(b.bubble.level).toFixed(2)} · ${b.time}</span>
      </div>
    `
    )
    .join("");

  count.textContent = bubbleRows.length;
}

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

// ---------- WebSocket ----------
let ws;
let reconnectDelay = 1000;

function connect() {
  setStatus("connecting");
  ws = new WebSocket(WS_URL);

  ws.onopen = () => {
    setStatus("live");
    reconnectDelay = 1000;
    ws.send(
      JSON.stringify({
        type: "subscribe",
        topics: ["levels", "bubbles", "trades", "sentiment", "calendar"],
      })
    );
    console.log("WS connected, subscribed");
  };

  ws.onmessage = (ev) => {
    let frame;
    try {
      frame = JSON.parse(ev.data);
    } catch {
      return;
    }
    console.log("WS frame:", frame);

    switch (frame.type) {
      case "levels": {
        const l = frame.data;
        if (l.window === "PW") {
          upsertPriceLine("PW-poc", l.poc, "#F0B90B", "PW PoC");
          upsertPriceLine("PW-vah", l.vah, "#26A69A", "PW VaH");
          upsertPriceLine("PW-val", l.val, "#EF5350", "PW VaL");
        } else if (l.window === "PS") {
          upsertPriceLine("PS-poc", l.poc, "#58A6FF", "PS PoC");
        }
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
      case "trades":
        break;
    }
  };

  ws.onclose = () => {
    setStatus("reconnecting");
    setTimeout(connect, reconnectDelay);
    reconnectDelay = Math.min(reconnectDelay * 2, 30000);
  };

  ws.onerror = (e) => {
    setStatus("ws error");
    console.error("WS error:", e);
  };
}

// ---------- REST levels fallback ----------
async function fetchLevels() {
  try {
    const res = await fetch(REST_LEVELS_URL);
    if (!res.ok) throw new Error(`levels ${res.status}`);
    const levels = await res.json();
    renderLevels(levels);
    for (const l of levels) {
      if (l.window === "PW") {
        upsertPriceLine("PW-poc", l.poc, "#F0B90B", "PW PoC");
        upsertPriceLine("PW-vah", l.vah, "#26A69A", "PW VaH");
        upsertPriceLine("PW-val", l.val, "#EF5350", "PW VaL");
      } else if (l.window === "PS") {
        upsertPriceLine("PS-poc", l.poc, "#58A6FF", "PS PoC");
      }
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
    Object.values(priceLines).forEach((l) => candleSeries.removePriceLine(l));
    Object.keys(priceLines).forEach((k) => delete priceLines[k]);
    clearMarkers();
    loadHistory(btn.dataset.tf);
    fetchLevels();
  });
});

// ---------- Boot ----------
loadHistory("15m");
fetchLevels();
connect();

// ---------- Resize ----------
const ro = new ResizeObserver(() => {
  chart.applyOptions({ width: chartEl.clientWidth, height: chartEl.clientHeight });
});
ro.observe(chartEl);

// ---------- Visible diagnostics ----------
console.log("XAUUSD terminal booted");