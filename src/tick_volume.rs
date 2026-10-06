//! Bounded, shared history of live tick-volume bars.
//!
//! The SiftingIO stream adapter writes here *before* broadcasting, so a client
//! that subscribes mid-bucket gets the in-progress bar in its replay and then
//! sees every following update. Clients upsert by `time`, which makes the
//! overlap harmless.
//!
//! A std `Mutex` is used (not tokio's `RwLock`) because the writer is the
//! synchronous tick handler and every critical section is a few pointer moves;
//! the lock is never held across an `.await`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::types::TickVolumeBar;

#[derive(Clone)]
pub struct TickVolumeStore {
    inner: Arc<Mutex<VecDeque<TickVolumeBar>>>,
    capacity: usize,
}

impl TickVolumeStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(capacity.min(4096)))),
            capacity: capacity.max(1),
        }
    }

    /// Insert or replace the bar for `bar.time`, keeping bars sorted by time
    /// and never holding more than `capacity`.
    pub fn upsert(&self, bar: TickVolumeBar) {
        let mut buf = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match buf.back() {
            // Hot path: every tick updates the newest bar in place.
            Some(last) if last.time == bar.time => {
                *buf.back_mut().unwrap() = bar;
            }
            Some(last) if last.time < bar.time => buf.push_back(bar),
            None => buf.push_back(bar),
            // Out-of-order (not expected from the adapter, but stay correct).
            Some(_) => match buf.binary_search_by_key(&bar.time, |b| b.time) {
                Ok(i) => buf[i] = bar,
                Err(i) => buf.insert(i, bar),
            },
        }
        while buf.len() > self.capacity {
            buf.pop_front();
        }
    }

    /// All bars, oldest first.
    pub fn snapshot(&self) -> Vec<TickVolumeBar> {
        let buf = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        buf.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(time: i64, ticks: u64) -> TickVolumeBar {
        TickVolumeBar {
            time,
            ticks,
            up_ticks: 0,
            down_ticks: 0,
            flat_ticks: ticks,
            close: 1.0,
            last_tick: time,
            ticks_per_sec: 0.0,
            closed: false,
            source: "sifting".into(),
        }
    }

    #[test]
    fn upsert_replaces_the_live_bar_and_appends_new_ones() {
        let s = TickVolumeStore::new(10);
        s.upsert(bar(0, 1));
        s.upsert(bar(0, 2));
        s.upsert(bar(900_000, 1));
        let snap = s.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].ticks, 2);
        assert_eq!(snap[1].time, 900_000);
    }

    #[test]
    fn capacity_is_enforced_oldest_first() {
        let s = TickVolumeStore::new(3);
        for i in 0..5 {
            s.upsert(bar(i * 900_000, 1));
        }
        let times: Vec<i64> = s.snapshot().iter().map(|b| b.time).collect();
        assert_eq!(times, vec![1_800_000, 2_700_000, 3_600_000]);
    }

    #[test]
    fn out_of_order_bars_stay_sorted() {
        let s = TickVolumeStore::new(10);
        s.upsert(bar(1_800_000, 1));
        s.upsert(bar(0, 1));
        s.upsert(bar(1_800_000, 5));
        let snap = s.snapshot();
        assert_eq!(snap.iter().map(|b| b.time).collect::<Vec<_>>(), vec![0, 1_800_000]);
        assert_eq!(snap[1].ticks, 5);
    }
}
