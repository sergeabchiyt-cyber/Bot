use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use crate::status::FeedStatus;
use crate::types::AggTrade;

const RECONNECT_SECS: u64 = 5;

fn num_str(v: &Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    if let Some(n) = v.as_f64() {
        return Some(format!("{n}"));
    }
    if let Some(i) = v.as_i64() {
        return Some(i.to_string());
    }
    None
}

// =====================================================================
// Bybit — XAUUSDT linear perpetual, publicTrade
// =====================================================================

pub async fn run_bybit_stream(tx: mpsc::Sender<AggTrade>, status: FeedStatus) -> Result<()> {
    let url = "wss://stream.bybit.com/v5/public/linear";
    loop {
        status.set("bybit", "connecting");
        info!("Connecting to Bybit trade stream");
        match connect_async(url).await {
            Ok((ws, _)) => {
                let (mut write, mut read) = ws.split();

                let sub = json!({
                    "op": "subscribe",
                    "args": ["publicTrade.XAUUSDT"]
                });
                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
                    warn!("Bybit subscribe send failed");
                    status.set("bybit", "error");
                    continue;
                }
                info!("Bybit stream connected");
                status.set("bybit", "connected");

                // Bybit drops idle connections; a protocol-level ping is
                // answered automatically by tungstenite, app-level ping:
                let ping_task = tokio::spawn(async move {
                    let mut iv = tokio::time::interval(std::time::Duration::from_secs(20));
                    iv.tick().await;
                    loop {
                        iv.tick().await;
                        if write.send(Message::Ping(vec![].into())).await.is_err() {
                            break;
                        }
                    }
                });

                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            status.mark_msg("bybit");
                            let v: Value = match serde_json::from_str(&text) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            if v["topic"].as_str() != Some("publicTrade.XAUUSDT") {
                                continue;
                            }
                            if let Some(arr) = v["data"].as_array() {
                                for t in arr {
                                    // Bybit: S="Buy" means the taker bought.
                                    let side = t["S"].as_str().unwrap_or("Buy");
                                    let price = t["p"].as_str().unwrap_or("0");
                                    let qty = t["v"].as_str().unwrap_or("0");
                                    let ts = t["T"].as_i64().unwrap_or(0);
                                    let agg = AggTrade::new(
                                        "bybit", "XAUUSDT", price, qty, ts,
                                        side == "Buy", true,
                                    );
                                    let _ = tx.send(agg).await;
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("Bybit read error: {}", e);
                            break;
                        }
                    }
                }

                ping_task.abort();
                status.set("bybit", "disconnected");
            }
            Err(e) => {
                warn!("Bybit connect failed: {}. Retrying in {RECONNECT_SECS}s", e);
                status.set("bybit", "error");
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(RECONNECT_SECS)).await;
    }
}

// =====================================================================
// OKX — XAU-USDT-SWAP trades, app-level {"op":"ping"} every 25s
// =====================================================================

pub async fn run_okx_stream(tx: mpsc::Sender<AggTrade>, status: FeedStatus) -> Result<()> {
    let url = "wss://ws.okx.com:8443/ws/v5/public";
    loop {
        status.set("okx", "connecting");
        info!("Connecting to OKX trade stream");
        match connect_async(url).await {
            Ok((ws, _)) => {
                let (mut write, mut read) = ws.split();

                let sub = json!({
                    "op": "subscribe",
                    "args": [{"channel": "trades", "instId": "XAU-USDT-SWAP"}]
                });
                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
                    warn!("OKX subscribe send failed");
                    status.set("okx", "error");
                    continue;
                }
                info!("OKX stream connected");
                status.set("okx", "connected");

                let ping_task = tokio::spawn(async move {
                    let mut iv = tokio::time::interval(std::time::Duration::from_secs(25));
                    iv.tick().await;
                    loop {
                        iv.tick().await;
                        let ping = json!({"op": "ping"});
                        if write.send(Message::Text(ping.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                });

                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            status.mark_msg("okx");
                            let v: Value = match serde_json::from_str(&text) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            if v["arg"]["channel"].as_str() != Some("trades") {
                                continue;
                            }
                            if let Some(arr) = v["data"].as_array() {
                                for t in arr {
                                    // OKX: side="buy" means the taker bought.
                                    let side = t["side"].as_str().unwrap_or("buy");
                                    let px = t["px"].as_str().unwrap_or("0");
                                    let sz = t["sz"].as_str().unwrap_or("0");
                                    let ts = t["ts"]
                                        .as_str()
                                        .and_then(|s| s.parse::<i64>().ok())
                                        .unwrap_or(0);
                                    let agg = AggTrade::new(
                                        "okx", "XAUUSDT", px, sz, ts,
                                        side == "buy", true,
                                    );
                                    let _ = tx.send(agg).await;
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("OKX read error: {}", e);
                            break;
                        }
                    }
                }

                ping_task.abort();
                status.set("okx", "disconnected");
            }
            Err(e) => {
                warn!("OKX connect failed: {}. Retrying in {RECONNECT_SECS}s", e);
                status.set("okx", "error");
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(RECONNECT_SECS)).await;
    }
}

// =====================================================================
// Bitget — XAUTUSDT USDT-FUTURES publicTrade (v2 public WS)
// Push rows are either arrays [price,size,side,ts,seq] or objects.
// =====================================================================

pub async fn run_bitget_stream(tx: mpsc::Sender<AggTrade>, symbol: String, status: FeedStatus) -> Result<()> {
    let url = "wss://ws.bitget.com/v2/ws/public";
    loop {
        status.set("bitget", "connecting");
        info!("Connecting to Bitget trade stream");
        match connect_async(url).await {
            Ok((ws, _)) => {
                let (mut write, mut read) = ws.split();

                let sub = json!({
                    "op": "subscribe",
                    "args": [
                        {"instType": "USDT-FUTURES", "channel": "publicTrade", "instId": symbol},
                    ]
                });
                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
                    warn!("Bitget subscribe send failed");
                    status.set("bitget", "error");
                    continue;
                }
                info!("Bitget stream connected ({symbol})");
                status.set("bitget", "connected");

                // Bitget requires an app-level "ping" within 30s windows.
                let ping_task = tokio::spawn(async move {
                    let mut iv = tokio::time::interval(std::time::Duration::from_secs(25));
                    iv.tick().await;
                    loop {
                        iv.tick().await;
                        if write.send(Message::Text("ping".into())).await.is_err() {
                            break;
                        }
                    }
                });

                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            if text == "pong" {
                                continue;
                            }
                            status.mark_msg("bitget");
                            let v: Value = match serde_json::from_str(&text) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            if let Some(err) = v.get("code").and_then(|c| c.as_str()) {
                                if err != "0" {
                                    warn!("Bitget error frame: {}", v["msg"].as_str().unwrap_or("?"));
                                }
                                continue;
                            }
                            // arg.instId guards the symbol.
                            let inst = v["arg"]["instId"].as_str().unwrap_or(&symbol);
                            if inst != symbol {
                                continue;
                            }
                            if let Some(arr) = v["data"].as_array() {
                                for t in arr {
                                    let (price, qty, ts, taker_buy, has_side) = if t.is_array() {
                                        // ["price","size","buy|sell","ts","seq"]
                                        (
                                            t.get(0).and_then(num_str),
                                            t.get(1).and_then(num_str),
                                            t.get(3).and_then(|x| x.as_str().and_then(|s| s.parse().ok()))
                                                .or_else(|| t.get(3).and_then(|x| x.as_i64())),
                                            t.get(2).and_then(|s| s.as_str()) == Some("buy"),
                                            true,
                                        )
                                    } else {
                                        // {price,size,side,ts,tradeId}
                                        (
                                            t["price"].as_str().map(String::from),
                                            t["size"].as_str().map(String::from),
                                            t["ts"].as_str().and_then(|s| s.parse().ok())
                                                .or_else(|| t["ts"].as_i64()),
                                            t["side"].as_str() == Some("buy"),
                                            true,
                                        )
                                    };
                                    if let (Some(price), Some(qty)) = (price, qty) {
                                        let ts = ts.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                                        let _ = tx.send(AggTrade::new(
                                            "bitget", &symbol, &price, &qty, ts, taker_buy, has_side,
                                        )).await;
                                    }
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("Bitget read error: {}", e);
                            break;
                        }
                    }
                }

                ping_task.abort();
                status.set("bitget", "disconnected");
            }
            Err(e) => {
                warn!("Bitget connect failed: {}. Retrying in {RECONNECT_SECS}s", e);
                status.set("bitget", "error");
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(RECONNECT_SECS)).await;
    }
}

// =====================================================================
// Gate.io — futures.trades on the USDT contract stream.
// `size` is signed: >0 buyer-taker, <0 seller-taker.
// =====================================================================

pub async fn run_gate_stream(tx: mpsc::Sender<AggTrade>, symbol: String, status: FeedStatus) -> Result<()> {
    let url = "wss://fx-ws.gateio.ws/v4/ws/usdt";
    loop {
        status.set("gate", "connecting");
        info!("Connecting to Gate.io trade stream");
        match connect_async(url).await {
            Ok((ws, _)) => {
                let (mut write, mut read) = ws.split();

                let sub = json!({
                    "time": chrono::Utc::now().timestamp(),
                    "channel": "futures.trades",
                    "event": "subscribe",
                    "payload": [symbol]
                });
                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
                    warn!("Gate subscribe send failed");
                    status.set("gate", "error");
                    continue;
                }
                info!("Gate.io stream connected ({symbol})");
                status.set("gate", "connected");

                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            status.mark_msg("gate");
                            let v: Value = match serde_json::from_str(&text) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            if v["channel"].as_str() != Some("futures.trades")
                                || v["event"].as_str() != Some("update")
                            {
                                continue;
                            }
                            if let Some(arr) = v["result"].as_array() {
                                for t in arr {
                                    if t["contract"].as_str() != Some(symbol.as_str()) {
                                        continue;
                                    }
                                    let Some(price) = num_str(&t["price"]) else { continue };
                                    let signed_size = t["size"].as_i64().unwrap_or(0);
                                    let qty = signed_size.abs().to_string();
                                    let ts = t["time"].as_i64().unwrap_or(0);
                                    let _ = tx.send(AggTrade::new(
                                        "gate", &symbol, &price, &qty, ts,
                                        signed_size > 0, true,
                                    )).await;
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("Gate read error: {}", e);
                            break;
                        }
                    }
                }

                status.set("gate", "disconnected");
            }
            Err(e) => {
                warn!("Gate connect failed: {}. Retrying in {RECONNECT_SECS}s", e);
                status.set("gate", "error");
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(RECONNECT_SECS)).await;
    }
}

// =====================================================================
// Kraken Futures — trade feed for PF_XAUTUSD (Tether Gold perp).
// Frames: {"feed":"trade", product_id, side, qty, price, time(ms), ...}
// =====================================================================

pub async fn run_kraken_stream(tx: mpsc::Sender<AggTrade>, product: String, status: FeedStatus) -> Result<()> {
    let url = "wss://futures.kraken.com/ws/v1";
    loop {
        status.set("kraken", "connecting");
        info!("Connecting to Kraken Futures trade stream");
        match connect_async(url).await {
            Ok((ws, _)) => {
                let (mut write, mut read) = ws.split();

                let sub = json!({
                    "event": "subscribe",
                    "feed": "trade",
                    "product_ids": [product]
                });
                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
                    warn!("Kraken subscribe send failed");
                    status.set("kraken", "error");
                    continue;
                }
                info!("Kraken Futures stream connected ({product})");
                status.set("kraken", "connected");

                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            status.mark_msg("kraken");
                            let v: Value = match serde_json::from_str(&text) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            if v.get("event").is_some() {
                                if v["event"].as_str() == Some("error") {
                                    warn!("Kraken error frame: {}", v["message"].as_str().unwrap_or("?"));
                                }
                                continue;
                            }
                            let feed = v["feed"].as_str().unwrap_or("");
                            match feed {
                                "trade" => {
                                    let price = num_str(&v["price"]).unwrap_or_default();
                                    let qty = num_str(&v["qty"]).unwrap_or_default();
                                    let ts = v["time"].as_i64().unwrap_or(0);
                                    let taker_buy = v["side"].as_str() == Some("buy");
                                    let _ = tx.send(AggTrade::new(
                                        "kraken", &product, &price, &qty, ts, taker_buy, true,
                                    )).await;
                                }
                                "trade_snapshot" => {
                                    // history replay — skip
                                }
                                _ => {}
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("Kraken read error: {}", e);
                            break;
                        }
                    }
                }

                status.set("kraken", "disconnected");
            }
            Err(e) => {
                warn!("Kraken connect failed: {}. Retrying in {RECONNECT_SECS}s", e);
                status.set("kraken", "error");
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(RECONNECT_SECS)).await;
    }
}

// =====================================================================
// AllTick — spot XAUUSD ticks (needs ALLTICK_TOKEN).
// subscribe cmd_id 22004, heartbeat cmd_id 22000 every 10s,
// trade push cmd_id 22001-22003 with price/volume/trade_direction.
// =====================================================================

pub async fn run_alltick_stream(tx: mpsc::Sender<AggTrade>, url: String, code: String, status: FeedStatus) -> Result<()> {
    let token = std::env::var("ALLTICK_TOKEN").unwrap_or_default();
    let full_url = format!("{url}?token={token}");

    loop {
        status.set("alltick", "connecting");
        info!("Connecting to AllTick trade stream");
        match connect_async(&full_url).await {
            Ok((ws, _)) => {
                let (mut write, mut read) = ws.split();
                let sub = json!({
                    "cmd_id": 22004, "seq_id": 1, "trace": "rust-xauusd-engine",
                    "data": { "symbol_list": [{ "code": code }] }
                });
                if write.send(Message::Text(sub.to_string().into())).await.is_err() {
                    warn!("AllTick subscribe send failed");
                    status.set("alltick", "error");
                    continue;
                }
                info!("AllTick stream connected ({code})");
                status.set("alltick", "connected");

                let ping_task = tokio::spawn(async move {
                    let mut iv = tokio::time::interval(std::time::Duration::from_secs(10));
                    iv.tick().await;
                    loop {
                        iv.tick().await;
                        let hb = json!({"cmd_id": 22000, "seq_id": 2, "trace": "hb", "data": {}});
                        if write.send(Message::Text(hb.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                });

                while let Some(msg) = read.next().await {
                    if let Ok(Message::Text(text)) = msg {
                        status.mark_msg("alltick");
                        let v: Value = match serde_json::from_str(&text) { Ok(v) => v, _ => continue };
                        let cmd = v["cmd_id"].as_i64().unwrap_or(0);
                        // 22001 quote / 22002 price / 22003 trade pushes
                        if !(22001..=22003).contains(&cmd) {
                            continue;
                        }
                        let rows: Vec<&Value> = match v["data"] {
                            Value::Array(ref a) => a.iter().collect(),
                            Value::Object(_) => vec![&v["data"]],
                            _ => continue,
                        };
                        for d in rows {
                            if d["code"].as_str().is_some_and(|c| c != code) {
                                continue;
                            }
                            let Some(price) = num_str(&d["price"]) else { continue };
                            let vol = num_str(&d["volume"]).unwrap_or_else(|| "0".into());
                            let ts = d["tick_time"].as_i64().unwrap_or(0);
                            // trade_direction: 1 aggressive buy, 2 aggressive sell.
                            // Price/quote pushes carry no direction → no flow side.
                            match d["trade_direction"].as_i64() {
                                Some(dir @ (1 | 2)) => {
                                    let _ = tx.send(AggTrade::new(
                                        "alltick", &code, &price, &vol, ts, dir == 1, true,
                                    )).await;
                                }
                                _ => {
                                    let _ = tx.send(AggTrade::new(
                                        "alltick", &code, &price, &vol, ts, true, false,
                                    )).await;
                                }
                            }
                        }
                    }
                }

                ping_task.abort();
                status.set("alltick", "disconnected");
            }
            Err(e) => {
                warn!("AllTick connect failed: {}. Retrying in {RECONNECT_SECS}s", e);
                status.set("alltick", "error");
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(RECONNECT_SECS)).await;
    }
}

// =====================================================================
// iTick — spot XAUUSD ticks (needs ITICK_TOKEN).
// token header handshake → wait {"resAc":"auth"} → subscribe
// {"ac":"subscribe","params":"XAUUSD,tick"} → pushes {s, ld, v, t, d}.
// `d`: 2 = aggressive buy, 1 = aggressive sell, 0/absent = unknown.
// =====================================================================

pub async fn run_itick_stream(tx: mpsc::Sender<AggTrade>, url: String, symbol: String, status: FeedStatus) -> Result<()> {
    let token = std::env::var("ITICK_TOKEN").unwrap_or_default();

    loop {
        status.set("itick", "connecting");
        info!("Connecting to iTick trade stream");
        let mut req = match url.as_str().into_client_request() {
            Ok(r) => r,
            Err(e) => {
                warn!("iTick bad url: {}", e);
                status.set("itick", "error");
                tokio::time::sleep(tokio::time::Duration::from_secs(RECONNECT_SECS)).await;
                continue;
            }
        };
        req.headers_mut().insert(
            http::header::HeaderName::from_static("token"),
            http::HeaderValue::from_str(&token)
                .unwrap_or_else(|_| http::HeaderValue::from_static("")),
        );

        match connect_async(req).await {
            Ok((ws, _)) => {
                let (mut write, mut read) = ws.split();
                let mut subscribed = false;
                info!("iTick stream connected ({symbol})");
                status.set("itick", "connected");

                let ping_task = tokio::spawn(async move {
                    let mut iv = tokio::time::interval(std::time::Duration::from_secs(25));
                    iv.tick().await;
                    loop {
                        iv.tick().await;
                        let ping = json!({"ac": "ping"});
                        if write.send(Message::Text(ping.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                });

                while let Some(msg) = read.next().await {
                    if let Ok(Message::Text(text)) = msg {
                        status.mark_msg("itick");
                        let v: Value = match serde_json::from_str(&text) { Ok(v) => v, _ => continue };

                        // Handshake/ack frames
                        if v.get("resAc").is_some() {
                            let ac = v["resAc"].as_str().unwrap_or("");
                            let ok = v["code"].as_i64().unwrap_or(0) == 0
                                || v["code"].as_i64() == Some(1);
                            if ac == "auth" && ok && !subscribed {
                                let sub = json!({"ac": "subscribe", "params": format!("{symbol},tick")});
                                if write.send(Message::Text(sub.to_string().into())).await.is_ok() {
                                    subscribed = true;
                                    info!("iTick subscribed to {symbol} ticks");
                                }
                            } else if !ok {
                                warn!("iTick ack error: {text}");
                            }
                            continue;
                        }

                        // Push frames carry the tick either at top level or under "data".
                        let d = match v.get("data").filter(|x| x.is_object()) {
                            Some(d) => d,
                            None => &v,
                        };
                        if d["type"].as_str().is_some_and(|t| t != "tick") {
                            continue;
                        }
                        if let Some(s) = d["s"].as_str() {
                            if s != symbol {
                                continue;
                            }
                        }
                        let Some(price) = num_str(&d["ld"]) else { continue };
                        let vol = num_str(&d["v"]).unwrap_or_else(|| "0".into());
                        let ts = d["t"].as_i64().unwrap_or(0);
                        match d["d"].as_i64() {
                            Some(dir @ (1 | 2)) => {
                                let _ = tx.send(AggTrade::new(
                                    "itick", &symbol, &price, &vol, ts, dir == 2, true,
                                )).await;
                            }
                            _ => {
                                // No taker side published — volume only.
                                let _ = tx.send(AggTrade::new(
                                    "itick", &symbol, &price, &vol, ts, true, false,
                                )).await;
                            }
                        }
                    }
                }

                ping_task.abort();
                status.set("itick", "disconnected");
            }
            Err(e) => {
                warn!("iTick connect failed: {}. Retrying in {RECONNECT_SECS}s", e);
                status.set("itick", "error");
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(RECONNECT_SECS)).await;
    }
}
