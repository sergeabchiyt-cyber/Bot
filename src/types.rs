//! C1 v2 wire types — the Node 1 market feed.
//!
//! Node 1 publishes market data and the economic calendar. Nothing in this
//! module can describe a trade: there is no side, no stop, no target, no size,
//! no account state and no outbound order. The only client -> server frames
//! are `subscribe` and `heartbeat`; everything else on the socket is dropped.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// Signed feed contract version (C1 v2). Published in `/status` and in every
/// `status` frame; consumers fail closed on an unknown version.
pub const CONTRACT_VERSION: &str = "2.0";

/// The only instrument Node 1 serves.
pub const SYMBOL: &str = "XAUUSD";

/// Heartbeat cadence, both directions.
pub const HEARTBEAT_SECS: u64 = 30;

/// Broadcast buffer. A subscriber that falls further behind than this gets a
/// `gap` frame instead of silently missing candles.
pub const BROADCAST_CAPACITY: usize = 4_096;

/// Closed tick-volume bars retained for replay and `/tick-volume`.
pub const TICK_VOLUME_CAPACITY: usize = 2_000;

/// Age at which a connected feed stops being `live` and becomes `stale`.
pub const FRESH_AGE_MS: i64 = 60_000;

/// A tick priced more than this far ahead of the local clock is bogus.
pub const MAX_FUTURE_MS: i64 = 5_000;

/// Timestamps below this are seconds, not milliseconds (`1e12` ms is 2001-09).
const EPOCH_MS_FLOOR: i64 = 1_000_000_000_000;

// =====================================================================
// Resolutions
// =====================================================================

/// The bar resolutions Node 1 publishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Interval {
    M1,
    M15,
    H1,
}

impl Interval {
    /// Every published resolution, coarsest last.
    pub const ALL: [Interval; 3] = [Interval::M1, Interval::M15, Interval::H1];

    pub fn as_str(self) -> &'static str {
        match self {
            Interval::M1 => "1m",
            Interval::M15 => "15m",
            Interval::H1 => "1h",
        }
    }

    /// Strict parse of the wire spelling. `60m` is accepted as a synonym of
    /// `1h` because that is how the upstream history endpoint spells it.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "1m" => Some(Interval::M1),
            "15m" => Some(Interval::M15),
            "1h" | "60m" => Some(Interval::H1),
            _ => None,
        }
    }

    pub fn millis(self) -> i64 {
        match self {
            Interval::M1 => 60_000,
            Interval::M15 => 15 * 60_000,
            Interval::H1 => 60 * 60_000,
        }
    }

    /// Bucket-open timestamp for `ts_ms`.
    pub fn align(self, ts_ms: i64) -> i64 {
        ts_ms - ts_ms.rem_euclid(self.millis())
    }

    /// Position in `CandleBook`'s per-resolution arrays.
    pub fn index(self) -> usize {
        match self {
            Interval::M1 => 0,
            Interval::M15 => 1,
            Interval::H1 => 2,
        }
    }
}

// =====================================================================
// Candles and tick volume
// =====================================================================

/// One OHLCV bar. `time` is the bucket-open epoch millisecond.
///
/// `volume` is the tick count, the same unit SiftingIO's historical bars use,
/// so history and live bars plot on one scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candle {
    pub symbol: String,
    pub interval: String,
    /// Bucket-open epoch milliseconds, always `time % interval == 0`.
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    /// Tick count in the bucket.
    pub volume: f64,
    /// Which feed produced the bar: `"sifting"` or `"synthetic"` (CI only).
    pub source: String,
    /// `true` exactly once, on the frame that finalises the bucket.
    pub closed: bool,
}

impl Candle {
    /// A bar that may be published: finite positive OHLC, non-negative finite
    /// volume, a body inside the high/low range, the contract symbol, a known
    /// interval and a bucket-aligned open time.
    pub fn is_valid(&self) -> bool {
        let positive = |v: f64| v.is_finite() && v > 0.0;
        positive(self.open)
            && positive(self.high)
            && positive(self.low)
            && positive(self.close)
            && self.volume.is_finite()
            && self.volume >= 0.0
            && self.high >= self.low
            && self.high >= self.open
            && self.high >= self.close
            && self.low <= self.open
            && self.low <= self.close
            && self.symbol == SYMBOL
            && self.time > 0
            && Interval::parse(&self.interval).is_some_and(|iv| iv.align(self.time) == self.time)
    }
}

/// Tick volume for one 15m bucket of the SiftingIO spot stream.
///
/// "Tick volume" is the number of price updates in the bucket — the same unit
/// the matching candle's `volume` uses, so `ticks == candle.volume`. Spot
/// XAUUSD has no taker side, so the up/down split uses the tick rule (price
/// above / below the previous tick). It is an activity gauge, not order flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TickVolumeBar {
    pub symbol: String,
    pub interval: String,
    /// Bucket open (epoch ms). Identical to the candle's `time`.
    pub time: i64,
    /// Price updates in the bucket (== candle `volume`).
    pub ticks: u64,
    /// Ticks that printed above the previous tick.
    pub up_ticks: u64,
    /// Ticks that printed below the previous tick.
    pub down_ticks: u64,
    /// Ticks at an unchanged price (`ticks - up_ticks - down_ticks`).
    pub flat_ticks: u64,
    /// Latest price in the bucket (== candle `close`).
    pub close: f64,
    /// Epoch ms of the latest tick counted in this bucket.
    pub last_tick: i64,
    /// Rolling tick rate over the last 10 s of stream time.
    pub ticks_per_sec: f64,
    /// `true` exactly once, on the frame that finalises the bucket.
    pub closed: bool,
    pub source: String,
}

// =====================================================================
// Freshness
// =====================================================================

/// Health of the market feed. `closed_market` wins over everything else: a
/// silent feed at the weekend is expected, not broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshnessState {
    Live,
    Stale,
    Offline,
    ClosedMarket,
}

impl FreshnessState {
    pub fn as_str(self) -> &'static str {
        match self {
            FreshnessState::Live => "live",
            FreshnessState::Stale => "stale",
            FreshnessState::Offline => "offline",
            FreshnessState::ClosedMarket => "closed_market",
        }
    }
}

impl Serialize for FreshnessState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// `freshness` object of C1 v2 (§4 of the contract).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Freshness {
    pub feed: String,
    pub last_tick_ms: i64,
    pub age_ms: i64,
    pub state: FreshnessState,
    pub market_open: bool,
}

impl Freshness {
    /// Classify the feed at `now_ms`.
    ///
    /// * `closed_market` — outside the XAUUSD session, whatever the age is.
    /// * `offline` — the socket is down, or no tick has ever arrived.
    /// * `live` — connected and `age_ms <= 60_000` while the market is open.
    /// * `stale` — connected but silent for longer than that.
    pub fn evaluate(
        feed: &str,
        connected: bool,
        last_tick_ms: i64,
        now_ms: i64,
        market_open: bool,
    ) -> Self {
        let age_ms = (now_ms - last_tick_ms).max(0);
        let state = if !market_open {
            FreshnessState::ClosedMarket
        } else if !connected || last_tick_ms <= 0 {
            FreshnessState::Offline
        } else if age_ms <= FRESH_AGE_MS {
            FreshnessState::Live
        } else {
            FreshnessState::Stale
        };
        Self {
            feed: feed.to_string(),
            last_tick_ms,
            age_ms,
            state,
            market_open,
        }
    }
}

// =====================================================================
// Outbound envelope
// =====================================================================

/// The feed topics a client may subscribe to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Topic {
    Candle,
    TickVolume,
    Calendar,
    Status,
}

impl Topic {
    pub const ALL: [Topic; 4] = [
        Topic::Candle,
        Topic::TickVolume,
        Topic::Calendar,
        Topic::Status,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Topic::Candle => "candle",
            Topic::TickVolume => "tick_volume",
            Topic::Calendar => "calendar",
            Topic::Status => "status",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "candle" => Some(Topic::Candle),
            "tick_volume" => Some(Topic::TickVolume),
            "calendar" => Some(Topic::Calendar),
            "status" => Some(Topic::Status),
            _ => None,
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// One frame Node 1 sends.
///
/// Live frames carry a strictly increasing `seq` across all topics. Replay
/// frames (the burst a client gets right after `subscribe`) carry
/// `replay: true` and never a `seq`. `heartbeat` and `gap` are control frames
/// and carry neither.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Publication {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(skip_serializing_if = "is_false")]
    pub replay: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Publication {
    pub fn live(kind: &str, seq: u64, data: serde_json::Value) -> Self {
        Self {
            kind: kind.to_string(),
            seq: Some(seq),
            replay: false,
            data: Some(data),
        }
    }

    pub fn replay(kind: &str, data: serde_json::Value) -> Self {
        Self {
            kind: kind.to_string(),
            seq: None,
            replay: true,
            data: Some(data),
        }
    }

    /// Keepalive for proxies. Carries no payload and no sequence number.
    pub fn heartbeat() -> Self {
        Self {
            kind: "heartbeat".to_string(),
            seq: None,
            replay: false,
            data: None,
        }
    }

    /// A subscriber overran the broadcast buffer and lost `dropped` frames.
    pub fn gap(dropped: u64) -> Self {
        Self {
            kind: "gap".to_string(),
            seq: None,
            replay: false,
            data: Some(serde_json::json!({ "dropped": dropped })),
        }
    }

    /// The feed topic this frame belongs to, if it is a feed frame.
    pub fn topic(&self) -> Option<Topic> {
        Topic::parse(&self.kind)
    }
}

// =====================================================================
// Inbound frames
// =====================================================================

/// The only two frames a client may send.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum WsFrame {
    /// `{"type":"subscribe","topics":[...],"intervals":[...]}`
    ///
    /// `intervals` is optional and defaults to every published resolution; it
    /// filters the `candle` frames (both replay and live) so a 15m chart does
    /// not pay for 1m traffic.
    #[serde(rename = "subscribe")]
    Subscribe {
        #[serde(default)]
        topics: Vec<String>,
        #[serde(default)]
        intervals: Vec<String>,
    },

    #[serde(rename = "heartbeat")]
    Heartbeat,
}

/// Frame kinds that belong to an execution path. None of them is part of C1:
/// an inbound one is ignored and never rebroadcast, whatever it carries.
///
/// This list is deliberately spelled out instead of relying on "unknown frame
/// types are ignored", so the refusal is explicit, logged and tested.
pub const REJECTED_KINDS: [&str; 6] = [
    "trades",
    "trade_intent",
    "execution_hello",
    "execution_report",
    "order",
    "bubbles",
];

/// True when `raw` carries `type` from [`REJECTED_KINDS`].
pub fn is_rejected_kind(raw: &serde_json::Value) -> bool {
    raw.get("type")
        .and_then(|value| value.as_str())
        .is_some_and(|kind| {
            REJECTED_KINDS
                .iter()
                .any(|rejected| kind.eq_ignore_ascii_case(rejected))
        })
}

/// True when `ts_ms` is a plausible epoch-millisecond timestamp: positive and
/// not so small that it is really a seconds value.
pub fn is_epoch_ms(ts_ms: i64) -> bool {
    ts_ms >= EPOCH_MS_FLOOR
}

/// Monotonic publication counter shared by every publisher in the process.
///
/// One sequencer per process is what makes `seq` strictly increasing *across
/// topics*: candles, tick volume, calendar and status all draw from it.
#[derive(Debug, Clone)]
pub struct Sequencer {
    next: Arc<AtomicU64>,
}

impl Sequencer {
    /// The first published frame carries `seq = 1` (the schema's minimum).
    pub fn new() -> Self {
        Self {
            next: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn issue(&self) -> u64 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }
}

impl Default for Sequencer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle() -> Candle {
        Candle {
            symbol: SYMBOL.into(),
            interval: "15m".into(),
            time: 1_700_000_100_000 - 1_700_000_100_000 % 900_000,
            open: 3381.0,
            high: 3385.5,
            low: 3378.25,
            close: 3383.75,
            volume: 412.0,
            source: "sifting".into(),
            closed: true,
        }
    }

    #[test]
    fn interval_spellings_round_trip_and_align() {
        for interval in Interval::ALL {
            assert_eq!(Interval::parse(interval.as_str()), Some(interval));
            assert_eq!(
                interval.align(1_700_000_123_456),
                1_700_000_123_456 - 1_700_000_123_456 % interval.millis()
            );
            assert_eq!(interval.align(interval.millis() * 3), interval.millis() * 3);
        }
        assert_eq!(Interval::parse("60m"), Some(Interval::H1));
        assert_eq!(Interval::parse("5m"), None);
        assert_eq!(Interval::parse(""), None);
        assert_eq!([0, 1, 2], [Interval::M1, Interval::M15, Interval::H1].map(Interval::index));
    }

    #[test]
    fn a_well_formed_candle_is_publishable() {
        assert!(candle().is_valid());
    }

    #[test]
    fn impossible_candles_are_rejected() {
        let mut bad = candle();
        bad.high = bad.low - 1.0;
        assert!(!bad.is_valid(), "high below low");

        let mut bad = candle();
        bad.close = f64::NAN;
        assert!(!bad.is_valid(), "NaN close");

        let mut bad = candle();
        bad.open = 0.0;
        assert!(!bad.is_valid(), "zero open");

        let mut bad = candle();
        bad.volume = -1.0;
        assert!(!bad.is_valid(), "negative volume");

        let mut bad = candle();
        bad.time += 1;
        assert!(!bad.is_valid(), "off-grid timestamp");

        let mut bad = candle();
        bad.symbol = "XAGUSD".into();
        assert!(!bad.is_valid(), "wrong symbol");

        let mut bad = candle();
        bad.interval = "5m".into();
        assert!(!bad.is_valid(), "unknown interval");
    }

    #[test]
    fn candle_wire_format_carries_every_required_field() {
        let wire = serde_json::to_value(candle()).expect("serialize candle");
        for field in [
            "symbol", "interval", "time", "open", "high", "low", "close", "volume", "closed",
        ] {
            assert!(wire.get(field).is_some(), "missing {field}: {wire}");
        }
        assert_eq!(wire["symbol"], SYMBOL);
        assert_eq!(wire["interval"], "15m");
        assert_eq!(wire["closed"], true);
        assert_eq!(wire["volume"], 412.0);
    }

    #[test]
    fn live_frames_have_a_seq_and_replay_frames_do_not() {
        let live = serde_json::to_value(Publication::live("candle", 7, serde_json::json!({})))
            .expect("serialize live");
        assert_eq!(live["seq"], 7);
        assert!(live.get("replay").is_none(), "live frame carries replay: {live}");

        let replay =
            serde_json::to_value(Publication::replay("candle", serde_json::json!({}))).unwrap();
        assert_eq!(replay["replay"], true);
        assert!(replay.get("seq").is_none(), "replay frame carries seq: {replay}");

        let beat = serde_json::to_value(Publication::heartbeat()).unwrap();
        assert_eq!(beat, serde_json::json!({ "type": "heartbeat" }));

        let gap = serde_json::to_value(Publication::gap(12)).unwrap();
        assert_eq!(gap["type"], "gap");
        assert_eq!(gap["data"]["dropped"], 12);
    }

    #[test]
    fn no_published_frame_can_carry_trade_vocabulary() {
        let trading_keys = [
            "side",
            "stop_loss",
            "take_profit",
            "lots",
            "risk_usd",
            "action",
            "venue",
        ];
        let frames = [
            Publication::live("candle", 1, serde_json::to_value(candle()).unwrap()),
            Publication::live(
                "status",
                2,
                serde_json::json!({ "contract_version": CONTRACT_VERSION, "boot_id": "b" }),
            ),
            Publication::replay("calendar", serde_json::json!({ "events": [] })),
        ];
        for frame in frames {
            let text = serde_json::to_string(&frame).unwrap();
            for key in trading_keys {
                assert!(
                    !text.contains(&format!("\"{key}\"")),
                    "{key} leaked into a market frame: {text}"
                );
            }
        }
    }

    #[test]
    fn only_subscribe_and_heartbeat_parse_as_inbound_frames() {
        let frame: WsFrame =
            serde_json::from_value(serde_json::json!({"type":"subscribe","topics":["candle"]}))
                .unwrap();
        assert!(matches!(frame, WsFrame::Subscribe { .. }));

        let frame: WsFrame =
            serde_json::from_value(serde_json::json!({"type":"heartbeat"})).unwrap();
        assert!(matches!(frame, WsFrame::Heartbeat));

        // `intervals` is optional.
        let frame: WsFrame = serde_json::from_value(serde_json::json!({
            "type": "subscribe",
            "topics": ["candle"],
            "intervals": ["1h"]
        }))
        .unwrap();
        match frame {
            WsFrame::Subscribe { topics, intervals } => {
                assert_eq!(topics, vec!["candle"]);
                assert_eq!(intervals, vec!["1h"]);
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn every_execution_shaped_frame_is_recognised_and_refused() {
        for kind in REJECTED_KINDS {
            let raw = serde_json::json!({
                "type": kind,
                "data": { "symbol": SYMBOL, "entry": 2340.0 }
            });
            assert!(is_rejected_kind(&raw), "{kind} must be refused");
            assert!(
                serde_json::from_value::<WsFrame>(raw.clone()).is_err(),
                "{kind} must not parse as a C1 inbound frame"
            );
        }
        assert!(!is_rejected_kind(&serde_json::json!({"type": "subscribe"})));
        assert!(!is_rejected_kind(&serde_json::json!({})));
    }

    #[test]
    fn freshness_states_follow_the_contract_table() {
        let live = Freshness::evaluate("sifting", true, 1_000_000, 1_030_000, true);
        assert_eq!(live.state, FreshnessState::Live);
        assert_eq!(live.age_ms, 30_000);

        let stale = Freshness::evaluate("sifting", true, 1_000_000, 1_060_001, true);
        assert_eq!(stale.state, FreshnessState::Stale);

        let offline = Freshness::evaluate("sifting", false, 0, 1_000_000, true);
        assert_eq!(offline.state, FreshnessState::Offline);

        let silent = Freshness::evaluate("sifting", true, 1_000_000, 1_000_000, true);
        assert_eq!(silent.state, FreshnessState::Live);

        // A weekend silence is `closed_market`, not `stale`.
        let weekend = Freshness::evaluate("sifting", true, 1_000_000, 999_999_999, false);
        assert_eq!(weekend.state, FreshnessState::ClosedMarket);
        assert!(!weekend.market_open);

        // A clock that runs backwards must not produce a negative age.
        let skewed = Freshness::evaluate("sifting", true, 2_000_000, 1_900_000, true);
        assert_eq!(skewed.age_ms, 0);
    }

    #[test]
    fn freshness_serializes_with_the_contract_state_names() {
        let wire = serde_json::to_value(Freshness::evaluate(
            "sifting",
            true,
            1_000_000,
            1_100_000,
            false,
        ))
        .unwrap();
        assert_eq!(wire["state"], "closed_market");
        assert_eq!(wire["market_open"], false);
        assert_eq!(wire["feed"], "sifting");
        assert!(wire["age_ms"].as_i64().unwrap() > 60_000);
    }

    #[test]
    fn the_sequencer_only_moves_forward() {
        let sequencer = Sequencer::new();
        let first = sequencer.issue();
        assert_eq!(first, 1);
        let mut previous = first;
        for _ in 0..1_000 {
            let next = sequencer.issue();
            assert!(next > previous);
            previous = next;
        }
    }

    #[test]
    fn second_based_timestamps_are_recognised() {
        assert!(!is_epoch_ms(1_700_000_000));
        assert!(is_epoch_ms(1_700_000_000_000));
    }
}
