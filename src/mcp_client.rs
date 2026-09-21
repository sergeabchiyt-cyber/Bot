use anyhow::Result;
use serde_json::json;

pub struct McpClient {
    pub browser_url: String,
    pub chelsea_url: String,
    pub http: reqwest::Client,
}

impl McpClient {
    pub fn new(browser_url: String, chelsea_url: String) -> Self {
        Self {
            browser_url,
            chelsea_url,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap(),
        }
    }

    pub async fn goto(&self, url: &str) -> Result<()> {
        let body = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "goto", "arguments": {"url": url}}
        });
        self.http.post(&self.browser_url).json(&body).send().await?;
        Ok(())
    }

    pub async fn markdown(&self) -> Result<String> {
        let body = json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "markdown", "arguments": {}}
        });
        let resp = self.http.post(&self.browser_url).json(&body).send().await?;
        let text = resp.text().await?;
        Ok(text)
    }

    pub async fn scrape_calendar(&self) -> Result<String> {
        self.goto("https://www.forexfactory.com/calendar").await?;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        self.markdown().await
    }

    pub async fn place_order(
        &self,
        side: &str,
        size: f64,
        sl: f64,
        tp: f64,
    ) -> Result<String> {
        if self.chelsea_url.is_empty() {
            return Ok(format!("mock-order-{}", chrono::Utc::now().timestamp()));
        }
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
        let resp = self.http.post(&self.chelsea_url).json(&body).send().await?;
        let v: serde_json::Value = resp.json().await?;
        Ok(v["result"]["content"][0]["text"]
            .as_str().unwrap_or("unknown").to_string())
    }
}