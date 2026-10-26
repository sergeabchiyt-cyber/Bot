//! Trade intent construction.
//!
//! Node 3 decides and Node 4 executes. An intent carries the decision and its
//! risk distances in price and pips. It never carries account, sizing, or
//! venue data. Those belong to Node 4.
//!
//! The `intent_id` is a pure function of the rule version, the source bar, the
//! side, and the level. Replays and reconnects therefore produce the same id,
//! and Node 4 can de-duplicate on it.

use crate::risk::Projection;
use crate::strategy::Trigger;
use crate::types::{
    ProjectedOrder, TradeIntent, INTENT_SCHEMA_VERSION, STRATEGY_NAME, STRATEGY_VERSION,
};

pub const SYMBOL: &str = "XAUUSD";

/// Lowercase alphanumeric segments joined by `-`. Any other character separates segments.
pub fn component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Deterministic, idempotent intent id.
pub fn intent_id(
    strategy_version: &str,
    source_candle_time: i64,
    side: &str,
    level_id: &str,
) -> String {
    format!(
        "n3-{}-{}-{}-{}",
        component(strategy_version),
        source_candle_time,
        component(side),
        component(level_id)
    )
}

/// Everything needed to turn one confirmed trigger into one intent.
pub struct IntentInputs<'a> {
    pub trigger: &'a Trigger,
    pub order: &'a ProjectedOrder,
    pub projection: Projection,
    /// Whether the optional volume gate was enabled when the retest was confirmed.
    pub volume_gate: bool,
    pub ttl_secs: u64,
    pub now_ms: i64,
}

pub fn build_intent(inputs: &IntentInputs<'_>) -> TradeIntent {
    let trigger = inputs.trigger;
    let order = inputs.order;
    let id = intent_id(
        STRATEGY_VERSION,
        trigger.source_candle_time,
        trigger.side,
        &trigger.level_id,
    );
    TradeIntent {
        schema_version: INTENT_SCHEMA_VERSION,
        intent_id: id,
        strategy: STRATEGY_NAME.into(),
        symbol: SYMBOL.into(),
        side: trigger.side.into(),
        order_type: "market".into(),
        reference_price: order.entry,
        stop_loss: order.stop_loss,
        take_profit: order.take_profit,
        risk_reward: order.risk_reward,
        level_name: trigger.level_name.clone(),
        source_candle_time: trigger.source_candle_time,
        created_at: inputs.now_ms,
        expires_at: inputs.now_ms + (inputs.ttl_secs as i64 * 1_000),
        atr: Some(inputs.projection.atr),
        atr_pips: Some(inputs.projection.atr_pips),
        level_id: Some(trigger.level_id.clone()),
        drawing_id: Some(trigger.drawing_id.clone()),
        strategy_version: Some(STRATEGY_VERSION.into()),
        // Only a gate that was actually enabled can report confirmation.
        volume_confirmed: if inputs.volume_gate { Some(true) } else { None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trigger() -> Trigger {
        Trigger {
            side: "buy",
            reference_price: 4_100.0,
            level_id: "PW_POC".into(),
            level_name: "PW PoC".into(),
            drawing_id: "PW:poc:1:2".into(),
            source_candle_time: 1_792_000_000_000,
        }
    }

    fn order() -> ProjectedOrder {
        ProjectedOrder {
            side: "buy".into(),
            entry: 4_100.0,
            stop_loss: 4_098.0,
            take_profit: 4_106.0,
            sl_pips: 200.0,
            tp_pips: 600.0,
            risk_reward: 3.0,
            sizing: "node4_managed".into(),
        }
    }

    fn projection() -> Projection {
        Projection {
            atr: 2.5,
            atr_pips: 250.0,
            sl_pips: 200.0,
            tp_pips: 600.0,
            rr: 3.0,
        }
    }

    fn inputs<'a>(trigger: &'a Trigger, order: &'a ProjectedOrder, gate: bool) -> IntentInputs<'a> {
        IntentInputs {
            trigger,
            order,
            projection: projection(),
            volume_gate: gate,
            ttl_secs: 120,
            now_ms: 1_000,
        }
    }

    #[test]
    fn intent_id_is_deterministic_and_versioned() {
        let a = intent_id("2.0.0", 42, "buy", "PW_POC");
        assert_eq!(a, intent_id("2.0.0", 42, "buy", "PW_POC"));
        assert_eq!(a, "n3-2-0-0-42-buy-pw-poc");
        assert_ne!(a, intent_id("2.0.1", 42, "buy", "PW_POC"), "rule change ⇒ new id");
        assert_ne!(a, intent_id("2.0.0", 43, "buy", "PW_POC"));
        assert_ne!(a, intent_id("2.0.0", 42, "sell", "PW_POC"));
    }

    #[test]
    fn same_trigger_yields_the_same_intent_twice() {
        let t = trigger();
        let o = order();
        let first = build_intent(&inputs(&t, &o, false));
        let second = build_intent(&inputs(&t, &o, false));
        assert_eq!(first.intent_id, second.intent_id);
        assert_eq!(first.reference_price, second.reference_price);
    }

    #[test]
    fn intent_carries_the_additive_v2_fields() {
        let t = trigger();
        let o = order();
        let intent = build_intent(&inputs(&t, &o, false));
        assert_eq!(intent.strategy, "vp_break_retest_v1");
        assert_eq!(intent.strategy_version.as_deref(), Some("2.0.0"));
        assert_eq!(intent.level_id.as_deref(), Some("PW_POC"));
        assert_eq!(intent.drawing_id.as_deref(), Some("PW:poc:1:2"));
        assert_eq!(intent.atr, Some(2.5));
        assert_eq!(intent.atr_pips, Some(250.0));
        assert_eq!(intent.volume_confirmed, None, "no gate ⇒ no confirmation claim");
        assert_eq!(intent.expires_at, 1_000 + 120_000);
        assert!((2.0..=3.0).contains(&intent.risk_reward));
    }

    #[test]
    fn volume_confirmed_is_set_only_when_the_gate_is_enabled() {
        let t = trigger();
        let o = order();
        assert_eq!(
            build_intent(&inputs(&t, &o, true)).volume_confirmed,
            Some(true)
        );
    }

    #[test]
    fn no_broker_venue_or_credential_identifier_in_crate_source() {
        // Identifiers that would tie Node 3 to a broker, venue, or account. The
        // shared service token name is allowed; it is a Node 3 <-> Node 4 secret
        // and is never placed in an intent or a public frame.
        // Built with concat! so this list does not match itself.
        let forbidden = [
            concat!("de", "riv", "_"),
            concat!("m", "t5"),
            concat!("meta", "trader"),
            concat!("pass", "word"),
            concat!("api", "_key"),
            concat!("api", "key"),
            concat!("account", "_id"),
            concat!("account", "_number"),
        ];
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut scanned = 0;
        for entry in std::fs::read_dir(&dir).expect("src dir") {
            let path = entry.expect("entry").path();
            let is_rust = path.extension().is_some_and(|ext| ext == "rs");
            if !is_rust {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read source").to_ascii_lowercase();
            for word in forbidden {
                assert!(
                    !text.contains(word),
                    "forbidden identifier '{word}' found in {}",
                    path.display()
                );
            }
            scanned += 1;
        }
        assert!(scanned >= 10, "expected to scan the crate sources, scanned {scanned}");
    }
}
