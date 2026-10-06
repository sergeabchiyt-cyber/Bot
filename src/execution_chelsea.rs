use anyhow::Result;
use serde_json::json;
use crate::config::Config;
use crate::types::TradeEvent;

pub struct ChelseaExecution {
    pub url: String,
    pub http: reqwest::Client,
}

impl ChelseaExecution {
    pub fn new(config: &Config) -> Self {
        Self {
            url: config.mcp_chelsea_url.clone().unwrap_or_default(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap(),
        }
    }

    pub async fn place_order(
        &self,
        side: &str,
        size: f64,
        sl: f64,
        tp: f64,
    ) -> Result<TradeEvent> {
        let body = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {
                "name": "place_order",
                "arguments": {
                    "symbol": "XAUUSD",
                    "side": side,
                    "size": size,
                    "stop_loss": sl,
                    "take_profit": tp
                }
            }
        });

        let resp = self.http.post(&self.url).json(&body).send().await?;
        let v: serde_json::Value = resp.json().await?;

        if let Some(err) = v.get("error") {
            anyhow::bail!("ChelseaAI error: {}", err);
        }

        let trade_id = v["result"]["content"][0]["text"]
            .as_str().unwrap_or("unknown").to_string();

        Ok(TradeEvent {
            trade_id,
            symbol: "XAUUSD".into(),
            side: side.into(),
            size,
            entry: 0.0,  // filled by broker ack
            sl,
            tp,
            status: "open".into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }
}