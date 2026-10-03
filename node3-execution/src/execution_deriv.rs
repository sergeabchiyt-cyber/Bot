use std::time::Duration;

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::time::{interval, sleep, timeout};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::diagnostics::DiagnosticsHub;
use crate::types::{DerivOpenContract, TradeEvent};

/// Concrete WebSocket stream type returned by `tokio_tungstenite::connect_async`.
type DerivWsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Hosts serving the legacy Deriv v3 WebSocket API (`/websockets/v3`).
///
/// `ws.derivws.com` frequently answers HTTP 520 (Cloudflare origin error) from
/// cloud/VPS networks, while the alternate hosts may still accept the same
/// handshake — so legacy connections fail over across all of them.
const LEGACY_WS_HOSTS: &[&str] = &["ws.derivws.com", "ws.binaryws.com", "wss.derivws.com"];
/// Official Deriv test `app_id` for the legacy v3 API (see deriv-com/deriv-api).
const LEGACY_TEST_APP_ID: &str = "1089";
/// Browser-like handshake headers. A bare WebSocket client (no `Origin` /
/// `User-Agent`) is far more likely to be rejected by Cloudflare in front of
/// Deriv's legacy endpoints, surfacing as `HTTP 520`.
const DERIV_ORIGIN: &str = "https://app.deriv.com";
const DERIV_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36 node3-execution/1.0";

pub struct DerivExecution {
    pub api_token: String,
    pub app_id: Option<String>,
    pub api_url: String,
    pub ws_url: String,
    pub http: reqwest::Client,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// `pat_...` Personal Access Token. PATs only work with Deriv's current
    /// API (REST + OTP-authenticated WebSocket) and never with the legacy
    /// `authorize` flow — sending one to the legacy endpoint fails.
    Pat,
    /// Anything else (legacy `a1-...` API tokens, OAuth JWTs, ...).
    Other,
}

pub fn token_kind(token: &str) -> TokenKind {
    if token.trim().to_ascii_lowercase().starts_with("pat_") {
        TokenKind::Pat
    } else {
        TokenKind::Other
    }
}

/// Env var holding the API token (demo / virtual account).
pub const DERIV_TOKEN_ENV_VAR: &str = "DERIV_DEMO_API";

/// Env var holding the App ID. PAT (`pat_...`) REST calls are rejected by
/// Deriv with `HTTP 401: Deriv-App-ID header is required for PAT tokens`
/// unless this is set to the App ID of a registered (free) Deriv app.
pub const DERIV_APP_ID_ENV_VAR: &str = "DERIV_APP_ID";

/// Human-readable token shape, reported on `/deriv` and `/diagnostics`.
pub fn token_kind_label(token: Option<&str>) -> &'static str {
    match token {
        Some(t) if token_kind(t) == TokenKind::Pat => "pat",
        Some(t) if !t.trim().is_empty() => "legacy",
        _ => "none",
    }
}

fn is_blank(value: Option<&str>) -> bool {
    value.map(|v| v.trim().is_empty()).unwrap_or(true)
}

/// Setup instructions for the "PAT without `DERIV_APP_ID`" misconfiguration —
/// the exact case Deriv answers with
/// `HTTP 401: Deriv-App-ID header is required for PAT tokens`.
pub fn pat_app_id_setup_hint() -> String {
    format!(
        "{DERIV_TOKEN_ENV_VAR} looks like a Personal Access Token (pat_...) but {DERIV_APP_ID_ENV_VAR} is not set, \
         so Deriv rejects every REST call with `HTTP 401: Deriv-App-ID header is required for PAT tokens`. \
         Register a free app at https://developers.deriv.com (API dashboard) and set {DERIV_APP_ID_ENV_VAR} to its \
         App ID in this service's environment (e.g. Render -> Environment, not only in a local .env), then restart Node 3."
    )
}

/// Actionable configuration hint, or `None` when the Deriv venue looks
/// correctly configured. Used at startup, in the logs and on the `/deriv` and
/// `/diagnostics` endpoints so a misconfiguration is visible without reading
/// the service logs.
pub fn deriv_setup_hint(token: Option<&str>, app_id: Option<&str>) -> Option<String> {
    match token {
        None => Some(format!(
            "{DERIV_TOKEN_ENV_VAR} is not configured — Deriv execution and the live account monitor stay disabled \
             (set {DERIV_TOKEN_ENV_VAR} and, for PAT tokens, {DERIV_APP_ID_ENV_VAR})."
        )),
        Some(_) if token_kind_label(token) == "pat" && is_blank(app_id) => {
            Some(pat_app_id_setup_hint())
        }
        _ => None,
    }
}

#[derive(Debug, Clone)]
struct DemoAccount {
    id: String,
    balance: Option<f64>,
    currency: Option<String>,
}

struct OtpTarget {
    url: String,
    account: DemoAccount,
}

struct LegacySession {
    ws: DerivWsStream,
    loginid: Option<String>,
    balance: Option<f64>,
    currency: Option<String>,
}

fn parse_f64(v: &serde_json::Value) -> Option<f64> {
    if let Some(n) = v.as_f64() {
        return Some(n);
    }
    if let Some(s) = v.as_str() {
        return s.trim().parse::<f64>().ok();
    }
    None
}

fn account_id_of(item: &serde_json::Value) -> Option<String> {
    for key in ["account_id", "loginid", "login_id", "id"] {
        if let Some(s) = item[key].as_str() {
            if !s.trim().is_empty() {
                return Some(s.trim().to_string());
            }
        }
    }
    None
}

fn is_demo_account(item: &serde_json::Value) -> bool {
    let ty = item["account_type"].as_str().unwrap_or("").to_ascii_lowercase();
    let group = item["group"].as_str().unwrap_or("").to_ascii_lowercase();
    if ty == "demo" || group == "demo" {
        return true;
    }
    // Deriv demo/virtual login ids start with VRT (e.g. VRTC1234567).
    if let Some(id) = account_id_of(item) {
        return id.to_ascii_uppercase().starts_with("VRT");
    }
    false
}

fn collect_items(data: &serde_json::Value) -> Vec<&serde_json::Value> {
    if let Some(arr) = data.as_array() {
        arr.iter().collect()
    } else if data.is_object() {
        vec![data]
    } else {
        Vec::new()
    }
}

/// Pick the demo account out of a `GET .../options/accounts` `data` payload,
/// which is normally an array but is a single object in some responses.
fn select_demo_account(data: &serde_json::Value) -> Option<DemoAccount> {
    for item in collect_items(data) {
        if is_demo_account(item) {
            if let Some(id) = account_id_of(item) {
                return Some(DemoAccount {
                    id,
                    balance: parse_f64(&item["balance"]),
                    currency: item["currency"].as_str().map(String::from),
                });
            }
        }
    }
    None
}

fn found_account_ids(data: &serde_json::Value) -> Vec<String> {
    collect_items(data).into_iter().filter_map(account_id_of).collect()
}

/// Render Deriv's REST error shape (`{"errors": [{code, message}]}`) as text.
fn rest_api_error(status: u16, body: &serde_json::Value) -> String {
    if let Some(first) = body
        .get("errors")
        .and_then(|e| e.as_array())
        .and_then(|a| a.first())
    {
        let code = first
            .get("code")
            .and_then(|c| c.as_str())
            .unwrap_or("Error");
        let msg = first
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("request failed");
        return format!("HTTP {status} {code}: {msg}");
    }
    if let Some(msg) = body.get("message").and_then(|m| m.as_str()) {
        return format!("HTTP {status}: {msg}");
    }
    format!("HTTP {status}: {body}")
}

/// Strip the single-use `otp` secret from a WebSocket URL before logging.
pub fn redact_ws_url(url: &str) -> String {
    let mut out = url.to_string();
    if let Some(pos) = out.find("otp=") {
        let end = out[pos..].find('&').map(|i| pos + i).unwrap_or(out.len());
        out.replace_range(pos + 4..end, "***");
    }
    out
}

fn strip_app_id_param(url: &str) -> Option<String> {
    let (head, query) = url.split_once('?')?;
    let parts: Vec<&str> = query.split('&').collect();
    let kept: Vec<&str> = parts
        .iter()
        .copied()
        .filter(|p| !p.starts_with("app_id="))
        .collect();
    if kept.len() == parts.len() {
        return None;
    }
    if kept.is_empty() {
        Some(head.to_string())
    } else {
        Some(format!("{head}?{}", kept.join("&")))
    }
}

fn is_invalid_token_error(code: &str, message: &str) -> bool {
    code.eq_ignore_ascii_case("invalidtoken")
        || message.to_ascii_lowercase().contains("invalid token")
        || message.to_ascii_lowercase().contains("invalidtoken")
}

/// Error text for "the token was rejected by REST but might be a legacy token".
fn is_no_demo_error(err: &anyhow::Error) -> bool {
    err.to_string().contains("No demo account")
}

/// Legacy `a1-...` tokens are rejected (401) by the current REST API, so a
/// REST failure with a non-PAT token is worth retrying on the legacy WS flow.
/// PATs never work on legacy, and "no demo account" must never fall through
/// to a flow that could touch a real-money account.
fn should_fallback_to_legacy(token: &str, err: &anyhow::Error) -> bool {
    if token_kind(token) == TokenKind::Pat {
        return false;
    }
    !is_no_demo_error(err)
}

/// Remedy text for a PAT failure: either "set `DERIV_APP_ID`" when it is
/// missing, or "check its value" when it is already configured.
fn pat_remedy(app_id_configured: bool) -> String {
    if app_id_configured {
        format!(
            "{DERIV_APP_ID_ENV_VAR} is set, so the App ID or the API URL is the problem: it must be the App ID of a \
             registered app from https://developers.deriv.com (API dashboard), and DERIV_API_URL must keep pointing \
             at Deriv's current API (https://api.derivws.com) rather than a legacy wss:// endpoint."
        )
    } else {
        pat_app_id_setup_hint()
    }
}

/// Guidance text for a PAT that failed against Deriv's current API.
fn pat_guidance(err: &anyhow::Error, app_id_configured: bool) -> String {
    let remedy = pat_remedy(app_id_configured);
    format!("Deriv auth failed: {err:#}. {remedy}")
}

impl DerivExecution {
    pub fn new(config: &Config) -> Self {
        // Blank / whitespace-only values must behave exactly like "not set",
        // otherwise the `Deriv-App-ID` header would be sent empty and Deriv
        // rejects the request with a confusing 401.
        let app_id = config
            .deriv_app_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(String::from);
        Self {
            api_token: config.deriv_demo_api.clone().unwrap_or_default(),
            app_id,
            api_url: config.deriv_api_url.clone(),
            ws_url: config.deriv_ws_url(),
            http: reqwest::Client::new(),
        }
    }

    /// True when a usable App ID is configured (PAT tokens need the
    /// `Deriv-App-ID` header on every REST call).
    pub fn app_id_configured(&self) -> bool {
        self.app_id.is_some()
    }

    fn uses_http_otp(&self) -> bool {
        let u = self.api_url.trim();
        u.starts_with("http://") || u.starts_with("https://")
    }

    fn rest_base(&self) -> String {
        self.api_url.trim().trim_end_matches('/').to_string()
    }

    fn rest(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let req = req.header("Authorization", format!("Bearer {}", self.api_token));
        match &self.app_id {
            Some(id) => req.header("Deriv-App-ID", id),
            None => req,
        }
    }

    /// Read a Deriv REST response, surfacing API / Cloudflare errors as text.
    async fn into_rest_json(resp: reqwest::Response, what: &str) -> Result<serde_json::Value> {
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        let body: serde_json::Value =
            serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        if (200..300).contains(&status) {
            return Ok(body);
        }
        if body.is_null() {
            let snippet: String = text.chars().take(300).collect();
            anyhow::bail!("Deriv {what} failed: HTTP {status}: {snippet}");
        }
        anyhow::bail!("Deriv {what} failed: {}", rest_api_error(status, &body));
    }

    async fn get_demo_account(&self) -> Result<DemoAccount> {
        let url = format!("{}/trading/v1/options/accounts", self.rest_base());
        let resp = self.rest(self.http.get(&url)).send().await?;
        let body = Self::into_rest_json(resp, "accounts").await?;

        let data = &body["data"];
        if let Some(account) = select_demo_account(data) {
            return Ok(account);
        }
        let found = found_account_ids(data);
        if found.is_empty() {
            anyhow::bail!(
                "No demo account found for this token (the Deriv API returned no accounts). Make sure the token belongs to a profile with a demo/virtual account."
            );
        }
        anyhow::bail!(
            "No demo account found for this token (found account(s): {}). Node 3 only trades demo accounts and will not touch real-money accounts.",
            found.join(", ")
        );
    }

    async fn get_otp_url(&self, account_id: &str) -> Result<String> {
        let url = format!(
            "{}/trading/v1/options/accounts/{}/otp",
            self.rest_base(),
            account_id
        );
        let resp = self.rest(self.http.post(&url)).send().await?;
        let body = Self::into_rest_json(resp, "OTP").await?;
        let ws_url = body["data"]["url"].as_str().ok_or_else(|| {
            anyhow::anyhow!("Deriv OTP response missing data.url: {body}")
        })?;
        if !(ws_url.starts_with("wss://") || ws_url.starts_with("ws://")) {
            anyhow::bail!("Deriv OTP response returned a non-WebSocket URL: {ws_url}");
        }
        Ok(ws_url.to_string())
    }

    /// REST accounts lookup + single-use OTP WebSocket URL for the demo account.
    async fn fetch_otp_ws(&self) -> Result<OtpTarget> {
        let account = self.get_demo_account().await?;
        let url = self.get_otp_url(&account.id).await?;
        Ok(OtpTarget { url, account })
    }

    /// Ordered legacy `/websockets/v3` candidates: the explicitly configured
    /// URL first, then host failover (Cloudflare 520s are often host-specific),
    /// then `app_id`-less variants for tokens that need none.
    fn legacy_candidates(&self) -> Vec<String> {
        fn with_app_id(base: &str, app_id: &str) -> String {
            if base.contains("app_id=") {
                return base.to_string();
            }
            if base.contains('?') {
                format!("{base}&app_id={app_id}")
            } else {
                format!("{base}?app_id={app_id}")
            }
        }

        let mut out: Vec<String> = Vec::new();
        let mut push = |u: String| {
            if !out.contains(&u) {
                out.push(u);
            }
        };

        let app_id = self.app_id.clone().filter(|s| !s.trim().is_empty());
        let configured_is_ws =
            self.ws_url.starts_with("wss://") || self.ws_url.starts_with("ws://");

        if configured_is_ws {
            push(self.ws_url.clone());
            if app_id.is_none() && !self.ws_url.contains("app_id=") {
                push(with_app_id(&self.ws_url, LEGACY_TEST_APP_ID));
            }
        }
        for host in LEGACY_WS_HOSTS {
            let base = format!("wss://{host}/websockets/v3");
            match &app_id {
                Some(id) => push(with_app_id(&base, id)),
                None => push(with_app_id(&base, LEGACY_TEST_APP_ID)),
            }
        }
        if configured_is_ws {
            if let Some(bare) = strip_app_id_param(&self.ws_url) {
                push(bare);
            }
        }
        if app_id.is_none() {
            for host in LEGACY_WS_HOSTS {
                push(format!("wss://{host}/websockets/v3"));
            }
        }
        out
    }

    /// Build the RFC 6455 client upgrade request for a Deriv WebSocket URL
    /// (OTP or legacy v3), with browser-like headers so Cloudflare in front of
    /// Deriv treats the client as a regular browser session.
    ///
    /// The request **must** be derived from the URL through
    /// [`IntoClientRequest`]: that is the only conversion in tungstenite that
    /// writes `Host`, `Connection: Upgrade`, `Upgrade: websocket`,
    /// `Sec-WebSocket-Version: 13` and the random `Sec-WebSocket-Key`. A
    /// hand-built `http::Request` is forwarded verbatim (the trait impl for
    /// `http::Request<()>` is trivial) and tungstenite's `generate_request`
    /// then rejects it before a single byte leaves the process with
    /// `WebSocket protocol error: Missing, duplicated or incorrect header
    /// sec-websocket-key` — which is exactly what used to break both the OTP
    /// and the legacy sockets.
    fn handshake_request(url: &str) -> Result<http::Request<()>> {
        let mut request = url
            .into_client_request()
            .map_err(|e| anyhow::anyhow!("invalid Deriv WS URL {}: {e}", redact_ws_url(url)))?;
        request.headers_mut().insert(
            http::header::ORIGIN,
            http::HeaderValue::from_static(DERIV_ORIGIN),
        );
        request.headers_mut().insert(
            http::header::USER_AGENT,
            http::HeaderValue::from_static(DERIV_USER_AGENT),
        );
        Ok(request)
    }

    /// WebSocket handshake (OTP or legacy v3) over the request built by
    /// [`Self::handshake_request`].
    async fn connect_ws(url: &str) -> Result<DerivWsStream> {
        let request = Self::handshake_request(url)?;
        let (ws, _) = timeout(
            Duration::from_secs(20),
            tokio_tungstenite::connect_async(request),
        )
        .await
        .map_err(|_| {
            anyhow::anyhow!("timed out connecting to {}", redact_ws_url(url))
        })?
        .map_err(|e| {
            anyhow::anyhow!("handshake failed for {}: {e}", redact_ws_url(url))
        })?;
        Ok(ws)
    }

    /// Read the next JSON text message, answering pings and timing out instead
    /// of hanging forever when Deriv stays silent.
    async fn next_json(ws: &mut DerivWsStream, what: &str, secs: u64) -> Result<serde_json::Value> {
        timeout(Duration::from_secs(secs), async {
            loop {
                match ws.next().await {
                    Some(Ok(Message::Text(text))) => {
                        return serde_json::from_str::<serde_json::Value>(&text).map_err(|e| {
                            anyhow::anyhow!("invalid JSON in Deriv {what} response: {e}")
                        });
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        let _ = ws.send(Message::Pong(payload)).await;
                    }
                    Some(Ok(Message::Close(_))) => {
                        return Err(anyhow::anyhow!("Deriv WS closed while waiting for {what}"));
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        return Err(anyhow::anyhow!(
                            "Deriv WS error while waiting for {what}: {e}"
                        ));
                    }
                    None => {
                        return Err(anyhow::anyhow!(
                            "Deriv WS stream ended while waiting for {what}"
                        ));
                    }
                }
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("timed out waiting for Deriv {what}"))?
    }

    /// Handshake + `authorize` against the legacy v3 API, failing over across
    /// hosts / `app_id` variants. Refuses non-demo accounts outright.
    async fn open_legacy_session(&self) -> Result<LegacySession> {
        let candidates = self.legacy_candidates();
        let mut last_err = String::from("no legacy WebSocket candidates configured");
        for url in &candidates {
            debug!("Trying Deriv legacy endpoint {url}");
            let mut ws = match Self::connect_ws(url).await {
                Ok(ws) => ws,
                Err(e) => {
                    last_err = e.to_string();
                    warn!("Deriv legacy endpoint failed: {last_err}");
                    continue;
                }
            };
            let auth = json!({
                "authorize": self.api_token,
                "req_id": 100
            });
            if let Err(e) = ws.send(Message::Text(auth.to_string().into())).await {
                last_err = format!("{url}: failed to send authorize: {e}");
                continue;
            }
            let resp = match Self::next_json(&mut ws, "authorize", 15).await {
                Ok(v) => v,
                Err(e) => {
                    last_err = format!("{url}: {e}");
                    let _ = ws.close(None).await;
                    continue;
                }
            };
            if let Some(err) = resp.get("error") {
                let code = err
                    .get("code")
                    .and_then(|c| c.as_str())
                    .unwrap_or("");
                let message = err
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("authorize failed");
                if is_invalid_token_error(code, message) {
                    let _ = ws.close(None).await;
                    if token_kind(&self.api_token) == TokenKind::Pat {
                        let remedy = pat_remedy(self.app_id_configured());
                        anyhow::bail!("Deriv rejected the PAT ({code}: {message}). {remedy}");
                    }
                    anyhow::bail!("Deriv rejected the API token ({code}: {message}). Check DERIV_DEMO_API — legacy a1-... tokens are authorized here, while PAT (pat_...) tokens need the default REST API plus DERIV_APP_ID.");
                }
                last_err = format!("{url}: authorize error {code}: {message}");
                warn!("Deriv legacy authorize failed: {last_err}");
                let _ = ws.close(None).await;
                continue;
            }

            let authz = &resp["authorize"];
            let loginid = authz
                .get("loginid")
                .and_then(|v| v.as_str())
                .map(String::from);
            if let Some(ref id) = loginid {
                if !id.to_ascii_uppercase().starts_with("VRT") {
                    let _ = ws.close(None).await;
                    anyhow::bail!("Deriv account {id} is not a demo (VRTC...) account — refusing to use a real-money account. Create a Deriv demo token and set it as DERIV_DEMO_API.");
                }
            }
            info!("Deriv legacy session authorized via {url}");
            return Ok(LegacySession {
                ws,
                loginid,
                balance: authz.get("balance").and_then(|b| b.as_f64()),
                currency: authz.get("currency").and_then(|c| c.as_str()).map(String::from),
            });
        }
        anyhow::bail!(
            "All Deriv legacy endpoints failed (tried {}). Last error: {last_err}. HTTP 520s here are Cloudflare origin errors on Deriv's side that usually clear on retry — the monitor keeps retrying automatically.",
            candidates.len()
        );
    }

    /// Shared `proposal` -> `buy` flow used on both OTP and legacy sockets.
    /// OTP sockets speak the current API (no `symbol` field); legacy sockets
    /// keep the historical payload untouched.
    async fn proposal_buy_flow(
        ws: &mut DerivWsStream,
        side: &str,
        stake: f64,
        sl: f64,
        tp: f64,
        otp_socket: bool,
    ) -> Result<(String, f64)> {
        let contract_type = if side == "buy" { "CALL" } else { "PUT" };
        let barrier = match side {
            "buy" => format!("+{:.3}", (tp - sl).abs() / 2.0),
            _ => format!("-{:.3}", (tp - sl).abs() / 2.0),
        };

        let mut proposal = json!({
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
        if !otp_socket {
            proposal["symbol"] = json!("frxXAUUSD");
        }
        ws.send(Message::Text(proposal.to_string().into()))
            .await
            .map_err(|e| anyhow::anyhow!("failed to send Deriv proposal: {e}"))?;

        let prop = Self::next_json(ws, "proposal", 20).await?;
        if let Some(err) = prop.get("error") {
            anyhow::bail!("Deriv proposal error: {err}");
        }
        let proposal_id = prop["proposal"]["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("no proposal id in Deriv response: {prop}"))?;
        let ask_price = prop["proposal"]["ask_price"].as_f64().unwrap_or(stake);

        let buy = json!({
            "buy": proposal_id,
            "price": ask_price,
            "req_id": 2
        });
        ws.send(Message::Text(buy.to_string().into()))
            .await
            .map_err(|e| anyhow::anyhow!("failed to send Deriv buy: {e}"))?;

        let buy_resp = Self::next_json(ws, "buy", 20).await?;
        if let Some(err) = buy_resp.get("error") {
            anyhow::bail!("Deriv buy error: {err}");
        }

        let contract_id = json_id_to_string(&buy_resp["buy"]["contract_id"])
            .unwrap_or_else(|| "0".to_string());
        let buy_price = buy_resp["buy"]["buy_price"].as_f64().unwrap_or(0.0);
        Ok((contract_id, buy_price))
    }

    fn trade_event(
        contract_id: String,
        buy_price: f64,
        side: &str,
        stake: f64,
        sl: f64,
        tp: f64,
    ) -> TradeEvent {
        TradeEvent {
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
        }
    }

    async fn place_order_otp(
        &self,
        otp_url: &str,
        side: &str,
        stake: f64,
        sl: f64,
        tp: f64,
    ) -> Result<TradeEvent> {
        // OTP URLs are pre-authenticated: no `authorize` call is sent.
        let mut ws = Self::connect_ws(otp_url).await.map_err(|e| {
            anyhow::anyhow!("Deriv OTP WebSocket ({}) failed: {e}", redact_ws_url(otp_url))
        })?;
        let (contract_id, buy_price) =
            Self::proposal_buy_flow(&mut ws, side, stake, sl, tp, true).await?;
        let _ = ws.close(None).await;
        Ok(Self::trade_event(contract_id, buy_price, side, stake, sl, tp))
    }

    pub async fn place_order(
        &self,
        side: &str,
        stake: f64,
        sl: f64,
        tp: f64,
    ) -> Result<TradeEvent> {
        if self.uses_http_otp() {
            match self.fetch_otp_ws().await {
                Ok(target) => {
                    return self
                        .place_order_otp(&target.url, side, stake, sl, tp)
                        .await;
                }
                Err(e) => {
                    if should_fallback_to_legacy(&self.api_token, &e) {
                        warn!("Deriv OTP order flow failed ({e:#}); falling back to legacy WS flow");
                    } else if token_kind(&self.api_token) == TokenKind::Pat {
                        return Err(anyhow::anyhow!("{}", pat_guidance(&e, self.app_id_configured())));
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        let mut session = self.open_legacy_session().await?;
        let (contract_id, buy_price) =
            Self::proposal_buy_flow(&mut session.ws, side, stake, sl, tp, false).await?;
        let _ = session.ws.close(None).await;
        Ok(Self::trade_event(
            contract_id,
            buy_price,
            side,
            stake,
            sl,
            tp,
        ))
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

enum SessionDial {
    /// Pre-authenticated single-use OTP URL from the current REST API.
    Otp(String),
    /// Legacy v3 API with handshake + `authorize` and host failover.
    Legacy,
}

/// Dial one monitor session, subscribe to balance/portfolio/open-contract
/// updates, and run the message loop until the socket drops.
async fn run_monitor_session(executor: &DerivExecution, hub: &DiagnosticsHub, dial: SessionDial) {
    let mut ws = match dial {
        SessionDial::Otp(url) => {
            info!("Connecting to Deriv monitor WS at {}", redact_ws_url(&url));
            match DerivExecution::connect_ws(&url).await {
                Ok(ws) => {
                    info!("Connected to Deriv WebSocket (authenticated OTP session)");
                    hub.update_deriv_status(true, true, None, None, None, None)
                        .await;
                    ws
                }
                Err(e) => {
                    warn!("Deriv OTP WS connection failed: {e:#}");
                    hub.update_deriv_status(
                        false,
                        false,
                        None,
                        None,
                        None,
                        Some(format!("Deriv OTP WS connection failed: {e:#}")),
                    )
                    .await;
                    return;
                }
            }
        }
        SessionDial::Legacy => {
            info!("Connecting to Deriv legacy monitor WS");
            match executor.open_legacy_session().await {
                Ok(session) => {
                    hub.update_deriv_status(
                        true,
                        true,
                        session.loginid,
                        session.balance,
                        session.currency,
                        None,
                    )
                    .await;
                    session.ws
                }
                Err(e) => {
                    warn!("Deriv legacy WS connection failed: {e:#}");
                    hub.update_deriv_status(
                        false,
                        false,
                        None,
                        None,
                        None,
                        Some(format!("Deriv WS connection failed: {e:#}")),
                    )
                    .await;
                    return;
                }
            }
        }
    };

    // Subscribe to live balance updates, open portfolio, and live contract updates.
    // (OTP sessions arrive pre-authenticated; legacy sessions authorized while dialing.)
    let subs = [
        json!({ "balance": 1, "subscribe": 1, "req_id": 2 }),
        json!({ "portfolio": 1, "req_id": 3 }),
        json!({ "proposal_open_contract": 1, "subscribe": 1, "req_id": 4 }),
    ];
    for req in &subs {
        if ws
            .send(Message::Text(req.to_string().into()))
            .await
            .is_err()
        {
            warn!("Deriv WS subscribe send failed; reconnecting");
            hub.update_deriv_status(
                false,
                false,
                None,
                None,
                None,
                Some("Deriv WS disconnected — reconnecting".into()),
            )
            .await;
            return;
        }
    }

    run_message_loop(&mut ws, hub).await;

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

/// Shared balance/portfolio/open-contract message loop for OTP and legacy
/// sockets: application-level pings plus protocol ping/pong.
async fn run_message_loop(ws: &mut DerivWsStream, hub: &DiagnosticsHub) {
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
                                    let loginid = b["loginid"]
                                        .as_str()
                                        .or_else(|| b["account_id"].as_str())
                                        .map(String::from);
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
}

/// Continuously monitors the Deriv Demo account for live balance and open contracts.
///
/// Current REST + OTP API first; legacy-token REST rejections automatically
/// fall back to the legacy WS flow. PAT configuration problems back off longer
/// since they need a settings change, not a retry.
pub async fn spawn_deriv_monitor(config: Config, hub: DiagnosticsHub) {
    let Some(token) = config.deriv_demo_api.clone() else {
        info!("DERIV_DEMO_API not set — Deriv account live monitor disabled");
        return;
    };

    // Surface a missing App ID (PAT tokens) before the first API call, so the
    // fix is visible in the logs instead of only as an HTTP 401 in the loop.
    if let Some(hint) = deriv_setup_hint(Some(&token), config.deriv_app_id.as_deref()) {
        warn!("{hint}");
    }
    info!(
        "Deriv auth config: token kind = {}, {} = {}",
        token_kind_label(Some(&token)),
        DERIV_APP_ID_ENV_VAR,
        if config.deriv_app_id_configured() {
            "set"
        } else {
            "not set"
        }
    );

    let executor = DerivExecution::new(&config);

    loop {
        let config_error = if executor.uses_http_otp() {
            match executor.fetch_otp_ws().await {
                Ok(target) => {
                    // REST already told us the account, balance and currency.
                    hub.update_deriv_status(
                        true,
                        false,
                        Some(target.account.id.clone()),
                        target.account.balance,
                        target.account.currency.clone(),
                        None,
                    )
                    .await;
                    run_monitor_session(&executor, &hub, SessionDial::Otp(target.url)).await;
                    false
                }
                Err(e) => {
                    if token_kind(&token) == TokenKind::Pat {
                        let msg = pat_guidance(&e, executor.app_id_configured());
                        warn!("{msg}");
                        hub.update_deriv_status(false, false, None, None, None, Some(msg))
                            .await;
                        true
                    } else if is_no_demo_error(&e) {
                        let msg = format!("{e:#}");
                        warn!("{msg}");
                        hub.update_deriv_status(false, false, None, None, None, Some(msg))
                            .await;
                        true
                    } else {
                        warn!("Deriv REST/OTP flow failed ({e:#}); falling back to legacy WS flow");
                        run_monitor_session(&executor, &hub, SessionDial::Legacy).await;
                        false
                    }
                }
            }
        } else {
            run_monitor_session(&executor, &hub, SessionDial::Legacy).await;
            false
        };

        sleep(Duration::from_secs(if config_error { 30 } else { 5 })).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_executor(api_url: &str, app_id: Option<&str>) -> DerivExecution {
        let config = Config {
            node1_ws_url: "wss://example.invalid/ws".into(),
            port: 10_000,
            mcp_chelsea_url: None,
            deriv_demo_api: Some("a1-test-token".into()),
            deriv_app_id: app_id.map(String::from),
            deriv_api_url: api_url.into(),
            volume_threshold: 10_500.0,
            sl_min_pips: 200.0,
            sl_max_pips: 300.0,
            tp_min_pips: 600.0,
            tp_max_pips: 800.0,
            rr_min: 2.0,
            rr_max: 3.0,
            order_size: 0.01,
        };
        DerivExecution::new(&config)
    }

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

    #[test]
    fn detects_pat_tokens() {
        assert_eq!(token_kind("pat_abc123"), TokenKind::Pat);
        assert_eq!(token_kind("  PAT_xyz "), TokenKind::Pat);
        assert_eq!(token_kind("a1-AbC123xYz"), TokenKind::Other);
        assert_eq!(token_kind("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig"), TokenKind::Other);
    }

    fn test_config_with_token(token: &str, app_id: Option<&str>) -> Config {
        Config {
            node1_ws_url: "wss://example.invalid/ws".into(),
            port: 10_000,
            mcp_chelsea_url: None,
            deriv_demo_api: Some(token.into()),
            deriv_app_id: app_id.map(String::from),
            deriv_api_url: "https://api.derivws.com".into(),
            volume_threshold: 10_500.0,
            sl_min_pips: 200.0,
            sl_max_pips: 300.0,
            tp_min_pips: 600.0,
            tp_max_pips: 800.0,
            rr_min: 2.0,
            rr_max: 3.0,
            order_size: 0.01,
        }
    }

    #[test]
    fn labels_token_kinds() {
        assert_eq!(token_kind_label(Some("pat_abc")), "pat");
        assert_eq!(token_kind_label(Some("  PAT_abc ")), "pat");
        assert_eq!(token_kind_label(Some("a1-legacy")), "legacy");
        assert_eq!(token_kind_label(Some("   ")), "none");
        assert_eq!(token_kind_label(None), "none");
    }

    #[test]
    fn flags_pat_without_app_id() {
        let hint = deriv_setup_hint(Some("pat_abc123"), None).expect("PAT needs an app id");
        assert!(hint.contains("DERIV_APP_ID"));
        assert!(hint.contains("pat_"));
        assert!(hint.contains("developers.deriv.com"));

        // A configured app id (even padded with spaces) is enough.
        assert!(deriv_setup_hint(Some("pat_abc123"), Some("1089")).is_none());
        assert!(deriv_setup_hint(Some("pat_abc123"), Some("  ")).is_some());

        // Legacy tokens never need an app id.
        assert!(deriv_setup_hint(Some("a1-legacy"), None).is_none());

        // No token at all is reported too.
        let missing = deriv_setup_hint(None, None).expect("missing token hint");
        assert!(missing.contains("DERIV_DEMO_API"));
    }

    #[test]
    fn pat_remedy_distinguishes_missing_from_wrong_app_id() {
        let missing = pat_remedy(false);
        assert!(missing.contains("DERIV_APP_ID"));
        assert!(missing.contains("developers.deriv.com"));

        let set = pat_remedy(true);
        assert!(set.contains("DERIV_APP_ID"));
        assert!(set.contains("DERIV_API_URL"));
    }

    #[test]
    fn pat_guidance_includes_the_underlying_error_and_remedy() {
        let err = anyhow::anyhow!(
            "Deriv accounts failed: HTTP 401: Deriv-App-ID header is required for PAT tokens"
        );
        let guidance = pat_guidance(&err, false);
        assert!(guidance.contains("HTTP 401"));
        assert!(guidance.contains("DERIV_APP_ID"));
        assert!(guidance.contains("DERIV_DEMO_API"));
    }

    #[test]
    fn blank_app_id_is_treated_as_unset() {
        // Whitespace-only DERIV_APP_ID must behave like "not set" so an empty
        // `Deriv-App-ID` header is never sent.
        let blank = DerivExecution::new(&test_config_with_token("pat_abc", Some("   ")));
        assert!(!blank.app_id_configured());
        assert!(blank.app_id.is_none());

        let padded = DerivExecution::new(&test_config_with_token("pat_abc", Some(" 1089 ")));
        assert!(padded.app_id_configured());
        assert_eq!(padded.app_id.as_deref(), Some("1089"));

        let absent = DerivExecution::new(&test_config_with_token("pat_abc", None));
        assert!(!absent.app_id_configured());
    }

    #[test]
    fn legacy_candidates_fail_over_across_hosts() {
        // Default REST config, no app id: 1089-based host failover, then bare URLs.
        let exec = test_executor("https://api.derivws.com", None);
        let c = exec.legacy_candidates();
        assert_eq!(
            c[0],
            "wss://ws.derivws.com/websockets/v3?app_id=1089"
        );
        assert!(c.contains(&"wss://ws.binaryws.com/websockets/v3?app_id=1089".to_string()));
        assert!(c.contains(&"wss://wss.derivws.com/websockets/v3?app_id=1089".to_string()));
        assert!(c.contains(&"wss://ws.derivws.com/websockets/v3".to_string()));
        // No duplicates.
        let mut dedup = c.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(dedup.len(), c.len());
    }

    #[test]
    fn legacy_candidates_honor_configured_url_and_app_id() {
        let exec = test_executor("wss://ws.derivws.com/websockets/v3", Some("4242"));
        let c = exec.legacy_candidates();
        assert_eq!(
            c[0],
            "wss://ws.derivws.com/websockets/v3?app_id=4242"
        );
        assert!(c.contains(&"wss://ws.binaryws.com/websockets/v3?app_id=4242".to_string()));
    }

    #[test]
    fn selects_demo_account_from_rest_payloads() {
        // Array payload with string balance (as returned by the accounts API).
        let data = json!([
            {"account_id": "CR90004580", "balance": 50.0, "currency": "USD", "account_type": "real"},
            {"account_id": "VRTC90004581", "balance": "10000.00", "currency": "USD", "account_type": "demo"}
        ]);
        let demo = select_demo_account(&data).expect("demo selected");
        assert_eq!(demo.id, "VRTC90004581");
        assert_eq!(demo.balance, Some(10000.0));
        assert_eq!(demo.currency.as_deref(), Some("USD"));

        // Single-object payload.
        let single = json!({"account_id": "VRTC1", "balance": 10.0, "currency": "USD", "account_type": "demo"});
        assert_eq!(select_demo_account(&single).unwrap().id, "VRTC1");

        // VRTC prefix alone is enough.
        let prefixed = json!([{"loginid": "VRTC777", "balance": 5.0}]);
        assert_eq!(select_demo_account(&prefixed).unwrap().id, "VRTC777");

        // Real-only payload selects nothing.
        let real_only = json!([{"account_id": "CR1", "account_type": "real"}]);
        assert!(select_demo_account(&real_only).is_none());
        assert_eq!(found_account_ids(&real_only), vec!["CR1".to_string()]);
    }

    #[test]
    fn formats_rest_api_errors() {
        let body = json!({
            "errors": [{"status": 401, "code": "Unauthorized", "message": "Invalid or missing authentication credentials"}],
            "meta": {"endpoint": "/accounts", "method": "GET"}
        });
        assert_eq!(
            rest_api_error(401, &body),
            "HTTP 401 Unauthorized: Invalid or missing authentication credentials"
        );
        assert_eq!(
            rest_api_error(400, &json!({"message": "bad input"})),
            "HTTP 400: bad input"
        );
    }

    #[test]
    fn redacts_and_strips_url_secrets() {
        assert_eq!(
            redact_ws_url("wss://api.derivws.com/trading/v1/options/ws/demo?otp=abc123xyz"),
            "wss://api.derivws.com/trading/v1/options/ws/demo?otp=***"
        );
        assert_eq!(
            redact_ws_url("wss://ws.derivws.com/websockets/v3?app_id=1089"),
            "wss://ws.derivws.com/websockets/v3?app_id=1089"
        );
        assert_eq!(
            strip_app_id_param("wss://ws.derivws.com/websockets/v3?app_id=1089"),
            Some("wss://ws.derivws.com/websockets/v3".to_string())
        );
        assert_eq!(
            strip_app_id_param("wss://h.test/v3?app_id=1&other=2"),
            Some("wss://h.test/v3?other=2".to_string())
        );
        assert_eq!(strip_app_id_param("wss://h.test/v3"), None);
    }

    #[test]
    fn handshake_request_carries_the_required_upgrade_headers() {
        // Regression: a hand-built `http::Request` carrying only `Origin` and
        // `User-Agent` reached tungstenite without `Sec-WebSocket-Key`, so the
        // OTP socket died before the upgrade ever left the process with
        // "Missing, duplicated or incorrect header sec-websocket-key".
        let url = "wss://api.derivws.com/trading/v1/options/ws/demo?otp=abc123xyz";
        let request = DerivExecution::handshake_request(url).expect("handshake request");

        // tungstenite's own generator is what rejected the old request: it must
        // accept this one and emit every RFC 6455 handshake header.
        let (bytes, key) =
            tokio_tungstenite::tungstenite::handshake::client::generate_request(request)
                .expect("tungstenite accepts the Deriv handshake request");
        let text = String::from_utf8(bytes).expect("handshake request is ASCII");
        let lowercase = text.to_lowercase();

        assert!(
            text.starts_with("GET /trading/v1/options/ws/demo?otp=abc123xyz HTTP/1.1\r\n"),
            "unexpected request line:\n{text}"
        );
        assert!(!key.is_empty(), "missing Sec-WebSocket-Key");
        let key_header = format!("sec-websocket-key: {}", key.to_lowercase());
        for header in [
            "host: api.derivws.com",
            "connection: upgrade",
            "upgrade: websocket",
            "sec-websocket-version: 13",
            key_header.as_str(),
            "origin: https://app.deriv.com",
            "user-agent: mozilla/5.0",
        ] {
            assert!(lowercase.contains(header), "missing `{header}` in:\n{text}");
        }
    }

    #[test]
    fn legacy_fallback_rules() {
        let auth_err = anyhow::anyhow!("Deriv accounts failed: HTTP 401 Unauthorized: nope");
        assert!(should_fallback_to_legacy("a1-legacy", &auth_err));
        assert!(!should_fallback_to_legacy("pat_abc", &auth_err));

        let no_demo = anyhow::anyhow!("No demo account found for this token (found account(s): CR1).");
        assert!(!should_fallback_to_legacy("a1-legacy", &no_demo));

        assert!(is_invalid_token_error("InvalidToken", "whatever"));
        assert!(is_invalid_token_error("", "The token is invalid token xyz"));
        assert!(!is_invalid_token_error("InputValidationFailed", "app_id is required"));
    }
}
