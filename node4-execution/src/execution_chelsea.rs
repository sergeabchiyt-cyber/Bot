//! Chelsea MCP live venue.
//!
//! A minimal JSON-RPC adapter for the `place_order` MCP tool. Sizing is a
//! Node 4 decision (`ORDER_SIZE`); the intent's stop loss and take profit are
//! passed verbatim.

use serde_json::json;

use crate::config::Config;
use crate::types::TradeEvent;

/// Distinguish "definitely not ordered" from "an order may have been sent".
pub enum ChelseaError {
    /// The request failed before (or at) the broker: nothing was ordered.
    Rejected(String),
    /// The request may have reached the broker: reconcile, never resend.
    Unknown(String),
}

impl std::fmt::Display for ChelseaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChelseaError::Rejected(msg) => write!(f, "{msg}"),
            ChelseaError::Unknown(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::fmt::Debug for ChelseaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}

impl std::error::Error for ChelseaError {}

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
    ) -> Result<TradeEvent, ChelseaError> {
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

        let resp = self
            .http
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .map_err(|err| ChelseaError::Rejected(format!("ChelseaAI request failed: {err}")))?;

        // A transport-level response we cannot read may still have been
        // processed: treat it as unknown rather than a clean rejection.
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|err| ChelseaError::Unknown(format!("unparseable ChelseaAI response: {err}")))?;

        if let Some(err) = v.get("error") {
            return Err(CheelseaError::Rejected(format!(
                "ChelseaAI error: {err}"
            )));
        }

        let Some(trade_id) = v["result"]["content"][0]["text"].as_str() else {
            return Err(CheelseaError::Unknown(
                "ChelseaAI response carried no order id — the order may have been placed"
                    .into(),
            ));
        };

        Ok(TradeEvent {
            trade_id: trade_id.to_string(),
            symbol: "XAUUSD".into(),
            side: side.into(),
            size,
            entry: 0.0,
            sl,
            tp,
            status: "open".into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            level_name: None,
            venue: Some("ChelseaLive".into()),
            rr: None,
            current_price: None,
            unrealized_pnl: None,
            closed_at: None,
            intent_id: None,
        })
    }
}
