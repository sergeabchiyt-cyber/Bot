//! Economic calendar.
//!
//! The engine previously only had one calendar path: drive a remote browser
//! over MCP, screenshot/snapshot forexfactory.com and regex a markdown table
//! out of it. That needs an external browser MCP server to be running, and
//! the markdown parser never matched Playwright's accessibility snapshot — so
//! in practice the calendar was always empty.
//!
//! This module makes the calendar work on a bare deployment: it pulls
//! ForexFactory's own weekly export feed over plain HTTPS (no key, no
//! browser), normalizes it, and keeps the MCP browser as an optional
//! fallback for when the feed is rate-limited.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::mcp_client::McpClient;
use crate::status::FeedStatus;

/// ForexFactory's official weekly export (same data as the calendar page).
pub const FF_WEEKLY_JSON: &str = "https://nfs.faireconomy.media/ff_calendar_thisweek.json";
/// Mirror used when the primary host is rate-limiting.
pub const FF_WEEKLY_JSON_CDN: &str = "https://cdn-nfs.faireconomy.media/ff_calendar_thisweek.json";

/// One normalized calendar event, broadcast as part of the `calendar` frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarEvent {
    /// Event title, e.g. "Core CPI m/m".
    pub event: String,
    /// ISO currency code, e.g. "USD".
    pub currency: String,
    /// "High" | "Medium" | "Low" | "Holiday".
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

fn blank_to_none(v: Option<String>) -> Option<String> {
    v.filter(|s| !s.trim().is_empty())
}

fn normalize_impact(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "high" | "red" => "High",
        "medium" | "orange" => "Medium",
        "low" | "yellow" => "Low",
        "holiday" | "non-economic" | "grey" | "gray" => "Holiday",
        "" => "Unknown",
        _ => return raw.trim().to_string(),
    }
    .to_string()
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
        let parsed: Option<DateTime<Utc>> = DateTime::parse_from_rfc3339(r.date.trim())
            .ok()
            .map(|d| d.with_timezone(&Utc));
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
            || cols[0].chars().all(|c| c == '-' || c == ':' || c.is_whitespace())
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
            time: if cols[0].is_empty() { None } else { Some(cols[0].to_string()) },
            timestamp: None,
            actual: None,
            forecast: None,
            previous: None,
        });
    }
    out
}

/// Fetches the calendar: direct HTTPS feed first, MCP browser as fallback.
pub struct CalendarSource {
    http: reqwest::Client,
    urls: Vec<String>,
}

impl CalendarSource {
    pub fn new(primary: Option<String>) -> Self {
        let mut urls = Vec::new();
        if let Some(u) = primary.filter(|u| !u.trim().is_empty()) {
            urls.push(u.trim().to_string());
        }
        for d in [FF_WEEKLY_JSON, FF_WEEKLY_JSON_CDN] {
            if !urls.iter().any(|u| u == d) {
                urls.push(d.to_string());
            }
        }
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                // The feed 403s the default reqwest agent.
                .user_agent("Mozilla/5.0 (compatible; xauusd-engine/0.3)")
                .build()
                .expect("calendar http client"),
            urls,
        }
    }

    /// Try each feed URL in order. Returns the first that parses.
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
                    warn!("Calendar feed {url} failed: {e}");
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no calendar feed URLs configured")))
    }

    async fn fetch_one(&self, url: &str) -> Result<Vec<CalendarEvent>> {
        let resp = self.http.get(url).send().await.context("calendar GET")?;
        let status = resp.status();
        let body = resp.text().await.context("reading calendar body")?;
        if !status.is_success() {
            bail!("calendar feed HTTP {status}");
        }
        parse_ff_json(&body)
    }

    /// Direct feed, then the browser MCP server if one is reachable.
    pub async fn fetch(
        &self,
        mcp: Option<&McpClient>,
        status: &FeedStatus,
    ) -> Result<(Vec<CalendarEvent>, &'static str)> {
        match self.fetch_direct().await {
            Ok(events) => {
                status.set("calendar", "connected");
                info!("Calendar: {} events from direct feed", events.len());
                return Ok((events, "ff_json"));
            }
            Err(e) => warn!("Direct calendar fetch failed: {e}"),
        }

        if let Some(mcp) = mcp {
            match mcp.scrape_calendar(status).await {
                Ok(md) => {
                    let events = parse_calendar_markdown(&md);
                    if !events.is_empty() {
                        status.set("calendar", "connected (mcp)");
                        info!("Calendar: {} events from MCP browser", events.len());
                        return Ok((events, "mcp_browser"));
                    }
                    warn!("MCP browser returned a page but no parseable events");
                }
                Err(e) => warn!("MCP calendar scrape failed: {e}"),
            }
        }

        status.set("calendar", "error");
        bail!("all calendar sources failed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
      {"title":"Core CPI m/m","country":"USD","date":"2026-09-24T08:30:00-04:00",
       "impact":"High","forecast":"0.3%","previous":"0.2%","actual":""},
      {"title":"Bank Holiday","country":"JPY","date":"2026-09-22T00:00:00-04:00",
       "impact":"Holiday","forecast":"","previous":"","actual":""},
      {"title":"Retail Sales m/m","country":"GBP","date":"2026-09-23T02:00:00-04:00",
       "impact":"Medium","forecast":"0.1%","previous":"-0.2%","actual":"0.4%"}
    ]"#;

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
        assert!(cpi.gold_relevant, "high-impact USD must flag as gold-relevant");

        assert!(!events[0].gold_relevant);
        assert!(!events[1].gold_relevant, "medium GBP is not gold-relevant");
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
    fn source_has_fallback_urls() {
        let s = CalendarSource::new(None);
        assert_eq!(s.urls.len(), 2);
        assert_eq!(s.urls[0], FF_WEEKLY_JSON);

        let s = CalendarSource::new(Some("https://example.test/cal.json".into()));
        assert_eq!(s.urls.len(), 3);
        assert_eq!(s.urls[0], "https://example.test/cal.json");
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
                matches!(e.impact.as_str(), "High" | "Medium" | "Low" | "Holiday" | "Unknown"),
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
