//! The one publication path.
//!
//! Every live frame is stamped here, so `seq` is strictly increasing across
//! topics by construction: candles, tick volume, calendar and status all draw
//! from the same [`Sequencer`]. Ordering inside a tick is fixed too —
//! finalised bars are published before the buckets that follow them, which is
//! what makes `closed == true` arrive exactly once *before* the next bucket's
//! first frame.

use tokio::sync::broadcast;

use crate::candles::TickEvent;
use crate::status::FreshnessTracker;
use crate::tick_volume::TickVolumeStore;
use crate::types::{Publication, Sequencer, Topic};

/// Write side of the market feed.
#[derive(Clone)]
pub struct MarketPublisher {
    tx: broadcast::Sender<Publication>,
    seq: Sequencer,
    tick_store: TickVolumeStore,
    freshness: FreshnessTracker,
}

impl MarketPublisher {
    pub fn new(
        tx: broadcast::Sender<Publication>,
        seq: Sequencer,
        tick_store: TickVolumeStore,
        freshness: FreshnessTracker,
    ) -> Self {
        Self {
            tx,
            seq,
            tick_store,
            freshness,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Publication> {
        self.tx.subscribe()
    }

    /// Publish one accepted tick: closed frames first, then the live buckets.
    ///
    /// A rejected tick publishes nothing — a dropped tick must never move a
    /// consumer's chart.
    pub fn publish_tick(&self, event: &TickEvent) {
        if !event.accepted() {
            return;
        }
        for candle in &event.closed {
            self.publish(Topic::Candle, candle);
        }
        if let Some(bar) = &event.tick_volume_closed {
            self.tick_store.upsert(bar.clone());
            self.publish(Topic::TickVolume, bar);
        }
        for candle in &event.live {
            self.publish(Topic::Candle, candle);
        }
        if let Some(bar) = &event.tick_volume {
            self.tick_store.upsert(bar.clone());
            self.publish(Topic::TickVolume, bar);
        }
        if let Some(ts) = event.last_tick_ts {
            self.freshness.mark_tick(ts);
        }
    }

    pub fn publish_calendar(&self, data: serde_json::Value) {
        let frame = Publication::live(Topic::Calendar.as_str(), self.seq.issue(), data);
        let _ = self.tx.send(frame);
    }

    pub fn publish_status(&self, data: serde_json::Value) {
        let frame = Publication::live(Topic::Status.as_str(), self.seq.issue(), data);
        let _ = self.tx.send(frame);
    }

    pub fn set_feed_connected(&self, connected: bool) {
        self.freshness.set_connected(connected);
    }

    fn publish<T: serde::Serialize>(&self, topic: Topic, data: &T) {
        let Ok(value) = serde_json::to_value(data) else {
            return;
        };
        let frame = Publication::live(topic.as_str(), self.seq.issue(), value);
        let _ = self.tx.send(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candles::{CandleBook, TickCounters};
    use crate::types::{Candle, Interval, TickVolumeBar};

    fn candle(interval: Interval, time: i64, closed: bool) -> Candle {
        Candle {
            symbol: crate::types::SYMBOL.into(),
            interval: interval.as_str().into(),
            time,
            open: 3380.0,
            high: 3381.0,
            low: 3379.0,
            close: 3380.5,
            volume: 3.0,
            source: "sifting".into(),
            closed,
        }
    }

    fn bar(time: i64, closed: bool) -> TickVolumeBar {
        TickVolumeBar {
            symbol: crate::types::SYMBOL.into(),
            interval: Interval::M15.as_str().into(),
            time,
            ticks: 3,
            up_ticks: 1,
            down_ticks: 1,
            flat_ticks: 1,
            close: 3380.5,
            last_tick: time + 1,
            ticks_per_sec: 0.3,
            closed,
            source: "sifting".into(),
        }
    }

    fn event() -> TickEvent {
        TickEvent {
            dropped: None,
            closed: vec![
                candle(Interval::M1, 0, true),
                candle(Interval::M15, 0, true),
            ],
            live: vec![
                candle(Interval::M1, 60_000, false),
                candle(Interval::M15, 60_000, false),
            ],
            tick_volume_closed: Some(bar(0, true)),
            tick_volume: Some(bar(60_000, false)),
            last_tick_ts: Some(60_000),
        }
    }

    fn publisher() -> (MarketPublisher, TickVolumeStore) {
        let (tx, _rx) = broadcast::channel(64);
        let store = TickVolumeStore::new(16);
        let publisher = MarketPublisher::new(
            tx,
            Sequencer::new(),
            store.clone(),
            FreshnessTracker::new("sifting"),
        );
        (publisher, store)
    }

    #[test]
    fn a_tick_publishes_closed_bars_before_the_next_bucket() {
        let (publisher, store) = publisher();
        let mut rx = publisher.subscribe();
        publisher.publish_tick(&event());

        let mut frames = Vec::new();
        while let Ok(frame) = rx.try_recv() {
            frames.push(frame);
        }
        let kinds: Vec<&str> = frames.iter().map(|frame| frame.kind.as_str()).collect();
        assert_eq!(
            kinds,
            vec![
                "candle",
                "candle",
                "tick_volume",
                "candle",
                "candle",
                "tick_volume"
            ]
        );
        assert!(
            frames[0].data.as_ref().unwrap()["closed"]
                .as_bool()
                .unwrap()
        );
        assert!(
            !frames[3].data.as_ref().unwrap()["closed"]
                .as_bool()
                .unwrap()
        );

        let seqs: Vec<u64> = frames.iter().filter_map(|frame| frame.seq).collect();
        assert_eq!(seqs.len(), frames.len(), "every live frame is numbered");
        assert!(
            seqs.windows(2).all(|pair| pair[0] < pair[1]),
            "seq must be strictly increasing: {seqs:?}"
        );

        // The live store holds the closed bar and the in-progress one.
        let stored = store.snapshot();
        assert_eq!(stored.len(), 2);
        assert!(stored[0].closed);
        assert!(!stored[1].closed);
    }

    #[test]
    fn a_rejected_tick_publishes_nothing() {
        let (publisher, store) = publisher();
        let mut rx = publisher.subscribe();
        let dropped = TickEvent {
            dropped: Some(crate::candles::DropReason::Late),
            ..TickEvent::default()
        };
        publisher.publish_tick(&dropped);
        assert!(rx.try_recv().is_err(), "a dropped tick must be silent");
        assert!(store.is_empty());
    }

    #[test]
    fn calendar_and_status_share_the_same_counter() {
        let (publisher, _store) = publisher();
        let mut rx = publisher.subscribe();
        publisher.publish_calendar(serde_json::json!({ "events": [] }));
        publisher.publish_status(serde_json::json!({ "contract_version": "2.0" }));

        let first = rx.try_recv().expect("calendar frame");
        let second = rx.try_recv().expect("status frame");
        assert_eq!(first.kind, "calendar");
        assert_eq!(second.kind, "status");
        assert_eq!(first.seq, Some(1));
        assert_eq!(second.seq, Some(2));
        assert!(!first.replay);
        assert!(first.seq.is_some());
    }

    #[test]
    fn freshness_is_marked_from_the_accepted_tick() {
        let (tx, _rx) = broadcast::channel(8);
        let freshness = FreshnessTracker::new("sifting");
        let publisher = MarketPublisher::new(
            tx,
            Sequencer::new(),
            TickVolumeStore::new(4),
            freshness.clone(),
        );
        publisher.publish_tick(&event());
        let snapshot = freshness.snapshot(60_000, true);
        assert_eq!(snapshot.last_tick_ms, 60_000);
        assert_eq!(snapshot.state, crate::types::FreshnessState::Live);
    }

    #[test]
    fn a_full_broadcast_buffer_never_blocks_a_closed_candle() {
        // Capacity 2 with a receiver that never reads: every send still
        // succeeds (tokio evicts the oldest frame), so a closed bar is never
        // dropped on the publisher's side. The *subscriber* is told how many
        // frames it missed, and the socket loop turns that into a `gap`.
        let (tx, mut rx) = broadcast::channel(2);
        let publisher = MarketPublisher::new(
            tx,
            Sequencer::new(),
            TickVolumeStore::new(4),
            FreshnessTracker::new("sifting"),
        );
        for step in 0..5 {
            let event = TickEvent {
                closed: vec![candle(Interval::M1, step as i64 * 60_000, true)],
                ..TickEvent::default()
            };
            publisher.publish_tick(&event);
        }
        match rx.try_recv() {
            Err(broadcast::error::TryRecvError::Lagged(dropped)) => {
                assert!(
                    dropped >= 3,
                    "expected at least 3 lost frames, got {dropped}"
                )
            }
            other => panic!("expected Lagged, got {other:?}"),
        }
    }

    #[test]
    fn a_book_feeds_the_publisher_end_to_end() {
        let book = CandleBook::new();
        let (publisher, store) = publisher();
        let mut rx = publisher.subscribe();

        let now = 1_700_000_500_000;
        let start = Interval::M15.align(now);
        for step in 0..3 {
            let event = book.on_tick(3380.0 + step as f64, start + step * 60_000, None, now);
            publisher.publish_tick(&event);
        }
        let mut candles = 0;
        while let Ok(frame) = rx.try_recv() {
            if frame.kind == "candle" {
                candles += 1;
            }
        }
        assert_eq!(
            candles, 11,
            "three ticks x three resolutions, plus two closed 1m bars"
        );
        assert_eq!(store.len(), 1);
        let counters: TickCounters = book.counters();
        assert_eq!(counters.accepted, 3);
        assert_eq!(book.bars(Interval::M1), 2);
    }
}
