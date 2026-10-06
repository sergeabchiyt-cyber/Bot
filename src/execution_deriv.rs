use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use crate::config::Config;
use crate::types::TradeEvent;

pub struct DerivExecution {
    pub api_token: String,
    pub app_id: Option<String>,
    pub api_url: String,
    pub http: reqwest::Client,
}

impl DerivExecution {
    pub fn new(config: &Config) -> Self {
        Self {
            api_token: config.deriv_demo_api.clone().unwrap_or_default(),
            app_id: config.deriv_app_id.clone(),
            api_url: config.deriv_api_url.clone(),
            http: reqwest::Client::new(),
        }
    }

    /// Fetch the demo account ID associated with this token.
    async fn get_demo_account_id(&self) -> Result<String> {
        let url = format!("{}/trading/v1/options/accounts", self.api_url);
        let mut req = self.http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_token));
        if let Some(app_id) = &self.app_id {
            req = req.header("Deriv-App-ID", app_id);
        }
        let body: serde_json::Value = req.send().await?.json().await?;

        // Accounts are listed under data[] with account_type == "demo"
        let accounts = body["data"].as_array()
            .ok_or_else(|| anyhow::anyhow!("No accounts in response: {}", body))?;
        let demo = accounts.iter()
            .find(|a| a["account_type"].as_str() == Some("demo")
                   || a["account_id"].as_str().map(|s| s.starts_with("VRTC")).unwrap_or(false))
            .ok_or_else(|| anyhow::anyhow!("No demo account found for this token"))?;

        demo["account_id"].as_str()
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("account_id missing"))
    }

    /// Request an OTP-issued WebSocket URL scoped to the demo account.
    async fn get_otp_url(&self, account_id: &str) -> Result<String> {
        let url = format!(
            "{}/trading/v1/options/accounts/{}/otp",
            self.api_url, account_id
        );
        let mut req = self.http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_token));
        if let Some(app_id) = &self.app_id {
            req = req.header("Deriv-App-ID", app_id);
        }
        let body: serde_json::Value = req.send().await?.json().await?;
        body["data"]["url"].as_str()
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("No OTP URL: {}", body))
    }

    /// Place a CALL (buy) or PUT (sell) contract on XAUUSD.
    pub async fn place_order(
        &self,
        side: &str,
        stake: f64,
        sl: f64,
        tp: f64,
    ) -> Result<TradeEvent> {
        let account_id = self.get_demo_account_id().await?;
        let otp_url = self.get_otp_url(&account_id).await?;

        let (mut ws, _) = tokio_tungstenite::connect_async(&otp_url).await?;

        let contract_type = if side == "buy" { "CALL" } else { "PUT" };

        // Barrier offsets derived from SL/TP distances
        let barrier = match side {
            "buy" => format!("+{:.3}", (tp - sl).abs() / 2.0),
            _     => format!("-{:.3}", (tp - sl).abs() / 2.0),
        };

        // 1. Proposal
        let proposal = json!({
            "proposal": 1,
            "amount": stake,
            "basis": "stake",
            "contract_type": contract_type,
            "currency": "USD",
            "underlying_symbol": "frxXAUUSD",
            "duration": 5,
            "duration_unit": "m",
            "barrier": barrier,
            "req_id": 1
        });
        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            serde_json::to_string(&proposal)?.into()
        )).await?;

        let msg = ws.next().await
            .ok_or_else(|| anyhow::anyhow!("no proposal response"))??;
        let prop: serde_json::Value = serde_json::from_str(msg.to_text().unwrap_or(""))?;
        if let Some(err) = prop.get("error") {
            anyhow::bail!("Deriv proposal error: {}", err);
        }
        let proposal_id = prop["proposal"]["id"].as_str()
            .ok_or_else(|| anyhow::anyhow!("no proposal id"))?;
        let ask_price = prop["proposal"]["ask_price"].as_f64().unwrap_or(stake);

        // 2. Buy
        let buy = json!({
            "buy": proposal_id,
            "price": ask_price,
            "req_id": 2
        });
        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            serde_json::to_string(&buy)?.into()
        )).await?;

        let msg = ws.next().await
            .ok_or_else(|| anyhow::anyhow!("no buy response"))??;
        let buy_resp: serde_json::Value = serde_json::from_str(msg.to_text().unwrap_or(""))?;
        if let Some(err) = buy_resp.get("error") {
            anyhow::bail!("Deriv buy error: {}", err);
        }

        let contract_id = buy_resp["buy"]["contract_id"].as_u64().unwrap_or(0);
        let buy_price = buy_resp["buy"]["buy_price"].as_f64().unwrap_or(0.0);

        let _ = ws.close(None).await;

        Ok(TradeEvent {
            trade_id: contract_id.to_string(),
            symbol: "XAUUSD".into(),
            side: side.into(),
            size: stake,
            entry: buy_price,
            sl,
            tp,
            status: "open".into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }
}