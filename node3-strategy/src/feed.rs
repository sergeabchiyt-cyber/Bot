//! Node 1 feed contract v2 (C1) consumer.
//!
//! Wire shape, from `contracts/c1-market-feed.schema.json` on branch `Node1`:
//!   * every frame has a `type`;
//!   * live data frames (`candle`, `tick_volume`, `calendar`, `status`) carry a strictly
//!     increasing `seq` and no `replay` field;
//!   * replay frames carry `"replay": true` and no `seq`;
//!   * `heartbeat` has no payload; `gap` carries `data.dropped`;
//!   * the payload always lives in `data`. Candle and tick data carry `symbol`,
//!     `interval` and `closed`. `status` data carries `contract_version`, `boot_id`
//!     and `freshness`.
//!
//! Checks applied before any frame can reach the store or the strategy:
//!   * `seq` strictly increases across live frames of one connection, and replay
//!     frames never carry `seq`. Node 1 reports dropped frames with a `gap` frame,
//!     which is a violation;
//!   * `status` data must carry `contract_version` `2.0`, a stable `boot_id` and a
//!     known `freshness.state`. Candles and ticks take freshness from the last
//!     `status` frame. Before the first one it is `unknown`, which is never tradeable;
//!   * candles must name `XAUUSD`, a supported `interval` and a `closed` flag. Open
//!     candles update freshness but never reach the store.
//!
//! Any violation returns `Err`. The caller reconnects and never trades on the frame.

use serde::Deserialize;
use serde_json::Value;

use crate::store::{interval_index, INTERVALS};
use crate::types::VpCandle;

pub const CONTRACT_VERSION: &str = "2.0";
pub const SYMBOL: &str = "XAUUSD";
const FRESHNESS_STATES: [&str; 4] = ["live", "stale", "offline", "closed_market"];

/// Freshness as reported by Node 1 in `status` frames.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Freshness {
    pub state: String,
    pub age_ms: Option<i64>,
    pub market_open: Option<bool>,
    pub last_tick_ms: Option<i64>,
}

impl Freshness {
    /// Freshness before the first `status` frame of a connection.
    pub fn unknown() -> Self {
        Self {
            state: "unknown".to_string(),
            age_ms: None,
            market_open: None,
            last_tick_ms: None,
        }
    }

    /// Only a live feed during an open market may drive a trade decision.
    pub fn tradeable(&self) -> bool {
        self.state == "live" && self.market_open == Some(true)
    }
}

#[derive(Debug, Deserialize)]
struct RawFreshness {
    state: String,
    #[serde(default)]
    age_ms: Option<i64>,
    #[serde(default)]
    market_open: Option<bool>,
    #[serde(default)]
    last_tick_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct StatusData {
    #[serde(default)]
    contract_version: Option<String>,
    #[serde(default)]
    boot_id: Option<String>,
    #[serde(default)]
    freshness: Option<RawFreshness>,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(rename = "type")]
    frame_type: String,
    #[serde(default)]
    seq: Option<u64>,
    #[serde(default)]
    replay: Option<bool>,
    #[serde(default)]
    data: Value,
}

/// A validated frame, ready for the store or strategy.
#[derive(Debug, Clone, PartialEq)]
pub enum FeedEvent {
    Candle {
        interval: &'static str,
        candle: VpCandle,
        closed: bool,
        /// True only for a closed bar newer than any already seen for this interval.
        is_new: bool,
        replay: bool,
        freshness: Freshness,
    },
    TickVolume {
        ticks: u64,
        last_tick_ms: Option<i64>,
        freshness: Freshness,
    },
    Calendar {
        items: usize,
        freshness: Freshness,
    },
    Status {
        freshness: Freshness,
    },
    Heartbeat,
    /// Frame types Node 3 does not use (for example, future additions to contract v2).
    Ignored,
}

/// Per-connection validator state. Create one for every new connection.
#[derive(Debug)]
pub struct FeedValidator {
    boot_id: Option<String>,
    last_seq: Option<u64>,
    last_closed: [Option<i64>; 3],
    freshness: Freshness,
}

impl Default for FeedValidator {
    fn default() -> Self {
        Self {
            boot_id: None,
            last_seq: None,
            last_closed: [None; 3],
            freshness: Freshness::unknown(),
        }
    }
}

impl FeedValidator {
    /// Validate one raw text frame. `Err` means the connection must be dropped.
    pub fn accept(&mut self, raw: &str) -> Result<FeedEvent, String> {
        let envelope: Envelope =
            serde_json::from_str(raw).map_err(|error| format!("malformed frame: {error}"))?;
        match envelope.frame_type.as_str() {
            "heartbeat" => Ok(FeedEvent::Heartbeat),
            "gap" => Err(format!(
                "Node 1 reported a feed gap (dropped {})",
                dropped_count(&envelope)
            )),
            "candle" | "tick_volume" | "calendar" | "status" => {
                let replay = self.check_sequence(&envelope)?;
                match envelope.frame_type.as_str() {
                    "candle" => self.candle(&envelope, replay),
                    "tick_volume" => self.tick_volume(&envelope),
                    "calendar" => self.calendar(&envelope),
                    _ => self.status(&envelope),
                }
            }
            _ => Ok(FeedEvent::Ignored),
        }
    }

    /// Returns whether the frame is a replay frame. Live frames must carry a `seq` that
    /// strictly increases; replay frames must carry none.
    fn check_sequence(&mut self, envelope: &Envelope) -> Result<bool, String> {
        match envelope.replay {
            Some(true) => {
                if envelope.seq.is_some() {
                    return Err("replay frame must not carry seq".to_string());
                }
                Ok(true)
            }
            Some(false) => Err("replay field must be true when present".to_string()),
            None => {
                let seq = envelope
                    .seq
                    .ok_or_else(|| format!("live {} frame missing seq", envelope.frame_type))?;
                if let Some(last) = self.last_seq {
                    if seq <= last {
                        return Err(format!("seq must increase: got {seq} after {last}"));
                    }
                }
                self.last_seq = Some(seq);
                Ok(false)
            }
        }
    }

    fn candle(&mut self, envelope: &Envelope, replay: bool) -> Result<FeedEvent, String> {
        check_symbol(&envelope.data)?;
        let label = envelope
            .data
            .get("interval")
            .and_then(Value::as_str)
            .ok_or_else(|| "candle missing interval".to_string())?;
        let index = interval_index(label)
            .ok_or_else(|| format!("unsupported candle interval '{label}'"))?;
        let interval: &'static str = INTERVALS[index];
        let closed = envelope
            .data
            .get("closed")
            .and_then(Value::as_bool)
            .ok_or_else(|| "candle missing closed flag".to_string())?;
        let candle: VpCandle = serde_json::from_value(envelope.data.clone())
            .map_err(|error| format!("malformed candle payload: {error}"))?;
        validate_candle(&candle)?;

        let freshness = self.freshness.clone();
        if !closed {
            return Ok(FeedEvent::Candle {
                interval,
                candle,
                closed,
                is_new: false,
                replay,
                freshness,
            });
        }
        let last = &mut self.last_closed[index];
        let is_new = match *last {
            Some(time) => candle.time > time,
            None => true,
        };
        if is_new {
            *last = Some(candle.time);
        }
        Ok(FeedEvent::Candle {
            interval,
            candle,
            closed,
            is_new,
            replay,
            freshness,
        })
    }

    fn tick_volume(&mut self, envelope: &Envelope) -> Result<FeedEvent, String> {
        check_symbol(&envelope.data)?;
        let ticks = envelope
            .data
            .get("ticks")
            .and_then(Value::as_u64)
            .ok_or_else(|| "tick_volume missing ticks".to_string())?;
        let last_tick_ms = envelope.data.get("last_tick").and_then(Value::as_i64);
        Ok(FeedEvent::TickVolume {
            ticks,
            last_tick_ms,
            freshness: self.freshness.clone(),
        })
    }

    fn calendar(&mut self, envelope: &Envelope) -> Result<FeedEvent, String> {
        let items = match &envelope.data {
            Value::Array(items) => items.len(),
            Value::Object(map) => map
                .get("events")
                .and_then(Value::as_array)
                .map(Vec::len)
                .or_else(|| {
                    map.get("count")
                        .and_then(Value::as_u64)
                        .and_then(|count| usize::try_from(count).ok())
                })
                .unwrap_or(0),
            _ => 0,
        };
        Ok(FeedEvent::Calendar {
            items,
            freshness: self.freshness.clone(),
        })
    }

    fn status(&mut self, envelope: &Envelope) -> Result<FeedEvent, String> {
        let data: StatusData = serde_json::from_value(envelope.data.clone())
            .map_err(|error| format!("malformed status payload: {error}"))?;
        match data.contract_version.as_deref() {
            Some(CONTRACT_VERSION) => {}
            Some(other) => {
                return Err(format!(
                    "unsupported contract_version '{other}', expected {CONTRACT_VERSION}"
                ))
            }
            None => return Err("status frame missing contract_version".to_string()),
        }
        let boot = data
            .boot_id
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "status frame missing boot_id".to_string())?;
        match self.boot_id.clone() {
            Some(previous) if previous != boot => {
                return Err(format!("boot_id changed from {previous} to {boot}"));
            }
            Some(_) => {}
            None => self.boot_id = Some(boot),
        }
        let raw = data
            .freshness
            .ok_or_else(|| "status frame missing freshness".to_string())?;
        if !FRESHNESS_STATES.contains(&raw.state.as_str()) {
            return Err(format!("unknown freshness state '{}'", raw.state));
        }
        let freshness = Freshness {
            state: raw.state,
            age_ms: raw.age_ms,
            market_open: raw.market_open,
            last_tick_ms: raw.last_tick_ms,
        };
        self.freshness = freshness.clone();
        Ok(FeedEvent::Status { freshness })
    }
}

fn dropped_count(envelope: &Envelope) -> String {
    envelope
        .data
        .get("dropped")
        .and_then(Value::as_u64)
        .map_or_else(|| "unknown".to_string(), |count| count.to_string())
}

fn check_symbol(data: &Value) -> Result<(), String> {
    match data.get("symbol").and_then(Value::as_str) {
        Some(SYMBOL) => Ok(()),
        Some(other) => Err(format!("unexpected symbol '{other}'")),
        None => Err("frame data missing symbol".to_string()),
    }
}

pub(crate) fn validate_candle(candle: &VpCandle) -> Result<(), String> {
    let finite = [
        candle.open,
        candle.high,
        candle.low,
        candle.close,
        candle.volume,
    ]
    .iter()
    .all(|value| value.is_finite());
    if !finite
        || candle.time <= 0
        || candle.low <= 0.0
        || candle.high < candle.low
        || candle.volume < 0.0
    {
        return Err(format!("malformed candle at {}", candle.time));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn status_frame(seq: u64, state: &str, market_open: bool, boot: &str) -> String {
        json!({
            "type": "status",
            "seq": seq,
            "data": {
                "contract_version": "2.0",
                "boot_id": boot,
                "market_open": market_open,
                "freshness": {
                    "feed": "sifting",
                    "last_tick_ms": 1_792_781_867_153_i64,
                    "age_ms": 120,
                    "state": state,
                    "market_open": market_open
                }
            }
        })
        .to_string()
    }

    fn candle_data(interval: &str, time: i64, closed: bool) -> Value {
        json!({
            "time": time,
            "open": 4100.0,
            "high": 4101.0,
            "low": 4099.5,
            "close": 4100.5,
            "volume": 12.0,
            "symbol": "XAUUSD",
            "interval": interval,
            "closed": closed,
            "source": "node1"
        })
    }

    fn candle_frame(seq: u64, interval: &str, time: i64, closed: bool) -> String {
        json!({"type": "candle", "seq": seq, "data": candle_data(interval, time, closed)})
            .to_string()
    }

    fn replay_candle_frame(interval: &str, time: i64, closed: bool) -> String {
        json!({"type": "candle", "replay": true, "data": candle_data(interval, time, closed)})
            .to_string()
    }

    #[test]
    fn accepts_live_closed_candle_and_marks_repeats_not_new() {
        let mut validator = FeedValidator::default();
        validator
            .accept(&status_frame(1, "live", true, "b1"))
            .expect("status");
        let first = validator
            .accept(&candle_frame(2, "15m", 900_000, true))
            .expect("first candle");
        assert!(matches!(
            first,
            FeedEvent::Candle {
                is_new: true,
                replay: false,
                ..
            }
        ));
        let repeat = validator
            .accept(&candle_frame(3, "15m", 900_000, true))
            .expect("repeat candle");
        assert!(matches!(repeat, FeedEvent::Candle { is_new: false, .. }));
    }

    #[test]
    fn candles_before_the_first_status_are_not_tradeable() {
        let mut validator = FeedValidator::default();
        let event = validator
            .accept(&candle_frame(1, "1m", 60_000, true))
            .expect("candle");
        match event {
            FeedEvent::Candle { freshness, .. } => {
                assert_eq!(freshness.state, "unknown");
                assert!(!freshness.tradeable());
            }
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn open_candles_are_never_new() {
        let mut validator = FeedValidator::default();
        validator
            .accept(&status_frame(1, "live", true, "b1"))
            .expect("status");
        let open = validator
            .accept(&candle_frame(2, "1m", 60_000, false))
            .expect("open candle");
        assert!(matches!(
            open,
            FeedEvent::Candle {
                closed: false,
                is_new: false,
                ..
            }
        ));
    }

    #[test]
    fn status_freshness_is_carried_to_later_frames() {
        let mut validator = FeedValidator::default();
        match validator
            .accept(&status_frame(1, "closed_market", false, "b1"))
            .expect("status")
        {
            FeedEvent::Status { freshness } => {
                assert_eq!(freshness.state, "closed_market");
                assert!(!freshness.tradeable());
            }
            other => panic!("unexpected event {other:?}"),
        }
        match validator
            .accept(&candle_frame(2, "1m", 60_000, true))
            .expect("candle")
        {
            FeedEvent::Candle { freshness, .. } => assert_eq!(freshness.state, "closed_market"),
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn rejects_non_increasing_sequence() {
        let mut validator = FeedValidator::default();
        validator
            .accept(&status_frame(1, "live", true, "b1"))
            .expect("status");
        validator
            .accept(&candle_frame(5, "1m", 60_000, true))
            .expect("candle");
        assert!(
            validator
                .accept(&candle_frame(5, "1m", 120_000, true))
                .is_err(),
            "duplicate seq"
        );
        assert!(
            validator
                .accept(&candle_frame(4, "1m", 180_000, true))
                .is_err(),
            "out of order"
        );
    }

    #[test]
    fn replay_frames_carry_no_seq_and_live_frames_need_one() {
        let mut validator = FeedValidator::default();
        let replay_with_seq = json!({
            "type": "candle",
            "seq": 1,
            "replay": true,
            "data": candle_data("1m", 60_000, true)
        })
        .to_string();
        assert!(validator.accept(&replay_with_seq).is_err());
        let live_without_seq =
            json!({"type": "candle", "data": candle_data("1m", 60_000, true)}).to_string();
        assert!(validator.accept(&live_without_seq).is_err());
        let replay = validator
            .accept(&replay_candle_frame("1m", 60_000, true))
            .expect("replay");
        assert!(matches!(replay, FeedEvent::Candle { replay: true, .. }));
    }

    #[test]
    fn rejects_v1_status_without_contract_version() {
        let mut validator = FeedValidator::default();
        let v1_status = json!({
            "type": "status",
            "seq": 1,
            "data": {"boot_id": "b1", "freshness": {"state": "live"}}
        })
        .to_string();
        let error = validator.accept(&v1_status).expect_err("v1 status");
        assert!(error.contains("contract_version"), "{error}");
    }

    #[test]
    fn rejects_v1_candle_with_top_level_fields() {
        // v1 put symbol, interval and closed at the top level. v2 keeps them in data.
        let mut validator = FeedValidator::default();
        let v1_candle = json!({
            "type": "candle",
            "seq": 1,
            "symbol": "XAUUSD",
            "interval": "1m",
            "closed": true,
            "data": {
                "time": 60_000,
                "open": 1.0,
                "high": 1.0,
                "low": 1.0,
                "close": 1.0,
                "volume": 1.0
            }
        })
        .to_string();
        assert!(validator.accept(&v1_candle).is_err());
    }

    #[test]
    fn rejects_boot_id_change_within_one_connection() {
        let mut validator = FeedValidator::default();
        validator
            .accept(&status_frame(1, "live", true, "b1"))
            .expect("status");
        let error = validator
            .accept(&status_frame(2, "live", true, "b2"))
            .expect_err("restart");
        assert!(error.contains("boot_id changed"), "{error}");
    }

    #[test]
    fn rejects_unsupported_contract_version() {
        let mut validator = FeedValidator::default();
        let v3 = json!({
            "type": "status",
            "seq": 1,
            "data": {"contract_version": "3.0", "boot_id": "b1", "freshness": {"state": "live"}}
        })
        .to_string();
        assert!(validator.accept(&v3).is_err());
    }

    #[test]
    fn rejects_unknown_freshness_state() {
        let mut validator = FeedValidator::default();
        let bad_state = json!({
            "type": "status",
            "seq": 1,
            "data": {"contract_version": "2.0", "boot_id": "b1", "freshness": {"state": "weird"}}
        })
        .to_string();
        assert!(validator.accept(&bad_state).is_err());
    }

    #[test]
    fn rejects_standalone_gap_frame() {
        let mut validator = FeedValidator::default();
        let error = validator
            .accept(r#"{"type":"gap","data":{"dropped":12}}"#)
            .expect_err("gap");
        assert!(error.contains("dropped 12"), "{error}");
    }

    #[test]
    fn rejects_wrong_symbol_and_unknown_interval() {
        let mut validator = FeedValidator::default();
        validator
            .accept(&status_frame(1, "live", true, "b1"))
            .expect("status");
        let wrong_symbol = json!({
            "type": "candle",
            "seq": 2,
            "data": {"time": 60_000, "open": 1.0, "high": 1.0, "low": 1.0, "close": 1.0,
                     "volume": 1.0, "symbol": "XAGUSD", "interval": "1m", "closed": true}
        })
        .to_string();
        assert!(validator.accept(&wrong_symbol).is_err());
        assert!(validator
            .accept(&candle_frame(3, "5m", 300_000, true))
            .is_err());
    }

    #[test]
    fn tick_volume_uses_the_last_known_freshness() {
        let mut validator = FeedValidator::default();
        validator
            .accept(&status_frame(1, "stale", true, "b1"))
            .expect("status");
        let tick_frame = json!({
            "type": "tick_volume",
            "seq": 2,
            "data": {"symbol": "XAUUSD", "interval": "1m", "time": 60_000, "ticks": 5,
                     "last_tick": 60_500, "closed": false}
        })
        .to_string();
        match validator.accept(&tick_frame).expect("tick") {
            FeedEvent::TickVolume {
                ticks, freshness, ..
            } => {
                assert_eq!(ticks, 5);
                assert_eq!(freshness.state, "stale");
            }
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn heartbeat_needs_no_fields() {
        let mut validator = FeedValidator::default();
        assert!(matches!(
            validator
                .accept(r#"{"type":"heartbeat"}"#)
                .expect("heartbeat"),
            FeedEvent::Heartbeat
        ));
    }

    #[test]
    fn rejects_malformed_candle_prices() {
        let mut validator = FeedValidator::default();
        validator
            .accept(&status_frame(1, "live", true, "b1"))
            .expect("status");
        let inverted = json!({
            "type": "candle",
            "seq": 2,
            "data": {"time": 60_000, "open": 1.0, "high": 1.0, "low": 2.0, "close": 1.0,
                     "volume": 1.0, "symbol": "XAUUSD", "interval": "1m", "closed": true}
        })
        .to_string();
        assert!(validator.accept(&inverted).is_err());
    }
}
