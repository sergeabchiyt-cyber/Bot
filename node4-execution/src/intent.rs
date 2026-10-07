//! The Node 3 ⇄ Node 4 execution protocol (version 1).
//!
//! This module is the only place that knows the wire schema. It mirrors
//! `docs/NODE3_NODE4_PROTOCOL.md`:
//!
//! * Node 3 → Node 4: `execution_hello_ack`, `trade_intent`, `heartbeat`,
//!   `execution_report_ack`.
//! * Node 4 → Node 3: `execution_hello`, `execution_report`, `heartbeat`.
//!
//! Node 4 validates an intent before it is allowed anywhere near a broker. It
//! may **reject** an unsafe or unsupported intent, but it must never silently
//! recompute the strategy: the stop-loss and take-profit Node 3 sent are
//! hard constraints.

use serde::{Deserialize, Serialize};

use crate::config::Config;

/// Intent schema version Node 4 accepts. Unknown versions fail closed.
pub const INTENT_SCHEMA_VERSION: u32 = 1;
/// Report schema version Node 4 emits.
pub const REPORT_SCHEMA_VERSION: u32 = 1;
/// Service name announced in `execution_hello`.
pub const SERVICE_NAME: &str = "xauusd-node4-execution";

/// Order types Node 4 can route to a broker. `market` is the only type the
/// current adapters implement; anything else is rejected as unsupported rather
/// than guessed at.
pub const SUPPORTED_ORDER_TYPES: &[&str] = &["market"];

/// Maximum accepted `intent_id` length. Broker comments are limited (MT5: 31
/// characters) so an unbounded ID would be truncated by the venue; the bridge
/// hashes long IDs instead, and this limit keeps the audit trail unambiguous.
pub const MAX_INTENT_ID_LEN: usize = 128;

// ---------------------------------------------------------------------------
// Node 3 → Node 4
// ---------------------------------------------------------------------------

/// One immutable strategy decision, as sent by Node 3.
///
/// The intent deliberately contains no venue, account, stake, lots, leverage,
/// slippage, MT5 login, or broker credential: those are Node 4's alone.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TradeIntent {
    pub schema_version: u32,
    pub intent_id: String,
    #[serde(default)]
    pub strategy: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub reference_price: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub risk_reward: f64,
    #[serde(default)]
    pub level_name: String,
    #[serde(default)]
    pub source_candle_time: i64,
    pub created_at: i64,
    pub expires_at: i64,
}

impl TradeIntent {
    pub fn is_expired_at(&self, timestamp_ms: i64) -> bool {
        self.expires_at <= timestamp_ms
    }

    /// Validate every field Node 4 relies on before a broker write.
    ///
    /// The checks are exactly the protocol's list: schema version, unique
    /// non-empty id, supported + explicitly mapped symbol, side, order type,
    /// finite prices/risk-reward, stop and target on the correct side, and the
    /// expiry clock. Venue *health* is a runtime condition and is checked by
    /// [`crate::engine::ExecutionEngine`] immediately before the write.
    pub fn validate(&self, config: &Config, now_ms: i64) -> Result<(), IntentRejection> {
        if self.schema_version != INTENT_SCHEMA_VERSION {
            return Err(IntentRejection::new(
                "unsupported_schema_version",
                format!(
                    "intent schema_version {} is not supported (this service speaks {})",
                    self.schema_version, INTENT_SCHEMA_VERSION
                ),
            ));
        }

        let intent_id = self.intent_id.trim();
        if intent_id.is_empty() {
            return Err(IntentRejection::new(
                "missing_intent_id",
                "intent_id is empty; Node 4 refuses an intent it cannot deduplicate",
            ));
        }
        if intent_id.len() > MAX_INTENT_ID_LEN {
            return Err(IntentRejection::new(
                "invalid_intent_id",
                format!(
                    "intent_id is {} characters; the maximum is {MAX_INTENT_ID_LEN}",
                    intent_id.len()
                ),
            ));
        }
        if !intent_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
        {
            return Err(IntentRejection::new(
                "invalid_intent_id",
                "intent_id may only contain ASCII alphanumerics, '-', '_', '.' and ':' \
                 (it becomes a broker comment and an audit key)",
            ));
        }

        if let Err(err) = config.resolve_symbol(&self.symbol) {
            return Err(IntentRejection::new("unsupported_symbol", err));
        }

        if !matches!(self.side.as_str(), "buy" | "sell") {
            return Err(IntentRejection::new(
                "unsupported_side",
                format!("side '{}' is not exactly 'buy' or 'sell'", self.side),
            ));
        }

        let order_type = self.order_type.trim().to_ascii_lowercase();
        if !SUPPORTED_ORDER_TYPES.contains(&order_type.as_str()) {
            return Err(IntentRejection::new(
                "unsupported_order_type",
                format!(
                    "order type '{}' is not supported (supported: {})",
                    self.order_type,
                    SUPPORTED_ORDER_TYPES.join(", ")
                ),
            ));
        }

        for (name, value) in [
            ("reference_price", self.reference_price),
            ("stop_loss", self.stop_loss),
            ("take_profit", self.take_profit),
            ("risk_reward", self.risk_reward),
        ] {
            if !value.is_finite() {
                return Err(IntentRejection::new(
                    "non_finite_price",
                    format!("{name} is not a finite number ({value})"),
                ));
            }
            if value <= 0.0 {
                return Err(IntentRejection::new(
                    "invalid_price",
                    format!("{name} must be positive, got {value}"),
                ));
            }
        }

        // Stop and take-profit must sit on the correct side of the strategy's
        // reference price. Node 4 never moves them; it only refuses to execute
        // a stop that would already be behind the entry.
        match self.side.as_str() {
            "buy" => {
                if self.stop_loss >= self.reference_price {
                    return Err(IntentRejection::new(
                        "invalid_stop_loss",
                        format!(
                            "buy stop loss {} is not below the reference price {}",
                            self.stop_loss, self.reference_price
                        ),
                    ));
                }
                if self.take_profit <= self.reference_price {
                    return Err(IntentRejection::new(
                        "invalid_take_profit",
                        format!(
                            "buy take profit {} is not above the reference price {}",
                            self.take_profit, self.reference_price
                        ),
                    ));
                }
            }
            "sell" => {
                if self.stop_loss <= self.reference_price {
                    return Err(IntentRejection::new(
                        "invalid_stop_loss",
                        format!(
                            "sell stop loss {} is not above the reference price {}",
                            self.stop_loss, self.reference_price
                        ),
                    ));
                }
                if self.take_profit >= self.reference_price {
                    return Err(IntentRejection::new(
                        "invalid_take_profit",
                        format!(
                            "sell take profit {} is not below the reference price {}",
                            self.take_profit, self.reference_price
                        ),
                    ));
                }
            }
            _ => unreachable!("side was validated above"),
        }

        // A risk/reward that disagrees with the prices by a wide margin means
        // one of the three numbers is not what the strategy meant; executing it
        // would silently change the trade's meaning.
        let risk = (self.reference_price - self.stop_loss).abs();
        let reward = (self.take_profit - self.reference_price).abs();
        if risk > 0.0 {
            let implied = reward / risk;
            if (implied - self.risk_reward).abs() > 0.25 * implied.max(1.0) {
                return Err(IntentRejection::new(
                    "risk_reward_mismatch",
                    format!(
                        "declared risk_reward {} does not match the prices ({} implied by \
                         stop {} / target {} around {})",
                        self.risk_reward,
                        implied,
                        self.stop_loss,
                        self.take_profit,
                        self.reference_price
                    ),
                ));
            }
        }

        if self.expires_at <= self.created_at {
            return Err(IntentRejection::new(
                "invalid_expiry",
                format!(
                    "expires_at {} is not after created_at {}",
                    self.expires_at, self.created_at
                ),
            ));
        }

        if self.is_expired_at(now_ms) {
            return Err(IntentRejection::new(
                "intent_expired",
                format!(
                    "intent expired at {} (now {now_ms}); an expired intent is never executed",
                    self.expires_at
                ),
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentRejection {
    pub code: String,
    pub message: String,
}

impl IntentRejection {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for IntentRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

// ---------------------------------------------------------------------------
// Node 4 → Node 3
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionStatus {
    /// Node 4 durably owns the ID; the broker result is still pending.
    Accepted,
    /// Broker-authoritative full fill.
    Filled,
    /// Broker-authoritative partial fill.
    Partial,
    /// Node 4 or the broker refused; nothing unknown reached the broker.
    Rejected,
    /// A write may have reached the broker; reconcile, never resend.
    Unknown,
    /// Pending order cancelled with broker confirmation.
    Cancelled,
    /// Position/deal lifecycle is complete.
    Closed,
}

impl ExecutionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionStatus::Accepted => "accepted",
            ExecutionStatus::Filled => "filled",
            ExecutionStatus::Partial => "partial",
            ExecutionStatus::Rejected => "rejected",
            ExecutionStatus::Unknown => "unknown",
            ExecutionStatus::Cancelled => "cancelled",
            ExecutionStatus::Closed => "closed",
        }
    }

    /// True when the broker confirmed a (possibly partial) fill.
    pub fn is_fill(&self) -> bool {
        matches!(self, ExecutionStatus::Filled | ExecutionStatus::Partial)
    }
}

/// Broker-authoritative report sent back to Node 3.
///
/// Field-for-field compatible with Node 3's `ExecutionReport`; the extra
/// ticket/retcode/duplicate fields are additive optionals that Node 3 ignores.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionReport {
    pub schema_version: u32,
    pub intent_id: String,
    pub status: ExecutionStatus,
    pub venue: String,
    pub symbol: String,
    pub side: String,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filled_price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity_unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_ticket: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deal_ticket: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_ticket: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retcode: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconciled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplicate: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl ExecutionReport {
    pub fn new(intent: &TradeIntent, status: ExecutionStatus, venue: &str, ts: i64) -> Self {
        Self {
            schema_version: REPORT_SCHEMA_VERSION,
            intent_id: intent.intent_id.clone(),
            status,
            venue: venue.to_string(),
            symbol: intent.symbol.clone(),
            side: intent.side.clone(),
            timestamp: ts,
            execution_id: None,
            filled_price: None,
            quantity: None,
            quantity_unit: None,
            order_ticket: None,
            deal_ticket: None,
            position_ticket: None,
            retcode: None,
            reconciled: None,
            duplicate: None,
            error_code: None,
            error_message: None,
        }
    }

    pub fn accepted(intent: &TradeIntent, venue: &str, ts: i64) -> Self {
        Self::new(intent, ExecutionStatus::Accepted, venue, ts)
    }

    pub fn rejected(
        intent: &TradeIntent,
        venue: &str,
        ts: i64,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        let mut report = Self::new(intent, ExecutionStatus::Rejected, venue, ts);
        report.error_code = Some(code.into());
        report.error_message = Some(message.into());
        report
    }

    /// A venue with no credentials is an explicit dry-run: the intent is still
    /// validated, persisted, and reported, but no order exists.
    pub fn signal_only(intent: &TradeIntent, venue: &str, ts: i64) -> Self {
        let mut report = Self::new(intent, ExecutionStatus::Accepted, venue, ts);
        report.error_code = Some("dry_run".into());
        report.error_message = Some(
            "execution venue is 'none': the intent was validated and recorded, and no broker \
             order was placed"
                .into(),
        );
        report
    }

    pub fn to_frame(&self) -> Option<String> {
        serde_json::to_string(&serde_json::json!({
            "type": "execution_report",
            "data": self,
        }))
        .ok()
    }
}

/// Result of asking a venue to reconcile an unclear write.
#[derive(Debug, Clone)]
pub struct ReconciliationOutcome {
    pub status: ExecutionStatus,
    pub source: String,
    pub detail: String,
    pub execution_id: Option<String>,
    pub filled_price: Option<f64>,
    pub quantity: Option<f64>,
    pub position_ticket: Option<i64>,
}

// ---------------------------------------------------------------------------
// Frame envelopes
// ---------------------------------------------------------------------------

/// Frame Node 3 sends to Node 4 on the private `/execution` socket.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node3Frame {
    /// Node 3's answer to `execution_hello`. Node 4 refuses to accept intents
    /// until `accepted == true`.
    ExecutionHelloAck {
        #[serde(default)]
        accepted: bool,
        #[serde(default)]
        protocol_version: Option<u32>,
        #[serde(default)]
        error: Option<String>,
    },
    TradeIntent {
        data: TradeIntent,
    },
    /// Node 3's answer to one `execution_report`.
    ExecutionReportAck {
        #[serde(default)]
        intent_id: String,
        #[serde(default)]
        accepted: bool,
        #[serde(default)]
        error: Option<String>,
    },
    Heartbeat,
    #[serde(other)]
    Unknown,
}

/// Frame Node 4 sends to Node 3.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node4Frame<'a> {
    ExecutionHello {
        token: &'a str,
        service: &'a str,
        protocol_version: u32,
    },
    Heartbeat,
}

impl Node4Frame<'_> {
    pub fn to_text(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"type\":\"heartbeat\"}".into())
    }
}

impl Node3Frame {
    pub fn parse(text: &str) -> Option<Self> {
        serde_json::from_str::<Node3Frame>(text).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn intent(side: &str) -> TradeIntent {
        let (reference, stop, target) = if side == "buy" {
            (2_650.25, 2_647.45, 2_656.25)
        } else {
            (2_650.25, 2_653.05, 2_644.25)
        };
        TradeIntent {
            schema_version: INTENT_SCHEMA_VERSION,
            intent_id: "n3-xauusd-1700000000000-buy-pw-poc".into(),
            strategy: "vp_break_retest_v1".into(),
            symbol: "XAUUSD".into(),
            side: side.into(),
            order_type: "market".into(),
            reference_price: reference,
            stop_loss: stop,
            take_profit: target,
            risk_reward: (target - reference).abs() / (reference - stop).abs(),
            level_name: "PW PoC".into(),
            source_candle_time: 1_700_000_000_000,
            created_at: 1_700_000_000_100,
            expires_at: 1_700_000_120_100,
        }
    }

    fn config() -> Config {
        Config {
            mt5_symbol_map: vec![("XAUUSD".into(), "XAUUSD.a".into())],
            ..Default::default()
        }
    }

    #[test]
    fn protocol_intent_round_trips_from_the_documented_payload() {
        let raw = r#"{
            "type": "trade_intent",
            "data": {
                "schema_version": 1,
                "intent_id": "n3-xauusd-1700000000000-buy-pw-poc",
                "strategy": "vp_break_retest_v1",
                "symbol": "XAUUSD",
                "side": "buy",
                "order_type": "market",
                "reference_price": 2650.25,
                "stop_loss": 2647.45,
                "take_profit": 2656.25,
                "risk_reward": 2.142857,
                "level_name": "PW PoC",
                "source_candle_time": 1700000000000,
                "created_at": 1700000000100,
                "expires_at": 1700000120100
            }
        }"#;
        let frame = Node3Frame::parse(raw).expect("frame parses");
        match frame {
            Node3Frame::TradeIntent { data } => {
                assert_eq!(data.intent_id, "n3-xauusd-1700000000000-buy-pw-poc");
                assert_eq!(data.side, "buy");
                assert!(data.validate(&config(), 1_700_000_001_000).is_ok());
            }
            other => panic!("unexpected frame: {other:?}"),
        }
    }

    #[test]
    fn hello_ack_requires_accepted_true() {
        let ok = Node3Frame::parse(
            r#"{"type":"execution_hello_ack","accepted":true,"protocol_version":1}"#,
        );
        match ok {
            Some(Node3Frame::ExecutionHelloAck { accepted, .. }) => assert!(accepted),
            other => panic!("unexpected: {other:?}"),
        }
        match Node3Frame::parse(
            r#"{"type":"execution_hello_ack","accepted":false,"error":"bad token"}"#,
        ) {
            Some(Node3Frame::ExecutionHelloAck {
                accepted, error, ..
            }) => {
                assert!(!accepted);
                assert_eq!(error.as_deref(), Some("bad token"));
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert!(matches!(
            Node3Frame::parse(r#"{"type":"something_else"}"#),
            Some(Node3Frame::Unknown)
        ));
    }

    #[test]
    fn validation_accepts_both_sides_and_rejects_broken_payloads() {
        let cfg = config();
        let now = 1_700_000_001_000;
        assert!(intent("buy").validate(&cfg, now).is_ok());
        assert!(intent("sell").validate(&cfg, now).is_ok());

        let mut bad = intent("buy");
        bad.schema_version = 2;
        assert_eq!(
            bad.validate(&cfg, now).unwrap_err().code,
            "unsupported_schema_version"
        );

        let mut bad = intent("buy");
        bad.intent_id = "   ".into();
        assert_eq!(
            bad.validate(&cfg, now).unwrap_err().code,
            "missing_intent_id"
        );

        let mut bad = intent("buy");
        bad.intent_id = "n3 intent with spaces".into();
        assert_eq!(
            bad.validate(&cfg, now).unwrap_err().code,
            "invalid_intent_id"
        );

        let mut bad = intent("buy");
        bad.symbol = "GOLD".into();
        assert_eq!(
            bad.validate(&cfg, now).unwrap_err().code,
            "unsupported_symbol"
        );

        let mut bad = intent("buy");
        bad.side = "long".into();
        assert_eq!(
            bad.validate(&cfg, now).unwrap_err().code,
            "unsupported_side"
        );

        let mut bad = intent("buy");
        bad.order_type = "limit".into();
        assert_eq!(
            bad.validate(&cfg, now).unwrap_err().code,
            "unsupported_order_type"
        );

        let mut bad = intent("buy");
        bad.risk_reward = f64::NAN;
        assert_eq!(
            bad.validate(&cfg, now).unwrap_err().code,
            "non_finite_price"
        );

        let mut bad = intent("buy");
        bad.reference_price = f64::INFINITY;
        assert_eq!(
            bad.validate(&cfg, now).unwrap_err().code,
            "non_finite_price"
        );

        let mut bad = intent("buy");
        bad.take_profit = 0.0;
        assert_eq!(bad.validate(&cfg, now).unwrap_err().code, "invalid_price");

        // Stop on the wrong side of the entry.
        let mut bad = intent("buy");
        bad.stop_loss = 2_660.0;
        assert_eq!(
            bad.validate(&cfg, now).unwrap_err().code,
            "invalid_stop_loss"
        );
        let mut bad = intent("sell");
        bad.stop_loss = 2_640.0;
        assert_eq!(
            bad.validate(&cfg, now).unwrap_err().code,
            "invalid_stop_loss"
        );

        // Target on the wrong side of the entry.
        let mut bad = intent("buy");
        bad.take_profit = 2_640.0;
        assert_eq!(
            bad.validate(&cfg, now).unwrap_err().code,
            "invalid_take_profit"
        );

        // A wildly inconsistent risk/reward is refused.
        let mut bad = intent("buy");
        bad.risk_reward = 9.0;
        assert_eq!(
            bad.validate(&cfg, now).unwrap_err().code,
            "risk_reward_mismatch"
        );

        // Expiry: `now` at or after `expires_at` is refused.
        let mut expired = intent("buy");
        expired.expires_at = now;
        assert_eq!(
            expired.validate(&cfg, now).unwrap_err().code,
            "intent_expired"
        );
        let mut backward = intent("buy");
        backward.expires_at = backward.created_at - 1;
        assert_eq!(
            backward.validate(&cfg, now).unwrap_err().code,
            "invalid_expiry"
        );
    }

    #[test]
    fn buy_and_sell_intents_keep_node3_prices_untouched() {
        let cfg = config();
        let buy = intent("buy");
        assert!(buy.validate(&cfg, 1_700_000_001_000).is_ok());
        assert_eq!(buy.stop_loss, 2_647.45);
        assert_eq!(buy.take_profit, 2_656.25);

        let sell = intent("sell");
        assert!(sell.validate(&cfg, 1_700_000_001_000).is_ok());
        assert!(sell.stop_loss > sell.reference_price);
        assert!(sell.take_profit < sell.reference_price);
    }

    #[test]
    fn reports_serialise_with_the_documented_status_strings() {
        let intent = intent("buy");
        let report = ExecutionReport::rejected(&intent, "deriv_mt5_demo", 42, "code", "message");
        let json: serde_json::Value = serde_json::to_value(&report).unwrap();
        assert_eq!(json["status"], "rejected");
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["venue"], "deriv_mt5_demo");
        assert_eq!(json["error_code"], "code");
        // Optional absent fields do not appear at all.
        assert!(json.get("filled_price").is_none());

        let frame = report.to_frame().expect("frame serialises");
        let value: serde_json::Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(value["type"], "execution_report");
        assert_eq!(value["data"]["intent_id"], intent.intent_id);

        let filled = {
            let mut r = ExecutionReport::new(&intent, ExecutionStatus::Filled, "none", 43);
            r.execution_id = Some("mt5-deal-456".into());
            r.filled_price = Some(2_650.28);
            r.quantity = Some(0.01);
            r.quantity_unit = Some("lots".into());
            r
        };
        let json: serde_json::Value = serde_json::to_value(&filled).unwrap();
        assert_eq!(json["status"], "filled");
        assert_eq!(json["quantity_unit"], "lots");
        assert_eq!(json["execution_id"], "mt5-deal-456");
        assert_eq!(
            serde_json::from_value::<ExecutionStatus>(serde_json::json!("filled")).unwrap(),
            ExecutionStatus::Filled
        );
        assert!(serde_json::from_value::<ExecutionStatus>(serde_json::json!("nonsense")).is_err());
        assert!(ExecutionStatus::Filled.is_fill());
        assert!(!ExecutionStatus::Unknown.is_fill());
    }

    #[test]
    fn hello_frame_matches_the_protocol_document() {
        let frame = Node4Frame::ExecutionHello {
            token: "secret",
            service: SERVICE_NAME,
            protocol_version: 1,
        };
        let value: serde_json::Value = serde_json::from_str(&frame.to_text()).unwrap();
        assert_eq!(value["type"], "execution_hello");
        assert_eq!(value["service"], "xauusd-node4-execution");
        assert_eq!(value["protocol_version"], 1);
        assert_eq!(value["token"], "secret");
    }
}
