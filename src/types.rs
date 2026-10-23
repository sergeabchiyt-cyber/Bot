use serde::{Deserialize, Serialize};

// =====================================================================
// Volume profile
// =====================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpCandle {
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    /// Which feed produced this candle: "binance" | "sifting" | ...
    #[serde(default)]
    pub source: String,
}

// =====================================================================
// Tick volume
// =====================================================================

/// Real-time tick volume for one 15m bucket of the SiftingIO spot stream.
///
/// "Tick volume" is the number of price updates in the bucket — the same unit
/// SiftingIO's historical bars use for `v` — so `ticks` always equals the
/// matching candle's `volume` and history + live plot on one scale.
///
/// Spot XAUUSD has no taker side, so the up/down split uses the tick rule
/// (price above / below the previous tick) instead. It's an activity and
/// pressure gauge, not directional order flow, and it never feeds delta.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TickVolumeBar {
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
    #[serde(default)]
    pub source: String,
}

/// Audit trail for one computed profile: how the histogram was built.
///
/// Every field maps 1:1 onto a TradingView Fixed Range Volume Profile input,
/// so a client (or a human with the chart open) can check that the engine was
/// fed the same range, the same row model and the same value-area percentage
/// before blaming the numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileMeta {
    /// `"rows"` (TV "Number Of Rows" layout) or `"price"` (fixed $ rows).
    pub row_mode: String,
    /// Row height in price units actually used.
    pub row_height: f64,
    /// Number of histogram rows actually produced (TV may exceed the input).
    pub rows: usize,
    /// Profile high / low = TV's histogram top / bottom.
    pub range_high: f64,
    pub range_low: f64,
    /// Resolution of the bars fed into the histogram (`"1m"`, `"5m"`, `"15m"`).
    pub input_interval: String,
    /// How many of those bars fell inside the window.
    pub input_bars: usize,
    /// Total volume in the histogram (tick count).
    pub total_volume: f64,
    /// TV "Value Area Volume" (70 = 70%).
    pub va_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VpLevels {
    pub window: String,
    pub poc: f64,
    pub vah: f64,
    pub val: f64,
    /// Inclusive start of the window this profile was computed over (ms).
    #[serde(default)]
    pub start: i64,
    /// Exclusive end of the window this profile was computed over (ms).
    /// For PS this is the session close, so clients can see it roll.
    #[serde(default)]
    pub end: i64,
    /// When this profile was last recomputed (ms).
    pub timestamp: i64,
    /// `bullish` / `bearish` for swing-anchored profiles, `neutral` for the
    /// calendar windows retained for compatibility.
    #[serde(default)]
    pub direction: String,
    /// Best structural extremes used by a swing profile. These are absent on
    /// PW/PS/CW levels and included so clients can audit the anchor selection.
    #[serde(default)]
    pub swing_high: Option<f64>,
    #[serde(default)]
    pub swing_low: Option<f64>,
    /// The open price of the PW candle at its Sunday 18:00 NY start, when
    /// that exact boundary candle is present in the seed. `start` remains the
    /// window's epoch-millisecond timestamp for backwards compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sunday_open: Option<f64>,
    /// How this profile was built (row model, input resolution, range). Absent
    /// on payloads produced by older builds; clients must not require it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<ProfileMeta>,
}

// =====================================================================
// WebSocket frames
// =====================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsFrame {
    #[serde(rename = "levels")]
    Levels { data: VpLevels },

    #[serde(rename = "candle")]
    Candle { data: VpCandle },

    #[serde(rename = "tick_volume")]
    TickVolume { data: TickVolumeBar },

    #[serde(rename = "calendar")]
    Calendar { data: serde_json::Value },

    #[serde(rename = "status")]
    Status { data: serde_json::Value },

    /// Evaluation of the built-in strategy engine (price + volume profile +
    /// tick volume). Read-only: `/strategy` serves the same snapshot.
    #[serde(rename = "strategy")]
    Strategy { data: serde_json::Value },

    /// Demo (paper) account snapshot — balance, equity, margin, open position.
    /// Read-only in both directions: no frame here submits an order.
    #[serde(rename = "demo")]
    Demo { data: serde_json::Value },

    #[serde(rename = "subscribe")]
    Subscribe { topics: Vec<String> },

    #[serde(rename = "heartbeat")]
    Heartbeat,

    // ---- Node3 AI wire contract (ws_client.py) ----
    // Engine -> Node3: base64 PCM audio of live econ-news coverage.
    // `data` is `{"data": "<b64 f32le 16kHz mono>", "ts", "source", "event", ...}`
    // or a bare base64 string — Node3 accepts both shapes.
    #[serde(rename = "audio_chunk")]
    AudioChunk {
        #[serde(default)]
        data: serde_json::Value,
    },

    // Engine -> Node3: one online-learner sample `{"features": [..], "target": 0|1}`.
    #[serde(rename = "learn")]
    Learn {
        #[serde(default)]
        data: serde_json::Value,
    },

    // Node3 -> engine: Moonshine transcript `{"text", "ts", "tier"}`.
    #[serde(rename = "transcript")]
    Transcript {
        #[serde(default)]
        data: serde_json::Value,
    },

    // Node3 -> engine: FinBERT/FOMC sentiment `{"hawkish","dovish","neutral",...}`.
    #[serde(rename = "sentiment")]
    Sentiment {
        #[serde(default)]
        data: serde_json::Value,
    },

    // Node3 -> engine: periodic health report (RSS, model tiers, learner stats).
    #[serde(rename = "health")]
    Health {
        #[serde(default)]
        data: serde_json::Value,
    },

    // Node3 -> engine: learner prediction `{"proba", "threshold"}`.
    #[serde(rename = "prediction")]
    Prediction {
        #[serde(default)]
        data: serde_json::Value,
    },

    // Engine -> Node3 request frames. Node3 only subscribes to
    // `audio_chunk` + `learn`, so these bypass the topic filter like
    // heartbeat does — they are rare, targeted requests for the AI node.
    #[serde(rename = "sentiment_req")]
    SentimentReq {
        #[serde(default)]
        data: serde_json::Value,
    },

    #[serde(rename = "predict_req")]
    PredictReq {
        #[serde(default)]
        data: serde_json::Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_wire_format_is_tagged_and_includes_the_sunday_open_price() {
        let frame = WsFrame::Levels {
            data: VpLevels {
                window: "PW".into(),
                poc: 2_340.0,
                vah: 2_350.0,
                val: 2_330.0,
                start: 1_700_000_000_000,
                end: 1_700_360_000_000,
                timestamp: 1_700_360_000_001,
                direction: "neutral".into(),
                swing_high: None,
                swing_low: None,
                sunday_open: Some(2_341.25),
                meta: None,
            },
        };
        let wire = serde_json::to_value(&frame).expect("serialize WS levels");
        assert_eq!(wire["type"], "levels");
        assert_eq!(wire["data"]["window"], "PW");
        for field in ["poc", "vah", "val"] {
            assert!(wire["data"][field].is_number(), "missing {field}: {wire}");
        }
        assert_eq!(wire["data"]["sunday_open"], 2_341.25);
    }

    /// Node1 has no broker write path: an execution/fill frame is not a
    /// variant the market service can parse, so an inbound one is ignored
    /// rather than fanned out.
    #[test]
    fn trade_frames_are_not_part_of_the_node1_wire_contract() {
        let raw = serde_json::json!({
            "type": "trades",
            "data": {
                "trade_id": "n3-1",
                "symbol": "XAUUSD",
                "side": "buy",
                "size": 0.01,
                "entry": 2340.0,
                "sl": 2335.0,
                "tp": 2350.0,
                "status": "signal",
                "timestamp": 1_700_000_000_000_i64
            }
        });
        assert!(
            serde_json::from_value::<WsFrame>(raw).is_err(),
            "Node1 must not accept an execution/fill frame"
        );
    }

    /// Order flow was removed with the exchange aggTrade adapters: the
    /// `bubbles` topic is not part of the wire contract any more, so an old
    /// client subscribing to it gets nothing rather than a silent 404.
    #[test]
    fn orderflow_bubbles_are_gone_from_the_market_contract() {
        let raw = serde_json::json!({
            "type": "bubbles",
            "data": {
                "kind": "BUY_BUBBLE", "level": 2340.0, "strength": 20.0,
                "timestamp": 1_700_000_000_000_i64, "exchange": "binance"
            }
        });
        assert!(
            serde_json::from_value::<WsFrame>(raw).is_err(),
            "the removed order-flow `bubbles` frame must no longer parse"
        );
    }

    #[test]
    fn market_candle_frame_still_parses() {
        let candle = serde_json::json!({
            "type": "candle",
            "data": {
                "time": 1_700_000_000_000_i64, "open": 1.0, "high": 2.0,
                "low": 0.5, "close": 1.5, "volume": 10.0, "source": "sifting"
            }
        });
        let frame: WsFrame = serde_json::from_value(candle).unwrap();
        assert!(
            matches!(frame, WsFrame::Candle { .. }),
            "the market candle frame must still parse"
        );
    }

    /// The strategy and demo frames are read-only projections: a client can
    /// subscribe to them, but neither carries any way to submit an order.
    #[test]
    fn strategy_and_demo_frames_round_trip() {
        for (raw, expected) in [
            (
                serde_json::json!({"type": "strategy", "data": {"action": "buy"}}),
                "strategy",
            ),
            (
                serde_json::json!({"type": "demo", "data": {"leverage": 1000}}),
                "demo",
            ),
        ] {
            let frame: WsFrame = serde_json::from_value(raw).expect("market frame");
            let wire = serde_json::to_value(&frame).unwrap();
            assert_eq!(wire["type"], expected);
        }
    }
}
