use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tracing::{info, warn};

use crate::status::FeedStatus;

/// Browser tools we look for in tools/list (in priority order).
const NAVIGATE_CANDIDATES: &[&str] = &[
    "browser_navigate", // official @playwright/mcp
    "navigate",
    "goto",
    "browser_visit",
    "playwright_navigate",
];

const READ_CANDIDATES: &[&str] = &[
    "browser_snapshot", // official @playwright/mcp
    "markdown",
    "get_markdown",
    "browser_get_visible_text",
    "playwright_get_visible_text",
    "snapshot",
    "browser_evaluate",
];

/// Minimal MCP (Model Context Protocol) client over the Streamable HTTP
/// transport. Implements the required lifecycle so `tools/call` actually
/// works against spec-compliant servers (Playwright MCP, Browserbase, ...):
///
///   1. POST initialize (protocolVersion 2025-06-18) → result + Mcp-Session-Id
///   2. POST notifications/initialized
///   3. POST tools/list → discover the server's actual tool names
///   4. POST tools/call → parse result.content[].text
///
/// Responses are accepted as plain JSON **or** as `text/event-stream`
/// (the spec lets servers answer either way).
pub struct McpClient {
    pub url: String,
    token: Option<String>,
    http: reqwest::Client,
    session: Mutex<Option<String>>,
    next_id: AtomicU64,
}

impl McpClient {
    pub fn new(url: String, token: Option<String>) -> Self {
        Self {
            url,
            token,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .unwrap(),
            session: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    /// POST one JSON-RPC message. Returns the parsed response message,
    /// or None for empty/202 responses (notifications) and SSE streams
    /// that did not carry the matching reply.
    async fn post(&self, body: &Value) -> Result<Option<Value>> {
        let mut req = self
            .http
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(body);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        if let Some(sid) = self.session.lock().unwrap().clone() {
            req = req.header("Mcp-Session-Id", sid);
        }

        let resp = req.send().await.context("MCP request failed")?;

        if let Some(sid) = resp
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
        {
            *self.session.lock().unwrap() = Some(sid.to_string());
        }

        let status = resp.status();
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let text = resp.text().await?;

        if !status.is_success() {
            bail!("MCP HTTP {status}: {}", truncate(&text, 300));
        }
        if text.trim().is_empty() {
            return Ok(None); // notification ack (202)
        }

        if content_type.contains("text/event-stream") {
            // SSE: take the data: line whose JSON id matches our request id.
            for line in text.lines() {
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data.is_empty() {
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<Value>(data) {
                    if body.get("id").is_some() && v.get("id") == body.get("id") {
                        return Ok(Some(v));
                    }
                }
            }
            return Ok(None);
        }

        Ok(Some(serde_json::from_str(&text)?))
    }

    /// Lifecycle step 1+2. Safe to call once per scrape cycle — keeps the
    /// client stateless-friendly and recovers from server restarts.
    pub async fn initialize(&self) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "xauusd-engine", "version": env!("CARGO_PKG_VERSION")}
            }
        });
        let resp = self
            .post(&body)
            .await?
            .ok_or_else(|| anyhow!("empty initialize response"))?;
        if let Some(err) = resp.get("error") {
            bail!("MCP initialize error: {err}");
        }
        let result = resp.get("result").cloned().unwrap_or(Value::Null);
        info!(
            "MCP initialized: server={:?}",
            result
                .get("serverInfo")
                .and_then(|s| s.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
        );

        // notifications/initialized (no id → typically a 202 empty reply)
        let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        if let Err(e) = self.post(&note).await {
            warn!("MCP initialized notification failed (continuing): {e}");
        }
        Ok(result)
    }

    /// Lifecycle step 3 — tool discovery.
    pub async fn list_tools(&self) -> Result<Vec<String>> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let body = json!({"jsonrpc": "2.0", "id": id, "method": "tools/list", "params": {}});
        let resp = self
            .post(&body)
            .await?
            .ok_or_else(|| anyhow!("empty tools/list response"))?;
        if let Some(err) = resp.get("error") {
            bail!("MCP tools/list error: {err}");
        }
        let mut names = Vec::new();
        if let Some(tools) = resp
            .pointer("/result/tools")
            .and_then(|t| t.as_array())
        {
            for t in tools {
                if let Some(name) = t.get("name").and_then(|n| n.as_str()) {
                    names.push(name.to_string());
                }
            }
        }
        Ok(names)
    }

    /// Pick a tool by trying env override first, then the candidate list
    /// against the server's actual tools/list output.
    pub async fn find_tool(&self, candidates: &[&str], env_key: &str) -> Option<String> {
        if let Ok(name) = std::env::var(env_key) {
            if !name.trim().is_empty() {
                return Some(name.trim().to_string());
            }
        }
        let tools = match self.list_tools().await {
            Ok(t) => t,
            Err(e) => {
                warn!("MCP tools/list failed: {e}");
                return None;
            }
        };
        info!("MCP server offers {} tools: {:?}", tools.len(), tools);
        candidates
            .iter()
            .find_map(|c| tools.iter().find(|t| t.eq_ignore_ascii_case(c)).cloned())
    }

    /// Lifecycle step 4 — invoke a tool and return the concatenated text
    /// content of the result.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        });
        let resp = self
            .post(&body)
            .await?
            .ok_or_else(|| anyhow!("empty tools/call response"))?;
        if let Some(err) = resp.get("error") {
            bail!("MCP tools/call error for '{name}': {err}");
        }
        let result = resp
            .get("result")
            .ok_or_else(|| anyhow!("tools/call response missing result"))?;
        if result.get("isError").and_then(|e| e.as_bool()) == Some(true) {
            bail!(
                "tool '{name}' returned an error: {}",
                extract_text(result).unwrap_or_else(|| "unknown".into())
            );
        }
        extract_text(result).ok_or_else(|| anyhow!("tools/call result has no text content"))
    }

    /// Navigate the remote browser to the ForexFactory calendar and pull
    /// the page content back through the MCP browser server.
    pub async fn scrape_calendar(&self, status: &FeedStatus) -> Result<String> {
        status.set("mcp_browser", "connecting");
        match self.scrape_calendar_inner().await {
            Ok(text) => {
                status.set("mcp_browser", "connected");
                Ok(text)
            }
            Err(e) => {
                status.set("mcp_browser", "error");
                Err(e)
            }
        }
    }

    async fn scrape_calendar_inner(&self) -> Result<String> {
        self.initialize().await.context("MCP initialize")?;

        let nav = self
            .find_tool(NAVIGATE_CANDIDATES, "MCP_BROWSER_TOOL_NAVIGATE")
            .await
            .ok_or_else(|| anyhow!("no navigation tool on browser MCP server"))?;
        let read = self
            .find_tool(READ_CANDIDATES, "MCP_BROWSER_TOOL_READ")
            .await
            .ok_or_else(|| anyhow!("no page-content tool on browser MCP server"))?;
        info!("MCP calendar scrape using tools: {nav} + {read}");

        self.call_tool(&nav, json!({"url": "https://www.forexfactory.com/calendar"}))
            .await
            .context("browser_navigate")?;
        // Give the SPA a moment to render the table.
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        self.call_tool(&read, json!({})).await.context("page read")
    }
}

fn extract_text(result: &Value) -> Option<String> {
    let content = result.get("content")?.as_array()?;
    let mut out = String::new();
    for item in content {
        if item.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                out.push_str(text);
                out.push('\n');
            }
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

fn truncate(s: &str, n: usize) -> &str {
    match s.get(..n) {
        Some(prefix) => prefix,
        None => s,
    }
}
