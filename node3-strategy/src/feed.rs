//! Node 1 feed contract v2 (C1) consumer.
//!
//! Every frame passes these checks before it can reach the store or the
//! strategy:
//!   * `contract_version` must be `2.0`;
//!   * `symbol` must be `XAUUSD` when present, and required on candle and tick frames;
//!   * `boot_id` must be stable for the life of the connection. A change means
//!     Node 1 restarted and the stream is no longer continuous;
//!   * live frames carry a contiguous `seq`. Replay frames carry no `seq`.
//!     A gap, duplicate, or out-of-order sequence is a violation;
//!   * `gap: true` is a violation;
//!   * `freshness.state` must be one of `live`, `stale`, `offline`, `closed_market`;
//!   * candles must name a supported `interval` and carry a `closed` flag.
//!     Open candles update freshness but never reach the store.
//!
//! Any violation returns `Err`. The caller reconnects and never trades on the
//! frame. Sequence numbers are assumed to be contiguous across all live topics
//! of one connection.

use serde::Deserialize;
use serde_json::Value;

use crate::store::{interval_index, INTERVALS};
use crate::types::VpCandle;

pub const CONTRACT_VERSION: &str = "2.0";
pub const SYMBOL: &str = "XAUUSD";
const FRESHNESS_STATES: [&str; 4] = ["live", "stale", "offline", "closed_market"];

/// Freshness as reported by Node 1 on each frame.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Freshness {
    pub state: String,
    pub age_ms: Option<i64>,
    pub market_open: Option<bool>,
    pub last_tick_ms: Option<i64>,
}

impl Freshness {
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
struct Envelope {
    #[serde(rename = "type")]
    frame_type: String,
    #[serde(default)]
    contract_version: Option<String>,
    #[serde(default)]
    boot_id: Option<String>,
    #[serde(default)]
    seq: Option<u64>,
    #[serde(default)]
    replay: Option<bool>,
    #[serde(default)]
    gap: Option<bool>,
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    interval: Option<String>,
    #[serde(default)]
    closed: Option<bool>,
    #[serde(default)]
    freshness: Option<RawFreshness>,
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
    /// Known-harmless frame types that carry nothing Node 3 uses.
    Ignored,
}

/// Per-connection validator state. Create one for every new connection.
#[derive(Debug, Default)]
pub struct FeedValidator {
    boot_id: Option<String>,
    last_seq: Option<u64>,
    last_closed: [Option<i64>; 3],
}

impl FeedValidator {
    /// Validate one raw text frame. `Err` means the connection must be dropped.
    pub fn accept(&mut self, raw: &str) -> Result<FeedEvent, String> {
        let envelope: Envelope =
            serde_json::from_str(raw).map_err(|error| format!("malformed frame: {error}"))?;
        match envelope.frame_type.as_str() {
            "heartbeat" => Ok(FeedEvent::Heartbeat),
            "candle" => self.candle(envelope),
            "tick_volume" => self.tick_volume(envelope),
            "calendar" => self.calendar(envelope),
            "status" => self.status(envelope),
            _ => Ok(FeedEvent::Ignored),
        }
    }

    fn candle(&mut self, envelope: Envelope) -> Result<FeedEvent, String> {
        let freshness = self.check_envelope(&envelope, true)?;
        let replay = envelope.replay.unwrap_or(false);
        let label = envelope
            .interval
            .clone()
            .ok_or_else(|| "candle frame missing interval".to_string())?;
        let index = interval_index(&label)
            .ok_or_else(|| format!("unsupported candle interval '{label}'"))?;
        let interval: &'static str = INTERVALS[index];
        let closed = envelope
            .closed
            .ok_or_else(|| "candle frame missing closed flag".to_string())?;
        let candle: VpCandle = serde_json::from_value(envelope.data.clone())
            .map_err(|error| format!("malformed candle payload: {error}"))?;
        validate_candle(&candle)?;

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

    fn tick_volume(&mut self, envelope: Envelope) -> Result<FeedEvent, String> {
        let freshness = self.check_envelope(&envelope, true)?;
        let ticks = envelope
            .data
            .get("ticks")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let last_tick_ms = envelope.data.get("last_tick").and_then(Value::as_i64);
        Ok(FeedEvent::TickVolume {
            ticks,
            last_tick_ms,
            freshness,
        })
    }

    fn calendar(&mut self, envelope: Envelope) -> Result<FeedEvent, String> {
        let freshness = self.check_envelope(&envelope, false)?;
        let items = match &envelope.data {
            Value::Array(items) => items.len(),
            Value::Null => 0,
            _ => 1,
        };
        Ok(FeedEvent::Calendar { items, freshness })
    }

    fn status(&mut self, envelope: Envelope) -> Result<FeedEvent, String> {
        let freshness = self.check_envelope(&envelope, false)?;
        Ok(FeedEvent::Status { freshness })
    }

    /// Checks shared by every data frame. Returns the validated freshness.
    fn check_envelope(&mut self, envelope: &Envelope, require_symbol: bool) -> Result<Freshness, String> {
        match envelope.contract_version.as_deref() {
            Some(CONTRACT_VERSION) => {}
            Some(other) => {
                return Err(format!(
                    "unsupported contract_version '{other}', expected {CONTRACT_VERSION}"
                ))
            }
            None => return Err("missing contract_version (Node 1 v1 frame)".to_string()),
        }
        match envelope.symbol.as_deref() {
            Some(SYMBOL) => {}
            Some(other) => return Err(format!("unexpected symbol '{other}'")),
            None if require_symbol => return Err("missing symbol".to_string()),
            None => {}
        }
        let boot = envelope
            .boot_id
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "missing boot_id".to_string())?;
        let known = self.boot_id.clone();
        match known {
            Some(previous) if previous.as_str() != boot => {
                return Err(format!("boot_id changed from {previous} to {boot}"))
            }
            Some(_) => {}
            None => self.boot_id = Some(boot.to_string()),
        }

        let replay = envelope.replay.unwrap_or(false);
        if replay && envelope.seq.is_some() {
            return Err("replay frame must not carry seq".to_string());
        }
        if !replay {
            let seq = envelope
                .seq
                .ok_or_else(|| "live frame missing seq".to_string())?;
            if let Some(last) = self.last_seq {
                if seq != last + 1 {
                    return Err(format!("seq gap: expected {} got {seq}", last + 1));
                }
            }
            self.last_seq = Some(seq);
        }

        if envelope.gap == Some(true) {
            return Err("Node 1 reported a feed gap".to_string());
        }
        let raw = envelope
            .freshness
            .as_ref()
            .ok_or_else(|| "missing freshness".to_string())?;
        if !FRESHNESS_STATES.contains(&raw.state.as_str()) {
            return Err(format!("unknown freshness state '{}'", raw.state));
        }
        Ok(Freshness {
            state: raw.state.clone(),
            age_ms: raw.age_ms,
            market_open: raw.market_open,
            last_tick_ms: raw.last_tick_ms,
        })
    }
}

fn validate_candle(candle: &VpCandle) -> Result<(), String> {
    let finite = [candle.open, candle.high, candle.low, candle.close, candle.volume]
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

    fn live_frame(seq: u64, body: &str) -> String {
        format!(
            r#"{{"type":"candle","contract_version":"2.0","boot_id":"b1","seq":{seq},"replay":false,"symbol":"XAUUSD",{body}}}"#
        )
    }

    fn candle_body(interval: &str, time: i64, closed: bool, state: &str) -> String {
        format!(
            r#""interval":"{interval}","closed":{closed},"freshness":{{"state":"{state}","market_open":true,"age_ms":120}},"data":{{"time":{time},"open":4100.0,"high":4101.0,"low":4099.5,"close":4100.5,"volume":12.0,"source":"node1"}}"#
        )
    }

    #[test]
    fn accepts_a_live_closed_candle_and_marks_repeats_not_new() {
        let mut validator = FeedValidator::default();
        let first = validator
            .accept(&live_frame(1, &candle_body("15m", 900_000, true, "live")))
            .expect("first candle");
        assert!(matches!(first, FeedEvent::Candle { is_new: true, .. }));
        let repeat = validator
            .accept(&live_frame(2, &candle_body("15m", 900_000, true, "live")))
            .expect("repeat candle");
        assert!(matches!(repeat, FeedEvent::Candle { is_new: false, .. }));
    }

    #[test]
    fn open_candles_are_never_new_and_still_report_freshness() {
        let mut validator = FeedValidator::default();
        let event = validator
            .accept(&live_frame(1, &candle_body("1m", 60_000, false, "live")))
            .expect("open candle");
        match event {
            FeedEvent::Candle {
                closed,
                is_new,
                freshness,
                ..
            } => {
                assert!(!closed);
                assert!(!is_new);
                assert!(freshness.tradeable());
            }
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn rejects_v1_candles_and_ignores_v1_levels() {
        let mut validator = FeedValidator::default();
        // Node 1 v1 `levels` frames are never read by Node 3: ignored, not trusted.
        assert_eq!(
            validator.accept(r#"{"type":"levels","data":{}}"#).expect("levels"),
            FeedEvent::Ignored
        );
        // A v1 candle carries no contract envelope, so it must be rejected.
        let v1_candle = r#"{"type":"candle","interval":"15m","closed":true,"data":{"time":1,"open":1,"high":1,"low":1,"close":1,"volume":1}}"#;
        assert!(validator.accept(v1_candle).is_err());
    }

    #[test]
    fn rejects_sequence_gaps_and_duplicates() {
        let mut validator = FeedValidator::default();
        validator
            .accept(&live_frame(1, &candle_body("15m", 900_000, true, "live")))
            .expect("seq 1");
        let gap = validator.accept(&live_frame(3, &candle_body("15m", 1_800_000, true, "live")));
        assert!(gap.is_err());
        let duplicate = validator.accept(&live_frame(1, &candle_body("15m", 1_800_000, true, "live")));
        assert!(duplicate.is_err());
    }

    #[test]
    fn replay_frames_carry_no_seq_and_do_not_advance_it() {
        let mut validator = FeedValidator::default();
        let replay = r#"{"type":"candle","contract_version":"2.0","boot_id":"b1","replay":true,"symbol":"XAUUSD","interval":"1m","closed":true,"freshness":{"state":"live"},"data":{"time":60000,"open":1.0,"high":1.0,"low":1.0,"close":1.0,"volume":1.0}}"#;
        let event = validator.accept(replay).expect("replay");
        assert!(matches!(event, FeedEvent::Candle { replay: true, is_new: true, .. }));
        let with_seq = r#"{"type":"candle","contract_version":"2.0","boot_id":"b1","seq":4,"replay":true,"symbol":"XAUUSD","interval":"1m","closed":true,"freshness":{"state":"live"},"data":{"time":120000,"open":1.0,"high":1.0,"low":1.0,"close":1.0,"volume":1.0}}"#;
        assert!(validator.accept(with_seq).is_err());
        // The live stream starts at seq 1 after replay: replay did not consume numbers.
        validator
            .accept(&live_frame(1, &candle_body("1m", 180_000, true, "live")))
            .expect("live seq 1 after replay");
    }

    #[test]
    fn rejects_boot_id_change_and_feed_gap_flag() {
        let mut validator = FeedValidator::default();
        validator
            .accept(&live_frame(1, &candle_body("15m", 900_000, true, "live")))
            .expect("first");
        let restarted = r#"{"type":"candle","contract_version":"2.0","boot_id":"b2","seq":2,"replay":false,"symbol":"XAUUSD","interval":"15m","closed":true,"freshness":{"state":"live"},"data":{"time":1800000,"open":1.0,"high":1.0,"low":1.0,"close":1.0,"volume":1.0}}"#;
        assert!(validator.accept(restarted).is_err());

        let mut gap_validator = FeedValidator::default();
        let gap = r#"{"type":"status","contract_version":"2.0","boot_id":"b1","seq":1,"replay":false,"gap":true,"freshness":{"state":"live"}}"#;
        assert!(gap_validator.accept(gap).is_err());
    }

    #[test]
    fn rejects_wrong_symbol_interval_and_unknown_freshness() {
        let mut validator = FeedValidator::default();
        let wrong_symbol = format!(
            r#"{{"type":"candle","contract_version":"2.0","boot_id":"b1","seq":1,"replay":false,"symbol":"EURUSD",{}}}"#,
            candle_body("1m", 60_000, true, "live")
        );
        assert!(validator.accept(&wrong_symbol).is_err());
        assert!(validator
            .accept(&live_frame(1, &candle_body("5m", 60_000, true, "live")))
            .is_err());
        assert!(validator
            .accept(&live_frame(1, &candle_body("1m", 60_000, true, "weird")))
            .is_err());
    }

    #[test]
    fn stale_or_closed_market_is_reported_but_not_tradeable() {
        let mut validator = FeedValidator::default();
        let stale = format!(
            r#"{{"type":"tick_volume","contract_version":"2.0","boot_id":"b1","seq":1,"replay":false,"symbol":"XAUUSD","freshness":{{"state":"stale","market_open":true,"age_ms":90000}},"data":{{"ticks":5}}}}"#
        );
        match validator.accept(&stale).expect("tick") {
            FeedEvent::TickVolume { freshness, .. } => assert!(!freshness.tradeable()),
            other => panic!("unexpected {other:?}"),
        }
        let closed = r#"{"type":"status","contract_version":"2.0","boot_id":"b1","seq":2,"replay":false,"freshness":{"state":"closed_market","market_open":false}}"#;
        match validator.accept(closed).expect("status") {
            FeedEvent::Status { freshness } => assert!(!freshness.tradeable()),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn heartbeat_and_unknown_types_pass_without_envelope_checks() {
        let mut validator = FeedValidator::default();
        assert_eq!(
            validator.accept(r#"{"type":"heartbeat"}"#).expect("heartbeat"),
            FeedEvent::Heartbeat
        );
        assert_eq!(
            validator.accept(r#"{"type":"demo","data":{}}"#).expect("demo"),
            FeedEvent::Ignored
        );
    }
}
