//! Feed liveness, freshness and process identity.
//!
//! Two different questions live here, and C1 v2 keeps them apart:
//!
//! * **feed state** (`FeedStatus`) — is the upstream socket/mirror connected?
//!   Reported in `/status`; the calendar loop uses it to describe its own
//!   health.
//! * **freshness** (`FreshnessTracker`) — is the *market data* usable right
//!   now? `live` / `stale` / `offline` / `closed_market`, which is what a
//!   consumer must gate decisions on.
//!
//! Nothing in this module broadcasts. The status poller in `ws_server` diffs
//! the two and publishes a `status` frame on every transition, so there is
//! exactly one publisher for that topic.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use serde_json::json;

use crate::types::Freshness;

/// Liveness of every upstream feed.
#[derive(Clone, Default)]
pub struct FeedStatus {
    feeds: Arc<DashMap<String, FeedState>>,
}

#[derive(Clone, Debug)]
struct FeedState {
    state: String,
    last_msg: i64,
    msgs: u64,
}

impl FeedStatus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Move a feed to `state` ("starting" | "connected" | "error" | "off").
    /// A repeat of the current state is a no-op, so the fingerprint below only
    /// changes on a real transition.
    pub fn set(&self, feed: &str, state: &str) {
        let mut entry = self.feeds.entry(feed.to_string()).or_insert(FeedState {
            state: String::new(),
            last_msg: 0,
            msgs: 0,
        });
        if entry.state == state {
            return;
        }
        entry.state = state.to_string();
    }

    /// Record an inbound message on a feed.
    pub fn mark_msg(&self, feed: &str) {
        if let Some(mut entry) = self.feeds.get_mut(feed) {
            entry.last_msg = chrono::Utc::now().timestamp_millis();
            entry.msgs += 1;
        }
    }

    /// `{ "<feed>": { "state", "last_msg", "msgs" } }`
    pub fn snapshot(&self) -> serde_json::Value {
        let mut feeds = serde_json::Map::new();
        for entry in self.feeds.iter() {
            feeds.insert(
                entry.key().clone(),
                json!({
                    "state": entry.value().state,
                    "last_msg": entry.value().last_msg,
                    "msgs": entry.value().msgs,
                }),
            );
        }
        serde_json::Value::Object(feeds)
    }

    /// A stable, order-independent string of every feed's state. The status
    /// poller compares it to decide whether a transition happened.
    pub fn fingerprint(&self) -> String {
        let mut states: BTreeMap<String, String> = BTreeMap::new();
        for entry in self.feeds.iter() {
            states.insert(entry.key().clone(), entry.value().state.clone());
        }
        states
            .into_iter()
            .map(|(feed, state)| format!("{feed}={state}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// The market feed's freshness, as C1 v2 §4 defines it.
#[derive(Clone)]
pub struct FreshnessTracker {
    inner: Arc<Mutex<Tracked>>,
}

struct Tracked {
    feed: String,
    connected: bool,
    last_tick_ms: i64,
}

impl FreshnessTracker {
    pub fn new(feed: &str) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Tracked {
                feed: feed.to_string(),
                connected: false,
                last_tick_ms: 0,
            })),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Tracked> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn set_connected(&self, connected: bool) {
        self.lock().connected = connected;
    }

    pub fn mark_tick(&self, ts_ms: i64) {
        let mut tracked = self.lock();
        tracked.connected = true;
        tracked.last_tick_ms = tracked.last_tick_ms.max(ts_ms);
    }

    /// Classify the feed at `now_ms`.
    pub fn snapshot(&self, now_ms: i64, market_open: bool) -> Freshness {
        let tracked = self.lock();
        Freshness::evaluate(
            &tracked.feed,
            tracked.connected,
            tracked.last_tick_ms,
            now_ms,
            market_open,
        )
    }
}

/// Identity of this process. `boot_id` is generated once at boot and never
/// changes, so a consumer can tell a restart from a reconnect.
#[derive(Clone)]
pub struct BootInfo {
    pub boot_id: String,
    pub started_ms: i64,
}

impl BootInfo {
    pub fn new(now_ms: i64) -> Self {
        Self {
            // Boot time plus pid: unique across restarts on one host, constant
            // for the lifetime of the process.
            boot_id: format!("boot-{now_ms}-{}", std::process::id()),
            started_ms: now_ms,
        }
    }

    pub fn uptime_ms(&self, now_ms: i64) -> i64 {
        (now_ms - self.started_ms).max(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FreshnessState, FRESH_AGE_MS};

    const NOW: i64 = 1_792_781_867_153;

    #[test]
    fn feed_states_are_reported_per_feed() {
        let status = FeedStatus::new();
        status.set("sifting", "connecting");
        status.set("calendar", "starting");

        let snapshot = status.snapshot();
        assert_eq!(snapshot["sifting"]["state"], "connecting");
        assert_eq!(snapshot["calendar"]["state"], "starting");
        assert_eq!(snapshot["sifting"]["msgs"], 0);

        status.mark_msg("sifting");
        assert_eq!(status.snapshot()["sifting"]["msgs"], 1);
    }

    #[test]
    fn the_fingerprint_only_moves_on_a_real_transition() {
        let status = FeedStatus::new();
        status.set("sifting", "connected");
        let first = status.fingerprint();
        status.set("sifting", "connected");
        assert_eq!(
            status.fingerprint(),
            first,
            "a repeated state is not a transition"
        );
        status.set("sifting", "error");
        assert_ne!(status.fingerprint(), first);
        assert_eq!(status.fingerprint(), "sifting=error");
    }

    #[test]
    fn the_fingerprint_is_order_independent() {
        let a = FeedStatus::new();
        a.set("sifting", "connected");
        a.set("calendar", "connected");

        let b = FeedStatus::new();
        b.set("calendar", "connected");
        b.set("sifting", "connected");

        assert_eq!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn freshness_moves_through_offline_live_stale_and_closed() {
        let tracker = FreshnessTracker::new("sifting");

        assert_eq!(
            tracker.snapshot(NOW, true).state,
            FreshnessState::Offline,
            "no socket, no ticks"
        );

        tracker.set_connected(true);
        assert_eq!(
            tracker.snapshot(NOW, true).state,
            FreshnessState::Offline,
            "connected but silent is offline until the first tick"
        );

        tracker.mark_tick(NOW - 1_000);
        let live = tracker.snapshot(NOW, true);
        assert_eq!(live.state, FreshnessState::Live);
        assert_eq!(live.age_ms, 1_000);
        assert_eq!(live.feed, "sifting");

        let stale = tracker.snapshot(NOW + FRESH_AGE_MS, true);
        assert_eq!(stale.state, FreshnessState::Stale);

        tracker.set_connected(false);
        assert_eq!(tracker.snapshot(NOW, true).state, FreshnessState::Offline);

        tracker.set_connected(true);
        assert_eq!(
            tracker.snapshot(NOW, false).state,
            FreshnessState::ClosedMarket
        );
    }

    #[test]
    fn ticks_never_move_freshness_backwards() {
        let tracker = FreshnessTracker::new("sifting");
        tracker.mark_tick(1_000);
        tracker.mark_tick(500);
        assert_eq!(tracker.snapshot(1_000, true).last_tick_ms, 1_000);
    }

    #[test]
    fn boot_identity_is_stable_and_unique_looking() {
        let boot = BootInfo::new(NOW);
        assert!(boot.boot_id.starts_with("boot-1792781867153-"));
        assert_eq!(boot.uptime_ms(NOW + 1_000), 1_000);
        assert_eq!(boot.uptime_ms(1), 0, "a clock that steps back reads as zero");
    }
}
