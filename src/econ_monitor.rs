//! Automated econ-news audio monitor.
//!
//! Bridges the economic calendar to Node3's AI pipeline (`ws_client.py`):
//!
//! 1. Watches the cached economic calendar for event windows (default: 15 min
//!    before -> 1 h after, high-impact USD, e.g. NFP / CPI / FOMC / Powell).
//! 2. When a window opens, discovers live coverage:
//!      - the configured watchlist `ECON_STREAM_SOURCES` (channel `/live`
//!        pages or direct media URLs), and
//!      - links found by **browsing the events** with the configured MCP
//!        browser server (`MCP_BROWSER_URL`, the ~31-tool Playwright-style
//!        server already used for the calendar fallback).
//! 3. Captures the audio (yt-dlp to resolve live/VOD stream URLs, ffmpeg to
//!    decode to 16kHz mono f32le PCM) and streams it as `audio_chunk`
//!    frames on `/ws`. Node3 subscribes to exactly this topic, transcribes
//!    with Moonshine, scores with FinBERT/FOMC-RoBERTa and reports
//!    `transcript`/`sentiment` frames back — the engine caches those for
//!    `GET /ai` ("what did the econ news deliver today").
//!
//! `ECON_TEST_TONE=1` streams a synthetic 16kHz tone instead — a
//! dependency-free plumbing test for CI and air-gapped boxes.

use crate::calendar::CalendarEvent;
use crate::config::Config;
use crate::mcp_client::McpClient;
use crate::status::FeedStatus;
use crate::types::WsFrame;
use std::collections::HashSet;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::sync::{RwLock, broadcast};
use tracing::{info, warn};

const SAMPLE_RATE: usize = 16_000;

/// High -> 2, Medium -> 1, everything else -> 0.
pub fn impact_rank(impact: &str) -> u8 {
    match impact.trim().to_ascii_lowercase().as_str() {
        "high" | "red" => 2,
        "medium" | "orange" => 1,
        _ => 0,
    }
}

/// Events whose capture window `[time - before, time + after]` contains
/// `now_ms`, at or above `min_impact_rank`, in one of `currencies`
/// ("ALL" matches everything).
pub fn active_events(
    events: &[CalendarEvent],
    now_ms: i64,
    before_secs: i64,
    after_secs: i64,
    min_impact_rank: u8,
    currencies: &[String],
) -> Vec<CalendarEvent> {
    events
        .iter()
        .filter(|e| {
            let Some(ts) = e.timestamp else {
                return false;
            };
            if impact_rank(&e.impact) < min_impact_rank {
                return false;
            }
            if !currencies.is_empty() {
                let cur = e.currency.to_ascii_uppercase();
                if !currencies
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(&cur) || c.eq_ignore_ascii_case("ALL"))
                {
                    return false;
                }
            }
            now_ms >= ts - before_secs * 1000 && now_ms <= ts + after_secs * 1000
        })
        .cloned()
        .collect()
}

/// Latest capture-stop time across active events (event time + after window).
pub fn window_end_ms(events: &[CalendarEvent], after_secs: i64) -> i64 {
    events
        .iter()
        .filter_map(|e| e.timestamp)
        .max()
        .map(|ts| ts + after_secs * 1000)
        .unwrap_or(0)
}

/// True when the title sounds like a Fed event with official live coverage.
pub fn fed_related(title: &str) -> bool {
    let t = title.to_ascii_lowercase();
    [
        "fomc",
        "federal reserve",
        "fed chair",
        "powell",
        "rate statement",
        "press conference",
        "monetary policy",
        "testimony",
        "beige book",
        "jackson hole",
    ]
    .iter()
    .any(|k| t.contains(k))
}

/// Scan page text (e.g. a YouTube search result pulled through the MCP
/// browser) for stream/watch URLs worth trying. Live-ish URLs first.
pub fn extract_live_urls(text: &str, cap: usize) -> Vec<String> {
    let mut raw: Vec<String> = Vec::new();
    let mut cur = String::new();
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_whitespace()
            || matches!(
                ch,
                '"' | '\'' | '(' | ')' | '<' | '>' | '[' | ']' | '{' | '}' | ',' | ';'
            )
        {
            push_candidate(&mut raw, &cur);
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    // Prefer explicitly-live URLs, fall back to watch pages (a press
    // conference VOD is exactly "what the news delivered" too).
    raw.sort_by_key(|u| if u.contains("live") { 0 } else { 1 });
    raw.truncate(cap);
    raw
}

fn push_candidate(out: &mut Vec<String>, tok: &str) {
    let tok = tok.trim_end_matches(['.', ':', '!', '?', ')']);
    if !tok.starts_with("http://") && !tok.starts_with("https://") {
        return;
    }
    let u = tok.to_ascii_lowercase();
    let liveish = u.contains("watch?v=")
        || u.contains("youtu.be/")
        || u.contains("/live")
        || u.contains("live.")
        || u.contains("webcast")
        || u.contains("livestream");
    if liveish && !out.iter().any(|x| x == tok) {
        out.push(tok.to_string());
    }
}

/// Minimal percent-encoder for search queries (unreserved chars kept).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Base64 (standard alphabet, padded) — the wire format Node3 expects.
pub fn b64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// One chunk of 16kHz mono f32le sine — the `ECON_TEST_TONE` source.
pub fn tone_chunk(samples: usize, phase: &mut f64, freq: f64) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples * 4);
    for _ in 0..samples {
        let v = 0.3 * (std::f64::consts::TAU * *phase).sin();
        *phase = (*phase + freq / SAMPLE_RATE as f64) % 1.0;
        out.extend_from_slice(&(v as f32).to_le_bytes());
    }
    out
}

fn chunk_bytes(chunk_ms: u64) -> usize {
    SAMPLE_RATE * 4 * (chunk_ms as usize) / 1000
}

fn audio_frame(pcm: &[u8], chunk_ms: u64, source: &str, event: &str, seq: u64) -> WsFrame {
    WsFrame::AudioChunk {
        data: serde_json::json!({
            "data": b64_encode(pcm),
            "ts": chrono::Utc::now().timestamp_millis(),
            "source": source,
            "event": event,
            "sample_rate": SAMPLE_RATE,
            "format": "f32le",
            "duration_ms": chunk_ms,
            "seq": seq,
        }),
    }
}

/// Continuous synthetic tone — plumbing proof without ffmpeg/yt-dlp/events.
async fn emit_tone(bc: &broadcast::Sender<WsFrame>, st: &FeedStatus, cfg: &Config) {
    let chunk_ms = cfg.econ_chunk_ms;
    let samples = chunk_bytes(chunk_ms) / 4;
    let mut phase = 0.0f64;
    let mut seq: u64 = 0;
    info!("econ monitor: test tone streaming ({samples} samples / {chunk_ms} ms per chunk)");
    loop {
        let pcm = tone_chunk(samples, &mut phase, 440.0);
        let _ = bc.send(audio_frame(
            &pcm,
            chunk_ms,
            "econ_monitor:tone",
            "test tone",
            seq,
        ));
        st.mark_msg("econ_monitor");
        seq += 1;
        tokio::time::sleep(std::time::Duration::from_millis(chunk_ms)).await;
    }
}

/// Does this URL point straight at media (no extractor needed)?
fn is_direct_media(url: &str) -> bool {
    let u = url
        .to_ascii_lowercase()
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .to_string();
    [
        ".m3u8", ".mp3", ".aac", ".m4a", ".wav", ".ogg", ".opus", ".mp4", ".webm",
    ]
    .iter()
    .any(|e| u.ends_with(*e))
}

/// Resolve a page URL (YouTube watch/live page, ...) to direct media via
/// yt-dlp. Direct media URLs pass through untouched.
async fn resolve_media(url: &str, cfg: &Config) -> Option<String> {
    if is_direct_media(url) {
        return Some(url.to_string());
    }
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(45),
        tokio::process::Command::new(&cfg.econ_ytdlp)
            .args([
                "-g",
                "-f",
                "bestaudio/best",
                "--no-playlist",
                "--no-warnings",
                url,
            ])
            .stdin(Stdio::null())
            .output(),
    )
    .await;
    let out = match out {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            warn!(
                "econ monitor: could not run {} ({e}) — install yt-dlp or use direct media URLs in ECON_STREAM_SOURCES",
                cfg.econ_ytdlp
            );
            return None;
        }
        Err(_) => {
            warn!("econ monitor: yt-dlp timed out on {url}");
            return None;
        }
    };
    if !out.status.success() {
        warn!(
            "econ monitor: yt-dlp could not resolve {url}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    let media = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim())
        .find(|l| l.starts_with("http"))
        .map(|l| l.to_string());
    if media.is_none() {
        warn!("econ monitor: yt-dlp returned no stream URL for {url} (not live?)");
    }
    media
}

/// Decode `media_url` to 16kHz mono f32le PCM with ffmpeg and broadcast it
/// as `audio_chunk` frames until the event window closes, the stream ends,
/// or `ECON_MAX_STREAM_SECS` elapses.
async fn capture_media(
    bc: &broadcast::Sender<WsFrame>,
    st: &FeedStatus,
    cfg: &Config,
    media_url: &str,
    label: &str,
    until_ms: i64,
) {
    let chunk_ms = cfg.econ_chunk_ms;
    let want = chunk_bytes(chunk_ms);
    let started = std::time::Instant::now();

    let sr = SAMPLE_RATE.to_string();
    let child = tokio::process::Command::new(&cfg.econ_ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            media_url,
            "-vn",
            "-f",
            "f32le",
            "-ar",
            sr.as_str(),
            "-ac",
            "1",
            "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            warn!(
                "econ monitor: could not start {} ({e}) — install ffmpeg to stream news audio",
                cfg.econ_ffmpeg
            );
            return;
        }
    };
    let Some(mut stdout) = child.stdout.take() else {
        return;
    };

    info!("econ monitor: capturing audio for {label}");
    let mut seq: u64 = 0;
    loop {
        if chrono::Utc::now().timestamp_millis() > until_ms
            || started.elapsed().as_secs() > cfg.econ_max_stream_secs
        {
            break;
        }
        let mut buf = vec![0u8; want];
        match stdout.read_exact(&mut buf).await {
            Ok(_) => {
                let _ = bc.send(audio_frame(&buf, chunk_ms, media_url, label, seq));
                st.mark_msg("econ_monitor");
                seq += 1;
            }
            Err(_) => break, // stream ended (or partial trailing chunk)
        }
    }
    let _ = child.kill().await;
    info!("econ monitor: capture for {label} stopped after {seq} chunks");
}

/// Ask the MCP browser server to browse the event's live coverage and pull
/// candidate stream links out of the page text.
async fn discover_streams(mcp: &McpClient, event: &CalendarEvent) -> Vec<String> {
    let query = format!("{} {} live", event.event, event.currency);
    let search = format!(
        "https://www.youtube.com/results?search_query={}",
        urlencode(&query)
    );
    let mut out = Vec::new();
    match mcp.browse_page(&search).await {
        Ok(text) => {
            info!(
                "econ monitor: browsed live coverage for {:?} ({} chars of page text)",
                event.event,
                text.len()
            );
            out.extend(extract_live_urls(&text, 4));
        }
        Err(e) => warn!("econ monitor: MCP browse failed for {:?}: {e}", event.event),
    }
    out
}

/// The automated monitor loop.
pub async fn run_econ_monitor(
    cfg: Config,
    bc: broadcast::Sender<WsFrame>,
    cal: Arc<RwLock<serde_json::Value>>,
    st: FeedStatus,
) {
    if !cfg.econ_monitor {
        st.set("econ_monitor", "off");
        return;
    }
    st.set("econ_monitor", "starting");

    let mcp = if cfg.econ_use_mcp {
        let m = McpClient::new(cfg.mcp_browser_url.clone(), cfg.mcp_browser_token.clone());
        match m.tool_count().await {
            Some(n) => info!(
                "econ monitor: MCP browser server at {} exposes {n} tools",
                cfg.mcp_browser_url
            ),
            None => warn!(
                "econ monitor: MCP browser server at {} unreachable — will retry when events open",
                cfg.mcp_browser_url
            ),
        }
        Some(m)
    } else {
        None
    };

    let mut tick =
        tokio::time::interval(std::time::Duration::from_secs(cfg.econ_scan_secs.max(10)));
    loop {
        tick.tick().await;

        if cfg.econ_test_tone {
            st.set("econ_monitor", "test tone (ECON_TEST_TONE=1)");
            emit_tone(&bc, &st, &cfg).await;
            continue;
        }

        let events_val = cal
            .read()
            .await
            .get("events")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));
        let events: Vec<CalendarEvent> = serde_json::from_value(events_val).unwrap_or_default();
        let now = chrono::Utc::now().timestamp_millis();
        let active = active_events(
            &events,
            now,
            cfg.econ_before_secs,
            cfg.econ_after_secs,
            cfg.econ_min_impact_rank(),
            &cfg.econ_currencies,
        );
        if active.is_empty() {
            st.set("econ_monitor", "idle (no active event windows)");
            continue;
        }

        let label = active
            .iter()
            .map(|e| format!("{} {}", e.currency, e.event))
            .collect::<Vec<_>>()
            .join(", ");
        info!("econ monitor: event window open — {label}");
        st.set("econ_monitor", &format!("watching: {label}"));

        // Candidate sources: user watchlist + (Fed events) the official Fed
        // channel + live coverage links discovered by browsing the event.
        let mut candidates: Vec<String> = cfg.econ_stream_sources.clone();
        for ev in &active {
            if fed_related(&ev.event) && !candidates.iter().any(|c| c.contains("federalreserve")) {
                candidates.push("https://www.youtube.com/@federalreserve/live".to_string());
            }
        }
        if let Some(mcp) = &mcp {
            for ev in active.iter().take(2) {
                candidates.extend(discover_streams(mcp, ev).await);
            }
        }
        let mut seen = HashSet::new();
        candidates.retain(|u| seen.insert(u.clone()));

        let until_ms = window_end_ms(&active, cfg.econ_after_secs);
        for url in candidates {
            if chrono::Utc::now().timestamp_millis() > until_ms {
                break; // window closed before we got to this source
            }
            let Some(media) = resolve_media(&url, &cfg).await else {
                continue;
            };
            st.set("econ_monitor", &format!("streaming: {url}"));
            capture_media(&bc, &st, &cfg, &media, &label, until_ms).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(title: &str, currency: &str, impact: &str, ts: i64) -> CalendarEvent {
        CalendarEvent {
            event: title.into(),
            currency: currency.into(),
            impact: impact.into(),
            time: None,
            timestamp: Some(ts),
            actual: None,
            forecast: None,
            previous: None,
            gold_relevant: impact_rank(impact) == 2 && currency == "USD",
        }
    }

    #[test]
    fn window_selects_only_active_high_impact_usd_events() {
        let now = 1_700_000_000_000i64;
        let events = vec![
            ev("Nonfarm Payrolls", "USD", "High", now - 60_000), // active
            ev("Core CPI m/m", "USD", "High", now + 10 * 60_000), // inside pre-window
            ev("German Factory Orders", "EUR", "High", now),     // wrong currency
            ev("ADP Non-Farm Employment", "USD", "Medium", now), // below min impact
            ev("FOMC Press Conference", "USD", "High", now - 2 * 3_600_000), // after window
        ];
        let active = active_events(&events, now, 900, 3600, 2, &["USD".to_string()]);
        let titles: Vec<&str> = active.iter().map(|e| e.event.as_str()).collect();
        assert_eq!(titles, vec!["Nonfarm Payrolls", "Core CPI m/m"]);
    }

    #[test]
    fn all_currency_matches_everything() {
        let now = 1_700_000_000_000i64;
        let events = vec![ev("SNB Rate Decision", "CHF", "High", now)];
        let active = active_events(&events, now, 900, 3600, 2, &["ALL".to_string()]);
        assert_eq!(active.len(), 1);
    }

    #[test]
    fn extracts_live_urls_and_prefers_live() {
        let text = r#"
            (https://www.youtube.com/watch?v=abc123XYZ_-)
            [https://www.youtube.com/@federalreserve/live]
            "https://www.youtube.com/watch?v=live999aaaaa live now"
            https://example.com/not-a-stream
        "#;
        let urls = extract_live_urls(text, 8);
        assert_eq!(urls[0], "https://www.youtube.com/@federalreserve/live");
        assert!(
            urls.iter()
                .any(|u| u.starts_with("https://www.youtube.com/watch?v=abc123XYZ_-"))
        );
        assert!(!urls.iter().any(|u| u.contains("example.com")));
    }

    #[test]
    fn b64_matches_known_vectors() {
        assert_eq!(b64_encode(b"Man"), "TWFu");
        assert_eq!(b64_encode(b"Ma"), "TWE=");
        assert_eq!(b64_encode(b"M"), "TQ==");
        assert_eq!(b64_encode(b""), "");
    }

    #[test]
    fn tone_chunk_is_f32le_of_expected_length() {
        let mut phase = 0.0;
        let pcm = tone_chunk(160, &mut phase, 440.0);
        assert_eq!(pcm.len(), 160 * 4);
        assert!(phase > 0.0 && phase < 1.0);
    }

    #[test]
    fn fed_events_are_recognized() {
        assert!(fed_related("FOMC Press Conference"));
        assert!(fed_related("Fed Chair Powell Speaks"));
        assert!(!fed_related("Core Retail Sales m/m"));
    }
}
