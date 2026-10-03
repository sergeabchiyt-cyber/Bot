use std::time::Duration;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::time::{interval, sleep};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::diagnostics::DiagnosticsHub;
use crate::types::{DerivOpenContract, TradeEvent};

pub struct DerivExecution {
    pub api_token: String,
    pub app_id: Option<String>,
    pub api_url: String,
    pub ws_url: String,
    pub http: reqwest::Client,
}

impl DerivExecution {
    pub fn new(config: &Config) -> Self {
        Self {
            api_token: config.deriv_demo_api.clone().unwrap_or_default(),
            app_id: config.deriv_app_id.clone(),
            api_url: config.deriv_api_url.clone(),
            ws_url: config.deriv_ws_url(),
            http: reqwest::Client::new(),
        }
    }

    fn uses_http_otp(&self) -> bool {
        let u = self.api_url.trim();
        u.starts_with("http://") || u.starts_with("https://")
    }

    async fn get_demo_account_id(&self) -> Result<String> {
        let url = format!("{}/trading/v1/options/accounts", self.api_url);
        let mut req = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_token));
        if let Some(app_id) = &self.app_id {
            req = req.header("Deriv-App-ID", app_id);
        }
        let body: serde_json::Value = req.send().await?.json().await?;

        let accounts = body["data"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("No accounts in response: {}", body))?;
        let demo = accounts
            .iter()
            .find(|a| {
                a["account_type"].as_str() == Some("demo")
                    || a["account_id"]
                        .as_str()
                        .map(|s| s.starts_with("VRTC"))
                        .unwrap_or(false)
            })
            .ok_or_else(|| anyhow::anyhow!("No demo account found for this token"))?;

        demo["account_id"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("account_id missing"))
    }

    async fn get_otp_url(&self, account_id: &str) -> Result<String> {
        let url = format!(
            "{}/trading/v1/options/accounts/{}/otp",
            self.api_url, account_id
        );
        let mut req = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_token));
        if let Some(app_id) = &self.app_id {
            req = req.header("Deriv-App-ID", app_id);
        }
        let body: serde_json::Value = req.send().await?.json().await?;
        body["data"]["url"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("No OTP URL: {}", body))
    }

    pub async fn place_order(
        &self,
        side: &str,
        stake: f64,
        sl: f64,
        tp: f64,
    ) -> Result<TradeEvent> {
        let target_ws_url = if self.uses_http_otp() {
            let account_id = self.get_demo_account_id().await?;
            self.get_otp_url(&account_id).await?
        } else {
            self.ws_url.clone()
        };

        let (mut ws, _) = tokio_tungstenite::connect_async(&target_ws_url).await?;

        // Direct Deriv WS v3 connections require an explicit `authorize` call first.
        if !self.uses_http_otp() {
            let auth = json!({
                "authorize": self.api_token,
                "req_id": 100
            });
            ws.send(Message::Text(serde_json::to_string(&auth)?.into()))
                .await?;
            let msg = ws
                .next()
                .await
                .ok_or_else(|| anyhow::anyhow!("no authorize response from Deriv"))??;
            let auth_resp: serde_json::Value = serde_json::from_str(msg.to_text().unwrap_or(""))?;
            if let Some(err) = auth_resp.get("error") {
                anyhow::bail!("Deriv authorize error: {}", err);
            }
        }

        let contract_type = if side == "buy" { "CALL" } else { "PUT" };
        let barrier = match side {
            "buy" => format!("+{:.3}", (tp - sl).abs() / 2.0),
            _ => format!("-{:.3}", (tp - sl).abs() / 2.0),
        };

        let proposal = json!({
            "proposal": 1,
            "amount": stake,
            "basis": "stake",
            "contract_type": contract_type,
            "currency": "USD",
            "underlying_symbol": "frxXAUUSD",
            "symbol": "frxXAUUSD",
            "duration": 5,
            "duration_unit": "m",
            "barrier": barrier,
            "req_id": 1
        });
        ws.send(Message::Text(serde_json::to_string(&proposal)?.into()))
            .await?;

        let msg = ws
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("no proposal response"))??;
        let prop: serde_json::Value = serde_json::from_str(msg.to_text().unwrap_or(""))?;
        if let Some(err) = prop.get("error") {
            anyhow::bail!("Deriv proposal error: {}", err);
        }
        let proposal_id = prop["proposal"]["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("no proposal id"))?;
        let ask_price = prop["proposal"]["ask_price"].as_f64().unwrap_or(stake);

        let buy = json!({
            "buy": proposal_id,
            "price": ask_price,
            "req_id": 2
        });
        ws.send(Message::Text(serde_json::to_string(&buy)?.into()))
            .await?;

        let msg = ws
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("no buy response"))??;
        let buy_resp: serde_json::Value = serde_json::from_str(msg.to_text().unwrap_or(""))?;
        if let Some(err) = buy_resp.get("error") {
            anyhow::bail!("Deriv buy error: {}", err);
        }

        let contract_id = json_id_to_string(&buy_resp["buy"]["contract_id"])
            .unwrap_or_else(|| "0".to_string());
        let buy_price = buy_resp["buy"]["buy_price"].as_f64().unwrap_or(0.0);

        let _ = ws.close(None).await;

        Ok(TradeEvent {
            trade_id: contract_id,
            symbol: "XAUUSD".into(),
            side: side.into(),
            size: stake,
            entry: buy_price,
            sl,
            tp,
            status: "open".into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            level_name: None,
            venue: Some("DerivDemo".into()),
            rr: None,
            current_price: None,
            unrealized_pnl: Some(0.0),
            closed_at: None,
        })
    }
}

pub fn json_id_to_string(v: &serde_json::Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    if let Some(u) = v.as_u64() {
        return Some(u.to_string());
    }
    if let Some(i) = v.as_i64() {
        return Some(i.to_string());
    }
    None
}

fn normalize_epoch_ms(raw: i64) -> i64 {
    if raw > 0 && raw < 10_000_000_000 {
        raw * 1000
    } else {
        raw
    }
}

fn normalize_display_symbol(symbol: &str) -> String {
    if let Some(stripped) = symbol.strip_prefix("frx") {
        stripped.to_string()
    } else {
        symbol.to_string()
    }
}

fn contract_type_to_side(contract_type: &str) -> String {
    let upper = contract_type.to_ascii_uppercase();
    if upper.contains("PUT") || upper.contains("DOWN") || upper.contains("SELL") {
        "sell".to_string()
    } else {
        "buy".to_string()
    }
}

pub fn parse_portfolio_contracts(body: &serde_json::Value) -> Vec<DerivOpenContract> {
    let Some(arr) = body["portfolio"]["contracts"].as_array() else {
        return Vec::new();
    };
    let now = chrono::Utc::now().timestamp_millis();
    let mut out = Vec::with_capacity(arr.len());

    for item in arr {
        let Some(contract_id) = json_id_to_string(&item["contract_id"]) else {
            continue;
        };
        let symbol = item["symbol"]
            .as_str()
            .or_else(|| item["underlying"].as_str())
            .unwrap_or("frxXAUUSD")
            .to_string();
        let display_symbol = normalize_display_symbol(&symbol);
        let contract_type = item["contract_type"].as_str().unwrap_or("CALL").to_string();
        let side = contract_type_to_side(&contract_type);
        let buy_price = item["buy_price"].as_f64().unwrap_or(0.0);
        let bid_price = item["bid_price"].as_f64().unwrap_or(buy_price);
        let payout = item["payout"].as_f64().unwrap_or(0.0);
        let profit = item["profit"].as_f64().unwrap_or(bid_price - buy_price);
        let profit_pct = if buy_price > 0.0 {
            (profit / buy_price) * 100.0
        } else {
            0.0
        };
        let currency = item["currency"].as_str().unwrap_or("USD").to_string();
        let date_start = item["date_start"]
            .as_i64()
            .map(normalize_epoch_ms)
            .unwrap_or(now);
        let date_expiry = item["expiry_time"]
            .as_i64()
            .or_else(|| item["date_expiry"].as_i64())
            .map(normalize_epoch_ms);
        let longcode = item["longcode"].as_str().map(String::from);

        out.push(DerivOpenContract {
            contract_id,
            symbol,
            display_symbol,
            contract_type,
            side,
            buy_price,
            bid_price,
            payout,
            entry_spot: item["entry_spot"].as_f64(),
            current_spot: item["current_spot"].as_f64(),
            barrier: item["barrier"].as_str().map(String::from),
            profit,
            profit_pct,
            currency,
            date_start,
            date_expiry,
            status: "open".into(),
            longcode,
        });
    }

    out
}

pub fn parse_open_contract(body: &serde_json::Value) -> Option<(DerivOpenContract, bool)> {
    let poc = body.get("proposal_open_contract")?;
    let contract_id = json_id_to_string(&poc["contract_id"])?;

    let symbol = poc["underlying"]
        .as_str()
        .or_else(|| poc["symbol"].as_str())
        .unwrap_or("frxXAUUSD")
        .to_string();
    let display_symbol = normalize_display_symbol(&symbol);
    let contract_type = poc["contract_type"].as_str().unwrap_or("CALL").to_string();
    let side = contract_type_to_side(&contract_type);
    let buy_price = poc["buy_price"].as_f64().unwrap_or(0.0);
    let bid_price = poc["bid_price"].as_f64().unwrap_or(buy_price);
    let payout = poc["payout"].as_f64().unwrap_or(0.0);
    let entry_spot = poc["entry_spot"]
        .as_f64()
        .or_else(|| poc["entry_tick"].as_f64());
    let current_spot = poc["current_spot"].as_f64();
    let barrier = poc["barrier"].as_str().map(String::from);
    let profit = poc["profit"].as_f64().unwrap_or(bid_price - buy_price);
    let profit_pct = poc["profit_percentage"].as_f64().unwrap_or_else(|| {
        if buy_price > 0.0 {
            (profit / buy_price) * 100.0
        } else {
            0.0
        }
    });
    let currency = poc["currency"].as_str().unwrap_or("USD").to_string();
    let date_start = poc["date_start"]
        .as_i64()
        .map(normalize_epoch_ms)
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    let date_expiry = poc["date_expiry"]
        .as_i64()
        .or_else(|| poc["expiry_time"].as_i64())
        .map(normalize_epoch_ms);
    let status = poc["status"].as_str().unwrap_or("open").to_string();
    let is_sold = poc["is_sold"].as_i64() == Some(1)
        || poc["is_sold"].as_bool() == Some(true)
        || status != "open";
    let longcode = poc["longcode"].as_str().map(String::from);

    Some((
        DerivOpenContract {
            contract_id,
            symbol,
            display_symbol,
            contract_type,
            side,
            buy_price,
            bid_price,
            payout,
            entry_spot,
            current_spot,
            barrier,
            profit,
            profit_pct,
            currency,
            date_start,
            date_expiry,
            status,
            longcode,
        },
        is_sold,
    ))
}

/// Continuously monitors the Deriv Demo account for live balance and open contracts.
pub async fn spawn_deriv_monitor(config: Config, hub: DiagnosticsHub) {
    let Some(token) = config.deriv_demo_api.clone() else {
        info!("DERIV_DEMO_API not set — Deriv account live monitor disabled");
        return;
    };

    let executor = DerivExecution::new(&config);

    loop {
        let ws_target = if executor.uses_http_otp() {
            match executor.get_demo_account_id().await {
                Ok(acc_id) => match executor.get_otp_url(&acc_id).await {
                    Ok(url) => url,
                    Err(e) => {
                        warn!("Failed to get Deriv OTP URL: {e}");
                        hub.update_deriv_status(
                            false,
                            false,
                            Some(acc_id),
                            None,
                            None,
                            Some(format!("OTP error: {e}")),
                        )
                        .await;
                        sleep(Duration::from_secs(10)).await;
                        continue;
                    }
                },
                Err(e) => {
                    warn!("Failed to fetch Deriv demo account ID: {e}");
                    hub.update_deriv_status(
                        false,
                        false,
                        None,
                        None,
                        None,
                        Some(format!("Account lookup error: {e}")),
                    )
                    .await;
                    sleep(Duration::from_secs(10)).await;
                    continue;
                }
            }
        } else {
            config.deriv_ws_url()
        };

        info!("Connecting to Deriv Demo monitor WS at {}", ws_target);
        match tokio_tungstenite::connect_async(&ws_target).await {
            Ok((mut ws, _)) => {
                info!("Connected to Deriv WebSocket");
                hub.update_deriv_status(true, false, None, None, None, None)
                    .await;

                // Authorize session and subscribe to balance + open contracts
                let auth_msg = json!({
                    "authorize": token,
                    "req_id": 1
                });
                if let Err(e) = ws.send(Message::Text(auth_msg.to_string().into())).await {
                    warn!("Deriv WS send authorize failed: {e}");
                    sleep(Duration::from_secs(5)).await;
                    continue;
                }

                let mut ping_timer = interval(Duration::from_secs(25));
                // Consume initial immediate tick
                ping_timer.tick().await;

                loop {
                    tokio::select! {
                        _ = ping_timer.tick() => {
                            let ping = json!({ "ping": 1 });
                            if ws.send(Message::Text(ping.to_string().into())).await.is_err() {
                                break;
                            }
                            let portfolio_req = json!({ "portfolio": 1 });
                            let _ = ws.send(Message::Text(portfolio_req.to_string().into())).await;
                        }
                        msg = ws.next() => {
                            let Some(msg) = msg else {
                                warn!("Deriv WS stream ended");
                                break;
                            };
                            match msg {
                                Ok(Message::Text(text)) => {
                                    let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) else {
                                        continue;
                                    };
                                    let msg_type = val["msg_type"].as_str().unwrap_or("");

                                    if let Some(err) = val.get("error") {
                                        let err_msg = err["message"]
                                            .as_str()
                                            .unwrap_or("Deriv API error")
                                            .to_string();
                                        warn!("Deriv API error on {msg_type}: {err_msg}");
                                        if msg_type == "authorize" {
                                            hub.update_deriv_status(
                                                true,
                                                false,
                                                None,
                                                None,
                                                None,
                                                Some(err_msg),
                                            )
                                            .await;
                                            break;
                                        }
                                        continue;
                                    }

                                    match msg_type {
                                        "authorize" => {
                                            let auth = &val["authorize"];
                                            let loginid = auth["loginid"].as_str().map(String::from);
                                            let balance = auth["balance"].as_f64();
                                            let currency = auth["currency"].as_str().map(String::from);

                                            hub.update_deriv_status(
                                                true,
                                                true,
                                                loginid,
                                                balance,
                                                currency,
                                                None,
                                            )
                                            .await;

                                            // Subscribe to live balance updates, open portfolio, and live contract updates
                                            let sub_balance = json!({ "balance": 1, "subscribe": 1, "req_id": 2 });
                                            let req_portfolio = json!({ "portfolio": 1, "req_id": 3 });
                                            let sub_poc = json!({ "proposal_open_contract": 1, "subscribe": 1, "req_id": 4 });

                                            let _ = ws.send(Message::Text(sub_balance.to_string().into())).await;
                                            let _ = ws.send(Message::Text(req_portfolio.to_string().into())).await;
                                            let _ = ws.send(Message::Text(sub_poc.to_string().into())).await;
                                        }
                                        "balance" => {
                                            let b = &val["balance"];
                                            if let Some(bal) = b["balance"].as_f64() {
                                                let currency = b["currency"].as_str().map(String::from);
                                                let loginid = b["loginid"].as_str().map(String::from);
                                                debug!("Deriv balance update: {bal}");
                                                hub.update_deriv_balance(bal, currency, loginid).await;
                                            }
                                        }
                                        "portfolio" => {
                                            let contracts = parse_portfolio_contracts(&val);
                                            for c in &contracts {
                                                if let Ok(cid) = c.contract_id.parse::<u64>() {
                                                    let sub_one = json!({
                                                        "proposal_open_contract": 1,
                                                        "contract_id": cid,
                                                        "subscribe": 1
                                                    });
                                                    let _ = ws.send(Message::Text(sub_one.to_string().into())).await;
                                                }
                                            }
                                            hub.update_deriv_portfolio(contracts).await;
                                        }
                                        "proposal_open_contract" => {
                                            if let Some((contract, is_closed)) = parse_open_contract(&val) {
                                                hub.upsert_deriv_contract(contract, is_closed).await;
                                                if is_closed {
                                                    let req_portfolio = json!({ "portfolio": 1 });
                                                    let _ = ws.send(Message::Text(req_portfolio.to_string().into())).await;
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                Ok(Message::Ping(p)) => {
                                    let _ = ws.send(Message::Pong(p)).await;
                                }
                                Ok(Message::Close(_)) => {
                                    warn!("Deriv WS closed by remote");
                                    break;
                                }
                                Err(e) => {
                                    warn!("Deriv WS error: {e}");
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                }

                hub.update_deriv_status(
                    false,
                    false,
                    None,
                    None,
                    None,
                    Some("Deriv WS disconnected — reconnecting".into()),
                )
                .await;
            }
            Err(e) => {
                warn!("Failed to connect to Deriv WS: {e}");
                hub.update_deriv_status(
                    false,
                    false,
                    None,
                    None,
                    None,
                    Some(format!("Deriv WS connection failed: {e}")),
                )
                .await;
            }
        }

        sleep(Duration::from_secs(5)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_portfolio_and_open_contract_messages() {
        let port = json!({
            "msg_type": "portfolio",
            "portfolio": {
                "contracts": [
                    {
                        "contract_id": 99887766_u64,
                        "symbol": "frxXAUUSD",
                        "contract_type": "CALL",
                        "buy_price": 10.0,
                        "payout": 19.5,
                        "currency": "USD",
                        "date_start": 1_700_000_000_i64,
                        "expiry_time": 1_700_000_300_i64
                    }
                ]
            }
        });
        let contracts = parse_portfolio_contracts(&port);
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].contract_id, "99887766");
        assert_eq!(contracts[0].display_symbol, "XAUUSD");
        assert_eq!(contracts[0].side, "buy");
        assert_eq!(contracts[0].buy_price, 10.0);
        assert_eq!(contracts[0].date_start, 1_700_000_000_000);

        let poc = json!({
            "msg_type": "proposal_open_contract",
            "proposal_open_contract": {
                "contract_id": 99887766_u64,
                "underlying": "frxXAUUSD",
                "contract_type": "PUT",
                "buy_price": 10.0,
                "bid_price": 14.5,
                "payout": 19.5,
                "entry_spot": 2650.25,
                "current_spot": 2648.10,
                "profit": 4.5,
                "profit_percentage": 45.0,
                "currency": "USD",
                "date_start": 1_700_000_000_i64,
                "date_expiry": 1_700_000_300_i64,
                "status": "open",
                "is_sold": 0
            }
        });
        let (parsed, is_closed) = parse_open_contract(&poc).expect("parse open contract");
        assert!(!is_closed);
        assert_eq!(parsed.side, "sell");
        assert_eq!(parsed.profit, 4.5);
        assert_eq!(parsed.profit_pct, 45.0);
        assert_eq!(parsed.current_spot, Some(2648.10));
    }
}
