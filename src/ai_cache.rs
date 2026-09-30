//! In-memory cache of everything Node3 reports back over `/ws`.
//!
//! Node3 (ws_client.py) transcribes the econ-news audio chunks the engine
//! streams to it and answers with `transcript` / `sentiment` / `health` /
//! `prediction` frames. Those frames land here so the desk can ask "what did
//! the econ news deliver today?" over `GET /ai` without keeping a WS open.
//!
//! Storage is deliberately bounded ring-buffer semantics: last N frames per
//! kind for at most the last 48h, and `/ai` filters to the current UTC day.

use crate::types::WsFrame;
use std::collections::VecDeque;
use std::sync::Mutex;

const CAP_TRANSCRIPTS: usize = 500;
const CAP_SENTIMENTS: usize = 500;
const CAP_PREDICTIONS: usize = 200;
/// Entries older than this are pruned regardless of the per-kind cap.
const RETAIN_MS: i64 = 48 * 60 * 60 * 1000;

#[derive(Default)]
struct KindBuf {
    items: VecDeque<serde_json::Value>,
}

pub struct AiCache {
    transcripts: Mutex<KindBuf>,
    sentiments: Mutex<KindBuf>,
    predictions: Mutex<KindBuf>,
    health: Mutex<Option<serde_json::Value>>,
}

impl AiCache {
    pub fn new() -> Self {
        Self {
            transcripts: Mutex::new(KindBuf::default()),
            sentiments: Mutex::new(KindBuf::default()),
            predictions: Mutex::new(KindBuf::default()),
            health: Mutex::new(None),
        }
    }

    /// Record one inbound AI frame. The frame's own `ts` is honoured when
    /// present, otherwise the receive time is stamped in.
    pub fn record(&self, frame: &WsFrame) {
        let (buf, cap, data) = match frame {
            WsFrame::Transcript { data } => (&self.transcripts, CAP_TRANSCRIPTS, data),
            WsFrame::Sentiment { data } => (&self.sentiments, CAP_SENTIMENTS, data),
            WsFrame::Prediction { data } => (&self.predictions, CAP_PREDICTIONS, data),
            WsFrame::Health { data } => {
                let mut entry = data.clone();
                if !entry.is_object() {
                    entry = serde_json::json!({ "data": entry });
                }
                if let Some(obj) = entry.as_object_mut() {
                    obj.entry("ts")
                        .or_insert_with(|| chrono::Utc::now().timestamp_millis().into());
                }
                *self.health.lock().unwrap() = Some(entry);
                return;
            }
            _ => return,
        };

        let mut entry = data.clone();
        if !entry.is_object() {
            entry = serde_json::json!({ "data": entry });
        }
        if let Some(obj) = entry.as_object_mut() {
            if !matches!(obj.get("ts"), Some(serde_json::Value::Number(_))) {
                obj.insert("ts".into(), chrono::Utc::now().timestamp_millis().into());
            }
        }

        let mut buf = buf.lock().unwrap();
        buf.items.push_back(entry);
        while buf.items.len() > cap {
            buf.items.pop_front();
        }
    }

    fn prune(&self, now_ms: i64) {
        let cutoff = now_ms - RETAIN_MS;
        for buf in [
            &self.transcripts,
            &self.sentiments,
            &self.predictions,
        ] {
            let mut buf = buf.lock().unwrap();
            while let Some(front) = buf.items.front() {
                let ts = front.get("ts").and_then(|t| t.as_i64()).unwrap_or(0);
                if ts < cutoff {
                    buf.items.pop_front();
                } else {
                    break;
                }
            }
        }
    }

    /// Snapshot of everything recorded since 00:00 UTC today — the answer to
    /// "what did the econ news deliver today".
    pub fn snapshot_today(&self) -> serde_json::Value {
        let now = chrono::Utc::now().timestamp_millis();
        self.prune(now);
        let day_start = chrono::Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|d| d.and_utc().timestamp_millis())
            .unwrap_or(0);

        let since = |buf: &Mutex<KindBuf>| -> Vec<serde_json::Value> {
            buf.lock()
                .unwrap()
                .items
                .iter()
                .filter(|v| v.get("ts").and_then(|t| t.as_i64()).unwrap_or(0) >= day_start)
                .cloned()
                .collect()
        };

        let transcripts = since(&self.transcripts);
        let sentiments = since(&self.sentiments);
        let predictions = since(&self.predictions);
        let health = self.health.lock().unwrap().clone();

        serde_json::json!({
            "day_start": day_start,
            "generated": now,
            "transcripts": transcripts,
            "sentiments": sentiments,
            "predictions": predictions,
            "health": health,
            "counts": {
                "transcripts": transcripts.len(),
                "sentiments": sentiments.len(),
                "predictions": predictions.len(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_transcripts_and_reports_todays_counts() {
        let cache = AiCache::new();
        cache.record(&WsFrame::Transcript {
            data: serde_json::json!({"text": "higher for longer", "ts": 1_700_000_000_000i64, "tier": "medium"}),
        });
        cache.record(&WsFrame::Sentiment {
            data: serde_json::json!({"hawkish": 0.8, "dovish": 0.1, "neutral": 0.1}),
        });
        let snap = cache.snapshot_today();
        // The sentiment frame had no ts, so the cache stamps one.
        assert!(snap["sentiments"][0]["ts"].is_number());
        assert!(snap["counts"]["sentiments"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn stale_entries_are_pruned() {
        let cache = AiCache::new();
        // Far in the past -> dropped by the 48h prune.
        cache.record(&WsFrame::Transcript {
            data: serde_json::json!({"text": "ancient", "ts": 1_000_000_000_000i64}),
        });
        let snap = cache.snapshot_today();
        assert!(snap["transcripts"].as_array().unwrap().is_empty());
    }

    #[test]
    fn health_is_kept_verbatim_plus_timestamp() {
        let cache = AiCache::new();
        cache.record(&WsFrame::Health {
            data: serde_json::json!({"rss_mb": 1870.7, "session_id": "ai-01"}),
        });
        let snap = cache.snapshot_today();
        assert_eq!(snap["health"]["rss_mb"], 1870.7);
        assert_eq!(snap["health"]["session_id"], "ai-01");
        assert!(snap["health"]["ts"].is_number());
    }
}
