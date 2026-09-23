use crate::types::WsFrame;
use dashmap::DashMap;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::debug;

/// Tracks the liveness of every upstream feed and broadcasts changes
/// as `WsFrame::Status` so the dashboard can show what is connected.
#[derive(Clone)]
pub struct FeedStatus {
    inner: Arc<FeedStatusInner>,
}

struct FeedStatusInner {
    tx: broadcast::Sender<WsFrame>,
    feeds: DashMap<String, FeedState>,
}

#[derive(Clone, Debug)]
struct FeedState {
    state: String,
    last_msg: i64,
    msgs: u64,
}

impl FeedStatus {
    pub fn new(tx: broadcast::Sender<WsFrame>) -> Self {
        Self {
            inner: Arc::new(FeedStatusInner {
                tx,
                feeds: DashMap::new(),
            }),
        }
    }

    /// Transition a feed to a new state ("starting" | "connected" |
    /// "disconnected" | "error: ..." | "off"). Broadcasts on change.
    pub fn set(&self, feed: &str, state: &str) {
        {
            let mut entry = self
                .inner
                .feeds
                .entry(feed.to_string())
                .or_insert(FeedState {
                    state: String::new(),
                    last_msg: 0,
                    msgs: 0,
                });
            if entry.state == state {
                return;
            }
            entry.state = state.to_string();
        }
        debug!(feed, state, "feed status changed");
        let _ = self.inner.tx.send(WsFrame::Status {
            data: self.snapshot(),
        });
    }

    /// Record an inbound message on a feed (no broadcast — too chatty).
    pub fn mark_msg(&self, feed: &str) {
        if let Some(mut e) = self.inner.feeds.get_mut(feed) {
            e.last_msg = chrono::Utc::now().timestamp_millis();
            e.msgs += 1;
        }
    }

    pub fn snapshot(&self) -> serde_json::Value {
        let mut feeds = serde_json::Map::new();
        for e in self.inner.feeds.iter() {
            feeds.insert(
                e.key().clone(),
                json!({
                    "state": e.value().state,
                    "last_msg": e.value().last_msg,
                    "msgs": e.value().msgs,
                }),
            );
        }
        json!({
            "feeds": feeds,
            "ts": chrono::Utc::now().timestamp_millis(),
        })
    }
}
