/* ============================================================
 * Dev-only mock backend + static server for manual UI checks.
 *
 *   node tests/mock-server.js          # http://0.0.0.0:8080
 *   PORT=9000 node tests/mock-server.js
 *
 * Serves the static frontend AND a mocked backend on ONE origin:
 *   GET /levels /candles /tick-volume /calendar   — mock API
 *   WS  /ws                                       — mock level stream
 *   GET /__mock/state                             — current scenario
 *   GET /__mock/cw/on|off                         — toggle CW + broadcast
 *
 * /js/config.js is rewritten at serve time so BACKEND_HOST points at the
 * serving origin — the browser never calls localhost or a second port,
 * which keeps this usable behind the Arena preview proxy.
 *
 * Default scenario is the captured "Monday morning" state from the bug
 * report: PW/PS/swing levels present, CW ABSENT (before the first 17:00
 * America/New_York daily close of the week).
 *
 * The candle payload mirrors the real feed's shape: a wall-clock 15m window
 * (weekends + the 21:00-22:00 UTC daily break) where closed buckets are
 * bodyless filler bars, plus one simulated upstream outage with buckets
 * missing entirely — the two things that used to leave blank columns in the
 * chart.
 * ============================================================ */
"use strict";

const http = require("http");
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");

const ROOT = path.join(__dirname, "..");
const PORT = Number(process.env.PORT) || 8080;
const HOST = "0.0.0.0";

const WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const CONFIG_HOST_LINE = /const BACKEND_HOST = "[^"]+";/;

/* ---------- scenario fixtures (values from the task fixture) ---------- */

const PW = { window: "PW", poc: 4285.3, vah: 4339.55, val: 4252.55, direction: "neutral", start: 0, end: 0, timestamp: 0, swing_high: null, swing_low: null };
const PS = { window: "PS", poc: 4285.66, vah: 4286.46, val: 4285.46, direction: "neutral", start: 0, end: 0, timestamp: 0, swing_high: null, swing_low: null };
const CW = { window: "CW", poc: 4286.1, vah: 4287.25, val: 4284.35, direction: "neutral", start: 0, end: 0, timestamp: 0, swing_high: null, swing_low: null };
const SWING = {
  window: "SWING_BULL", poc: 4285.71, vah: 4286.46, val: 4285.46, direction: "bullish",
  start: 0, end: 0, timestamp: 0, swing_high: 4286.46, swing_low: 4285.46,
};

const scenario = { cw: false }; // ← captured Monday-morning state

function levelSnapshot() {
  const now = Date.now();
  const list = scenario.cw ? [PW, PS, CW, SWING] : [PW, PS, SWING];
  return list.map((l) => ({ ...l, timestamp: now }));
}

/* ---------- synthetic 15m candles around the fixture prices ---------- */

const BUCKET_MS = 900000;

/**
 * Gold trades Sunday 22:00 to Friday 21:00 UTC with a 21:00-22:00 UTC daily
 * break — the same calendar the real feed follows.
 */
function isMarketClosed(ms) {
  const d = new Date(ms);
  const day = d.getUTCDay(); // 0 = Sunday
  const mins = d.getUTCHours() * 60 + d.getUTCMinutes();
  if (day === 6) return true;                                 // Saturday
  if (day === 5 && mins >= 21 * 60) return true;              // Friday close
  if (day === 0 && mins < 22 * 60) return true;               // Sunday pre-open
  return day >= 1 && day <= 4 && mins >= 21 * 60 && mins < 22 * 60; // daily break
}

/**
 * A wall-clock window of 15m buckets, exactly like the backend's /candles:
 * closed-market buckets are bodyless filler bars (open === close, the last
 * traded price repeated) and an upstream outage leaves 16 buckets missing.
 * Both used to punch blank columns into the chart.
 */
function candles() {
  const bars = [];
  const nowMin = Math.floor(Date.now() / BUCKET_MS) * BUCKET_MS;
  const outageStart = nowMin - 62 * BUCKET_MS;   // ~15h ago, 4h of missing data
  const outageEnd = outageStart + 16 * BUCKET_MS;
  let price = 4285.0;

  for (let i = 319; i >= 0; i--) {               // ~200 drawn bars once closed-market filler is out
    const t = nowMin - i * BUCKET_MS;
    if (t >= outageStart && t < outageEnd) continue; // upstream outage: no bar at all

    if (isMarketClosed(t)) {
      bars.push({
        time: t, open: price, high: price + 0.65, low: price, close: price,
        volume: 4600 + Math.floor(Math.random() * 400), source: "mock",
      });
      continue;
    }

    const drift = Math.sin(i / 7) * 1.2 + (Math.random() - 0.5) * 0.6;
    const open = price;
    const close = price + drift;
    const high = Math.max(open, close) + Math.random() * 0.4;
    const low = Math.min(open, close) - Math.random() * 0.4;
    bars.push({ time: t, open, high, low, close, volume: 800 + Math.floor(Math.random() * 400), source: "mock" });
    price = close;
  }
  return bars;
}

function tickVolume(bars) {
  return bars.slice(-96).map((b) => {
    const up = Math.floor(b.volume / 2 + Math.random() * 40);
    return {
      time: b.time,
      ticks: b.volume,
      up_ticks: up,
      down_ticks: b.volume - up,
      flat_ticks: 0,
      close: b.close,
      last_tick: b.time + 899000,
      ticks_per_sec: 1 + Math.random() * 3,
      closed: b.time < Date.now() - 900000,
      source: "mock",
    };
  });
}

/* ---------- minimal RFC 6455 WebSocket server (text frames only) ---------- */

const sockets = new Set();

function wsAccept(key) {
  return crypto.createHash("sha1").update(key + WS_GUID).digest("base64");
}

function wsSend(socket, text) {
  const payload = Buffer.from(text, "utf8");
  let header;
  if (payload.length < 126) {
    header = Buffer.from([0x81, payload.length]);
  } else if (payload.length < 65536) {
    header = Buffer.alloc(4);
    header[0] = 0x81;
    header[1] = 126;
    header.writeUInt16BE(payload.length, 2);
  } else {
    header = Buffer.alloc(10);
    header[0] = 0x81;
    header[1] = 127;
    header.writeBigUInt64BE(BigInt(payload.length), 2);
  }
  socket.write(Buffer.concat([header, payload]));
}

function broadcastLevels() {
  // Emulate the backend's post-close broadcast: one frame per window.
  for (const lvl of levelSnapshot()) {
    const frame = JSON.stringify({ type: "levels", data: lvl });
    for (const s of sockets) wsSend(s, frame);
  }
}

/** CW removed = the task's "snapshot with no CW" frame. */
function broadcastSnapshotWithoutCw() {
  const frame = JSON.stringify({ type: "levels", data: levelSnapshot() });
  for (const s of sockets) wsSend(s, frame);
}

function attachWebSocket(req, socket) {
  const key = req.headers["sec-websocket-key"];
  if (!key) {
    socket.end("HTTP/1.1 400 Bad Request\r\n\r\n");
    return;
  }
  socket.write(
    "HTTP/1.1 101 Switching Protocols\r\n" +
      "Upgrade: websocket\r\n" +
      "Connection: Upgrade\r\n" +
      `Sec-WebSocket-Accept: ${wsAccept(key)}\r\n\r\n`
  );
  sockets.add(socket);

  let buf = Buffer.alloc(0);
  socket.on("data", (chunk) => {
    buf = Buffer.concat([buf, chunk]);
    while (buf.length >= 2) {
      const opcode = buf[0] & 0x0f;
      const masked = (buf[1] & 0x80) !== 0;
      let len = buf[1] & 0x7f;
      let off = 2;
      if (len === 126) {
        if (buf.length < 4) return;
        len = buf.readUInt16BE(2);
        off = 4;
      } else if (len === 127) {
        if (buf.length < 10) return;
        len = Number(buf.readBigUInt64BE(2));
        off = 10;
      }
      const maskLen = masked ? 4 : 0;
      if (buf.length < off + maskLen + len) return;
      const mask = masked ? buf.slice(off, off + 4) : null;
      const payload = Buffer.from(buf.slice(off + maskLen, off + maskLen + len));
      if (mask) for (let i = 0; i < payload.length; i++) payload[i] ^= mask[i % 4];
      buf = buf.slice(off + maskLen + len);

      if (opcode === 0x8) {
        socket.end();
        return;
      }
      if (opcode === 0x9) {
        const pong = Buffer.from([0x8a, 0]);
        socket.write(pong);
        continue;
      }
      if (opcode !== 0x1) continue;

      let msg;
      try {
        msg = JSON.parse(payload.toString("utf8"));
      } catch {
        continue;
      }
      if (msg && msg.type === "subscribe") {
        // Replay the cached snapshot exactly like the real backend.
        for (const lvl of levelSnapshot()) {
          wsSend(socket, JSON.stringify({ type: "levels", data: lvl }));
        }
      }
    }
  });
  socket.on("error", () => {});
  socket.on("close", () => sockets.delete(socket));
}

/* ---------- HTTP ---------- */

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".ico": "image/x-icon",
};

function json(res, body, status = 200) {
  res.writeHead(status, {
    "content-type": "application/json",
    "access-control-allow-origin": "*",
    "cache-control": "no-store",
  });
  res.end(JSON.stringify(body));
}

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://${req.headers.host || "localhost"}`);
  const p = url.pathname;

  // ---- mock API ----
  if (p === "/levels") return json(res, levelSnapshot());
  if (p === "/candles") return json(res, candles());
  if (p === "/tick-volume") return json(res, tickVolume(candles()));
  if (p === "/calendar") return json(res, { source: "mock", events: [] });
  if (p === "/health" || p === "/status") return json(res, { ok: true, mock: true });

  // ---- scenario control ----
  if (p === "/__mock/state") return json(res, scenario);
  if (p === "/__mock/cw/on" || p === "/__mock/cw/off") {
    const on = p.endsWith("/on");
    const changed = scenario.cw !== on;
    scenario.cw = on;
    if (changed) {
      if (on) broadcastLevels(); // daily close → CW frames arrive
      else broadcastSnapshotWithoutCw(); // week reset → snapshot without CW
    }
    return json(res, scenario);
  }

  // ---- static files ----
  let rel = p === "/" ? "/index.html" : p;
  const file = path.normalize(path.join(ROOT, rel));
  if (!file.startsWith(ROOT)) {
    res.writeHead(403);
    return res.end();
  }

  fs.readFile(file, (err, data) => {
    if (err) {
      res.writeHead(404, { "content-type": "text/plain" });
      return res.end("not found");
    }
    if (rel === "/js/config.js") {
      // Point the frontend at this serving origin (works behind the
      // preview proxy; no localhost or second-port calls from the page).
      data = Buffer.from(
        data.toString("utf8").replace(CONFIG_HOST_LINE, "const BACKEND_HOST = location.host;"),
        "utf8"
      );
    }
    res.writeHead(200, {
      "content-type": MIME[path.extname(file)] || "application/octet-stream",
      "cache-control": "no-store",
    });
    res.end(data);
  });
});

server.on("upgrade", (req, socket) => {
  if (new URL(req.url, "http://x").pathname === "/ws") attachWebSocket(req, socket);
  else socket.destroy();
});

server.listen(PORT, HOST, () => {
  console.log(`mock UI  →  http://${HOST}:${PORT}/`);
  console.log(`scenario →  CW ${scenario.cw ? "present" : "ABSENT (Monday morning)"} — toggle via /__mock/cw/on|off`);
});
