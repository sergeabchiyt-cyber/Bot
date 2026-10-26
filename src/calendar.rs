//! Economic calendar.
//!
//! Fetches normalized macroeconomic events over three tiers:
//!
//! 1. **Direct weekly JSON** (`FF_WEEKLY_JSON` or `CALENDAR_URL`) — keyless
//!    HTTPS export normalized into [`CalendarEvent`] rows.
//! 2. **SiftingIO economic calendar** (`/v1/fnd/economic-calendar`) —
//!    authenticated first-party API fallback using `SIFTING_API_KEY` when
//!    shared cloud egress IPs are rate-limited or blocked by Cloudflare on the
//!    direct feed.
//! 3. **Browser MCP** (`McpClient`) — optional external browser fallback, only
//!    attempted when explicitly configured (`MCP_BROWSER_URL` /
//!    `CALENDAR_USE_MCP=1`).

use std::time::Duration as StdDuration;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use reqwest::StatusCode;
use reqwest::header::{ACCEPT, HeaderMap, RETRY_AFTER};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::mcp_client::McpClient;
use crate::status::FeedStatus;

/// ForexFactory's official weekly export (same data as the calendar page).
/// Note: the historical `cdn-nfs.faireconomy.media` hostname no longer resolves
/// in DNS (`NXDOMAIN`) and has been removed so fallbacks reach live sources.
pub const FF_WEEKLY_JSON: &str = "https://nfs.faireconomy.media/ff_calendar_thisweek.json";

const CALENDAR_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const MAX_HTTP_RETRIES: usize = 2;
const MAX_RETRY_WAIT_SECS: f64 = 5.0;

/// One normalized calendar event, broadcast as part of the `calendar` frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarEvent {
    /// Event title, e.g. "Core CPI m/m".
    pub event: String,
    /// ISO currency code, e.g. "USD".
    pub currency: String,
    /// "High" | "Medium" | "Low" | "Holiday" | "Unknown".
    pub impact: String,
    /// Event time in UTC, RFC3339. None for all-day / tentative events.
    pub time: Option<String>,
    /// Epoch millis for the event, when it has a concrete time.
    pub timestamp: Option<i64>,
    pub actual: Option<String>,
    pub forecast: Option<String>,
    pub previous: Option<String>,
    /// True when this event can move gold: USD (or the metal itself) + high impact.
    pub gold_relevant: bool,
}

/// Raw shape of the ForexFactory weekly JSON feed.
#[derive(Debug, Deserialize)]
struct FfRow {
    #[serde(default)]
    title: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    impact: String,
    #[serde(default)]
    actual: Option<String>,
    #[serde(default)]
    forecast: Option<String>,
    #[serde(default)]
    previous: Option<String>,
}

/// Raw shape of SiftingIO's `GET /v1/fnd/economic-calendar` response.
#[derive(Debug, Deserialize)]
struct SiftingCalendarEnvelope {
    #[serde(default, alias = "data")]
    events: Vec<SiftingCalendarRow>,
}

#[derive(Debug, Deserialize)]
struct SiftingCalendarRow {
    #[serde(default, alias = "title")]
    name: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    currency: String,
    #[serde(default)]
    impact: String,
    #[serde(default, alias = "date")]
    scheduled_at: String,
    #[serde(default)]
    actual: Option<serde_json::Value>,
    #[serde(default, alias = "forecast")]
    consensus: Option<serde_json::Value>,
    #[serde(default)]
    previous: Option<serde_json::Value>,
}

fn blank_to_none(v: Option<String>) -> Option<String> {
    v.filter(|s| !s.trim().is_empty())
}

fn json_scalar_to_opt_string(v: Option<serde_json::Value>) -> Option<String> {
    match v? {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => blank_to_none(Some(s)),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn normalize_impact(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "high" | "red" => "High",
        "medium" | "moderate" | "orange" => "Medium",
        "low" | "yellow" => "Low",
        "holiday" | "non-economic" | "grey" | "gray" => "Holiday",
        "" => "Unknown",
        _ => return raw.trim().to_string(),
    }
    .to_string()
}

fn country_to_currency(country: &str) -> String {
    match country.trim().to_ascii_uppercase().as_str() {
        "" => "USD".to_string(),
        "US" | "USA" => "USD".to_string(),
        "EU" | "EA" => "EUR".to_string(),
        "GB" | "UK" => "GBP".to_string(),
        "JP" => "JPY".to_string(),
        "CA" => "CAD".to_string(),
        "AU" => "AUD".to_string(),
        "NZ" => "NZD".to_string(),
        "CH" => "CHF".to_string(),
        "CN" => "CNY".to_string(),
        other => other.to_string(),
    }
}

fn parse_event_datetime(raw: &str) -> Option<DateTime<Utc>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return date.and_hms_opt(0, 0, 0).map(|naive| naive.and_utc());
    }
    None
}

/// Gold reacts hardest to high-impact USD prints; flag those for consumers.
fn is_gold_relevant(currency: &str, impact: &str) -> bool {
    let cur = currency.trim().to_ascii_uppercase();
    let high = impact.eq_ignore_ascii_case("High");
    high && matches!(cur.as_str(), "USD" | "XAU" | "ALL")
}

/// Parse the ForexFactory weekly JSON export into normalized events.
pub fn parse_ff_json(body: &str) -> Result<Vec<CalendarEvent>> {
    let trimmed = body.trim_start();
    if trimmed.starts_with('<') {
        bail!("calendar feed returned HTML (rate limited / blocked), not JSON");
    }
    let rows: Vec<FfRow> =
        serde_json::from_str(trimmed).context("decoding ForexFactory weekly JSON")?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        if r.title.trim().is_empty() {
            continue;
        }
        let parsed = parse_event_datetime(&r.date);
        let impact = normalize_impact(&r.impact);
        let currency = r.country.trim().to_ascii_uppercase();
        out.push(CalendarEvent {
            gold_relevant: is_gold_relevant(&currency, &impact),
            event: r.title.trim().to_string(),
            currency,
            impact,
            time: parsed.map(|d| d.to_rfc3339()),
            timestamp: parsed.map(|d| d.timestamp_millis()),
            actual: blank_to_none(r.actual),
            forecast: blank_to_none(r.forecast),
            previous: blank_to_none(r.previous),
        });
    }
    out.sort_by_key(|e| e.timestamp.unwrap_or(i64::MAX));
    Ok(out)
}

/// Parse SiftingIO's `GET /v1/fnd/economic-calendar` payload into normalized events.
pub fn parse_sifting_calendar_json(body: &str) -> Result<Vec<CalendarEvent>> {
    let envelope: SiftingCalendarEnvelope =
        serde_json::from_str(body.trim_start()).context("decoding SiftingIO economic calendar")?;

    let mut out = Vec::with_capacity(envelope.events.len());
    for row in envelope.events {
        let title = row.name.trim();
        if title.is_empty() {
            continue;
        }
        let currency = if !row.currency.trim().is_empty() {
            row.currency.trim().to_ascii_uppercase()
        } else {
            country_to_currency(&row.country)
        };
        let impact = normalize_impact(&row.impact);
        let parsed = parse_event_datetime(&row.scheduled_at);
        out.push(CalendarEvent {
            gold_relevant: is_gold_relevant(&currency, &impact),
            event: title.to_string(),
            currency,
            impact,
            time: parsed.map(|d| d.to_rfc3339()),
            timestamp: parsed.map(|d| d.timestamp_millis()),
            actual: json_scalar_to_opt_string(row.actual),
            forecast: json_scalar_to_opt_string(row.consensus),
            previous: json_scalar_to_opt_string(row.previous),
        });
    }
    out.sort_by_key(|e| e.timestamp.unwrap_or(i64::MAX));
    Ok(out)
}

/// Parse a markdown/text table (the MCP browser fallback path).
/// Accepts `| time | currency | impact | event |` rows and skips headers,
/// separators and the sticky "Date" rows ForexFactory renders.
pub fn parse_calendar_markdown(md: &str) -> Vec<CalendarEvent> {
    let mut out = Vec::new();
    for line in md.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cols: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(|s| s.trim())
            .collect();
        if cols.len() < 4 {
            continue;
        }
        // Header row, markdown separator row, or empty event name.
        if cols[0].eq_ignore_ascii_case("time")
            || cols[0].starts_with(':')
            || cols[0]
                .chars()
                .all(|c| c == '-' || c == ':' || c.is_whitespace())
            || cols[3].is_empty()
        {
            continue;
        }
        let impact = normalize_impact(cols[2]);
        let currency = cols[1].to_ascii_uppercase();
        out.push(CalendarEvent {
            gold_relevant: is_gold_relevant(&currency, &impact),
            event: cols[3].to_string(),
            currency,
            impact,
            time: if cols[0].is_empty() {
                None
            } else {
                Some(cols[0].to_string())
            },
            timestamp: None,
            actual: None,
            forecast: None,
            previous: None,
        });
    }
    out
}

fn retry_wait(headers: &HeaderMap, attempt: usize) -> StdDuration {
    if let Some(secs) = headers
        .get(RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|secs| secs.is_finite() && *secs >= 0.0)
    {
        return StdDuration::from_secs_f64(secs.min(MAX_RETRY_WAIT_SECS));
    }
    StdDuration::from_millis(500 * (1_u64 << attempt))
}

fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || (status.is_server_error() && status != StatusCode::NOT_IMPLEMENTED)
}

fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(StdDuration::from_secs(20))
        .user_agent(CALENDAR_USER_AGENT)
        .build()
        .expect("calendar http client")
}

/// Fetches the calendar: direct HTTPS feed first, SiftingIO API second, and an
/// external MCP browser third when configured.
pub struct CalendarSource {
    http: reqwest::Client,
    urls: Vec<String>,
    sifting: Option<(String, String)>,
}

impl CalendarSource {
    #[cfg(test)]
    pub fn new(primary: Option<String>) -> Self {
        Self::with_sifting(primary, None, None)
    }

    pub fn with_sifting(
        primary: Option<String>,
        sifting_base_url: Option<String>,
        sifting_api_key: Option<String>,
    ) -> Self {
        let mut urls = Vec::new();
        if let Some(u) = primary.filter(|u| !u.trim().is_empty()) {
            urls.push(u.trim().to_string());
        }
        if !urls.iter().any(|u| u == FF_WEEKLY_JSON) {
            urls.push(FF_WEEKLY_JSON.to_string());
        }
        let sifting = match (
            sifting_base_url.filter(|u| !u.trim().is_empty()),
            sifting_api_key.filter(|k| !k.trim().is_empty()),
        ) {
            (Some(base), Some(key)) => Some((base.trim().to_string(), key.trim().to_string())),
            _ => None,
        };
        Self {
            http: build_http_client(),
            urls,
            sifting,
        }
    }

    #[cfg(test)]
    fn with_urls_and_sifting(urls: Vec<String>, sifting: Option<(String, String)>) -> Self {
        Self {
            http: build_http_client(),
            urls,
            sifting,
        }
    }

    /// Try each direct JSON feed URL in order. Returns the first that parses.
    pub async fn fetch_direct(&self) -> Result<Vec<CalendarEvent>> {
        let mut last_err = None;
        for url in &self.urls {
            match self.fetch_one(url).await {
                Ok(events) if !events.is_empty() => return Ok(events),
                Ok(_) => {
                    warn!("Calendar feed {url} returned 0 events");
                    last_err = Some(anyhow::anyhow!("{url} returned 0 events"));
                }
                Err(e) => {
                    warn!("Calendar feed {url} failed: {e:#}");
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no calendar feed URLs configured")))
    }

    async fn fetch_one(&self, url: &str) -> Result<Vec<CalendarEvent>> {
        for attempt in 0..=MAX_HTTP_RETRIES {
            let resp = self
                .http
                .get(url)
                .header(ACCEPT, "application/json, text/plain, */*")
                .send()
                .await
                .with_context(|| format!("calendar GET {url}"))?;
            let status = resp.status();
            let headers = resp.headers().clone();
            let body = resp.text().await.context("reading calendar body")?;
            if is_retryable(status) && attempt < MAX_HTTP_RETRIES {
                tokio::time::sleep(retry_wait(&headers, attempt)).await;
                continue;
            }
            if !status.is_success() {
                bail!("calendar feed {url} HTTP {status}");
            }
            return parse_ff_json(&body);
        }
        bail!("calendar feed {url} exhausted retry budget")
    }

    /// Fetch from SiftingIO's `/v1/fnd/economic-calendar` using the configured key.
    pub async fn fetch_sifting(&self) -> Result<Vec<CalendarEvent>> {
        let Some((base_url, api_key)) = &self.sifting else {
            bail!("SiftingIO calendar fallback is not configured");
        };
        let now = Utc::now();
        let from = (now - Duration::days(3)).format("%Y-%m-%d").to_string();
        let to = (now + Duration::days(14)).format("%Y-%m-%d").to_string();
        let endpoint = format!(
            "{}/v1/fnd/economic-calendar",
            base_url.trim_end_matches('/')
        );
        let mut url =
            reqwest::Url::parse(&endpoint).context("invalid SiftingIO calendar endpoint")?;
        url.query_pairs_mut()
            .append_pair("from", &from)
            .append_pair("to", &to)
            .append_pair("limit", "500");

        for attempt in 0..=MAX_HTTP_RETRIES {
            let resp = self
                .http
                .get(url.clone())
                .header("X-API-Key", api_key)
                .header("Accept-Encoding", "gzip")
                .header(ACCEPT, "application/json")
                .send()
                .await
                .context("SiftingIO calendar GET")?;
            let status = resp.status();
            let headers = resp.headers().clone();
            let body = resp.text().await.context("reading SiftingIO calendar")?;
            if is_retryable(status) && attempt < MAX_HTTP_RETRIES {
                tokio::time::sleep(retry_wait(&headers, attempt)).await;
                continue;
            }
            if !status.is_success() {
                bail!("SiftingIO calendar HTTP {status}");
            }
            return parse_sifting_calendar_json(&body);
        }
        bail!("SiftingIO calendar exhausted retry budget")
    }

    /// Direct feed, then SiftingIO API (if keyed), then browser MCP (if configured).
    pub async fn fetch(
        &self,
        mcp: Option<&McpClient>,
        status: &FeedStatus,
    ) -> Result<(Vec<CalendarEvent>, &'static str)> {
        let mut failures = Vec::new();

        match self.fetch_direct().await {
            Ok(events) => {
                status.set("calendar", "connected");
                status.mark_msg("calendar");
                info!("Calendar: {} events from direct feed", events.len());
                return Ok((events, "ff_json"));
            }
            Err(e) => {
                warn!("Direct calendar fetch failed: {e:#}");
                failures.push(format!("direct: {e:#}"));
            }
        }

        if self.sifting.is_some() {
            match self.fetch_sifting().await {
                Ok(events) if !events.is_empty() => {
                    status.set("calendar", "connected");
                    status.mark_msg("calendar");
                    info!("Calendar: {} events from SiftingIO API", events.len());
                    return Ok((events, "sifting"));
                }
                Ok(_) => {
                    warn!("SiftingIO calendar returned 0 events");
                    failures.push("sifting: 0 events".to_string());
                }
                Err(e) => {
                    warn!("SiftingIO calendar fallback failed: {e:#}");
                    failures.push(format!("sifting: {e:#}"));
                }
            }
        }

        if let Some(mcp) = mcp {
            match mcp.scrape_calendar(status).await {
                Ok(md) => {
                    let events = parse_calendar_markdown(&md);
                    if !events.is_empty() {
                        status.set("calendar", "connected (mcp)");
                        status.mark_msg("calendar");
                        info!("Calendar: {} events from MCP browser", events.len());
                        return Ok((events, "mcp_browser"));
                    }
                    warn!("MCP browser returned a page but no parseable events");
                    failures.push("mcp: 0 parseable events".to_string());
                }
                Err(e) => {
                    warn!("MCP calendar scrape failed: {e:#}");
                    failures.push(format!("mcp: {e:#}"));
                }
            }
        }

        status.set("calendar", "error");
        bail!("all calendar sources failed ({})", failures.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, response::IntoResponse, routing::get};

    const SAMPLE: &str = r#"[
      {"title":"Core CPI m/m","country":"USD","date":"2026-09-24T08:30:00-04:00",
       "impact":"High","forecast":"0.3%","previous":"0.2%","actual":""},
      {"title":"Bank Holiday","country":"JPY","date":"2026-09-22T00:00:00-04:00",
       "impact":"Holiday","forecast":"","previous":"","actual":""},
      {"title":"Retail Sales m/m","country":"GBP","date":"2026-09-23T02:00:00-04:00",
       "impact":"Medium","forecast":"0.1%","previous":"-0.2%","actual":"0.4%"}
    ]"#;

    const SIFTING_SAMPLE: &str = r#"{
      "events": [
        {
          "event_id": "us_cpi",
          "name": "Consumer Price Index (CPI)",
          "country": "US",
          "currency": "USD",
          "agency": "BLS",
          "impact": "high",
          "scheduled_at": "2026-10-27T12:30:00Z",
          "actual": null,
          "consensus": 0.3,
          "previous": 0.2,
          "released_at": null
        },
        {
          "event_id": "us_claims",
          "name": "Initial Jobless Claims",
          "country": "US",
          "currency": "",
          "agency": "DOL",
          "impact": "medium",
          "scheduled_at": "2026-10-26T12:30:00Z",
          "actual": 218000,
          "consensus": 220000,
          "previous": 215000,
          "released_at": "2026-10-26T12:30:02Z"
        }
      ],
      "count": 2,
      "filter": {}
    }"#;

    async fn spawn_mock(router: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        format!("http://{addr}")
    }

    #[test]
    fn parses_forexfactory_weekly_json() {
        let events = parse_ff_json(SAMPLE).expect("parse");
        assert_eq!(events.len(), 3);
        // Sorted chronologically.
        assert_eq!(events[0].event, "Bank Holiday");
        assert_eq!(events[2].event, "Core CPI m/m");

        let cpi = &events[2];
        assert_eq!(cpi.currency, "USD");
        assert_eq!(cpi.impact, "High");
        assert_eq!(cpi.forecast.as_deref(), Some("0.3%"));
        assert_eq!(cpi.actual, None, "blank actual must be None");
        // 08:30 EDT == 12:30 UTC
        assert_eq!(cpi.time.as_deref(), Some("2026-09-24T12:30:00+00:00"));
        assert!(cpi.timestamp.is_some());
        assert!(
            cpi.gold_relevant,
            "high-impact USD must flag as gold-relevant"
        );

        assert!(!events[0].gold_relevant);
        assert!(!events[1].gold_relevant, "medium GBP is not gold-relevant");
    }

    #[test]
    fn parses_sifting_economic_calendar_json() {
        let events = parse_sifting_calendar_json(SIFTING_SAMPLE).expect("parse sifting calendar");
        assert_eq!(events.len(), 2);
        // Sorted chronologically: Oct 26 before Oct 27.
        assert_eq!(events[0].event, "Initial Jobless Claims");
        assert_eq!(events[0].currency, "USD");
        assert_eq!(events[0].impact, "Medium");
        assert_eq!(events[0].actual.as_deref(), Some("218000"));
        assert!(!events[0].gold_relevant);

        assert_eq!(events[1].event, "Consumer Price Index (CPI)");
        assert_eq!(events[1].currency, "USD");
        assert_eq!(events[1].impact, "High");
        assert_eq!(events[1].actual, None);
        assert_eq!(events[1].forecast.as_deref(), Some("0.3"));
        assert_eq!(events[1].previous.as_deref(), Some("0.2"));
        assert!(events[1].timestamp.is_some());
        assert!(events[1].gold_relevant);
    }

    #[test]
    fn rejects_html_rate_limit_page() {
        let err = parse_ff_json("<!DOCTYPE html><html>Request Denied</html>").unwrap_err();
        assert!(err.to_string().contains("HTML"), "got: {err}");
    }

    #[test]
    fn handles_empty_feed() {
        assert!(parse_ff_json("[]").unwrap().is_empty());
    }

    #[test]
    fn markdown_fallback_skips_headers_and_separators() {
        let md = "\
| Time | Currency | Impact | Event |
|------|----------|--------|-------|
| 8:30am | USD | High | Core CPI m/m |
| 2:00am | GBP | Medium | Retail Sales m/m |
not a table row
";
        let events = parse_calendar_markdown(md);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event, "Core CPI m/m");
        assert_eq!(events[0].impact, "High");
        assert!(events[0].gold_relevant);
        assert_eq!(events[1].currency, "GBP");
    }

    #[test]
    fn impact_colors_are_normalized() {
        assert_eq!(normalize_impact("red"), "High");
        assert_eq!(normalize_impact("Orange"), "Medium");
        assert_eq!(normalize_impact("yellow"), "Low");
        assert_eq!(normalize_impact("grey"), "Holiday");
    }

    #[test]
    fn source_uses_live_urls_and_optional_sifting_fallback() {
        let s = CalendarSource::new(None);
        assert_eq!(s.urls, vec![FF_WEEKLY_JSON.to_string()]);
        assert!(s.sifting.is_none());

        let s = CalendarSource::with_sifting(
            Some("https://example.test/cal.json".into()),
            Some("https://api.sifting.io".into()),
            Some("sft_test".into()),
        );
        assert_eq!(s.urls.len(), 2);
        assert_eq!(s.urls[0], "https://example.test/cal.json");
        assert_eq!(s.urls[1], FF_WEEKLY_JSON);
        assert!(s.sifting.is_some());
    }

    #[tokio::test]
    async fn sifting_calendar_recovers_when_direct_feed_is_blocked() {
        let router = Router::new()
            .route(
                "/ff_blocked.json",
                get(|| async {
                    (
                        StatusCode::FORBIDDEN,
                        "<!DOCTYPE html><html>Request Denied</html>",
                    )
                        .into_response()
                }),
            )
            .route(
                "/v1/fnd/economic-calendar",
                get(|| async {
                    let body: serde_json::Value =
                        serde_json::from_str(SIFTING_SAMPLE).expect("json");
                    Json(body).into_response()
                }),
            );
        let base = spawn_mock(router).await;
        let source = CalendarSource::with_urls_and_sifting(
            vec![format!("{base}/ff_blocked.json")],
            Some((base, "sft_test".into())),
        );
        let status = FeedStatus::new();
        let (events, origin) = source.fetch(None, &status).await.expect("sifting fallback");
        assert_eq!(origin, "sifting");
        assert_eq!(events.len(), 2);
        let snap = status.snapshot();
        assert_eq!(snap["calendar"]["state"], "connected");
        assert_eq!(snap["calendar"]["msgs"], 1);
        assert!(
            snap.get("mcp_browser").is_none(),
            "mcp_browser must stay untouched when unconfigured"
        );
    }

    #[tokio::test]
    async fn honest_error_state_when_all_configured_sources_fail() {
        let router = Router::new().route(
            "/broken.json",
            get(|| async { (StatusCode::FORBIDDEN, "blocked").into_response() }),
        );
        let base = spawn_mock(router).await;
        let source =
            CalendarSource::with_urls_and_sifting(vec![format!("{base}/broken.json")], None);
        let status = FeedStatus::new();
        let err = source.fetch(None, &status).await.unwrap_err();
        assert!(err.to_string().contains("all calendar sources failed"));
        assert_eq!(status.snapshot()["calendar"]["state"], "error");
    }
}

/// Live network test — hits the real ForexFactory feed. Ignored by default so
/// offline/sandboxed `cargo test` stays green; CI runs it with `--ignored`.
#[cfg(test)]
mod live_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires network"]
    async fn real_forexfactory_feed_returns_events() {
        let source = CalendarSource::new(None);
        let events = source
            .fetch_direct()
            .await
            .expect("live ForexFactory feed must return parseable events");

        assert!(!events.is_empty(), "live feed returned zero events");
        println!("LIVE CALENDAR: {} events", events.len());
        for e in events.iter().take(10) {
            println!(
                "  {} | {} | {} | {} | forecast={:?} previous={:?} gold={}",
                e.time.as_deref().unwrap_or("-"),
                e.currency,
                e.impact,
                e.event,
                e.forecast,
                e.previous,
                e.gold_relevant
            );
        }

        // Every event must be normalized, not raw junk.
        for e in &events {
            assert!(!e.event.is_empty(), "empty event title");
            assert!(
                matches!(
                    e.impact.as_str(),
                    "High" | "Medium" | "Low" | "Holiday" | "Unknown"
                ),
                "unnormalized impact: {}",
                e.impact
            );
        }
        // A real week always contains at least one USD event.
        assert!(
            events.iter().any(|e| e.currency == "USD"),
            "live feed had no USD events — parser is probably mismapping fields"
        );
        // Events are chronologically sorted.
        let stamps: Vec<i64> = events.iter().filter_map(|e| e.timestamp).collect();
        assert!(stamps.windows(2).all(|w| w[0] <= w[1]), "events not sorted");
    }
}
