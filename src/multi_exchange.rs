use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http;

pub async fn run_alltick_stream(tx: mpsc::Sender<AggTrade>) -> Result<()> {
    let url = std::env::var("ALLTICK_WS_URL")
        .unwrap_or_else(|_| "wss://quote.alltick.co/quote-b-ws-api".to_string());
    let token = std::env::var("ALLTICK_TOKEN").unwrap_or_default();
    let full_url = format!("{}?token={}", url, token);

    loop {
        info!("Connecting to AllTick trade stream");
        match connect_async(&full_url).await {
            Ok((ws, _)) => {
                let (mut write, mut read) = ws.split();
                let sub = serde_json::json!({
                    "cmd_id": 22004, "seq_id": 1, "trace": "rust-xauusd-engine",
                    "data": { "symbol_list": [{ "code": "XAUUSD" }] }
                });
                if write.send(Message::Text(sub.to_string().into())).await.is_err() { continue; }

                let ping_task = tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                        let hb = serde_json::json!({"cmd_id": 22000, "seq_id": 2, "trace": "hb", "data": {}});
                        if write.send(Message::Text(hb.to_string().into())).await.is_err() { break; }
                    }
                });

                while let Some(msg) = read.next().await {
                    if let Ok(Message::Text(text)) = msg {
                        let v: Value = match serde_json::from_str(&text) { Ok(v) => v, _ => continue };
                        if v["cmd_id"].as_i64() != Some(22998) { continue; } // Trade tick push
                        
                        if let Some(data) = v["data"].as_object() {
                            let price = data["price"].as_str().unwrap_or("0");
                            let vol = data["volume"].as_str().unwrap_or("0");
                            let ts = data["tick_time"].as_i64().unwrap_or(0);
                            
                            // Map trade_direction: 1=Aggressive Buy (is_buyer_maker=false), 2=Aggressive Sell (is_buyer_maker=true)
                            let dir = data["trade_direction"].as_i64().unwrap_or(1);
                            let is_buyer_maker = dir == 2; 
                            
                            let agg = AggTrade {
                                event_type: "alltick".into(), event_time: ts, symbol: "XAUUSD".into(),
                                agg_id: 0, price: price.into(), quantity: vol.into(),
                                first_trade_id: 0, last_trade_id: 0, trade_time: ts,
                                is_buyer_maker, symbol_type: None, exchange: "alltick".into(),
                            };
                            let _ = tx.send(agg).await;
                        }
                    }
                }
                ping_task.abort();
            }
            Err(e) => warn!("AllTick connect failed: {}. Retrying in 5s", e),
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

pub async fn run_itick_stream(tx: mpsc::Sender<AggTrade>) -> Result<()> {
    // Free tier URL
    let url = std::env::var("ITICK_WS_URL").unwrap_or_else(|_| "wss://api-free.itick.org/forex".to_string());
    let token = std::env::var("ITICK_TOKEN").unwrap_or_default();
    
    loop {
        // Inject header using tokio-tungstenite's internal http re-export (no extra Cargo.toml deps)
        let mut req = url.clone().into_client_request()?;
        req.headers_mut().insert(
            http::header::HeaderName::from_static("token"),
            http::header::HeaderValue::from_str(&token).unwrap_or_else(|_| http::header::HeaderValue::from_static("")),
        );

        info!("Connecting to iTick trade stream");
        match connect_async(req).await {
            Ok((ws, _)) => {
                let (_, mut read) = ws.split();
                while let Some(msg) = read.next().await {
                    if let Ok(Message::Text(text)) = msg {
                        let v: Value = match serde_json::from_str(&text) { Ok(v) => v, _ => continue };
                        
                        // FIX: 'type' is nested inside 'data'
                        let data = match v["data"].as_object() { Some(d) => d, None => continue };
                        if data["type"].as_str() != Some("tick") { continue; }
                        
                        let price = data["ld"].as_f64().unwrap_or(0.0);
                        let vol = data["v"].as_f64().unwrap_or(0.0);
                        let ts = data["t"].as_i64().unwrap_or(0);
                        
                        let agg = AggTrade {
                            event_type: "itick".into(), event_time: ts, symbol: "XAUUSD".into(),
                            agg_id: 0, price: price.to_string(), quantity: vol.to_string(),
                            first_trade_id: 0, last_trade_id: 0, trade_time: ts,
                            is_buyer_maker: false, // iTick provides no taker side
                            symbol_type: None, exchange: "itick".into(),
                        };
                        let _ = tx.send(agg).await;
                    }
                }
            }
            Err(e) => warn!("iTick connect failed: {}. Retrying in 5s", e),
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}