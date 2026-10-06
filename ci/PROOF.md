# CI proof — run 37520828403
commit: d73824ce6b0584daad84f3506831614312cb822b
generated: 2026-10-06T19:41:51Z

## Unit tests
(missing)

## Live calendar feed test
(missing)

## Runtime smoke test
(missing)

## rustfmt check
Diff in /home/runner/work/Bot/Bot/src/ai_cache.rs:81:
 
     fn prune(&self, now_ms: i64) {
         let cutoff = now_ms - RETAIN_MS;
-        for buf in [
-            &self.transcripts,
-            &self.sentiments,
-            &self.predictions,
-        ] {
+        for buf in [&self.transcripts, &self.sentiments, &self.predictions] {
             let mut buf = buf.lock().unwrap();
             while let Some(front) = buf.items.front() {
                 let ts = front.get("ts").and_then(|t| t.as_i64()).unwrap_or(0);
Diff in /home/runner/work/Bot/Bot/src/calendar.rs:11:
 //! browser), normalizes it, and keeps the MCP browser as an optional
 //! fallback for when the feed is rate-limited.
 
-use anyhow::{bail, Context, Result};
+use anyhow::{Context, Result, bail};
 use chrono::{DateTime, Utc};
 use serde::{Deserialize, Serialize};
 use tracing::{info, warn};
Diff in /home/runner/work/Bot/Bot/src/calendar.rs:142:
         // Header row, markdown separator row, or empty event name.
         if cols[0].eq_ignore_ascii_case("time")
             || cols[0].starts_with(':')
-            || cols[0].chars().all(|c| c == '-' || c == ':' || c.is_whitespace())
+            || cols[0]
+                .chars()
+                .all(|c| c == '-' || c == ':' || c.is_whitespace())
             || cols[3].is_empty()
         {
             continue;
Diff in /home/runner/work/Bot/Bot/src/calendar.rs:154:
             event: cols[3].to_string(),
             currency,
             impact,
-            time: if cols[0].is_empty() { None } else { Some(cols[0].to_string()) },
+            time: if cols[0].is_empty() {
+                None
+            } else {
+                Some(cols[0].to_string())
+            },
             timestamp: None,
             actual: None,
             forecast: None,
Diff in /home/runner/work/Bot/Bot/src/calendar.rs:285:
         // 08:30 EDT == 12:30 UTC
         assert_eq!(cpi.time.as_deref(), Some("2026-09-24T12:30:00+00:00"));
         assert!(cpi.timestamp.is_some());
-        assert!(cpi.gold_relevant, "high-impact USD must flag as gold-relevant");
+        assert!(
+            cpi.gold_relevant,
+            "high-impact USD must flag as gold-relevant"
+        );
 
         assert!(!events[0].gold_relevant);
         assert!(!events[1].gold_relevant, "medium GBP is not gold-relevant");
Diff in /home/runner/work/Bot/Bot/src/calendar.rs:373:
         for e in &events {
             assert!(!e.event.is_empty(), "empty event title");
             assert!(
-                matches!(e.impact.as_str(), "High" | "Medium" | "Low" | "Holiday" | "Unknown"),
+                matches!(
+                    e.impact.as_str(),
+                    "High" | "Medium" | "Low" | "Holiday" | "Unknown"
+                ),
                 "unnormalized impact: {}",
                 e.impact
             );
Diff in /home/runner/work/Bot/Bot/src/config.rs:133:
 }
 
 fn env_f64(key: &str, default: f64) -> f64 {
-    env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
+    env::var(key)
+        .ok()
+        .and_then(|v| v.parse().ok())
+        .unwrap_or(default)
 }
 
 fn env_bool(key: &str, default: bool) -> bool {
Diff in /home/runner/work/Bot/Bot/src/config.rs:147:
 }
 
 fn env_u64(key: &str, default: u64) -> u64 {
-    env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
+    env::var(key)
+        .ok()
+        .and_then(|v| v.parse().ok())
+        .unwrap_or(default)
 }
 
 fn env_i64(key: &str, default: i64) -> i64 {
Diff in /home/runner/work/Bot/Bot/src/config.rs:154:
-    env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
+    env::var(key)
+        .ok()
+        .and_then(|v| v.parse().ok())
+        .unwrap_or(default)
 }
 
 /// Comma-separated list env, trimmed and stripped of empties.
Diff in /home/runner/work/Bot/Bot/src/config.rs:167:
 /// tri-state feed switch: FEED_X=on|off|auto (auto = on only when its
 /// credentials exist, or always-on for keyless public feeds).
 fn env_switch(key: &str, has_credentials: bool) -> bool {
-    match env::var(key).unwrap_or_default().to_ascii_lowercase().as_str() {
+    match env::var(key)
+        .unwrap_or_default()
+        .to_ascii_lowercase()
+        .as_str()
+    {
         "1" | "true" | "on" | "yes" => true,
         "0" | "false" | "off" | "no" => false,
         _ => has_credentials, // auto
Diff in /home/runner/work/Bot/Bot/src/config.rs:184:
         let binance_symbol = env_str("BINANCE_SYMBOL", "XAUUSDT").to_lowercase();
 
         Self {
-            port: env::var("PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(3000),
+            port: env::var("PORT")
+                .ok()
+                .and_then(|v| v.parse().ok())
+                .unwrap_or(3000),
 
             binance_ws_url: env::var("BINANCE_WS_URL").unwrap_or_else(|_| {
                 format!("wss://fstream.binance.com/ws/{binance_symbol}@aggTrade")
Diff in /home/runner/work/Bot/Bot/src/config.rs:295:
             print!("CORS_ALLOWED_ORIGIN is set in the environment; skipping");
             return;
         }
-        assert_eq!(Config::from_env().cors_allowed_origin, DEFAULT_CORS_ALLOWED_ORIGIN);
+        assert_eq!(
+            Config::from_env().cors_allowed_origin,
+            DEFAULT_CORS_ALLOWED_ORIGIN
+        );
     }
 }
 
Diff in /home/runner/work/Bot/Bot/src/econ_monitor.rs:29:
 use std::process::Stdio;
 use std::sync::Arc;
 use tokio::io::AsyncReadExt;
-use tokio::sync::{broadcast, RwLock};
+use tokio::sync::{RwLock, broadcast};
 use tracing::{info, warn};
 
 const SAMPLE_RATE: usize = 16_000;
Diff in /home/runner/work/Bot/Bot/src/econ_monitor.rs:264:
     let out = tokio::time::timeout(
         std::time::Duration::from_secs(45),
         tokio::process::Command::new(&cfg.econ_ytdlp)
-            .args(["-g", "-f", "bestaudio/best", "--no-playlist", "--no-warnings", url])
+            .args([
+                "-g",
+                "-f",
+                "bestaudio/best",
+                "--no-playlist",
+                "--no-warnings",
+                url,
+            ])
             .stdin(Stdio::null())
             .output(),
     )
Diff in /home/runner/work/Bot/Bot/src/econ_monitor.rs:272:
     let out = match out {
         Ok(Ok(out)) => out,
         Ok(Err(e)) => {
-            warn!("econ monitor: could not run {} ({e}) — install yt-dlp or use direct media URLs in ECON_STREAM_SOURCES", cfg.econ_ytdlp);
+            warn!(
+                "econ monitor: could not run {} ({e}) — install yt-dlp or use direct media URLs in ECON_STREAM_SOURCES",
+                cfg.econ_ytdlp
+            );
             return None;
         }
         Err(_) => {
Diff in /home/runner/work/Bot/Bot/src/econ_monitor.rs:469:
         // channel + live coverage links discovered by browsing the event.
         let mut candidates: Vec<String> = cfg.econ_stream_sources.clone();
         for ev in &active {
-            if fed_related(&ev.event)
-                && !candidates.iter().any(|c| c.contains("federalreserve"))
-            {
+            if fed_related(&ev.event) && !candidates.iter().any(|c| c.contains("federalreserve")) {
                 candidates.push("https://www.youtube.com/@federalreserve/live".to_string());
             }
         }
Diff in /home/runner/work/Bot/Bot/src/econ_monitor.rs:519:
     fn window_selects_only_active_high_impact_usd_events() {
         let now = 1_700_000_000_000i64;
         let events = vec![
-            ev("Nonfarm Payrolls", "USD", "High", now - 60_000),           // active
-            ev("Core CPI m/m", "USD", "High", now + 10 * 60_000),          // inside pre-window
-            ev("German Factory Orders", "EUR", "High", now),               // wrong currency
-            ev("ADP Non-Farm Employment", "USD", "Medium", now),           // below min impact
+            ev("Nonfarm Payrolls", "USD", "High", now - 60_000), // active
+            ev("Core CPI m/m", "USD", "High", now + 10 * 60_000), // inside pre-window
+            ev("German Factory Orders", "EUR", "High", now),     // wrong currency
+            ev("ADP Non-Farm Employment", "USD", "Medium", now), // below min impact
             ev("FOMC Press Conference", "USD", "High", now - 2 * 3600_000), // after window
         ];
         let active = active_events(&events, now, 900, 3600, 2, &["USD".to_string()]);
Diff in /home/runner/work/Bot/Bot/src/econ_monitor.rs:548:
         "#;
         let urls = extract_live_urls(text, 8);
         assert_eq!(urls[0], "https://www.youtube.com/@federalreserve/live");
-        assert!(urls.iter().any(|u| u.starts_with("https://www.youtube.com/watch?v=abc123XYZ_-")));
+        assert!(
+            urls.iter()
+                .any(|u| u.starts_with("https://www.youtube.com/watch?v=abc123XYZ_-"))
+        );
         assert!(!urls.iter().any(|u| u.contains("example.com")));
     }
 
Diff in /home/runner/work/Bot/Bot/src/main.rs:1:
 mod ai_cache;
+mod binance_ws;
 mod calendar;
 mod config;
-mod status;
-mod types;
-mod binance_ws;
-mod sifting_rest;
-mod multi_exchange;
-mod volume_profile;
-mod order_flow;
 mod econ_monitor;
 mod mcp_client;
+mod multi_exchange;
+mod order_flow;
+mod sifting_rest;
 mod sifting_ws;
+mod status;
 mod tick_volume;
+mod types;
+mod volume_profile;
 mod ws_server;
 
 use std::collections::BTreeMap;
Diff in /home/runner/work/Bot/Bot/src/main.rs:18:
 use std::sync::Arc;
-use tokio::sync::{broadcast, mpsc, RwLock};
+use tokio::sync::{RwLock, broadcast, mpsc};
 use tracing::{info, warn};
 use tracing_subscriber::EnvFilter;
 
Diff in /home/runner/work/Bot/Bot/src/main.rs:90:
         .init();
 
     let config = Config::from_env();
-    info!("Starting XAUUSD engine v{} on port {}", env!("CARGO_PKG_VERSION"), config.port);
+    info!(
+        "Starting XAUUSD engine v{} on port {}",
+        env!("CARGO_PKG_VERSION"),
+        config.port
+    );
 
     let (bc_tx, _) = broadcast::channel(4096);
     let (tick_tx, mut tick_rx) = mpsc::channel(8192);
Diff in /home/runner/work/Bot/Bot/src/main.rs:292:
                 *cached_levels.write().await = levels;
                 seeded = true;
             }
-            Err(e) => warn!(
-                "SiftingIO REST history fetch failed; refusing Binance price fallback: {e}"
-            ),
+            Err(e) => {
+                warn!("SiftingIO REST history fetch failed; refusing Binance price fallback: {e}")
+            }
         }
     } else {
         warn!("No SIFTING_API_KEY set — historical VP seed is unavailable");
Diff in /home/runner/work/Bot/Bot/src/mcp_client.rs:1:
-use anyhow::{anyhow, bail, Context, Result};
-use serde_json::{json, Value};
-use std::sync::atomic::{AtomicU64, Ordering};
+use anyhow::{Context, Result, anyhow, bail};
+use serde_json::{Value, json};
 use std::sync::Mutex;
+use std::sync::atomic::{AtomicU64, Ordering};
 use tracing::{info, warn};
 
 use crate::status::FeedStatus;
Diff in /home/runner/work/Bot/Bot/src/mcp_client.rs:174:
             bail!("MCP tools/list error: {err}");
         }
         let mut names = Vec::new();
-        if let Some(tools) = resp
-            .pointer("/result/tools")
-            .and_then(|t| t.as_array())
-        {
+        if let Some(tools) = resp.pointer("/result/tools").and_then(|t| t.as_array()) {
             for t in tools {
                 if let Some(name) = t.get("name").and_then(|n| n.as_str()) {
                     names.push(name.to_string());
Diff in /home/runner/work/Bot/Bot/src/mcp_client.rs:241:
     /// the page content back through the MCP browser server.
     pub async fn scrape_calendar(&self, status: &FeedStatus) -> Result<String> {
         status.set("mcp_browser", "connecting");
-        match self.browse_page("https://www.forexfactory.com/calendar").await {
+        match self
+            .browse_page("https://www.forexfactory.com/calendar")
+            .await
+        {
             Ok(text) => {
                 status.set("mcp_browser", "connected");
                 Ok(text)
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:1:
 use anyhow::Result;
 use futures_util::{SinkExt, StreamExt};
-use serde_json::{json, Value};
+use serde_json::{Value, json};
 use tokio::sync::mpsc;
 use tokio_tungstenite::connect_async;
+use tokio_tungstenite::tungstenite::Message;
 use tokio_tungstenite::tungstenite::client::IntoClientRequest;
 use tokio_tungstenite::tungstenite::http;
-use tokio_tungstenite::tungstenite::Message;
 use tracing::{error, info, warn};
 
 use crate::status::FeedStatus;
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:43:
                     "op": "subscribe",
                     "args": ["publicTrade.XAUUSDT"]
                 });
-                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
+                if write
+                    .send(Message::Text(sub.to_string().into()))
+                    .await
+                    .is_err()
+                {
                     warn!("Bybit subscribe send failed");
                     status.set("bybit", "error");
                     continue;
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:83:
                                     let qty = t["v"].as_str().unwrap_or("0");
                                     let ts = t["T"].as_i64().unwrap_or(0);
                                     let agg = AggTrade::new(
-                                        "bybit", "XAUUSDT", price, qty, ts,
-                                        side == "Buy", true,
+                                        "bybit",
+                                        "XAUUSDT",
+                                        price,
+                                        qty,
+                                        ts,
+                                        side == "Buy",
+                                        true,
                                     );
                                     let _ = tx.send(agg).await;
                                 }
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:127:
                     "op": "subscribe",
                     "args": [{"channel": "trades", "instId": "XAU-USDT-SWAP"}]
                 });
-                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
+                if write
+                    .send(Message::Text(sub.to_string().into()))
+                    .await
+                    .is_err()
+                {
                     warn!("OKX subscribe send failed");
                     status.set("okx", "error");
                     continue;
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:141:
                     loop {
                         iv.tick().await;
                         let ping = json!({"op": "ping"});
-                        if write.send(Message::Text(ping.to_string().into())).await.is_err() {
+                        if write
+                            .send(Message::Text(ping.to_string().into()))
+                            .await
+                            .is_err()
+                        {
                             break;
                         }
                     }
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:169:
                                         .and_then(|s| s.parse::<i64>().ok())
                                         .unwrap_or(0);
                                     let agg = AggTrade::new(
-                                        "okx", "XAUUSDT", px, sz, ts,
-                                        side == "buy", true,
+                                        "okx",
+                                        "XAUUSDT",
+                                        px,
+                                        sz,
+                                        ts,
+                                        side == "buy",
+                                        true,
                                     );
                                     let _ = tx.send(agg).await;
                                 }
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:201:
 // Push rows are either arrays [price,size,side,ts,seq] or objects.
 // =====================================================================
 
-pub async fn run_bitget_stream(tx: mpsc::Sender<AggTrade>, symbol: String, status: FeedStatus) -> Result<()> {
+pub async fn run_bitget_stream(
+    tx: mpsc::Sender<AggTrade>,
+    symbol: String,
+    status: FeedStatus,
+) -> Result<()> {
     let url = "wss://ws.bitget.com/v2/ws/public";
     loop {
         status.set("bitget", "connecting");
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:216:
                         {"instType": "USDT-FUTURES", "channel": "publicTrade", "instId": symbol},
                     ]
                 });
-                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
+                if write
+                    .send(Message::Text(sub.to_string().into()))
+                    .await
+                    .is_err()
+                {
                     warn!("Bitget subscribe send failed");
                     status.set("bitget", "error");
                     continue;
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:249:
                             };
                             if let Some(err) = v.get("code").and_then(|c| c.as_str()) {
                                 if err != "0" {
-                                    warn!("Bitget error frame: {}", v["msg"].as_str().unwrap_or("?"));
+                                    warn!(
+                                        "Bitget error frame: {}",
+                                        v["msg"].as_str().unwrap_or("?")
+                                    );
                                 }
                                 continue;
                             }
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:265:
                                         (
                                             t.get(0).and_then(num_str),
                                             t.get(1).and_then(num_str),
-                                            t.get(3).and_then(|x| x.as_str().and_then(|s| s.parse().ok()))
+                                            t.get(3)
+                                                .and_then(|x| {
+                                                    x.as_str().and_then(|s| s.parse().ok())
+                                                })
                                                 .or_else(|| t.get(3).and_then(|x| x.as_i64())),
                                             t.get(2).and_then(|s| s.as_str()) == Some("buy"),
                                             true,
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:275:
                                         (
                                             t["price"].as_str().map(String::from),
                                             t["size"].as_str().map(String::from),
-                                            t["ts"].as_str().and_then(|s| s.parse().ok())
+                                            t["ts"]
+                                                .as_str()
+                                                .and_then(|s| s.parse().ok())
                                                 .or_else(|| t["ts"].as_i64()),
                                             t["side"].as_str() == Some("buy"),
                                             true,
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:282:
                                         )
                                     };
                                     if let (Some(price), Some(qty)) = (price, qty) {
-                                        let ts = ts.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
-                                        let _ = tx.send(AggTrade::new(
-                                            "bitget", &symbol, &price, &qty, ts, taker_buy, has_side,
-                                        )).await;
+                                        let ts = ts.unwrap_or_else(|| {
+                                            chrono::Utc::now().timestamp_millis()
+                                        });
+                                        let _ = tx
+                                            .send(AggTrade::new(
+                                                "bitget", &symbol, &price, &qty, ts, taker_buy,
+                                                has_side,
+                                            ))
+                                            .await;
                                     }
                                 }
                             }
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:302:
                 status.set("bitget", "disconnected");
             }
             Err(e) => {
-                warn!("Bitget connect failed: {}. Retrying in {RECONNECT_SECS}s", e);
+                warn!(
+                    "Bitget connect failed: {}. Retrying in {RECONNECT_SECS}s",
+                    e
+                );
                 status.set("bitget", "error");
             }
         }
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:315:
 // `size` is signed: >0 buyer-taker, <0 seller-taker.
 // =====================================================================
 
-pub async fn run_gate_stream(tx: mpsc::Sender<AggTrade>, symbol: String, status: FeedStatus) -> Result<()> {
+pub async fn run_gate_stream(
+    tx: mpsc::Sender<AggTrade>,
+    symbol: String,
+    status: FeedStatus,
+) -> Result<()> {
     let url = "wss://fx-ws.gateio.ws/v4/ws/usdt";
     loop {
         status.set("gate", "connecting");
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:330:
                     "event": "subscribe",
                     "payload": [symbol]
                 });
-                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
+                if write
+                    .send(Message::Text(sub.to_string().into()))
+                    .await
+                    .is_err()
+                {
                     warn!("Gate subscribe send failed");
                     status.set("gate", "error");
                     continue;
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:356:
                                     if t["contract"].as_str() != Some(symbol.as_str()) {
                                         continue;
                                     }
-                                    let Some(price) = num_str(&t["price"]) else { continue };
+                                    let Some(price) = num_str(&t["price"]) else {
+                                        continue;
+                                    };
                                     let signed_size = t["size"].as_i64().unwrap_or(0);
                                     let qty = signed_size.abs().to_string();
                                     let ts = t["time"].as_i64().unwrap_or(0);
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:363:
-                                    let _ = tx.send(AggTrade::new(
-                                        "gate", &symbol, &price, &qty, ts,
-                                        signed_size > 0, true,
-                                    )).await;
+                                    let _ = tx
+                                        .send(AggTrade::new(
+                                            "gate",
+                                            &symbol,
+                                            &price,
+                                            &qty,
+                                            ts,
+                                            signed_size > 0,
+                                            true,
+                                        ))
+                                        .await;
                                 }
                             }
                         }
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:391:
 // Frames: {"feed":"trade", product_id, side, qty, price, time(ms), ...}
 // =====================================================================
 
-pub async fn run_kraken_stream(tx: mpsc::Sender<AggTrade>, product: String, status: FeedStatus) -> Result<()> {
+pub async fn run_kraken_stream(
+    tx: mpsc::Sender<AggTrade>,
+    product: String,
+    status: FeedStatus,
+) -> Result<()> {
     let url = "wss://futures.kraken.com/ws/v1";
     loop {
         status.set("kraken", "connecting");
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:405:
                     "feed": "trade",
                     "product_ids": [product]
                 });
-                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
+                if write
+                    .send(Message::Text(sub.to_string().into()))
+                    .await
+                    .is_err()
+                {
                     warn!("Kraken subscribe send failed");
                     status.set("kraken", "error");
                     continue;
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:423:
                             };
                             if v.get("event").is_some() {
                                 if v["event"].as_str() == Some("error") {
-                                    warn!("Kraken error frame: {}", v["message"].as_str().unwrap_or("?"));
+                                    warn!(
+                                        "Kraken error frame: {}",
+                                        v["message"].as_str().unwrap_or("?")
+                                    );
                                 }
                                 continue;
                             }
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:434:
                                     let qty = num_str(&v["qty"]).unwrap_or_default();
                                     let ts = v["time"].as_i64().unwrap_or(0);
                                     let taker_buy = v["side"].as_str() == Some("buy");
-                                    let _ = tx.send(AggTrade::new(
-                                        "kraken", &product, &price, &qty, ts, taker_buy, true,
-                                    )).await;
+                                    let _ = tx
+                                        .send(AggTrade::new(
+                                            "kraken", &product, &price, &qty, ts, taker_buy, true,
+                                        ))
+                                        .await;
                                 }
                                 "trade_snapshot" => {
                                     // history replay — skip
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:455:
                 status.set("kraken", "disconnected");
             }
             Err(e) => {
-                warn!("Kraken connect failed: {}. Retrying in {RECONNECT_SECS}s", e);
+                warn!(
+                    "Kraken connect failed: {}. Retrying in {RECONNECT_SECS}s",
+                    e
+                );
                 status.set("kraken", "error");
             }
         }
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:469:
 // trade push cmd_id 22001-22003 with price/volume/trade_direction.
 // =====================================================================
 
-pub async fn run_alltick_stream(tx: mpsc::Sender<AggTrade>, url: String, code: String, status: FeedStatus) -> Result<()> {
+pub async fn run_alltick_stream(
+    tx: mpsc::Sender<AggTrade>,
+    url: String,
+    code: String,
+    status: FeedStatus,
+) -> Result<()> {
     let token = std::env::var("ALLTICK_TOKEN").unwrap_or_default();
     let full_url = format!("{url}?token={token}");
 
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:483:
                     "cmd_id": 22004, "seq_id": 1, "trace": "rust-xauusd-engine",
                     "data": { "symbol_list": [{ "code": code }] }
                 });
-                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
+                if write
+                    .send(Message::Text(sub.to_string().into()))
+                    .await
+                    .is_err()
+                {
                     warn!("AllTick subscribe send failed");
                     status.set("alltick", "error");
                     continue;
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:497:
                     loop {
                         iv.tick().await;
                         let hb = json!({"cmd_id": 22000, "seq_id": 2, "trace": "hb", "data": {}});
-                        if write.send(Message::Text(hb.to_string().into())).await.is_err() {
+                        if write
+                            .send(Message::Text(hb.to_string().into()))
+                            .await
+                            .is_err()
+                        {
                             break;
                         }
                     }
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:506:
                 while let Some(msg) = read.next().await {
                     if let Ok(Message::Text(text)) = msg {
                         status.mark_msg("alltick");
-                        let v: Value = match serde_json::from_str(&text) { Ok(v) => v, _ => continue };
+                        let v: Value = match serde_json::from_str(&text) {
+                            Ok(v) => v,
+                            _ => continue,
+                        };
                         let cmd = v["cmd_id"].as_i64().unwrap_or(0);
                         // 22001 quote / 22002 price / 22003 trade pushes
                         if !(22001..=22003).contains(&cmd) {
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:521:
                             if d["code"].as_str().is_some_and(|c| c != code) {
                                 continue;
                             }
-                            let Some(price) = num_str(&d["price"]) else { continue };
+                            let Some(price) = num_str(&d["price"]) else {
+                                continue;
+                            };
                             let vol = num_str(&d["volume"]).unwrap_or_else(|| "0".into());
                             let ts = d["tick_time"].as_i64().unwrap_or(0);
                             // trade_direction: 1 aggressive buy, 2 aggressive sell.
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:528:
                             // Price/quote pushes carry no direction → no flow side.
                             match d["trade_direction"].as_i64() {
                                 Some(dir @ (1 | 2)) => {
-                                    let _ = tx.send(AggTrade::new(
-                                        "alltick", &code, &price, &vol, ts, dir == 1, true,
-                                    )).await;
+                                    let _ = tx
+                                        .send(AggTrade::new(
+                                            "alltick",
+                                            &code,
+                                            &price,
+                                            &vol,
+                                            ts,
+                                            dir == 1,
+                                            true,
+                                        ))
+                                        .await;
                                 }
                                 _ => {
-                                    let _ = tx.send(AggTrade::new(
-                                        "alltick", &code, &price, &vol, ts, true, false,
-                                    )).await;
+                                    let _ = tx
+                                        .send(AggTrade::new(
+                                            "alltick", &code, &price, &vol, ts, true, false,
+                                        ))
+                                        .await;
                                 }
                             }
                         }
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:546:
                 status.set("alltick", "disconnected");
             }
             Err(e) => {
-                warn!("AllTick connect failed: {}. Retrying in {RECONNECT_SECS}s", e);
+                warn!(
+                    "AllTick connect failed: {}. Retrying in {RECONNECT_SECS}s",
+                    e
+                );
                 status.set("alltick", "error");
             }
         }
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:561:
 // `d`: 2 = aggressive buy, 1 = aggressive sell, 0/absent = unknown.
 // =====================================================================
 
-pub async fn run_itick_stream(tx: mpsc::Sender<AggTrade>, url: String, symbol: String, status: FeedStatus) -> Result<()> {
+pub async fn run_itick_stream(
+    tx: mpsc::Sender<AggTrade>,
+    url: String,
+    symbol: String,
+    status: FeedStatus,
+) -> Result<()> {
     let token = std::env::var("ITICK_TOKEN").unwrap_or_default();
 
     loop {
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:617:
                 while let Some(msg) = read.next().await {
                     if let Ok(Message::Text(text)) = msg {
                         status.mark_msg("itick");
-                        let v: Value = match serde_json::from_str(&text) { Ok(v) => v, _ => continue };
+                        let v: Value = match serde_json::from_str(&text) {
+                            Ok(v) => v,
+                            _ => continue,
+                        };
 
                         // Handshake/ack frames
                         if v.get("resAc").is_some() {
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:625:
                             let ok = v["code"].as_i64().unwrap_or(0) == 0
                                 || v["code"].as_i64() == Some(1);
                             if ac == "auth" && ok && !subscribed {
-                                let sub = json!({"ac": "subscribe", "params": format!("{symbol},tick")});
+                                let sub =
+                                    json!({"ac": "subscribe", "params": format!("{symbol},tick")});
                                 if out_tx.send(Message::Text(sub.to_string().into())).is_ok() {
                                     subscribed = true;
                                     info!("iTick subscribed to {symbol} ticks");
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:649:
                                 continue;
                             }
                         }
-                        let Some(price) = num_str(&d["ld"]) else { continue };
+                        let Some(price) = num_str(&d["ld"]) else {
+                            continue;
+                        };
                         let vol = num_str(&d["v"]).unwrap_or_else(|| "0".into());
                         let ts = d["t"].as_i64().unwrap_or(0);
                         match d["d"].as_i64() {
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:656:
                             Some(dir @ (1 | 2)) => {
-                                let _ = tx.send(AggTrade::new(
-                                    "itick", &symbol, &price, &vol, ts, dir == 2, true,
-                                )).await;
+                                let _ = tx
+                                    .send(AggTrade::new(
+                                        "itick",
+                                        &symbol,
+                                        &price,
+                                        &vol,
+                                        ts,
+                                        dir == 2,
+                                        true,
+                                    ))
+                                    .await;
                             }
                             _ => {
                                 // No taker side published — volume only.
Diff in /home/runner/work/Bot/Bot/src/multi_exchange.rs:663:
-                                let _ = tx.send(AggTrade::new(
-                                    "itick", &symbol, &price, &vol, ts, true, false,
-                                )).await;
+                                let _ = tx
+                                    .send(AggTrade::new(
+                                        "itick", &symbol, &price, &vol, ts, true, false,
+                                    ))
+                                    .await;
                             }
                         }
                     }
Diff in /home/runner/work/Bot/Bot/src/order_flow.rs:1:
-use std::collections::{HashMap, VecDeque};
 use chrono::Utc;
+use std::collections::{HashMap, VecDeque};
 
 use crate::types::{AggTrade, OrderflowEvent};
 use crate::volume_profile::VolumeProfileEngine;
Diff in /home/runner/work/Bot/Bot/src/order_flow.rs:11:
 const MIN_SAMPLES: usize = 50;
 
 // Absolute floors in USD notional.
-const FLOOR_BUBBLE: f64 = 50_000.0;   // $50k net delta over 500 trades
+const FLOOR_BUBBLE: f64 = 50_000.0; // $50k net delta over 500 trades
 const FLOOR_ABSORPTION: f64 = 100_000.0; // $100k total volume over 500 trades
 const ABSORPTION_DELTA_CAP: f64 = 20_000.0; // |net delta| must be < $20k to count as "near zero"
 
Diff in /home/runner/work/Bot/Bot/src/order_flow.rs:46:
         let qty = trade.qty_f64();
         // Feeds without a taker side (quote ticks) must not fabricate
         // directional delta — they only count toward total volume.
-        let signed_qty = if trade.has_flow_side { trade.signed_delta() } else { 0.0 };
+        let signed_qty = if trade.has_flow_side {
+            trade.signed_delta()
+        } else {
+            0.0
+        };
 
         let notional_delta = signed_qty * price;
         let notional_volume = qty * price;
Diff in /home/runner/work/Bot/Bot/src/order_flow.rs:53:
-        
+
         self.cumulative_delta += notional_delta;
         self.last_exchange = trade.exchange.clone();
-        self.delta_history.push_back((trade.trade_time, price, notional_delta, notional_volume));
-        
+        self.delta_history
+            .push_back((trade.trade_time, price, notional_delta, notional_volume));
+
         while self.delta_history.len() > self.window_size {
             self.delta_history.pop_front();
         }
Diff in /home/runner/work/Bot/Bot/src/order_flow.rs:61:
 
         if self.delta_history.len() >= LOOKBACK {
-            let net_delta: f64 = self.delta_history.iter().rev().take(LOOKBACK).map(|(_, _, d, _)| *d).sum();
-            let total_vol: f64 = self.delta_history.iter().rev().take(LOOKBACK).map(|(_, _, _, v)| *v).sum();
+            let net_delta: f64 = self
+                .delta_history
+                .iter()
+                .rev()
+                .take(LOOKBACK)
+                .map(|(_, _, d, _)| *d)
+                .sum();
+            let total_vol: f64 = self
+                .delta_history
+                .iter()
+                .rev()
+                .take(LOOKBACK)
+                .map(|(_, _, _, v)| *v)
+                .sum();
             let t = self.delta_history.back().unwrap().0;
 
             self.bubble_mags.push_back((t, net_delta.abs()));
Diff in /home/runner/work/Bot/Bot/src/order_flow.rs:68:
-            
+
             if net_delta.abs() < ABSORPTION_DELTA_CAP {
                 self.abs_vols.push_back((t, total_vol));
             }
Diff in /home/runner/work/Bot/Bot/src/order_flow.rs:72:
 
             let cutoff = t - SLIDING_WINDOW_MS;
             while let Some(&front) = self.bubble_mags.front() {
-                if front.0 < cutoff { self.bubble_mags.pop_front(); } else { break; }
+                if front.0 < cutoff {
+                    self.bubble_mags.pop_front();
+                } else {
+                    break;
+                }
             }
             while let Some(&front) = self.abs_vols.front() {
-                if front.0 < cutoff { self.abs_vols.pop_front(); } else { break; }
+                if front.0 < cutoff {
+                    self.abs_vols.pop_front();
+                } else {
+                    break;
+                }
             }
         }
     }
Diff in /home/runner/work/Bot/Bot/src/order_flow.rs:82:
 
     pub fn recent_delta_notional(&self) -> f64 {
         let n = LOOKBACK.min(self.delta_history.len());
-        self.delta_history.iter().rev().take(n).map(|(_, _, d, _)| *d).sum()
+        self.delta_history
+            .iter()
+            .rev()
+            .take(n)
+            .map(|(_, _, d, _)| *d)
+            .sum()
     }
 
     pub fn recent_total_volume(&self) -> f64 {
Diff in /home/runner/work/Bot/Bot/src/order_flow.rs:89:
         let n = LOOKBACK.min(self.delta_history.len());
-        self.delta_history.iter().rev().take(n).map(|(_, _, _, v)| *v).sum()
+        self.delta_history
+            .iter()
+            .rev()
+            .take(n)
+            .map(|(_, _, _, v)| *v)
+            .sum()
     }
 
     fn price_at(&self, lookback: usize) -> Option<f64> {
Diff in /home/runner/work/Bot/Bot/src/order_flow.rs:125:
     }
 
     fn percentile(sorted: &[f64], p: f64) -> f64 {
-        if sorted.is_empty() { return 0.0; }
+        if sorted.is_empty() {
+            return 0.0;
+        }
         let idx = (((sorted.len() as f64) - 1.0) * p).round() as usize;
         sorted[idx.min(sorted.len() - 1)]
     }
Diff in /home/runner/work/Bot/Bot/src/order_flow.rs:191:
         if let Some(start_price) = self.price_at(LOOKBACK) {
             let displacement = current_price - start_price;
 
-            if total_vol > absorption_threshold 
-                && net_delta.abs() < ABSORPTION_DELTA_CAP * 2.0 
-                && displacement >= -0.5 
+            if total_vol > absorption_threshold
+                && net_delta.abs() < ABSORPTION_DELTA_CAP * 2.0
+                && displacement >= -0.5
                 && self.should_emit("ABS_BUY", now_ms)
             {
-                let reference = Self::nearest_level(vp, current_price)
-                    .unwrap_or(current_price);
+                let reference = Self::nearest_level(vp, current_price).unwrap_or(current_price);
                 events.push(OrderflowEvent {
                     kind: "ABS_BUY".into(),
                     level: reference,
Diff in /home/runner/work/Bot/Bot/src/order_flow.rs:207:
                 });
             }
 
-            if total_vol > absorption_threshold 
+            if total_vol > absorption_threshold
                 && net_delta.abs() < ABSORPTION_DELTA_CAP * 2.0
-                && displacement <= 0.5 
+                && displacement <= 0.5
                 && self.should_emit("ABS_SELL", now_ms)
             {
-                let reference = Self::nearest_level(vp, current_price)
-                    .unwrap_or(current_price);
+                let reference = Self::nearest_level(vp, current_price).unwrap_or(current_price);
                 events.push(OrderflowEvent {
                     kind: "ABS_SELL".into(),
                     level: reference,
Diff in /home/runner/work/Bot/Bot/src/order_flow.rs:234:
         events
     }
 }
+
Diff in /home/runner/work/Bot/Bot/src/sifting_rest.rs:89:
 ) -> Result<(Vec<VpCandle>, Option<String>)> {
     if let Some(response_symbol) = response.meta.symbol.as_deref() {
         if !response_symbol.eq_ignore_ascii_case(symbol) {
-            anyhow::bail!(
-                "SiftingIO returned symbol {response_symbol}, expected {symbol}"
-            );
+            anyhow::bail!("SiftingIO returned symbol {response_symbol}, expected {symbol}");
         }
     }
     if let Some(interval) = response.meta.interval.as_deref() {
Diff in /home/runner/work/Bot/Bot/src/sifting_rest.rs:98:
         if interval != expected_interval {
-            anyhow::bail!(
-                "SiftingIO returned interval {interval}, expected {expected_interval}"
-            );
+            anyhow::bail!("SiftingIO returned interval {interval}, expected {expected_interval}");
         }
     }
 
Diff in /home/runner/work/Bot/Bot/src/sifting_rest.rs:373:
             None,
         )
         .unwrap();
-        assert!(url
-            .as_str()
-            .starts_with("https://api.sifting.io/v1/hist/commodities/XAUUSD/bars?"));
+        assert!(
+            url.as_str()
+                .starts_with("https://api.sifting.io/v1/hist/commodities/XAUUSD/bars?")
+        );
         assert!(url.as_str().contains("interval=15m"));
         assert!(url.as_str().contains("order=desc"));
         assert!(url.as_str().contains("limit=2000"));
Diff in /home/runner/work/Bot/Bot/src/sifting_rest.rs:400:
         assert!(url.as_str().contains("interval=1m"));
         assert!(url.as_str().contains("order=asc"));
         assert!(url.as_str().contains("limit=2000"));
-        assert!(url.as_str().contains("cursor=cursor%2Fwith%2Breserved%3Dchars"));
+        assert!(
+            url.as_str()
+                .contains("cursor=cursor%2Fwith%2Breserved%3Dchars")
+        );
     }
 
     #[test]
Diff in /home/runner/work/Bot/Bot/src/sifting_rest.rs:433:
         let start = 1_700_000_000_000;
         let week: i64 = 5 * 86_400_000;
         let candles: Vec<VpCandle> = (0..(119 * 60)).map(|i| bar(start + i * 60_000)).collect();
-        assert!(profile_history_covers(&candles, start, start + week, 60_000, 2 * 3_600_000));
+        assert!(profile_history_covers(
+            &candles,
+            start,
+            start + week,
+            60_000,
+            2 * 3_600_000
+        ));
     }
 
     /// ...while a page that only *reaches* the ends but is mostly empty fails.
Diff in /home/runner/work/Bot/Bot/src/sifting_rest.rs:446:
         let candles: Vec<VpCandle> = (0..(week / 1_800_000) - 1)
             .map(|i| bar(start + i * 1_800_000))
             .collect();
-        assert!(!profile_history_covers(&candles, start, start + week, 60_000, 2 * 3_600_000));
+        assert!(!profile_history_covers(
+            &candles,
+            start,
+            start + week,
+            60_000,
+            2 * 3_600_000
+        ));
         // Nothing at all, or nothing near the far end, is rejected too.
         assert!(!profile_history_covers(&[], start, start + week, 60_000, 0));
-        assert!(!profile_history_covers(&candles, start, start + 2 * week, 60_000, 0));
+        assert!(!profile_history_covers(
+            &candles,
+            start,
+            start + 2 * week,
+            60_000,
+            0
+        ));
     }
 
     #[test]
Diff in /home/runner/work/Bot/Bot/src/sifting_ws.rs:470:
         }
         let bar = st.on_tick(100.0, T0 + 10, None).unwrap().live.1;
         // first=flat, +up, =flat, -down, +up, -down
-        assert_eq!((bar.ticks, bar.up_ticks, bar.down_ticks, bar.flat_ticks), (6, 2, 2, 2));
+        assert_eq!(
+            (bar.ticks, bar.up_ticks, bar.down_ticks, bar.flat_ticks),
+            (6, 2, 2, 2)
+        );
     }
 
     #[test]
Diff in /home/runner/work/Bot/Bot/src/sifting_ws.rs:488:
     fn bucket_roll_emits_one_closed_bar_then_a_fresh_live_bar() {
         let mut st = TickState::new();
         st.on_tick(100.0, T0, None);
-        assert!(st.on_tick(100.5, T0 + 1_000, None).unwrap().closed.is_none());
+        assert!(
+            st.on_tick(100.5, T0 + 1_000, None)
+                .unwrap()
+                .closed
+                .is_none()
+        );
 
         let out = st.on_tick(101.0, T0 + M15, None).unwrap();
         let (closed_candle, closed_bar) = out.closed.expect("bucket must close");
Diff in /home/runner/work/Bot/Bot/src/sifting_ws.rs:550:
             );
         }
         // Control frames and other symbols are ignored.
-        handle_text(r#"{"f":"pong"}"#, "XAUUSD", &mut st, &bc, &ctx, Some(&profile_tx), &store);
         handle_text(
+            r#"{"f":"pong"}"#,
+            "XAUUSD",
+            &mut st,
+            &bc,
+            &ctx,
+            Some(&profile_tx),
+            &store,
+        );
+        handle_text(
             &tick(1.0, T0 + M15 + 1).replace("XAUUSD", "XAGUSD"),
             "XAUUSD",
             &mut st,
Diff in /home/runner/work/Bot/Bot/src/sifting_ws.rs:572:
         }
         assert_eq!(
             kinds,
-            ["candle", "tv", "candle", "tv", "candle", "tv_closed", "candle", "tv"]
+            [
+                "candle",
+                "tv",
+                "candle",
+                "tv",
+                "candle",
+                "tv_closed",
+                "candle",
+                "tv"
+            ]
         );
 
-        let closed = crx.try_recv().expect("closed candle goes to the chart consumer");
+        let closed = crx
+            .try_recv()
+            .expect("closed candle goes to the chart consumer");
         assert_eq!((closed.time, closed.volume), (T0, 2.0));
         assert!(crx.try_recv().is_err());
 
Diff in /home/runner/work/Bot/Bot/src/sifting_ws.rs:602:
         assert_eq!(
             keys,
             [
-                "close", "closed", "down_ticks", "flat_ticks", "last_tick", "source",
-                "ticks", "ticks_per_sec", "time", "up_ticks"
+                "close",
+                "closed",
+                "down_ticks",
+                "flat_ticks",
+                "last_tick",
+                "source",
+                "ticks",
+                "ticks_per_sec",
+                "time",
+                "up_ticks"
             ]
         );
     }
Diff in /home/runner/work/Bot/Bot/src/tick_volume.rs:105:
         s.upsert(bar(0, 1));
         s.upsert(bar(1_800_000, 5));
         let snap = s.snapshot();
-        assert_eq!(snap.iter().map(|b| b.time).collect::<Vec<_>>(), vec![0, 1_800_000]);
+        assert_eq!(
+            snap.iter().map(|b| b.time).collect::<Vec<_>>(),
+            vec![0, 1_800_000]
+        );
         assert_eq!(snap[1].ticks, 5);
     }
 }
Diff in /home/runner/work/Bot/Bot/src/volume_profile/histogram.rs:126:
             },
         }
     }
-
 }
 
 /// One price row of the histogram, with the up/down split TradingView's
Diff in /home/runner/work/Bot/Bot/src/volume_profile/histogram.rs:370:
     let poc_idx = bins
         .iter()
         .enumerate()
-        .max_by(|a, b| {
-            a.1.partial_cmp(b.1)
-                .unwrap_or(std::cmp::Ordering::Equal)
-        })
+        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
         .map(|(i, _)| i)?;
 
     // Value area: start at the POC and repeatedly add the larger adjacent
Diff in /home/runner/work/Bot/Bot/src/volume_profile/histogram.rs:554:
             candle(2, 4150.0, 4150.5, 10.0),
         ];
         let hist = histogram(&candles, None, &rows_model(12)).expect("profile");
-        assert_eq!(hist.rows.last().unwrap().high, 4150.5, "top row clipped to the profile high");
+        assert_eq!(
+            hist.rows.last().unwrap().high,
+            4150.5,
+            "top row clipped to the profile high"
+        );
         assert_eq!(hist.range_high, 4150.5);
     }
 
Diff in /home/runner/work/Bot/Bot/src/volume_profile/session.rs:71:
 /// the market traded 4150+). A session therefore only counts when at least one
 /// of its bars has a body, which is the same closed-market signature the
 /// dashboard uses to drop filler runs.
-pub fn previous_session_slice(
-    candles: &[VpCandle],
-    now_ms: i64,
-) -> (i64, i64, Vec<VpCandle>) {
+pub fn previous_session_slice(candles: &[VpCandle], now_ms: i64) -> (i64, i64, Vec<VpCandle>) {
     let mut end = last_session_close_utc(now_ms);
     let first_end = end;
     let mut first_start = session_close_shift(end, -1);
Diff in /home/runner/work/Bot/Bot/src/volume_profile/session.rs:170:
         // Sunday 19:00 NY: the most recent close is Sunday 18:00, so the
         // session being described would be the (closed) weekend session.
         let (start, end, slice) = previous_session_slice(&candles, ny(2025, 6, 8, 19, 0));
-        assert_eq!(end, sat_close, "PS ends where the last traded session closed");
+        assert_eq!(
+            end, sat_close,
+            "PS ends where the last traded session closed"
+        );
         assert_eq!(start, fri_close, "PS starts at the previous 18:00 NY");
         assert_eq!(slice.len(), 30, "PS carries the Friday session's bars");
         assert!(slice.iter().any(|c| c.open != c.close));
Diff in /home/runner/work/Bot/Bot/src/volume_profile/swing.rs:8:
 
 use crate::types::{VpCandle, VpLevels};
 
+use super::StoredProfile;
 use super::histogram;
 use super::timeframe::LowerTf;
-use super::StoredProfile;
 
 /// Bars on each side of a fractal pivot. 3 × 15m = 45 minutes of
 /// confirmation lag: responsive enough to track the current swing without
Diff in /home/runner/work/Bot/Bot/src/volume_profile/swing.rs:138:
             // double tops/bottoms anchor at their second touch.
             let is_high = high.is_finite()
                 && (index - SWING_PIVOT_RADIUS..=index + SWING_PIVOT_RADIUS)
-                    .all(|j| {
-                        j == index || !candles[j].high.is_finite() || high >= candles[j].high
-                    });
+                    .all(|j| j == index || !candles[j].high.is_finite() || high >= candles[j].high);
             let is_low = low.is_finite()
                 && (index - SWING_PIVOT_RADIUS..=index + SWING_PIVOT_RADIUS)
-                    .all(|j| {
-                        j == index || !candles[j].low.is_finite() || low <= candles[j].low
-                    });
+                    .all(|j| j == index || !candles[j].low.is_finite() || low <= candles[j].low);
 
             if is_high {
                 push_pivot(&mut pivots, PivotKind::High, index, candles);
Diff in /home/runner/work/Bot/Bot/src/volume_profile/swing.rs:213:
         {
             high_rel = i;
         }
-        if c.low.is_finite()
-            && (!window[low_rel].low.is_finite() || c.low <= window[low_rel].low)
-        {
+        if c.low.is_finite() && (!window[low_rel].low.is_finite() || c.low <= window[low_rel].low) {
             low_rel = i;
         }
     }
Diff in /home/runner/work/Bot/Bot/src/volume_profile/swing.rs:252:
     }
 }
 
-fn push_pivot(
-    pivots: &mut Vec<Pivot>,
-    kind: PivotKind,
-    index: usize,
-    candles: &[VpCandle],
-) {
+fn push_pivot(pivots: &mut Vec<Pivot>, kind: PivotKind, index: usize, candles: &[VpCandle]) {
     if let Some(previous) = pivots.last_mut() {
         if previous.kind == kind {
             let replace = match kind {
Diff in /home/runner/work/Bot/Bot/src/volume_profile/swing.rs:297:
         prices
             .iter()
             .enumerate()
-            .map(|(i, price)| {
-                candle(
-                    start + i as i64 * 15 * MIN,
-                    price - 0.5,
-                    price + 0.5,
-                    100.0,
-                )
-            })
+            .map(|(i, price)| candle(start + i as i64 * 15 * MIN, price - 0.5, price + 0.5, 100.0))
             .collect()
     }
 
Diff in /home/runner/work/Bot/Bot/src/volume_profile/swing.rs:313:
         histogram::ProfileModel::default()
     }
 
-    fn levels_of(
-        computed: Option<(VpLevels, StoredProfile)>,
-    ) -> VpLevels {
+    fn levels_of(computed: Option<(VpLevels, StoredProfile)>) -> VpLevels {
         computed.expect("swing profile").0
     }
 
Diff in /home/runner/work/Bot/Bot/src/volume_profile/swing.rs:322:
     #[test]
     fn bearish_swing_is_anchored_high_to_low() {
         let prices = [
-            100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 104.0, 103.0, 102.0, 101.0,
-            100.0, 99.0, 98.0, 97.0, 96.0, 95.0, 96.0, 97.0, 98.0, 99.0, 100.0, 101.0,
-            102.0, 103.0, 104.0,
+            100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 104.0, 103.0, 102.0, 101.0, 100.0, 99.0,
+            98.0, 97.0, 96.0, 95.0, 96.0, 97.0, 98.0, 99.0, 100.0, 101.0, 102.0, 103.0, 104.0,
         ];
-        let swing = levels_of(compute_swing(&swing_history(&prices), &model(), LowerTf::Tv));
+        let swing = levels_of(compute_swing(
+            &swing_history(&prices),
+            &model(),
+            LowerTf::Tv,
+        ));
         assert_eq!(swing.window, "SWING_BEAR");
         assert_eq!(swing.direction, "bearish");
         assert_eq!(swing.swing_high, Some(105.5));
Diff in /home/runner/work/Bot/Bot/src/volume_profile/swing.rs:337:
     #[test]
     fn bullish_swing_is_anchored_low_to_high() {
         let prices = [
-            105.0, 104.0, 103.0, 102.0, 101.0, 100.0, 101.0, 102.0, 103.0, 104.0, 105.0,
-            106.0, 107.0, 108.0, 109.0, 110.0, 109.0, 108.0, 107.0, 106.0, 105.0, 104.0,
-            103.0, 102.0, 101.0,
+            105.0, 104.0, 103.0, 102.0, 101.0, 100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0,
+            107.0, 108.0, 109.0, 110.0, 109.0, 108.0, 107.0, 106.0, 105.0, 104.0, 103.0, 102.0,
+            101.0,
         ];
-        let swing = levels_of(compute_swing(&swing_history(&prices), &model(), LowerTf::Tv));
+        let swing = levels_of(compute_swing(
+            &swing_history(&prices),
+            &model(),
+            LowerTf::Tv,
+        ));
         assert_eq!(swing.window, "SWING_BULL");
         assert_eq!(swing.direction, "bullish");
         assert_eq!(swing.swing_low, Some(99.5));
Diff in /home/runner/work/Bot/Bot/src/volume_profile/swing.rs:354:
         // Double top with two equal 103 touches: the leg must start at the
         // *second* touch (index 4), not the first.
         let prices = [
-            100.0, 101.0, 102.0, 103.0, 103.0, 102.0, 101.0, 100.0, 99.0, 98.0,
-            99.0, 100.0, 101.0, 102.0, 103.0,
+            100.0, 101.0, 102.0, 103.0, 103.0, 102.0, 101.0, 100.0, 99.0, 98.0, 99.0, 100.0, 101.0,
+            102.0, 103.0,
         ];
         let candles = swing_history(&prices);
         let swing = levels_of(compute_swing(&candles, &model(), LowerTf::Tv));
Diff in /home/runner/work/Bot/Bot/src/volume_profile/timeframe.rs:141:
 /// coarser than the requested resolution (the 15m chart seed fallback), the
 /// history is used as-is and its own resolution is reported, so a client can
 /// see that this profile was built from 15m bars.
-pub fn prepare_input(
-    candles: &[VpCandle],
-    lower_tf: LowerTf,
-) -> (Vec<VpCandle>, &'static str) {
+pub fn prepare_input(candles: &[VpCandle], lower_tf: LowerTf) -> (Vec<VpCandle>, &'static str) {
     if candles.is_empty() {
         return (Vec::new(), "15m");
     }
Diff in /home/runner/work/Bot/Bot/src/volume_profile/timeframe.rs:300:
     #[test]
     fn prepare_input_never_downscales_coarse_history() {
         // 15m history: asking for TV parity cannot invent finer bars.
-        let candles = vec![bar(0, 4100.0, 4101.0, 1.0), bar(900_000, 4101.0, 4102.0, 1.0)];
+        let candles = vec![
+            bar(0, 4100.0, 4101.0, 1.0),
+            bar(900_000, 4101.0, 4102.0, 1.0),
+        ];
         let (out, label) = prepare_input(&candles, LowerTf::Tv);
         assert_eq!(out.len(), 2);
         assert_eq!(label, "15m");
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:39:
 // Re-exported so the binary can construct the TradingView-parity model from
 // `Config` without reaching into the submodules.
 pub use histogram::{ProfileModel, RowMode};
-pub use timeframe::{parse_label, LowerTf};
+pub use timeframe::{LowerTf, parse_label};
 
 /// Keep the complete fixed 2,000-candle Sifting 15m swing seed (about 21
 /// days) plus room for live closes and session/week boundaries. The 1m profile
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:75:
     pub fn message(self) -> &'static str {
         match self {
             Self::Invalid => "start must be before end, and the range at most 35 days",
-            Self::NotRetained => {
-                "the requested range has no retained 1m history to profile"
-            }
+            Self::NotRetained => "the requested range has no retained 1m history to profile",
         }
     }
 }
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:197:
 
     /// Seed independent profile and swing histories, then compute once. This
     /// avoids caching an empty CW snapshot between two seed operations.
-    pub fn ingest_history(
-        &mut self,
-        profile_candles: Vec<VpCandle>,
-        swing_candles: Vec<VpCandle>,
-    ) {
+    pub fn ingest_history(&mut self, profile_candles: Vec<VpCandle>, swing_candles: Vec<VpCandle>) {
         upsert_candles(&mut self.profile_candles, profile_candles);
         upsert_candles(&mut self.swing_candles, swing_candles);
         self.recompute(Utc::now().timestamp_millis());
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:257:
         // Keep out-of-order candles in storage; the individual windows apply
         // their own time bounds. This also lets a historical backfill arrive
         // before a caller advances its synthetic/test clock.
-        self.profile_candles
-            .retain(|c| c.time >= retain_floor);
-        self.swing_candles
-            .retain(|c| c.time >= now_ms - RETAIN_MS);
+        self.profile_candles.retain(|c| c.time >= retain_floor);
+        self.swing_candles.retain(|c| c.time >= now_ms - RETAIN_MS);
 
         let pw: Vec<_> = self
             .profile_candles
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:458:
             },
             None => self.model,
         };
-        let hist = histogram::histogram(&input, None, &model)
-            .ok_or(CustomRangeError::NotRetained)?;
+        let hist =
+            histogram::histogram(&input, None, &model).ok_or(CustomRangeError::NotRetained)?;
         Ok(VpAudit {
             window: "CUSTOM".into(),
             start: start_ms,
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:567:
     let input_bars = input.len();
     let histogram = histogram::histogram(&input, range, model)?;
     let levels = histogram.levels(
-        label,
-        start,
-        end,
-        direction,
-        swing_high,
-        swing_low,
-        interval,
-        input_bars,
+        label, start, end, direction, swing_high, swing_low, interval, input_bars,
     );
     let stored = StoredProfile {
         histogram,
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:596:
 }
 
 fn upsert_candles(candles: &mut Vec<VpCandle>, incoming: Vec<VpCandle>) {
-    let mut by_time: std::collections::BTreeMap<i64, VpCandle> =
-        candles.drain(..).map(|candle| (candle.time, candle)).collect();
+    let mut by_time: std::collections::BTreeMap<i64, VpCandle> = candles
+        .drain(..)
+        .map(|candle| (candle.time, candle))
+        .collect();
     for candle in incoming {
         by_time.insert(candle.time, candle);
     }
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:651:
         let mut e = VolumeProfileEngine::new();
 
         // Session A: Mon 18:00 -> Tue 18:00, price band 3300-3310.
-        fill(&mut e, ny(2025, 6, 9, 18, 0), ny(2025, 6, 10, 18, 0), 3300.0, 3310.0);
+        fill(
+            &mut e,
+            ny(2025, 6, 9, 18, 0),
+            ny(2025, 6, 10, 18, 0),
+            3300.0,
+            3310.0,
+        );
         // Session B: Tue 18:00 -> Wed 18:00, price band 3400-3410.
-        fill(&mut e, ny(2025, 6, 10, 18, 0), ny(2025, 6, 11, 18, 0), 3400.0, 3410.0);
+        fill(
+            &mut e,
+            ny(2025, 6, 10, 18, 0),
+            ny(2025, 6, 11, 18, 0),
+            3400.0,
+            3410.0,
+        );
         // Session C (in progress): Wed 18:00 -> now, band 3500-3510.
-        fill(&mut e, ny(2025, 6, 11, 18, 0), ny(2025, 6, 12, 10, 0), 3500.0, 3510.0);
+        fill(
+            &mut e,
+            ny(2025, 6, 11, 18, 0),
+            ny(2025, 6, 12, 10, 0),
+            3500.0,
+            3510.0,
+        );
 
         // At Tue 20:00 the last closed session is A.
         e.recompute(ny(2025, 6, 10, 20, 0));
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:662:
         let ps_a = e.ps_levels.clone().expect("PS for session A");
         assert_eq!(ps_a.start, ny(2025, 6, 9, 18, 0));
         assert_eq!(ps_a.end, ny(2025, 6, 10, 18, 0));
-        assert!((3300.0..=3310.0).contains(&ps_a.poc), "poc {} not in A", ps_a.poc);
+        assert!(
+            (3300.0..=3310.0).contains(&ps_a.poc),
+            "poc {} not in A",
+            ps_a.poc
+        );
 
         // Nothing has closed yet at Wed 10:00 -> PS must NOT move.
         assert!(!e.refresh_on_session_close(ny(2025, 6, 11, 10, 0)));
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:673:
         let ps_b = e.ps_levels.clone().expect("PS for session B");
         assert_eq!(ps_b.start, ny(2025, 6, 10, 18, 0));
         assert_eq!(ps_b.end, ny(2025, 6, 11, 18, 0));
-        assert!((3400.0..=3410.0).contains(&ps_b.poc), "poc {} not in B", ps_b.poc);
+        assert!(
+            (3400.0..=3410.0).contains(&ps_b.poc),
+            "poc {} not in B",
+            ps_b.poc
+        );
         assert_ne!(ps_a.poc, ps_b.poc, "PS was frozen across a session close");
 
         // Thu 18:00 close passes -> PS rolls to session C.
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:680:
         assert!(e.refresh_on_session_close(ny(2025, 6, 12, 18, 1)));
         let ps_c = e.ps_levels.clone().expect("PS for session C");
         assert_eq!(ps_c.start, ny(2025, 6, 11, 18, 0));
-        assert!((3500.0..=3510.0).contains(&ps_c.poc), "poc {} not in C", ps_c.poc);
+        assert!(
+            (3500.0..=3510.0).contains(&ps_c.poc),
+            "poc {} not in C",
+            ps_c.poc
+        );
     }
 
     #[test]
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:687:
     fn ps_skips_the_weekend_gap() {
         let mut e = VolumeProfileEngine::new();
         // Friday session: Thu 18:00 -> Fri 18:00 (market closes Fri 18:00 NY).
-        fill(&mut e, ny(2025, 6, 12, 18, 0), ny(2025, 6, 13, 18, 0), 3350.0, 3360.0);
+        fill(
+            &mut e,
+            ny(2025, 6, 12, 18, 0),
+            ny(2025, 6, 13, 18, 0),
+            3350.0,
+            3360.0,
+        );
 
         // Saturday noon: the "last closed session" window (Fri 18:00 ->
         // Sat 18:00) has no data, so PS must fall back to the Friday session
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:702:
     #[test]
     fn refresh_fires_once_per_close_and_ignores_empty_weekend_sessions() {
         let mut e = VolumeProfileEngine::new();
-        fill(&mut e, ny(2025, 6, 12, 18, 0), ny(2025, 6, 13, 18, 0), 3350.0, 3360.0);
+        fill(
+            &mut e,
+            ny(2025, 6, 12, 18, 0),
+            ny(2025, 6, 13, 18, 0),
+            3350.0,
+            3360.0,
+        );
         // Saturday noon: PS is the Friday session.
         e.recompute(ny(2025, 6, 14, 12, 0));
         assert_eq!(
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:726:
     fn cw_appears_once_monday_closes_and_freezes_intraday() {
         let mut e = VolumeProfileEngine::new();
         // Monday session: Sun 18:00 -> Mon 18:00.
-        fill(&mut e, ny(2025, 6, 8, 18, 0), ny(2025, 6, 9, 18, 0), 3300.0, 3310.0);
+        fill(
+            &mut e,
+            ny(2025, 6, 8, 18, 0),
+            ny(2025, 6, 9, 18, 0),
+            3300.0,
+            3310.0,
+        );
         // Tuesday session, still in progress.
-        fill(&mut e, ny(2025, 6, 9, 18, 0), ny(2025, 6, 10, 12, 0), 3400.0, 3410.0);
+        fill(
+            &mut e,
+            ny(2025, 6, 9, 18, 0),
+            ny(2025, 6, 10, 12, 0),
+            3400.0,
+            3410.0,
+        );
 
         // Monday 12:00: nothing has closed this week yet -> no CW.
         e.recompute(ny(2025, 6, 9, 12, 0));
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:752:
         // Tuesday 12:00: Tuesday still open -> CW frozen on Monday's snapshot.
         e.recompute(ny(2025, 6, 10, 12, 0));
         let frozen = e.cw_levels.clone().expect("CW stays through Tuesday");
-        assert_eq!((frozen.start, frozen.end, frozen.poc), (cw.start, cw.end, cw.poc));
+        assert_eq!(
+            (frozen.start, frozen.end, frozen.poc),
+            (cw.start, cw.end, cw.poc)
+        );
     }
 
     #[test]
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:772:
         let mut e = VolumeProfileEngine::new();
         let now = ny(2025, 6, 11, 12, 0);
         let week_start = VolumeProfileEngine::most_recent_week_start_utc(now);
-        fill(&mut e, week_start - 6 * 24 * H, week_start - 24 * H, 3100.0, 3110.0);
+        fill(
+            &mut e,
+            week_start - 6 * 24 * H,
+            week_start - 24 * H,
+            3100.0,
+            3110.0,
+        );
         fill(&mut e, week_start + H, now, 3200.0, 3210.0);
         e.recompute(now);
         let pw = e.pw_levels.clone().expect("PW");
Diff in /home/runner/work/Bot/Bot/src/volume_profile.rs:893:
         );
         // A range reaching years back has no retained bars to profile.
         let err = e
-            .audit_custom_range(
-                base - 400 * 24 * 60 * MIN,
-                base - 399 * 24 * 60 * MIN,
-                None,
-            )
+            .audit_custom_range(base - 400 * 24 * 60 * MIN, base - 399 * 24 * 60 * MIN, None)
             .unwrap_err();
         assert_eq!(err, CustomRangeError::NotRetained);
         assert_eq!(e.retained_bounds(), Some((base, base + 105 * MIN)));
Diff in /home/runner/work/Bot/Bot/src/ws_server.rs:1:
-use std::sync::Arc;
-use std::time::Duration;
 use axum::{
+    Json, Router,
     extract::{
-        ws::{Message, WebSocket, WebSocketUpgrade},
         Query, State,
+        ws::{Message, WebSocket, WebSocketUpgrade},
     },
-    http::{header, HeaderValue, Method},
+    http::{HeaderValue, Method, header},
     response::{IntoResponse, Response},
     routing::get,
-    Json, Router,
 };
 use dashmap::DashMap;
 use futures_util::{SinkExt, StreamExt};
Diff in /home/runner/work/Bot/Bot/src/ws_server.rs:15:
-use tokio::sync::{broadcast, RwLock};
-use tracing::info;
+use std::sync::Arc;
+use std::time::Duration;
+use tokio::sync::{RwLock, broadcast};
 use tower_http::cors::{AllowOrigin, CorsLayer};
+use tracing::info;
 
 use crate::ai_cache::AiCache;
 use crate::config::Config;
Diff in /home/runner/work/Bot/Bot/src/ws_server.rs:21:
 use crate::status::FeedStatus;
 use crate::tick_volume::TickVolumeStore;
-use crate::types::{TickVolumeBar, VpLevels, VpCandle, WsFrame};
+use crate::types::{TickVolumeBar, VpCandle, VpLevels, WsFrame};
 use crate::volume_profile::{CustomRangeError, VolumeProfileEngine};
 
 /// Fourteen ATR true ranges need fifteen OHLC bars (the first close is the
Diff in /home/runner/work/Bot/Bot/src/ws_server.rs:150:
     // Hand-selected range: the chart already knows which range it drew, so
     // profile exactly that instead of one of the engine's own windows.
     if params.contains_key("start") || params.contains_key("end") {
-        let (start, end) = match (parse_epoch_ms(&params, "start"), parse_epoch_ms(&params, "end"))
-        {
+        let (start, end) = match (
+            parse_epoch_ms(&params, "start"),
+            parse_epoch_ms(&params, "end"),
+        ) {
             (Some(start), Some(end)) => (start, end),
             _ => {
                 return (
Diff in /home/runner/work/Bot/Bot/src/ws_server.rs:328:
             "audit": "/vp?window=PW",
         },
     });
-    (
-        [(header::CACHE_CONTROL, "no-store")],
-        Json(body),
-    )
-        .into_response()
+    ([(header::CACHE_CONTROL, "no-store")], Json(body)).into_response()
 }
 
-async fn ws_handler(
-    ws: WebSocketUpgrade,
-    State(state): State<AppState>,
-) -> impl IntoResponse {
+async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
     ws.on_upgrade(|socket| handle_socket(socket, state))
 }
 
Diff in /home/runner/work/Bot/Bot/src/ws_server.rs:623:
             let res = send("GET", uri, Some(ALLOWED)).await;
             assert_eq!(res.status(), StatusCode::OK, "{uri}");
             assert_eq!(acao(&res).as_deref(), Some(ALLOWED), "{uri}");
-            assert_ne!(acao(&res).as_deref(), Some("*"), "{uri} must not use a wildcard");
+            assert_ne!(
+                acao(&res).as_deref(),
+                Some("*"),
+                "{uri} must not use a wildcard"
+            );
 
             let vary = res
                 .headers()
Diff in /home/runner/work/Bot/Bot/src/ws_server.rs:644:
             // browser on any other page cannot read the response.
             let res = send("GET", uri, Some(FOREIGN)).await;
             assert_eq!(res.status(), StatusCode::OK, "{uri}");
-            assert_eq!(acao(&res), None, "{uri} leaked an allow-header to a foreign origin");
+            assert_eq!(
+                acao(&res),
+                None,
+                "{uri} leaked an allow-header to a foreign origin"
+            );
 
             let res = send("GET", uri, None).await;
             assert_eq!(res.status(), StatusCode::OK, "{uri}");
Diff in /home/runner/work/Bot/Bot/src/ws_server.rs:658:
         // off — with them a wildcard would expose the API to any site.
         let res = send("GET", "/levels", Some(ALLOWED)).await;
         assert!(
-            !res.headers().contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
+            !res.headers()
+                .contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
             "credentials must not be enabled"
         );
 
Diff in /home/runner/work/Bot/Bot/src/ws_server.rs:678:
             .await
             .unwrap();
         assert_eq!(acao(&res).as_deref(), Some(dev));
-        let res = app(dev).oneshot(build("GET", "/levels", Some(ALLOWED))).await.unwrap();
+        let res = app(dev)
+            .oneshot(build("GET", "/levels", Some(ALLOWED)))
+            .await
+            .unwrap();
         assert_eq!(acao(&res), None);
     }
 
Diff in /home/runner/work/Bot/Bot/src/ws_server.rs:702:
             .unwrap()
             .to_str()
             .unwrap();
-        assert!(methods.contains("GET") && methods.contains("OPTIONS"), "{methods}");
         assert!(
+            methods.contains("GET") && methods.contains("OPTIONS"),
+            "{methods}"
+        );
+        assert!(
             res.headers()
                 .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
                 .unwrap()
Diff in /home/runner/work/Bot/Bot/src/ws_server.rs:719:
                 .unwrap(),
             "600"
         );
-        assert!(!res.headers().contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS));
+        assert!(
+            !res.headers()
+                .contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
+        );
 
         let req = Request::builder()
             .method("OPTIONS")
Diff in /home/runner/work/Bot/Bot/src/ws_server.rs:794:
         keys.sort();
         assert_eq!(
             keys,
-            vec![
-                "close", "high", "low", "open", "source", "time", "volume"
-            ]
+            vec!["close", "high", "low", "open", "source", "time", "volume"]
         );
     }
 
Diff in /home/runner/work/Bot/Bot/src/ws_server.rs:805:
         let res = send("GET", "/tick-volume", Some(ALLOWED)).await;
         assert_eq!(res.status(), StatusCode::OK);
         assert_eq!(
-            res.headers().get(header::CACHE_CONTROL).unwrap().to_str().unwrap(),
+            res.headers()
+                .get(header::CACHE_CONTROL)
+                .unwrap()
+                .to_str()
+                .unwrap(),
             "no-store"
         );
         let body = axum::body::to_bytes(res.into_body(), usize::MAX)

## rustfmt patch
diff --git a/src/ai_cache.rs b/src/ai_cache.rs
index 4277a50..dac75f4 100644
--- a/src/ai_cache.rs
+++ b/src/ai_cache.rs
@@ -81,11 +81,7 @@ impl AiCache {
 
     fn prune(&self, now_ms: i64) {
         let cutoff = now_ms - RETAIN_MS;
-        for buf in [
-            &self.transcripts,
-            &self.sentiments,
-            &self.predictions,
-        ] {
+        for buf in [&self.transcripts, &self.sentiments, &self.predictions] {
             let mut buf = buf.lock().unwrap();
             while let Some(front) = buf.items.front() {
                 let ts = front.get("ts").and_then(|t| t.as_i64()).unwrap_or(0);
diff --git a/src/calendar.rs b/src/calendar.rs
index 4f7e134..435ca6b 100644
--- a/src/calendar.rs
+++ b/src/calendar.rs
@@ -11,7 +11,7 @@
 //! browser), normalizes it, and keeps the MCP browser as an optional
 //! fallback for when the feed is rate-limited.
 
-use anyhow::{bail, Context, Result};
+use anyhow::{Context, Result, bail};
 use chrono::{DateTime, Utc};
 use serde::{Deserialize, Serialize};
 use tracing::{info, warn};
@@ -142,7 +142,9 @@ pub fn parse_calendar_markdown(md: &str) -> Vec<CalendarEvent> {
         // Header row, markdown separator row, or empty event name.
         if cols[0].eq_ignore_ascii_case("time")
             || cols[0].starts_with(':')
-            || cols[0].chars().all(|c| c == '-' || c == ':' || c.is_whitespace())
+            || cols[0]
+                .chars()
+                .all(|c| c == '-' || c == ':' || c.is_whitespace())
             || cols[3].is_empty()
         {
             continue;
@@ -154,7 +156,11 @@ pub fn parse_calendar_markdown(md: &str) -> Vec<CalendarEvent> {
             event: cols[3].to_string(),
             currency,
             impact,
-            time: if cols[0].is_empty() { None } else { Some(cols[0].to_string()) },
+            time: if cols[0].is_empty() {
+                None
+            } else {
+                Some(cols[0].to_string())
+            },
             timestamp: None,
             actual: None,
             forecast: None,
@@ -285,7 +291,10 @@ mod tests {
         // 08:30 EDT == 12:30 UTC
         assert_eq!(cpi.time.as_deref(), Some("2026-09-24T12:30:00+00:00"));
         assert!(cpi.timestamp.is_some());
-        assert!(cpi.gold_relevant, "high-impact USD must flag as gold-relevant");
+        assert!(
+            cpi.gold_relevant,
+            "high-impact USD must flag as gold-relevant"
+        );
 
         assert!(!events[0].gold_relevant);
         assert!(!events[1].gold_relevant, "medium GBP is not gold-relevant");
@@ -373,7 +382,10 @@ mod live_tests {
         for e in &events {
             assert!(!e.event.is_empty(), "empty event title");
             assert!(
-                matches!(e.impact.as_str(), "High" | "Medium" | "Low" | "Holiday" | "Unknown"),
+                matches!(
+                    e.impact.as_str(),
+                    "High" | "Medium" | "Low" | "Holiday" | "Unknown"
+                ),
                 "unnormalized impact: {}",
                 e.impact
             );
diff --git a/src/config.rs b/src/config.rs
index fdf4a60..ce4cc49 100644
--- a/src/config.rs
+++ b/src/config.rs
@@ -133,7 +133,10 @@ fn env_opt(key: &str) -> Option<String> {
 }
 
 fn env_f64(key: &str, default: f64) -> f64 {
-    env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
+    env::var(key)
+        .ok()
+        .and_then(|v| v.parse().ok())
+        .unwrap_or(default)
 }
 
 fn env_bool(key: &str, default: bool) -> bool {
@@ -147,11 +150,17 @@ fn env_bool(key: &str, default: bool) -> bool {
 }
 
 fn env_u64(key: &str, default: u64) -> u64 {
-    env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
+    env::var(key)
+        .ok()
+        .and_then(|v| v.parse().ok())
+        .unwrap_or(default)
 }
 
 fn env_i64(key: &str, default: i64) -> i64 {
-    env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
+    env::var(key)
+        .ok()
+        .and_then(|v| v.parse().ok())
+        .unwrap_or(default)
 }
 
 /// Comma-separated list env, trimmed and stripped of empties.
@@ -167,7 +176,11 @@ fn env_list(key: &str, default: &str) -> Vec<String> {
 /// tri-state feed switch: FEED_X=on|off|auto (auto = on only when its
 /// credentials exist, or always-on for keyless public feeds).
 fn env_switch(key: &str, has_credentials: bool) -> bool {
-    match env::var(key).unwrap_or_default().to_ascii_lowercase().as_str() {
+    match env::var(key)
+        .unwrap_or_default()
+        .to_ascii_lowercase()
+        .as_str()
+    {
         "1" | "true" | "on" | "yes" => true,
         "0" | "false" | "off" | "no" => false,
         _ => has_credentials, // auto
@@ -184,7 +197,10 @@ impl Config {
         let binance_symbol = env_str("BINANCE_SYMBOL", "XAUUSDT").to_lowercase();
 
         Self {
-            port: env::var("PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(3000),
+            port: env::var("PORT")
+                .ok()
+                .and_then(|v| v.parse().ok())
+                .unwrap_or(3000),
 
             binance_ws_url: env::var("BINANCE_WS_URL").unwrap_or_else(|_| {
                 format!("wss://fstream.binance.com/ws/{binance_symbol}@aggTrade")
@@ -295,6 +311,9 @@ mod tests {
             print!("CORS_ALLOWED_ORIGIN is set in the environment; skipping");
             return;
         }
-        assert_eq!(Config::from_env().cors_allowed_origin, DEFAULT_CORS_ALLOWED_ORIGIN);
+        assert_eq!(
+            Config::from_env().cors_allowed_origin,
+            DEFAULT_CORS_ALLOWED_ORIGIN
+        );
     }
 }
diff --git a/src/econ_monitor.rs b/src/econ_monitor.rs
index 97a9352..150bc78 100644
--- a/src/econ_monitor.rs
+++ b/src/econ_monitor.rs
@@ -29,7 +29,7 @@ use std::collections::HashSet;
 use std::process::Stdio;
 use std::sync::Arc;
 use tokio::io::AsyncReadExt;
-use tokio::sync::{broadcast, RwLock};
+use tokio::sync::{RwLock, broadcast};
 use tracing::{info, warn};
 
 const SAMPLE_RATE: usize = 16_000;
@@ -264,7 +264,14 @@ async fn resolve_media(url: &str, cfg: &Config) -> Option<String> {
     let out = tokio::time::timeout(
         std::time::Duration::from_secs(45),
         tokio::process::Command::new(&cfg.econ_ytdlp)
-            .args(["-g", "-f", "bestaudio/best", "--no-playlist", "--no-warnings", url])
+            .args([
+                "-g",
+                "-f",
+                "bestaudio/best",
+                "--no-playlist",
+                "--no-warnings",
+                url,
+            ])
             .stdin(Stdio::null())
             .output(),
     )
@@ -272,7 +279,10 @@ async fn resolve_media(url: &str, cfg: &Config) -> Option<String> {
     let out = match out {
         Ok(Ok(out)) => out,
         Ok(Err(e)) => {
-            warn!("econ monitor: could not run {} ({e}) — install yt-dlp or use direct media URLs in ECON_STREAM_SOURCES", cfg.econ_ytdlp);
+            warn!(
+                "econ monitor: could not run {} ({e}) — install yt-dlp or use direct media URLs in ECON_STREAM_SOURCES",
+                cfg.econ_ytdlp
+            );
             return None;
         }
         Err(_) => {
@@ -469,9 +479,7 @@ pub async fn run_econ_monitor(
         // channel + live coverage links discovered by browsing the event.
         let mut candidates: Vec<String> = cfg.econ_stream_sources.clone();
         for ev in &active {
-            if fed_related(&ev.event)
-                && !candidates.iter().any(|c| c.contains("federalreserve"))
-            {
+            if fed_related(&ev.event) && !candidates.iter().any(|c| c.contains("federalreserve")) {
                 candidates.push("https://www.youtube.com/@federalreserve/live".to_string());
             }
         }
@@ -519,10 +527,10 @@ mod tests {
     fn window_selects_only_active_high_impact_usd_events() {
         let now = 1_700_000_000_000i64;
         let events = vec![
-            ev("Nonfarm Payrolls", "USD", "High", now - 60_000),           // active
-            ev("Core CPI m/m", "USD", "High", now + 10 * 60_000),          // inside pre-window
-            ev("German Factory Orders", "EUR", "High", now),               // wrong currency
-            ev("ADP Non-Farm Employment", "USD", "Medium", now),           // below min impact
+            ev("Nonfarm Payrolls", "USD", "High", now - 60_000), // active
+            ev("Core CPI m/m", "USD", "High", now + 10 * 60_000), // inside pre-window
+            ev("German Factory Orders", "EUR", "High", now),     // wrong currency
+            ev("ADP Non-Farm Employment", "USD", "Medium", now), // below min impact
             ev("FOMC Press Conference", "USD", "High", now - 2 * 3600_000), // after window
         ];
         let active = active_events(&events, now, 900, 3600, 2, &["USD".to_string()]);
@@ -548,7 +556,10 @@ mod tests {
         "#;
         let urls = extract_live_urls(text, 8);
         assert_eq!(urls[0], "https://www.youtube.com/@federalreserve/live");
-        assert!(urls.iter().any(|u| u.starts_with("https://www.youtube.com/watch?v=abc123XYZ_-")));
+        assert!(
+            urls.iter()
+                .any(|u| u.starts_with("https://www.youtube.com/watch?v=abc123XYZ_-"))
+        );
         assert!(!urls.iter().any(|u| u.contains("example.com")));
     }
 
diff --git a/src/main.rs b/src/main.rs
index 43127dc..787e6b8 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,22 +1,22 @@
 mod ai_cache;
+mod binance_ws;
 mod calendar;
 mod config;
-mod status;
-mod types;
-mod binance_ws;
-mod sifting_rest;
-mod multi_exchange;
-mod volume_profile;
-mod order_flow;
 mod econ_monitor;
 mod mcp_client;
+mod multi_exchange;
+mod order_flow;
+mod sifting_rest;
 mod sifting_ws;
+mod status;
 mod tick_volume;
+mod types;
+mod volume_profile;
 mod ws_server;
 
 use std::collections::BTreeMap;
 use std::sync::Arc;
-use tokio::sync::{broadcast, mpsc, RwLock};
+use tokio::sync::{RwLock, broadcast, mpsc};
 use tracing::{info, warn};
 use tracing_subscriber::EnvFilter;
 
@@ -90,7 +90,11 @@ async fn main() -> anyhow::Result<()> {
         .init();
 
     let config = Config::from_env();
-    info!("Starting XAUUSD engine v{} on port {}", env!("CARGO_PKG_VERSION"), config.port);
+    info!(
+        "Starting XAUUSD engine v{} on port {}",
+        env!("CARGO_PKG_VERSION"),
+        config.port
+    );
 
     let (bc_tx, _) = broadcast::channel(4096);
     let (tick_tx, mut tick_rx) = mpsc::channel(8192);
@@ -292,9 +296,9 @@ async fn main() -> anyhow::Result<()> {
                 *cached_levels.write().await = levels;
                 seeded = true;
             }
-            Err(e) => warn!(
-                "SiftingIO REST history fetch failed; refusing Binance price fallback: {e}"
-            ),
+            Err(e) => {
+                warn!("SiftingIO REST history fetch failed; refusing Binance price fallback: {e}")
+            }
         }
     } else {
         warn!("No SIFTING_API_KEY set — historical VP seed is unavailable");
diff --git a/src/mcp_client.rs b/src/mcp_client.rs
index 09f7539..08e6c2b 100644
--- a/src/mcp_client.rs
+++ b/src/mcp_client.rs
@@ -1,7 +1,7 @@
-use anyhow::{anyhow, bail, Context, Result};
-use serde_json::{json, Value};
-use std::sync::atomic::{AtomicU64, Ordering};
+use anyhow::{Context, Result, anyhow, bail};
+use serde_json::{Value, json};
 use std::sync::Mutex;
+use std::sync::atomic::{AtomicU64, Ordering};
 use tracing::{info, warn};
 
 use crate::status::FeedStatus;
@@ -174,10 +174,7 @@ impl McpClient {
             bail!("MCP tools/list error: {err}");
         }
         let mut names = Vec::new();
-        if let Some(tools) = resp
-            .pointer("/result/tools")
-            .and_then(|t| t.as_array())
-        {
+        if let Some(tools) = resp.pointer("/result/tools").and_then(|t| t.as_array()) {
             for t in tools {
                 if let Some(name) = t.get("name").and_then(|n| n.as_str()) {
                     names.push(name.to_string());
@@ -241,7 +238,10 @@ impl McpClient {
     /// the page content back through the MCP browser server.
     pub async fn scrape_calendar(&self, status: &FeedStatus) -> Result<String> {
         status.set("mcp_browser", "connecting");
-        match self.browse_page("https://www.forexfactory.com/calendar").await {
+        match self
+            .browse_page("https://www.forexfactory.com/calendar")
+            .await
+        {
             Ok(text) => {
                 status.set("mcp_browser", "connected");
                 Ok(text)
diff --git a/src/multi_exchange.rs b/src/multi_exchange.rs
index 6cce745..d6de891 100644
--- a/src/multi_exchange.rs
+++ b/src/multi_exchange.rs
@@ -1,11 +1,11 @@
 use anyhow::Result;
 use futures_util::{SinkExt, StreamExt};
-use serde_json::{json, Value};
+use serde_json::{Value, json};
 use tokio::sync::mpsc;
 use tokio_tungstenite::connect_async;
+use tokio_tungstenite::tungstenite::Message;
 use tokio_tungstenite::tungstenite::client::IntoClientRequest;
 use tokio_tungstenite::tungstenite::http;
-use tokio_tungstenite::tungstenite::Message;
 use tracing::{error, info, warn};
 
 use crate::status::FeedStatus;
@@ -43,7 +43,11 @@ pub async fn run_bybit_stream(tx: mpsc::Sender<AggTrade>, status: FeedStatus) ->
                     "op": "subscribe",
                     "args": ["publicTrade.XAUUSDT"]
                 });
-                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
+                if write
+                    .send(Message::Text(sub.to_string().into()))
+                    .await
+                    .is_err()
+                {
                     warn!("Bybit subscribe send failed");
                     status.set("bybit", "error");
                     continue;
@@ -83,8 +87,13 @@ pub async fn run_bybit_stream(tx: mpsc::Sender<AggTrade>, status: FeedStatus) ->
                                     let qty = t["v"].as_str().unwrap_or("0");
                                     let ts = t["T"].as_i64().unwrap_or(0);
                                     let agg = AggTrade::new(
-                                        "bybit", "XAUUSDT", price, qty, ts,
-                                        side == "Buy", true,
+                                        "bybit",
+                                        "XAUUSDT",
+                                        price,
+                                        qty,
+                                        ts,
+                                        side == "Buy",
+                                        true,
                                     );
                                     let _ = tx.send(agg).await;
                                 }
@@ -127,7 +136,11 @@ pub async fn run_okx_stream(tx: mpsc::Sender<AggTrade>, status: FeedStatus) -> R
                     "op": "subscribe",
                     "args": [{"channel": "trades", "instId": "XAU-USDT-SWAP"}]
                 });
-                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
+                if write
+                    .send(Message::Text(sub.to_string().into()))
+                    .await
+                    .is_err()
+                {
                     warn!("OKX subscribe send failed");
                     status.set("okx", "error");
                     continue;
@@ -141,7 +154,11 @@ pub async fn run_okx_stream(tx: mpsc::Sender<AggTrade>, status: FeedStatus) -> R
                     loop {
                         iv.tick().await;
                         let ping = json!({"op": "ping"});
-                        if write.send(Message::Text(ping.to_string().into())).await.is_err() {
+                        if write
+                            .send(Message::Text(ping.to_string().into()))
+                            .await
+                            .is_err()
+                        {
                             break;
                         }
                     }
@@ -169,8 +186,13 @@ pub async fn run_okx_stream(tx: mpsc::Sender<AggTrade>, status: FeedStatus) -> R
                                         .and_then(|s| s.parse::<i64>().ok())
                                         .unwrap_or(0);
                                     let agg = AggTrade::new(
-                                        "okx", "XAUUSDT", px, sz, ts,
-                                        side == "buy", true,
+                                        "okx",
+                                        "XAUUSDT",
+                                        px,
+                                        sz,
+                                        ts,
+                                        side == "buy",
+                                        true,
                                     );
                                     let _ = tx.send(agg).await;
                                 }
@@ -201,7 +223,11 @@ pub async fn run_okx_stream(tx: mpsc::Sender<AggTrade>, status: FeedStatus) -> R
 // Push rows are either arrays [price,size,side,ts,seq] or objects.
 // =====================================================================
 
-pub async fn run_bitget_stream(tx: mpsc::Sender<AggTrade>, symbol: String, status: FeedStatus) -> Result<()> {
+pub async fn run_bitget_stream(
+    tx: mpsc::Sender<AggTrade>,
+    symbol: String,
+    status: FeedStatus,
+) -> Result<()> {
     let url = "wss://ws.bitget.com/v2/ws/public";
     loop {
         status.set("bitget", "connecting");
@@ -216,7 +242,11 @@ pub async fn run_bitget_stream(tx: mpsc::Sender<AggTrade>, symbol: String, statu
                         {"instType": "USDT-FUTURES", "channel": "publicTrade", "instId": symbol},
                     ]
                 });
-                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
+                if write
+                    .send(Message::Text(sub.to_string().into()))
+                    .await
+                    .is_err()
+                {
                     warn!("Bitget subscribe send failed");
                     status.set("bitget", "error");
                     continue;
@@ -249,7 +279,10 @@ pub async fn run_bitget_stream(tx: mpsc::Sender<AggTrade>, symbol: String, statu
                             };
                             if let Some(err) = v.get("code").and_then(|c| c.as_str()) {
                                 if err != "0" {
-                                    warn!("Bitget error frame: {}", v["msg"].as_str().unwrap_or("?"));
+                                    warn!(
+                                        "Bitget error frame: {}",
+                                        v["msg"].as_str().unwrap_or("?")
+                                    );
                                 }
                                 continue;
                             }
@@ -265,7 +298,10 @@ pub async fn run_bitget_stream(tx: mpsc::Sender<AggTrade>, symbol: String, statu
                                         (
                                             t.get(0).and_then(num_str),
                                             t.get(1).and_then(num_str),
-                                            t.get(3).and_then(|x| x.as_str().and_then(|s| s.parse().ok()))
+                                            t.get(3)
+                                                .and_then(|x| {
+                                                    x.as_str().and_then(|s| s.parse().ok())
+                                                })
                                                 .or_else(|| t.get(3).and_then(|x| x.as_i64())),
                                             t.get(2).and_then(|s| s.as_str()) == Some("buy"),
                                             true,
@@ -275,17 +311,24 @@ pub async fn run_bitget_stream(tx: mpsc::Sender<AggTrade>, symbol: String, statu
                                         (
                                             t["price"].as_str().map(String::from),
                                             t["size"].as_str().map(String::from),
-                                            t["ts"].as_str().and_then(|s| s.parse().ok())
+                                            t["ts"]
+                                                .as_str()
+                                                .and_then(|s| s.parse().ok())
                                                 .or_else(|| t["ts"].as_i64()),
                                             t["side"].as_str() == Some("buy"),
                                             true,
                                         )
                                     };
                                     if let (Some(price), Some(qty)) = (price, qty) {
-                                        let ts = ts.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
-                                        let _ = tx.send(AggTrade::new(
-                                            "bitget", &symbol, &price, &qty, ts, taker_buy, has_side,
-                                        )).await;
+                                        let ts = ts.unwrap_or_else(|| {
+                                            chrono::Utc::now().timestamp_millis()
+                                        });
+                                        let _ = tx
+                                            .send(AggTrade::new(
+                                                "bitget", &symbol, &price, &qty, ts, taker_buy,
+                                                has_side,
+                                            ))
+                                            .await;
                                     }
                                 }
                             }
@@ -302,7 +345,10 @@ pub async fn run_bitget_stream(tx: mpsc::Sender<AggTrade>, symbol: String, statu
                 status.set("bitget", "disconnected");
             }
             Err(e) => {
-                warn!("Bitget connect failed: {}. Retrying in {RECONNECT_SECS}s", e);
+                warn!(
+                    "Bitget connect failed: {}. Retrying in {RECONNECT_SECS}s",
+                    e
+                );
                 status.set("bitget", "error");
             }
         }
@@ -315,7 +361,11 @@ pub async fn run_bitget_stream(tx: mpsc::Sender<AggTrade>, symbol: String, statu
 // `size` is signed: >0 buyer-taker, <0 seller-taker.
 // =====================================================================
 
-pub async fn run_gate_stream(tx: mpsc::Sender<AggTrade>, symbol: String, status: FeedStatus) -> Result<()> {
+pub async fn run_gate_stream(
+    tx: mpsc::Sender<AggTrade>,
+    symbol: String,
+    status: FeedStatus,
+) -> Result<()> {
     let url = "wss://fx-ws.gateio.ws/v4/ws/usdt";
     loop {
         status.set("gate", "connecting");
@@ -330,7 +380,11 @@ pub async fn run_gate_stream(tx: mpsc::Sender<AggTrade>, symbol: String, status:
                     "event": "subscribe",
                     "payload": [symbol]
                 });
-                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
+                if write
+                    .send(Message::Text(sub.to_string().into()))
+                    .await
+                    .is_err()
+                {
                     warn!("Gate subscribe send failed");
                     status.set("gate", "error");
                     continue;
@@ -356,14 +410,23 @@ pub async fn run_gate_stream(tx: mpsc::Sender<AggTrade>, symbol: String, status:
                                     if t["contract"].as_str() != Some(symbol.as_str()) {
                                         continue;
                                     }
-                                    let Some(price) = num_str(&t["price"]) else { continue };
+                                    let Some(price) = num_str(&t["price"]) else {
+                                        continue;
+                                    };
                                     let signed_size = t["size"].as_i64().unwrap_or(0);
                                     let qty = signed_size.abs().to_string();
                                     let ts = t["time"].as_i64().unwrap_or(0);
-                                    let _ = tx.send(AggTrade::new(
-                                        "gate", &symbol, &price, &qty, ts,
-                                        signed_size > 0, true,
-                                    )).await;
+                                    let _ = tx
+                                        .send(AggTrade::new(
+                                            "gate",
+                                            &symbol,
+                                            &price,
+                                            &qty,
+                                            ts,
+                                            signed_size > 0,
+                                            true,
+                                        ))
+                                        .await;
                                 }
                             }
                         }
@@ -391,7 +454,11 @@ pub async fn run_gate_stream(tx: mpsc::Sender<AggTrade>, symbol: String, status:
 // Frames: {"feed":"trade", product_id, side, qty, price, time(ms), ...}
 // =====================================================================
 
-pub async fn run_kraken_stream(tx: mpsc::Sender<AggTrade>, product: String, status: FeedStatus) -> Result<()> {
+pub async fn run_kraken_stream(
+    tx: mpsc::Sender<AggTrade>,
+    product: String,
+    status: FeedStatus,
+) -> Result<()> {
     let url = "wss://futures.kraken.com/ws/v1";
     loop {
         status.set("kraken", "connecting");
@@ -405,7 +472,11 @@ pub async fn run_kraken_stream(tx: mpsc::Sender<AggTrade>, product: String, stat
                     "feed": "trade",
                     "product_ids": [product]
                 });
-                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
+                if write
+                    .send(Message::Text(sub.to_string().into()))
+                    .await
+                    .is_err()
+                {
                     warn!("Kraken subscribe send failed");
                     status.set("kraken", "error");
                     continue;
@@ -423,7 +494,10 @@ pub async fn run_kraken_stream(tx: mpsc::Sender<AggTrade>, product: String, stat
                             };
                             if v.get("event").is_some() {
                                 if v["event"].as_str() == Some("error") {
-                                    warn!("Kraken error frame: {}", v["message"].as_str().unwrap_or("?"));
+                                    warn!(
+                                        "Kraken error frame: {}",
+                                        v["message"].as_str().unwrap_or("?")
+                                    );
                                 }
                                 continue;
                             }
@@ -434,9 +508,11 @@ pub async fn run_kraken_stream(tx: mpsc::Sender<AggTrade>, product: String, stat
                                     let qty = num_str(&v["qty"]).unwrap_or_default();
                                     let ts = v["time"].as_i64().unwrap_or(0);
                                     let taker_buy = v["side"].as_str() == Some("buy");
-                                    let _ = tx.send(AggTrade::new(
-                                        "kraken", &product, &price, &qty, ts, taker_buy, true,
-                                    )).await;
+                                    let _ = tx
+                                        .send(AggTrade::new(
+                                            "kraken", &product, &price, &qty, ts, taker_buy, true,
+                                        ))
+                                        .await;
                                 }
                                 "trade_snapshot" => {
                                     // history replay — skip
@@ -455,7 +531,10 @@ pub async fn run_kraken_stream(tx: mpsc::Sender<AggTrade>, product: String, stat
                 status.set("kraken", "disconnected");
             }
             Err(e) => {
-                warn!("Kraken connect failed: {}. Retrying in {RECONNECT_SECS}s", e);
+                warn!(
+                    "Kraken connect failed: {}. Retrying in {RECONNECT_SECS}s",
+                    e
+                );
                 status.set("kraken", "error");
             }
         }
@@ -469,7 +548,12 @@ pub async fn run_kraken_stream(tx: mpsc::Sender<AggTrade>, product: String, stat
 // trade push cmd_id 22001-22003 with price/volume/trade_direction.
 // =====================================================================
 
-pub async fn run_alltick_stream(tx: mpsc::Sender<AggTrade>, url: String, code: String, status: FeedStatus) -> Result<()> {
+pub async fn run_alltick_stream(
+    tx: mpsc::Sender<AggTrade>,
+    url: String,
+    code: String,
+    status: FeedStatus,
+) -> Result<()> {
     let token = std::env::var("ALLTICK_TOKEN").unwrap_or_default();
     let full_url = format!("{url}?token={token}");
 
@@ -483,7 +567,11 @@ pub async fn run_alltick_stream(tx: mpsc::Sender<AggTrade>, url: String, code: S
                     "cmd_id": 22004, "seq_id": 1, "trace": "rust-xauusd-engine",
                     "data": { "symbol_list": [{ "code": code }] }
                 });
-                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
+                if write
+                    .send(Message::Text(sub.to_string().into()))
+                    .await
+                    .is_err()
+                {
                     warn!("AllTick subscribe send failed");
                     status.set("alltick", "error");
                     continue;
@@ -497,7 +585,11 @@ pub async fn run_alltick_stream(tx: mpsc::Sender<AggTrade>, url: String, code: S
                     loop {
                         iv.tick().await;
                         let hb = json!({"cmd_id": 22000, "seq_id": 2, "trace": "hb", "data": {}});
-                        if write.send(Message::Text(hb.to_string().into())).await.is_err() {
+                        if write
+                            .send(Message::Text(hb.to_string().into()))
+                            .await
+                            .is_err()
+                        {
                             break;
                         }
                     }
@@ -506,7 +598,10 @@ pub async fn run_alltick_stream(tx: mpsc::Sender<AggTrade>, url: String, code: S
                 while let Some(msg) = read.next().await {
                     if let Ok(Message::Text(text)) = msg {
                         status.mark_msg("alltick");
-                        let v: Value = match serde_json::from_str(&text) { Ok(v) => v, _ => continue };
+                        let v: Value = match serde_json::from_str(&text) {
+                            Ok(v) => v,
+                            _ => continue,
+                        };
                         let cmd = v["cmd_id"].as_i64().unwrap_or(0);
                         // 22001 quote / 22002 price / 22003 trade pushes
                         if !(22001..=22003).contains(&cmd) {
@@ -521,21 +616,33 @@ pub async fn run_alltick_stream(tx: mpsc::Sender<AggTrade>, url: String, code: S
                             if d["code"].as_str().is_some_and(|c| c != code) {
                                 continue;
                             }
-                            let Some(price) = num_str(&d["price"]) else { continue };
+                            let Some(price) = num_str(&d["price"]) else {
+                                continue;
+                            };
                             let vol = num_str(&d["volume"]).unwrap_or_else(|| "0".into());
                             let ts = d["tick_time"].as_i64().unwrap_or(0);
                             // trade_direction: 1 aggressive buy, 2 aggressive sell.
                             // Price/quote pushes carry no direction → no flow side.
                             match d["trade_direction"].as_i64() {
                                 Some(dir @ (1 | 2)) => {
-                                    let _ = tx.send(AggTrade::new(
-                                        "alltick", &code, &price, &vol, ts, dir == 1, true,
-                                    )).await;
+                                    let _ = tx
+                                        .send(AggTrade::new(
+                                            "alltick",
+                                            &code,
+                                            &price,
+                                            &vol,
+                                            ts,
+                                            dir == 1,
+                                            true,
+                                        ))
+                                        .await;
                                 }
                                 _ => {
-                                    let _ = tx.send(AggTrade::new(
-                                        "alltick", &code, &price, &vol, ts, true, false,
-                                    )).await;
+                                    let _ = tx
+                                        .send(AggTrade::new(
+                                            "alltick", &code, &price, &vol, ts, true, false,
+                                        ))
+                                        .await;
                                 }
                             }
                         }
@@ -546,7 +653,10 @@ pub async fn run_alltick_stream(tx: mpsc::Sender<AggTrade>, url: String, code: S
                 status.set("alltick", "disconnected");
             }
             Err(e) => {
-                warn!("AllTick connect failed: {}. Retrying in {RECONNECT_SECS}s", e);
+                warn!(
+                    "AllTick connect failed: {}. Retrying in {RECONNECT_SECS}s",
+                    e
+                );
                 status.set("alltick", "error");
             }
         }
@@ -561,7 +671,12 @@ pub async fn run_alltick_stream(tx: mpsc::Sender<AggTrade>, url: String, code: S
 // `d`: 2 = aggressive buy, 1 = aggressive sell, 0/absent = unknown.
 // =====================================================================
 
-pub async fn run_itick_stream(tx: mpsc::Sender<AggTrade>, url: String, symbol: String, status: FeedStatus) -> Result<()> {
+pub async fn run_itick_stream(
+    tx: mpsc::Sender<AggTrade>,
+    url: String,
+    symbol: String,
+    status: FeedStatus,
+) -> Result<()> {
     let token = std::env::var("ITICK_TOKEN").unwrap_or_default();
 
     loop {
@@ -617,7 +732,10 @@ pub async fn run_itick_stream(tx: mpsc::Sender<AggTrade>, url: String, symbol: S
                 while let Some(msg) = read.next().await {
                     if let Ok(Message::Text(text)) = msg {
                         status.mark_msg("itick");
-                        let v: Value = match serde_json::from_str(&text) { Ok(v) => v, _ => continue };
+                        let v: Value = match serde_json::from_str(&text) {
+                            Ok(v) => v,
+                            _ => continue,
+                        };
 
                         // Handshake/ack frames
                         if v.get("resAc").is_some() {
@@ -625,7 +743,8 @@ pub async fn run_itick_stream(tx: mpsc::Sender<AggTrade>, url: String, symbol: S
                             let ok = v["code"].as_i64().unwrap_or(0) == 0
                                 || v["code"].as_i64() == Some(1);
                             if ac == "auth" && ok && !subscribed {
-                                let sub = json!({"ac": "subscribe", "params": format!("{symbol},tick")});
+                                let sub =
+                                    json!({"ac": "subscribe", "params": format!("{symbol},tick")});
                                 if out_tx.send(Message::Text(sub.to_string().into())).is_ok() {
                                     subscribed = true;
                                     info!("iTick subscribed to {symbol} ticks");
@@ -649,20 +768,32 @@ pub async fn run_itick_stream(tx: mpsc::Sender<AggTrade>, url: String, symbol: S
                                 continue;
                             }
                         }
-                        let Some(price) = num_str(&d["ld"]) else { continue };
+                        let Some(price) = num_str(&d["ld"]) else {
+                            continue;
+                        };
                         let vol = num_str(&d["v"]).unwrap_or_else(|| "0".into());
                         let ts = d["t"].as_i64().unwrap_or(0);
                         match d["d"].as_i64() {
                             Some(dir @ (1 | 2)) => {
-                                let _ = tx.send(AggTrade::new(
-                                    "itick", &symbol, &price, &vol, ts, dir == 2, true,
-                                )).await;
+                                let _ = tx
+                                    .send(AggTrade::new(
+                                        "itick",
+                                        &symbol,
+                                        &price,
+                                        &vol,
+                                        ts,
+                                        dir == 2,
+                                        true,
+                                    ))
+                                    .await;
                             }
                             _ => {
                                 // No taker side published — volume only.
-                                let _ = tx.send(AggTrade::new(
-                                    "itick", &symbol, &price, &vol, ts, true, false,
-                                )).await;
+                                let _ = tx
+                                    .send(AggTrade::new(
+                                        "itick", &symbol, &price, &vol, ts, true, false,
+                                    ))
+                                    .await;
                             }
                         }
                     }
diff --git a/src/order_flow.rs b/src/order_flow.rs
index aad9885..11fc349 100644
--- a/src/order_flow.rs
+++ b/src/order_flow.rs
@@ -1,5 +1,5 @@
-use std::collections::{HashMap, VecDeque};
 use chrono::Utc;
+use std::collections::{HashMap, VecDeque};
 
 use crate::types::{AggTrade, OrderflowEvent};
 use crate::volume_profile::VolumeProfileEngine;
@@ -11,7 +11,7 @@ const SLIDING_WINDOW_MS: i64 = 300_000; // 5 minutes
 const MIN_SAMPLES: usize = 50;
 
 // Absolute floors in USD notional.
-const FLOOR_BUBBLE: f64 = 50_000.0;   // $50k net delta over 500 trades
+const FLOOR_BUBBLE: f64 = 50_000.0; // $50k net delta over 500 trades
 const FLOOR_ABSORPTION: f64 = 100_000.0; // $100k total volume over 500 trades
 const ABSORPTION_DELTA_CAP: f64 = 20_000.0; // |net delta| must be < $20k to count as "near zero"
 
@@ -46,48 +46,83 @@ impl OrderFlowAnalyzer {
         let qty = trade.qty_f64();
         // Feeds without a taker side (quote ticks) must not fabricate
         // directional delta — they only count toward total volume.
-        let signed_qty = if trade.has_flow_side { trade.signed_delta() } else { 0.0 };
+        let signed_qty = if trade.has_flow_side {
+            trade.signed_delta()
+        } else {
+            0.0
+        };
 
         let notional_delta = signed_qty * price;
         let notional_volume = qty * price;
-        
+
         self.cumulative_delta += notional_delta;
         self.last_exchange = trade.exchange.clone();
-        self.delta_history.push_back((trade.trade_time, price, notional_delta, notional_volume));
-        
+        self.delta_history
+            .push_back((trade.trade_time, price, notional_delta, notional_volume));
+
         while self.delta_history.len() > self.window_size {
             self.delta_history.pop_front();
         }
 
         if self.delta_history.len() >= LOOKBACK {
-            let net_delta: f64 = self.delta_history.iter().rev().take(LOOKBACK).map(|(_, _, d, _)| *d).sum();
-            let total_vol: f64 = self.delta_history.iter().rev().take(LOOKBACK).map(|(_, _, _, v)| *v).sum();
+            let net_delta: f64 = self
+                .delta_history
+                .iter()
+                .rev()
+                .take(LOOKBACK)
+                .map(|(_, _, d, _)| *d)
+                .sum();
+            let total_vol: f64 = self
+                .delta_history
+                .iter()
+                .rev()
+                .take(LOOKBACK)
+                .map(|(_, _, _, v)| *v)
+                .sum();
             let t = self.delta_history.back().unwrap().0;
 
             self.bubble_mags.push_back((t, net_delta.abs()));
-            
+
             if net_delta.abs() < ABSORPTION_DELTA_CAP {
                 self.abs_vols.push_back((t, total_vol));
             }
 
             let cutoff = t - SLIDING_WINDOW_MS;
             while let Some(&front) = self.bubble_mags.front() {
-                if front.0 < cutoff { self.bubble_mags.pop_front(); } else { break; }
+                if front.0 < cutoff {
+                    self.bubble_mags.pop_front();
+                } else {
+                    break;
+                }
             }
             while let Some(&front) = self.abs_vols.front() {
-                if front.0 < cutoff { self.abs_vols.pop_front(); } else { break; }
+                if front.0 < cutoff {
+                    self.abs_vols.pop_front();
+                } else {
+                    break;
+                }
             }
         }
     }
 
     pub fn recent_delta_notional(&self) -> f64 {
         let n = LOOKBACK.min(self.delta_history.len());
-        self.delta_history.iter().rev().take(n).map(|(_, _, d, _)| *d).sum()
+        self.delta_history
+            .iter()
+            .rev()
+            .take(n)
+            .map(|(_, _, d, _)| *d)
+            .sum()
     }
 
     pub fn recent_total_volume(&self) -> f64 {
         let n = LOOKBACK.min(self.delta_history.len());
-        self.delta_history.iter().rev().take(n).map(|(_, _, _, v)| *v).sum()
+        self.delta_history
+            .iter()
+            .rev()
+            .take(n)
+            .map(|(_, _, _, v)| *v)
+            .sum()
     }
 
     fn price_at(&self, lookback: usize) -> Option<f64> {
@@ -125,7 +160,9 @@ impl OrderFlowAnalyzer {
     }
 
     fn percentile(sorted: &[f64], p: f64) -> f64 {
-        if sorted.is_empty() { return 0.0; }
+        if sorted.is_empty() {
+            return 0.0;
+        }
         let idx = (((sorted.len() as f64) - 1.0) * p).round() as usize;
         sorted[idx.min(sorted.len() - 1)]
     }
@@ -191,13 +228,12 @@ impl OrderFlowAnalyzer {
         if let Some(start_price) = self.price_at(LOOKBACK) {
             let displacement = current_price - start_price;
 
-            if total_vol > absorption_threshold 
-                && net_delta.abs() < ABSORPTION_DELTA_CAP * 2.0 
-                && displacement >= -0.5 
+            if total_vol > absorption_threshold
+                && net_delta.abs() < ABSORPTION_DELTA_CAP * 2.0
+                && displacement >= -0.5
                 && self.should_emit("ABS_BUY", now_ms)
             {
-                let reference = Self::nearest_level(vp, current_price)
-                    .unwrap_or(current_price);
+                let reference = Self::nearest_level(vp, current_price).unwrap_or(current_price);
                 events.push(OrderflowEvent {
                     kind: "ABS_BUY".into(),
                     level: reference,
@@ -207,13 +243,12 @@ impl OrderFlowAnalyzer {
                 });
             }
 
-            if total_vol > absorption_threshold 
+            if total_vol > absorption_threshold
                 && net_delta.abs() < ABSORPTION_DELTA_CAP * 2.0
-                && displacement <= 0.5 
+                && displacement <= 0.5
                 && self.should_emit("ABS_SELL", now_ms)
             {
-                let reference = Self::nearest_level(vp, current_price)
-                    .unwrap_or(current_price);
+                let reference = Self::nearest_level(vp, current_price).unwrap_or(current_price);
                 events.push(OrderflowEvent {
                     kind: "ABS_SELL".into(),
                     level: reference,
@@ -233,4 +268,4 @@ impl OrderFlowAnalyzer {
 
         events
     }
-}
\ No newline at end of file
+}
diff --git a/src/sifting_rest.rs b/src/sifting_rest.rs
index e25e4f4..f557c87 100644
--- a/src/sifting_rest.rs
+++ b/src/sifting_rest.rs
@@ -89,16 +89,12 @@ fn parse_page(
 ) -> Result<(Vec<VpCandle>, Option<String>)> {
     if let Some(response_symbol) = response.meta.symbol.as_deref() {
         if !response_symbol.eq_ignore_ascii_case(symbol) {
-            anyhow::bail!(
-                "SiftingIO returned symbol {response_symbol}, expected {symbol}"
-            );
+            anyhow::bail!("SiftingIO returned symbol {response_symbol}, expected {symbol}");
         }
     }
     if let Some(interval) = response.meta.interval.as_deref() {
         if interval != expected_interval {
-            anyhow::bail!(
-                "SiftingIO returned interval {interval}, expected {expected_interval}"
-            );
+            anyhow::bail!("SiftingIO returned interval {interval}, expected {expected_interval}");
         }
     }
 
@@ -373,9 +369,10 @@ mod tests {
             None,
         )
         .unwrap();
-        assert!(url
-            .as_str()
-            .starts_with("https://api.sifting.io/v1/hist/commodities/XAUUSD/bars?"));
+        assert!(
+            url.as_str()
+                .starts_with("https://api.sifting.io/v1/hist/commodities/XAUUSD/bars?")
+        );
         assert!(url.as_str().contains("interval=15m"));
         assert!(url.as_str().contains("order=desc"));
         assert!(url.as_str().contains("limit=2000"));
@@ -400,7 +397,10 @@ mod tests {
         assert!(url.as_str().contains("interval=1m"));
         assert!(url.as_str().contains("order=asc"));
         assert!(url.as_str().contains("limit=2000"));
-        assert!(url.as_str().contains("cursor=cursor%2Fwith%2Breserved%3Dchars"));
+        assert!(
+            url.as_str()
+                .contains("cursor=cursor%2Fwith%2Breserved%3Dchars")
+        );
     }
 
     #[test]
@@ -433,7 +433,13 @@ mod tests {
         let start = 1_700_000_000_000;
         let week: i64 = 5 * 86_400_000;
         let candles: Vec<VpCandle> = (0..(119 * 60)).map(|i| bar(start + i * 60_000)).collect();
-        assert!(profile_history_covers(&candles, start, start + week, 60_000, 2 * 3_600_000));
+        assert!(profile_history_covers(
+            &candles,
+            start,
+            start + week,
+            60_000,
+            2 * 3_600_000
+        ));
     }
 
     /// ...while a page that only *reaches* the ends but is mostly empty fails.
@@ -446,10 +452,22 @@ mod tests {
         let candles: Vec<VpCandle> = (0..(week / 1_800_000) - 1)
             .map(|i| bar(start + i * 1_800_000))
             .collect();
-        assert!(!profile_history_covers(&candles, start, start + week, 60_000, 2 * 3_600_000));
+        assert!(!profile_history_covers(
+            &candles,
+            start,
+            start + week,
+            60_000,
+            2 * 3_600_000
+        ));
         // Nothing at all, or nothing near the far end, is rejected too.
         assert!(!profile_history_covers(&[], start, start + week, 60_000, 0));
-        assert!(!profile_history_covers(&candles, start, start + 2 * week, 60_000, 0));
+        assert!(!profile_history_covers(
+            &candles,
+            start,
+            start + 2 * week,
+            60_000,
+            0
+        ));
     }
 
     #[test]
diff --git a/src/sifting_ws.rs b/src/sifting_ws.rs
index ce7d4f5..9cd2a27 100644
--- a/src/sifting_ws.rs
+++ b/src/sifting_ws.rs
@@ -470,7 +470,10 @@ mod tests {
         }
         let bar = st.on_tick(100.0, T0 + 10, None).unwrap().live.1;
         // first=flat, +up, =flat, -down, +up, -down
-        assert_eq!((bar.ticks, bar.up_ticks, bar.down_ticks, bar.flat_ticks), (6, 2, 2, 2));
+        assert_eq!(
+            (bar.ticks, bar.up_ticks, bar.down_ticks, bar.flat_ticks),
+            (6, 2, 2, 2)
+        );
     }
 
     #[test]
@@ -488,7 +491,12 @@ mod tests {
     fn bucket_roll_emits_one_closed_bar_then_a_fresh_live_bar() {
         let mut st = TickState::new();
         st.on_tick(100.0, T0, None);
-        assert!(st.on_tick(100.5, T0 + 1_000, None).unwrap().closed.is_none());
+        assert!(
+            st.on_tick(100.5, T0 + 1_000, None)
+                .unwrap()
+                .closed
+                .is_none()
+        );
 
         let out = st.on_tick(101.0, T0 + M15, None).unwrap();
         let (closed_candle, closed_bar) = out.closed.expect("bucket must close");
@@ -550,7 +558,15 @@ mod tests {
             );
         }
         // Control frames and other symbols are ignored.
-        handle_text(r#"{"f":"pong"}"#, "XAUUSD", &mut st, &bc, &ctx, Some(&profile_tx), &store);
+        handle_text(
+            r#"{"f":"pong"}"#,
+            "XAUUSD",
+            &mut st,
+            &bc,
+            &ctx,
+            Some(&profile_tx),
+            &store,
+        );
         handle_text(
             &tick(1.0, T0 + M15 + 1).replace("XAUUSD", "XAGUSD"),
             "XAUUSD",
@@ -572,10 +588,21 @@ mod tests {
         }
         assert_eq!(
             kinds,
-            ["candle", "tv", "candle", "tv", "candle", "tv_closed", "candle", "tv"]
+            [
+                "candle",
+                "tv",
+                "candle",
+                "tv",
+                "candle",
+                "tv_closed",
+                "candle",
+                "tv"
+            ]
         );
 
-        let closed = crx.try_recv().expect("closed candle goes to the chart consumer");
+        let closed = crx
+            .try_recv()
+            .expect("closed candle goes to the chart consumer");
         assert_eq!((closed.time, closed.volume), (T0, 2.0));
         assert!(crx.try_recv().is_err());
 
@@ -602,8 +629,16 @@ mod tests {
         assert_eq!(
             keys,
             [
-                "close", "closed", "down_ticks", "flat_ticks", "last_tick", "source",
-                "ticks", "ticks_per_sec", "time", "up_ticks"
+                "close",
+                "closed",
+                "down_ticks",
+                "flat_ticks",
+                "last_tick",
+                "source",
+                "ticks",
+                "ticks_per_sec",
+                "time",
+                "up_ticks"
             ]
         );
     }
diff --git a/src/tick_volume.rs b/src/tick_volume.rs
index 44caf6a..2f09fc7 100644
--- a/src/tick_volume.rs
+++ b/src/tick_volume.rs
@@ -105,7 +105,10 @@ mod tests {
         s.upsert(bar(0, 1));
         s.upsert(bar(1_800_000, 5));
         let snap = s.snapshot();
-        assert_eq!(snap.iter().map(|b| b.time).collect::<Vec<_>>(), vec![0, 1_800_000]);
+        assert_eq!(
+            snap.iter().map(|b| b.time).collect::<Vec<_>>(),
+            vec![0, 1_800_000]
+        );
         assert_eq!(snap[1].ticks, 5);
     }
 }
diff --git a/src/volume_profile.rs b/src/volume_profile.rs
index ade6928..bc0c9ae 100644
--- a/src/volume_profile.rs
+++ b/src/volume_profile.rs
@@ -39,7 +39,7 @@ use histogram::Histogram;
 // Re-exported so the binary can construct the TradingView-parity model from
 // `Config` without reaching into the submodules.
 pub use histogram::{ProfileModel, RowMode};
-pub use timeframe::{parse_label, LowerTf};
+pub use timeframe::{LowerTf, parse_label};
 
 /// Keep the complete fixed 2,000-candle Sifting 15m swing seed (about 21
 /// days) plus room for live closes and session/week boundaries. The 1m profile
@@ -75,9 +75,7 @@ impl CustomRangeError {
     pub fn message(self) -> &'static str {
         match self {
             Self::Invalid => "start must be before end, and the range at most 35 days",
-            Self::NotRetained => {
-                "the requested range has no retained 1m history to profile"
-            }
+            Self::NotRetained => "the requested range has no retained 1m history to profile",
         }
     }
 }
@@ -197,11 +195,7 @@ impl VolumeProfileEngine {
 
     /// Seed independent profile and swing histories, then compute once. This
     /// avoids caching an empty CW snapshot between two seed operations.
-    pub fn ingest_history(
-        &mut self,
-        profile_candles: Vec<VpCandle>,
-        swing_candles: Vec<VpCandle>,
-    ) {
+    pub fn ingest_history(&mut self, profile_candles: Vec<VpCandle>, swing_candles: Vec<VpCandle>) {
         upsert_candles(&mut self.profile_candles, profile_candles);
         upsert_candles(&mut self.swing_candles, swing_candles);
         self.recompute(Utc::now().timestamp_millis());
@@ -257,10 +251,8 @@ impl VolumeProfileEngine {
         // Keep out-of-order candles in storage; the individual windows apply
         // their own time bounds. This also lets a historical backfill arrive
         // before a caller advances its synthetic/test clock.
-        self.profile_candles
-            .retain(|c| c.time >= retain_floor);
-        self.swing_candles
-            .retain(|c| c.time >= now_ms - RETAIN_MS);
+        self.profile_candles.retain(|c| c.time >= retain_floor);
+        self.swing_candles.retain(|c| c.time >= now_ms - RETAIN_MS);
 
         let pw: Vec<_> = self
             .profile_candles
@@ -458,8 +450,8 @@ impl VolumeProfileEngine {
             },
             None => self.model,
         };
-        let hist = histogram::histogram(&input, None, &model)
-            .ok_or(CustomRangeError::NotRetained)?;
+        let hist =
+            histogram::histogram(&input, None, &model).ok_or(CustomRangeError::NotRetained)?;
         Ok(VpAudit {
             window: "CUSTOM".into(),
             start: start_ms,
@@ -567,14 +559,7 @@ fn build_profile(
     let input_bars = input.len();
     let histogram = histogram::histogram(&input, range, model)?;
     let levels = histogram.levels(
-        label,
-        start,
-        end,
-        direction,
-        swing_high,
-        swing_low,
-        interval,
-        input_bars,
+        label, start, end, direction, swing_high, swing_low, interval, input_bars,
     );
     let stored = StoredProfile {
         histogram,
@@ -596,8 +581,10 @@ fn upsert_candle(candles: &mut Vec<VpCandle>, candle: VpCandle) {
 }
 
 fn upsert_candles(candles: &mut Vec<VpCandle>, incoming: Vec<VpCandle>) {
-    let mut by_time: std::collections::BTreeMap<i64, VpCandle> =
-        candles.drain(..).map(|candle| (candle.time, candle)).collect();
+    let mut by_time: std::collections::BTreeMap<i64, VpCandle> = candles
+        .drain(..)
+        .map(|candle| (candle.time, candle))
+        .collect();
     for candle in incoming {
         by_time.insert(candle.time, candle);
     }
@@ -651,18 +638,40 @@ mod tests {
         let mut e = VolumeProfileEngine::new();
 
         // Session A: Mon 18:00 -> Tue 18:00, price band 3300-3310.
-        fill(&mut e, ny(2025, 6, 9, 18, 0), ny(2025, 6, 10, 18, 0), 3300.0, 3310.0);
+        fill(
+            &mut e,
+            ny(2025, 6, 9, 18, 0),
+            ny(2025, 6, 10, 18, 0),
+            3300.0,
+            3310.0,
+        );
         // Session B: Tue 18:00 -> Wed 18:00, price band 3400-3410.
-        fill(&mut e, ny(2025, 6, 10, 18, 0), ny(2025, 6, 11, 18, 0), 3400.0, 3410.0);
+        fill(
+            &mut e,
+            ny(2025, 6, 10, 18, 0),
+            ny(2025, 6, 11, 18, 0),
+            3400.0,
+            3410.0,
+        );
         // Session C (in progress): Wed 18:00 -> now, band 3500-3510.
-        fill(&mut e, ny(2025, 6, 11, 18, 0), ny(2025, 6, 12, 10, 0), 3500.0, 3510.0);
+        fill(
+            &mut e,
+            ny(2025, 6, 11, 18, 0),
+            ny(2025, 6, 12, 10, 0),
+            3500.0,
+            3510.0,
+        );
 
         // At Tue 20:00 the last closed session is A.
         e.recompute(ny(2025, 6, 10, 20, 0));
         let ps_a = e.ps_levels.clone().expect("PS for session A");
         assert_eq!(ps_a.start, ny(2025, 6, 9, 18, 0));
         assert_eq!(ps_a.end, ny(2025, 6, 10, 18, 0));
-        assert!((3300.0..=3310.0).contains(&ps_a.poc), "poc {} not in A", ps_a.poc);
+        assert!(
+            (3300.0..=3310.0).contains(&ps_a.poc),
+            "poc {} not in A",
+            ps_a.poc
+        );
 
         // Nothing has closed yet at Wed 10:00 -> PS must NOT move.
         assert!(!e.refresh_on_session_close(ny(2025, 6, 11, 10, 0)));
@@ -673,21 +682,35 @@ mod tests {
         let ps_b = e.ps_levels.clone().expect("PS for session B");
         assert_eq!(ps_b.start, ny(2025, 6, 10, 18, 0));
         assert_eq!(ps_b.end, ny(2025, 6, 11, 18, 0));
-        assert!((3400.0..=3410.0).contains(&ps_b.poc), "poc {} not in B", ps_b.poc);
+        assert!(
+            (3400.0..=3410.0).contains(&ps_b.poc),
+            "poc {} not in B",
+            ps_b.poc
+        );
         assert_ne!(ps_a.poc, ps_b.poc, "PS was frozen across a session close");
 
         // Thu 18:00 close passes -> PS rolls to session C.
         assert!(e.refresh_on_session_close(ny(2025, 6, 12, 18, 1)));
         let ps_c = e.ps_levels.clone().expect("PS for session C");
         assert_eq!(ps_c.start, ny(2025, 6, 11, 18, 0));
-        assert!((3500.0..=3510.0).contains(&ps_c.poc), "poc {} not in C", ps_c.poc);
+        assert!(
+            (3500.0..=3510.0).contains(&ps_c.poc),
+            "poc {} not in C",
+            ps_c.poc
+        );
     }
 
     #[test]
     fn ps_skips_the_weekend_gap() {
         let mut e = VolumeProfileEngine::new();
         // Friday session: Thu 18:00 -> Fri 18:00 (market closes Fri 18:00 NY).
-        fill(&mut e, ny(2025, 6, 12, 18, 0), ny(2025, 6, 13, 18, 0), 3350.0, 3360.0);
+        fill(
+            &mut e,
+            ny(2025, 6, 12, 18, 0),
+            ny(2025, 6, 13, 18, 0),
+            3350.0,
+            3360.0,
+        );
 
         // Saturday noon: the "last closed session" window (Fri 18:00 ->
         // Sat 18:00) has no data, so PS must fall back to the Friday session
@@ -702,7 +725,13 @@ mod tests {
     #[test]
     fn refresh_fires_once_per_close_and_ignores_empty_weekend_sessions() {
         let mut e = VolumeProfileEngine::new();
-        fill(&mut e, ny(2025, 6, 12, 18, 0), ny(2025, 6, 13, 18, 0), 3350.0, 3360.0);
+        fill(
+            &mut e,
+            ny(2025, 6, 12, 18, 0),
+            ny(2025, 6, 13, 18, 0),
+            3350.0,
+            3360.0,
+        );
         // Saturday noon: PS is the Friday session.
         e.recompute(ny(2025, 6, 14, 12, 0));
         assert_eq!(
@@ -726,9 +755,21 @@ mod tests {
     fn cw_appears_once_monday_closes_and_freezes_intraday() {
         let mut e = VolumeProfileEngine::new();
         // Monday session: Sun 18:00 -> Mon 18:00.
-        fill(&mut e, ny(2025, 6, 8, 18, 0), ny(2025, 6, 9, 18, 0), 3300.0, 3310.0);
+        fill(
+            &mut e,
+            ny(2025, 6, 8, 18, 0),
+            ny(2025, 6, 9, 18, 0),
+            3300.0,
+            3310.0,
+        );
         // Tuesday session, still in progress.
-        fill(&mut e, ny(2025, 6, 9, 18, 0), ny(2025, 6, 10, 12, 0), 3400.0, 3410.0);
+        fill(
+            &mut e,
+            ny(2025, 6, 9, 18, 0),
+            ny(2025, 6, 10, 12, 0),
+            3400.0,
+            3410.0,
+        );
 
         // Monday 12:00: nothing has closed this week yet -> no CW.
         e.recompute(ny(2025, 6, 9, 12, 0));
@@ -752,7 +793,10 @@ mod tests {
         // Tuesday 12:00: Tuesday still open -> CW frozen on Monday's snapshot.
         e.recompute(ny(2025, 6, 10, 12, 0));
         let frozen = e.cw_levels.clone().expect("CW stays through Tuesday");
-        assert_eq!((frozen.start, frozen.end, frozen.poc), (cw.start, cw.end, cw.poc));
+        assert_eq!(
+            (frozen.start, frozen.end, frozen.poc),
+            (cw.start, cw.end, cw.poc)
+        );
     }
 
     #[test]
@@ -772,7 +816,13 @@ mod tests {
         let mut e = VolumeProfileEngine::new();
         let now = ny(2025, 6, 11, 12, 0);
         let week_start = VolumeProfileEngine::most_recent_week_start_utc(now);
-        fill(&mut e, week_start - 6 * 24 * H, week_start - 24 * H, 3100.0, 3110.0);
+        fill(
+            &mut e,
+            week_start - 6 * 24 * H,
+            week_start - 24 * H,
+            3100.0,
+            3110.0,
+        );
         fill(&mut e, week_start + H, now, 3200.0, 3210.0);
         e.recompute(now);
         let pw = e.pw_levels.clone().expect("PW");
@@ -893,11 +943,7 @@ mod tests {
         );
         // A range reaching years back has no retained bars to profile.
         let err = e
-            .audit_custom_range(
-                base - 400 * 24 * 60 * MIN,
-                base - 399 * 24 * 60 * MIN,
-                None,
-            )
+            .audit_custom_range(base - 400 * 24 * 60 * MIN, base - 399 * 24 * 60 * MIN, None)
             .unwrap_err();
         assert_eq!(err, CustomRangeError::NotRetained);
         assert_eq!(e.retained_bounds(), Some((base, base + 105 * MIN)));
diff --git a/src/volume_profile/histogram.rs b/src/volume_profile/histogram.rs
index 6e4ad57..6a27ab3 100644
--- a/src/volume_profile/histogram.rs
+++ b/src/volume_profile/histogram.rs
@@ -126,7 +126,6 @@ impl ProfileModel {
             },
         }
     }
-
 }
 
 /// One price row of the histogram, with the up/down split TradingView's
@@ -370,10 +369,7 @@ pub fn histogram(
     let poc_idx = bins
         .iter()
         .enumerate()
-        .max_by(|a, b| {
-            a.1.partial_cmp(b.1)
-                .unwrap_or(std::cmp::Ordering::Equal)
-        })
+        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
         .map(|(i, _)| i)?;
 
     // Value area: start at the POC and repeatedly add the larger adjacent
@@ -554,7 +550,11 @@ mod tests {
             candle(2, 4150.0, 4150.5, 10.0),
         ];
         let hist = histogram(&candles, None, &rows_model(12)).expect("profile");
-        assert_eq!(hist.rows.last().unwrap().high, 4150.5, "top row clipped to the profile high");
+        assert_eq!(
+            hist.rows.last().unwrap().high,
+            4150.5,
+            "top row clipped to the profile high"
+        );
         assert_eq!(hist.range_high, 4150.5);
     }
 
diff --git a/src/volume_profile/session.rs b/src/volume_profile/session.rs
index 668983f..d861c2b 100644
--- a/src/volume_profile/session.rs
+++ b/src/volume_profile/session.rs
@@ -71,10 +71,7 @@ fn ny_close_on(year: i32, month: u32, day: u32) -> i64 {
 /// the market traded 4150+). A session therefore only counts when at least one
 /// of its bars has a body, which is the same closed-market signature the
 /// dashboard uses to drop filler runs.
-pub fn previous_session_slice(
-    candles: &[VpCandle],
-    now_ms: i64,
-) -> (i64, i64, Vec<VpCandle>) {
+pub fn previous_session_slice(candles: &[VpCandle], now_ms: i64) -> (i64, i64, Vec<VpCandle>) {
     let mut end = last_session_close_utc(now_ms);
     let first_end = end;
     let mut first_start = session_close_shift(end, -1);
@@ -170,7 +167,10 @@ mod tests {
         // Sunday 19:00 NY: the most recent close is Sunday 18:00, so the
         // session being described would be the (closed) weekend session.
         let (start, end, slice) = previous_session_slice(&candles, ny(2025, 6, 8, 19, 0));
-        assert_eq!(end, sat_close, "PS ends where the last traded session closed");
+        assert_eq!(
+            end, sat_close,
+            "PS ends where the last traded session closed"
+        );
         assert_eq!(start, fri_close, "PS starts at the previous 18:00 NY");
         assert_eq!(slice.len(), 30, "PS carries the Friday session's bars");
         assert!(slice.iter().any(|c| c.open != c.close));
diff --git a/src/volume_profile/swing.rs b/src/volume_profile/swing.rs
index 28304f9..c7dfa19 100644
--- a/src/volume_profile/swing.rs
+++ b/src/volume_profile/swing.rs
@@ -8,9 +8,9 @@
 
 use crate::types::{VpCandle, VpLevels};
 
+use super::StoredProfile;
 use super::histogram;
 use super::timeframe::LowerTf;
-use super::StoredProfile;
 
 /// Bars on each side of a fractal pivot. 3 × 15m = 45 minutes of
 /// confirmation lag: responsive enough to track the current swing without
@@ -138,14 +138,10 @@ fn latest_swing(candles: &[VpCandle]) -> Option<SwingLeg> {
             // double tops/bottoms anchor at their second touch.
             let is_high = high.is_finite()
                 && (index - SWING_PIVOT_RADIUS..=index + SWING_PIVOT_RADIUS)
-                    .all(|j| {
-                        j == index || !candles[j].high.is_finite() || high >= candles[j].high
-                    });
+                    .all(|j| j == index || !candles[j].high.is_finite() || high >= candles[j].high);
             let is_low = low.is_finite()
                 && (index - SWING_PIVOT_RADIUS..=index + SWING_PIVOT_RADIUS)
-                    .all(|j| {
-                        j == index || !candles[j].low.is_finite() || low <= candles[j].low
-                    });
+                    .all(|j| j == index || !candles[j].low.is_finite() || low <= candles[j].low);
 
             if is_high {
                 push_pivot(&mut pivots, PivotKind::High, index, candles);
@@ -213,9 +209,7 @@ fn fallback_leg(candles: &[VpCandle]) -> Option<SwingLeg> {
         {
             high_rel = i;
         }
-        if c.low.is_finite()
-            && (!window[low_rel].low.is_finite() || c.low <= window[low_rel].low)
-        {
+        if c.low.is_finite() && (!window[low_rel].low.is_finite() || c.low <= window[low_rel].low) {
             low_rel = i;
         }
     }
@@ -252,12 +246,7 @@ fn fallback_leg(candles: &[VpCandle]) -> Option<SwingLeg> {
     }
 }
 
-fn push_pivot(
-    pivots: &mut Vec<Pivot>,
-    kind: PivotKind,
-    index: usize,
-    candles: &[VpCandle],
-) {
+fn push_pivot(pivots: &mut Vec<Pivot>, kind: PivotKind, index: usize, candles: &[VpCandle]) {
     if let Some(previous) = pivots.last_mut() {
         if previous.kind == kind {
             let replace = match kind {
@@ -297,14 +286,7 @@ mod tests {
         prices
             .iter()
             .enumerate()
-            .map(|(i, price)| {
-                candle(
-                    start + i as i64 * 15 * MIN,
-                    price - 0.5,
-                    price + 0.5,
-                    100.0,
-                )
-            })
+            .map(|(i, price)| candle(start + i as i64 * 15 * MIN, price - 0.5, price + 0.5, 100.0))
             .collect()
     }
 
@@ -313,20 +295,21 @@ mod tests {
         histogram::ProfileModel::default()
     }
 
-    fn levels_of(
-        computed: Option<(VpLevels, StoredProfile)>,
-    ) -> VpLevels {
+    fn levels_of(computed: Option<(VpLevels, StoredProfile)>) -> VpLevels {
         computed.expect("swing profile").0
     }
 
     #[test]
     fn bearish_swing_is_anchored_high_to_low() {
         let prices = [
-            100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 104.0, 103.0, 102.0, 101.0,
-            100.0, 99.0, 98.0, 97.0, 96.0, 95.0, 96.0, 97.0, 98.0, 99.0, 100.0, 101.0,
-            102.0, 103.0, 104.0,
+            100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 104.0, 103.0, 102.0, 101.0, 100.0, 99.0,
+            98.0, 97.0, 96.0, 95.0, 96.0, 97.0, 98.0, 99.0, 100.0, 101.0, 102.0, 103.0, 104.0,
         ];
-        let swing = levels_of(compute_swing(&swing_history(&prices), &model(), LowerTf::Tv));
+        let swing = levels_of(compute_swing(
+            &swing_history(&prices),
+            &model(),
+            LowerTf::Tv,
+        ));
         assert_eq!(swing.window, "SWING_BEAR");
         assert_eq!(swing.direction, "bearish");
         assert_eq!(swing.swing_high, Some(105.5));
@@ -337,11 +320,15 @@ mod tests {
     #[test]
     fn bullish_swing_is_anchored_low_to_high() {
         let prices = [
-            105.0, 104.0, 103.0, 102.0, 101.0, 100.0, 101.0, 102.0, 103.0, 104.0, 105.0,
-            106.0, 107.0, 108.0, 109.0, 110.0, 109.0, 108.0, 107.0, 106.0, 105.0, 104.0,
-            103.0, 102.0, 101.0,
+            105.0, 104.0, 103.0, 102.0, 101.0, 100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0,
+            107.0, 108.0, 109.0, 110.0, 109.0, 108.0, 107.0, 106.0, 105.0, 104.0, 103.0, 102.0,
+            101.0,
         ];
-        let swing = levels_of(compute_swing(&swing_history(&prices), &model(), LowerTf::Tv));
+        let swing = levels_of(compute_swing(
+            &swing_history(&prices),
+            &model(),
+            LowerTf::Tv,
+        ));
         assert_eq!(swing.window, "SWING_BULL");
         assert_eq!(swing.direction, "bullish");
         assert_eq!(swing.swing_low, Some(99.5));
@@ -354,8 +341,8 @@ mod tests {
         // Double top with two equal 103 touches: the leg must start at the
         // *second* touch (index 4), not the first.
         let prices = [
-            100.0, 101.0, 102.0, 103.0, 103.0, 102.0, 101.0, 100.0, 99.0, 98.0,
-            99.0, 100.0, 101.0, 102.0, 103.0,
+            100.0, 101.0, 102.0, 103.0, 103.0, 102.0, 101.0, 100.0, 99.0, 98.0, 99.0, 100.0, 101.0,
+            102.0, 103.0,
         ];
         let candles = swing_history(&prices);
         let swing = levels_of(compute_swing(&candles, &model(), LowerTf::Tv));
diff --git a/src/volume_profile/timeframe.rs b/src/volume_profile/timeframe.rs
index 8932936..cef72a9 100644
--- a/src/volume_profile/timeframe.rs
+++ b/src/volume_profile/timeframe.rs
@@ -141,10 +141,7 @@ pub fn tv_lower_timeframe(candles: &[VpCandle]) -> (i64, &'static str) {
 /// coarser than the requested resolution (the 15m chart seed fallback), the
 /// history is used as-is and its own resolution is reported, so a client can
 /// see that this profile was built from 15m bars.
-pub fn prepare_input(
-    candles: &[VpCandle],
-    lower_tf: LowerTf,
-) -> (Vec<VpCandle>, &'static str) {
+pub fn prepare_input(candles: &[VpCandle], lower_tf: LowerTf) -> (Vec<VpCandle>, &'static str) {
     if candles.is_empty() {
         return (Vec::new(), "15m");
     }
@@ -300,7 +297,10 @@ mod tests {
     #[test]
     fn prepare_input_never_downscales_coarse_history() {
         // 15m history: asking for TV parity cannot invent finer bars.
-        let candles = vec![bar(0, 4100.0, 4101.0, 1.0), bar(900_000, 4101.0, 4102.0, 1.0)];
+        let candles = vec![
+            bar(0, 4100.0, 4101.0, 1.0),
+            bar(900_000, 4101.0, 4102.0, 1.0),
+        ];
         let (out, label) = prepare_input(&candles, LowerTf::Tv);
         assert_eq!(out.len(), 2);
         assert_eq!(label, "15m");
diff --git a/src/ws_server.rs b/src/ws_server.rs
index f49c592..151b0c7 100644
--- a/src/ws_server.rs
+++ b/src/ws_server.rs
@@ -1,26 +1,26 @@
-use std::sync::Arc;
-use std::time::Duration;
 use axum::{
+    Json, Router,
     extract::{
-        ws::{Message, WebSocket, WebSocketUpgrade},
         Query, State,
+        ws::{Message, WebSocket, WebSocketUpgrade},
     },
-    http::{header, HeaderValue, Method},
+    http::{HeaderValue, Method, header},
     response::{IntoResponse, Response},
     routing::get,
-    Json, Router,
 };
 use dashmap::DashMap;
 use futures_util::{SinkExt, StreamExt};
-use tokio::sync::{broadcast, RwLock};
-use tracing::info;
+use std::sync::Arc;
+use std::time::Duration;
+use tokio::sync::{RwLock, broadcast};
 use tower_http::cors::{AllowOrigin, CorsLayer};
+use tracing::info;
 
 use crate::ai_cache::AiCache;
 use crate::config::Config;
 use crate::status::FeedStatus;
 use crate::tick_volume::TickVolumeStore;
-use crate::types::{TickVolumeBar, VpLevels, VpCandle, WsFrame};
+use crate::types::{TickVolumeBar, VpCandle, VpLevels, WsFrame};
 use crate::volume_profile::{CustomRangeError, VolumeProfileEngine};
 
 /// Fourteen ATR true ranges need fifteen OHLC bars (the first close is the
@@ -150,8 +150,10 @@ async fn vp_audit(
     // Hand-selected range: the chart already knows which range it drew, so
     // profile exactly that instead of one of the engine's own windows.
     if params.contains_key("start") || params.contains_key("end") {
-        let (start, end) = match (parse_epoch_ms(&params, "start"), parse_epoch_ms(&params, "end"))
-        {
+        let (start, end) = match (
+            parse_epoch_ms(&params, "start"),
+            parse_epoch_ms(&params, "end"),
+        ) {
             (Some(start), Some(end)) => (start, end),
             _ => {
                 return (
@@ -328,17 +330,10 @@ async fn status_snapshot(State(state): State<AppState>) -> Response {
             "audit": "/vp?window=PW",
         },
     });
-    (
-        [(header::CACHE_CONTROL, "no-store")],
-        Json(body),
-    )
-        .into_response()
+    ([(header::CACHE_CONTROL, "no-store")], Json(body)).into_response()
 }
 
-async fn ws_handler(
-    ws: WebSocketUpgrade,
-    State(state): State<AppState>,
-) -> impl IntoResponse {
+async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
     ws.on_upgrade(|socket| handle_socket(socket, state))
 }
 
@@ -623,7 +618,11 @@ mod tests {
             let res = send("GET", uri, Some(ALLOWED)).await;
             assert_eq!(res.status(), StatusCode::OK, "{uri}");
             assert_eq!(acao(&res).as_deref(), Some(ALLOWED), "{uri}");
-            assert_ne!(acao(&res).as_deref(), Some("*"), "{uri} must not use a wildcard");
+            assert_ne!(
+                acao(&res).as_deref(),
+                Some("*"),
+                "{uri} must not use a wildcard"
+            );
 
             let vary = res
                 .headers()
@@ -644,7 +643,11 @@ mod tests {
             // browser on any other page cannot read the response.
             let res = send("GET", uri, Some(FOREIGN)).await;
             assert_eq!(res.status(), StatusCode::OK, "{uri}");
-            assert_eq!(acao(&res), None, "{uri} leaked an allow-header to a foreign origin");
+            assert_eq!(
+                acao(&res),
+                None,
+                "{uri} leaked an allow-header to a foreign origin"
+            );
 
             let res = send("GET", uri, None).await;
             assert_eq!(res.status(), StatusCode::OK, "{uri}");
@@ -658,7 +661,8 @@ mod tests {
         // off — with them a wildcard would expose the API to any site.
         let res = send("GET", "/levels", Some(ALLOWED)).await;
         assert!(
-            !res.headers().contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
+            !res.headers()
+                .contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
             "credentials must not be enabled"
         );
 
@@ -678,7 +682,10 @@ mod tests {
             .await
             .unwrap();
         assert_eq!(acao(&res).as_deref(), Some(dev));
-        let res = app(dev).oneshot(build("GET", "/levels", Some(ALLOWED))).await.unwrap();
+        let res = app(dev)
+            .oneshot(build("GET", "/levels", Some(ALLOWED)))
+            .await
+            .unwrap();
         assert_eq!(acao(&res), None);
     }
 
@@ -702,7 +709,10 @@ mod tests {
             .unwrap()
             .to_str()
             .unwrap();
-        assert!(methods.contains("GET") && methods.contains("OPTIONS"), "{methods}");
+        assert!(
+            methods.contains("GET") && methods.contains("OPTIONS"),
+            "{methods}"
+        );
         assert!(
             res.headers()
                 .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
@@ -719,7 +729,10 @@ mod tests {
                 .unwrap(),
             "600"
         );
-        assert!(!res.headers().contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS));
+        assert!(
+            !res.headers()
+                .contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
+        );
 
         let req = Request::builder()
             .method("OPTIONS")
@@ -794,9 +807,7 @@ mod tests {
         keys.sort();
         assert_eq!(
             keys,
-            vec![
-                "close", "high", "low", "open", "source", "time", "volume"
-            ]
+            vec!["close", "high", "low", "open", "source", "time", "volume"]
         );
     }
 
@@ -805,7 +816,11 @@ mod tests {
         let res = send("GET", "/tick-volume", Some(ALLOWED)).await;
         assert_eq!(res.status(), StatusCode::OK);
         assert_eq!(
-            res.headers().get(header::CACHE_CONTROL).unwrap().to_str().unwrap(),
+            res.headers()
+                .get(header::CACHE_CONTROL)
+                .unwrap()
+                .to_str()
+                .unwrap(),
             "no-store"
         );
         let body = axum::body::to_bytes(res.into_body(), usize::MAX)
