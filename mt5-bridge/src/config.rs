//! Bridge configuration.
//!
//! Everything is read from the environment of the bridge process. The bridge
//! runs **next to the MT5 terminal** (see `README.md`), so these values are
//! terminal-host secrets and must never be committed to the repository.
//!
//! Two credentials exist and they are different things:
//!
//! * `MT5_LOGIN` / `MT5_PASSWORD` — the **broker account** credential. The
//!   terminal is already logged in; the bridge uses `MT5_LOGIN` only to assert
//!   that the terminal it is talking to is the expected account. `MT5_PASSWORD`
//!   is never transmitted to Node 4, the browser, or the EA link; it is only
//!   validated for presence so a half-configured deployment fails at startup.
//! * `MT5_BRIDGE_TOKEN` / `MT5_EA_TOKEN` — **service** credentials for the
//!   bridge's two links (Node 4 ⇄ bridge over WSS, EA ⇄ bridge over loopback
//!   TCP). They are not the MT5 account password.

use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;

/// This bridge is demo-only by design. Selecting any other expected account
/// type is a hard configuration error (fail closed); there is deliberately no
/// "allow live" switch.
pub const EXPECTED_ACCOUNT_TYPE: &str = "demo";

fn env_str(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_opt(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(v) if !v.trim().is_empty() => Some(v.trim().to_string()),
        _ => None,
    }
}

fn env_f64(key: &str, default: f64) -> f64 {
    env::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn env_u16(key: &str, default: u16) -> u16 {
    env::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn env_i64(key: &str, default: i64) -> i64 {
    env::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

pub fn env_truthy(key: &str) -> bool {
    match env::var(key) {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

/// An EA method that is only reachable when the EA link is authenticated.
#[derive(Clone, Debug)]
pub struct BridgeConfig {
    /// Node 4 WebSocket endpoint the bridge dials **out** to, e.g.
    /// `wss://execution-southeastasia-sng-main.onrender.com/ws`.
    pub node4_ws_url: String,
    /// `MT5_BRIDGE_TOKEN`: shared secret Node 4 validates on `bridge_hello`.
    pub node4_token: Option<String>,
    /// Loopback address the bridge listens on for the EA (`MQL5` sockets are
    /// client-only, so the EA dials in).
    pub ea_bind_addr: String,
    pub ea_port: u16,
    /// `MT5_EA_TOKEN`: shared secret the EA must present in its `HELLO`.
    /// Without it the bridge runs read-only (no order/write methods).
    pub ea_token: Option<String>,
    /// `MT5_LOGIN`: expected broker login of the demo account.
    pub expected_login: Option<i64>,
    /// True when `MT5_PASSWORD` is present (informational/validation only).
    pub account_password_set: bool,
    /// `MT5_SYMBOL`: the instrument Node 4 asks for (strategy symbol).
    pub requested_symbol: String,
    /// `MT5_SYMBOL_MAP`: explicit requested→broker symbol mapping only
    /// (`XAUUSD=XAUUSD.a`). No suffix guessing happens anywhere.
    pub symbol_map: BTreeMap<String, String>,
    /// `MT5_VOLUME_LOTS`: fallback volume when Node 4 does not send one.
    /// Node 4's `ORDER_SIZE` is a **USD options stake** and is never used as an
    /// MT5 lot size.
    pub default_volume_lots: f64,
    /// `MT5_ORDER_TIMEOUT_MS`: how long to wait for a broker fill before the
    /// outcome is treated as unknown (never auto-retried).
    pub order_timeout_ms: u64,
    /// `MT5_REQUEST_TIMEOUT_MS`: timeout for idempotent reads.
    pub request_timeout_ms: u64,
    /// `MT5_ACCOUNT_MAX_AGE_MS`: how long a demo-guard account read is trusted.
    pub account_max_age_ms: u64,
    /// `MT5_MAX_QUOTE_AGE_MS`: reject orders whose quote is older than this.
    pub max_quote_age_ms: u64,
    /// `MT5_HISTORY_FILE`: append-only JSONL store of deals + order audit.
    pub history_file: PathBuf,
    /// `MT5_HISTORY_PAGE_SIZE`: deals per history page.
    pub history_page_size: usize,
    /// `MT5_PUSH_INTERVAL_MS`: snapshot push cadence to Node 4.
    pub push_interval_ms: u64,
    /// `MT5_MAX_DEVIATION_POINTS`: slippage the EA may accept.
    pub max_deviation_points: u32,
    /// `MT5_MAGIC`: magic number stamped on every Node 4 order; used to
    /// reconcile broker positions against Node 4's own trades.
    pub magic: i64,
    /// `MT5_TRADING_ENABLED` (default true): set to `0` to start halted.
    pub trading_enabled_on_start: bool,
    /// `MT5_HALT_ON_EA_DISCONNECT` (default true): halt new orders when the
    /// terminal link drops, requiring an explicit resume.
    pub halt_on_ea_disconnect: bool,
    /// `MT5_SIM_TERMINAL=1`: use the in-process fake terminal instead of a real
    /// EA link. Demo/testing only; the fake reports a demo account and refuses
    /// nothing, so it must never be used against a live strategy ledger.
    pub sim_terminal: bool,
    /// Extra safety: refuse to start when the requested symbol is not XAUUSD.
    /// The strategy is XAUUSD-only; a different symbol here is a mistake, and
    /// overriding it requires `MT5_ALLOW_OTHER_SYMBOL=1`.
    pub allow_other_symbol: bool,
}

impl BridgeConfig {
    pub fn from_env() -> Self {
        let mut symbol_map = BTreeMap::new();
        if let Some(raw) = env_opt("MT5_SYMBOL_MAP") {
            for pair in raw.split(',') {
                let pair = pair.trim();
                if pair.is_empty() {
                    continue;
                }
                let Some((from, to)) = pair.split_once('=') else {
                    // Surface invalid entries through `validate()` instead of
                    // silently dropping them.
                    symbol_map.insert(pair.to_string(), String::new());
                    continue;
                };
                symbol_map.insert(from.trim().to_string(), to.trim().to_string());
            }
        }

        Self {
            node4_ws_url: env_str(
                "NODE4_WS_URL",
                "wss://execution-southeastasia-sng-main.onrender.com/mt5/bridge",
            ),
            node4_token: env_opt("MT5_BRIDGE_TOKEN"),
            ea_bind_addr: env_str("MT5_EA_BIND_ADDR", "127.0.0.1"),
            ea_port: env_u16("MT5_EA_PORT", 5055),
            ea_token: env_opt("MT5_EA_TOKEN"),
            expected_login: env::var("MT5_LOGIN")
                .ok()
                .and_then(|v| v.trim().parse::<i64>().ok()),
            account_password_set: env_opt("MT5_PASSWORD").is_some(),
            requested_symbol: env_str("MT5_SYMBOL", "XAUUSD"),
            symbol_map,
            default_volume_lots: env_f64("MT5_VOLUME_LOTS", 0.01),
            order_timeout_ms: env_u64("MT5_ORDER_TIMEOUT_MS", 15_000),
            request_timeout_ms: env_u64("MT5_REQUEST_TIMEOUT_MS", 5_000),
            account_max_age_ms: env_u64("MT5_ACCOUNT_MAX_AGE_MS", 5_000),
            max_quote_age_ms: env_u64("MT5_MAX_QUOTE_AGE_MS", 3_000),
            history_file: PathBuf::from(env_str("MT5_HISTORY_FILE", "data/mt5_history.jsonl")),
            history_page_size: env_u64("MT5_HISTORY_PAGE_SIZE", 100) as usize,
            push_interval_ms: env_u64("MT5_PUSH_INTERVAL_MS", 2_000),
            max_deviation_points: env_u64("MT5_MAX_DEVIATION_POINTS", 20) as u32,
            magic: env_i64("MT5_MAGIC", 330_033),
            trading_enabled_on_start: env::var("MT5_TRADING_ENABLED")
                .map(|v| !matches!(v.trim(), "0" | "false" | "no" | "off"))
                .unwrap_or(true),
            halt_on_ea_disconnect: env::var("MT5_HALT_ON_EA_DISCONNECT")
                .map(|v| !matches!(v.trim(), "0" | "false" | "no" | "off"))
                .unwrap_or(true),
            sim_terminal: env_truthy("MT5_SIM_TERMINAL"),
            allow_other_symbol: env_truthy("MT5_ALLOW_OTHER_SYMBOL"),
        }
    }

    /// `true` when the EA link may execute write methods (orders/modifications).
    /// Without `MT5_EA_TOKEN` the bridge is read-only: it still reports account
    /// state, but it refuses every order path (fail closed).
    pub fn ea_write_enabled(&self) -> bool {
        self.ea_token.is_some()
    }

    /// Human-readable reason for a missing EA token, used in diagnostics.
    pub fn ea_write_disabled_reason(&self) -> Option<String> {
        if self.ea_write_enabled() {
            None
        } else {
            Some(
                "MT5_EA_TOKEN is not set — the bridge runs read-only and refuses \
                 every order/modify/close method"
                    .to_string(),
            )
        }
    }

    /// Resolve the broker symbol for a requested (strategy) symbol name.
    ///
    /// The requested name is used verbatim when it exists in the terminal.
    /// Otherwise the **explicit** `MT5_SYMBOL_MAP` entry is used. A broker
    /// suffix is never guessed, and an unmapped symbol fails closed.
    pub fn resolve_symbol<'a>(
        &'a self,
        requested: &str,
        available: &[String],
    ) -> Result<String, String> {
        if available.iter().any(|s| s == requested) {
            return Ok(requested.to_string());
        }
        if let Some(mapped) = self.symbol_map.get(requested) {
            if mapped.is_empty() {
                return Err(format!(
                    "MT5_SYMBOL_MAP entry '{requested}' is not a `requested=broker` pair"
                ));
            }
            if available.iter().any(|s| s == mapped) {
                return Ok(mapped.clone());
            }
            return Err(format!(
                "MT5_SYMBOL_MAP maps {requested} -> {mapped}, but the terminal does not offer {mapped}"
            ));
        }
        Err(format!(
            "symbol {requested} is not available in the terminal and has no explicit \
             MT5_SYMBOL_MAP entry (broker suffixes are never guessed)"
        ))
    }

    /// Startup validation. Every returned string is a hard error: the bridge
    /// refuses to start so a misconfiguration cannot silently reach a broker.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        let url = self.node4_ws_url.trim();
        if !(url.starts_with("wss://") || url.starts_with("ws://")) {
            errors.push(format!(
                "NODE4_WS_URL must be a ws:// or wss:// URL, got '{url}'"
            ));
        }
        if self.node4_token.is_none() {
            errors.push(
                "MT5_BRIDGE_TOKEN is not set — Node 4 would reject the bridge handshake; \
                 set the same value in Node 4's MT5_BRIDGE_TOKEN"
                    .into(),
            );
        }
        if self.ea_port == 0 {
            errors.push("MT5_EA_PORT must not be 0".into());
        }
        if self.account_password_set && self.expected_login.is_none() {
            errors.push(
                "MT5_PASSWORD is set but MT5_LOGIN is not — set MT5_LOGIN to the demo \
                 account number so the bridge can assert the terminal's account"
                    .into(),
            );
        }
        if !self.default_volume_lots.is_finite() || self.default_volume_lots <= 0.0 {
            errors.push(format!(
                "MT5_VOLUME_LOTS must be a positive number of lots, got {}",
                self.default_volume_lots
            ));
        }
        if self.order_timeout_ms == 0 || self.request_timeout_ms == 0 {
            errors.push("MT5_ORDER_TIMEOUT_MS and MT5_REQUEST_TIMEOUT_MS must be > 0".into());
        }
        if self.max_quote_age_ms == 0 || self.max_quote_age_ms > 60_000 {
            errors.push("MT5_MAX_QUOTE_AGE_MS must be in 1..=60000".into());
        }
        if self.history_page_size == 0 || self.history_page_size > 5_000 {
            errors.push("MT5_HISTORY_PAGE_SIZE must be in 1..=5000".into());
        }
        if self.magic <= 0 {
            errors.push("MT5_MAGIC must be a positive integer".into());
        }
        for (from, to) in &self.symbol_map {
            if from.is_empty() || to.is_empty() {
                errors.push(format!(
                    "MT5_SYMBOL_MAP contains an invalid entry '{from}={to}' \
                     (expected `requested=broker`, comma separated)"
                ));
            }
        }
        if !self.allow_other_symbol && !self.requested_symbol.eq_ignore_ascii_case("XAUUSD") {
            errors.push(format!(
                "MT5_SYMBOL is '{}' but this strategy is XAUUSD-only; set \
                 MT5_ALLOW_OTHER_SYMBOL=1 only if you really intend a different instrument",
                self.requested_symbol
            ));
        }
        if let Ok(expected) = env::var("MT5_EXPECT_ACCOUNT_TYPE") {
            if !expected.trim().is_empty()
                && expected.trim().to_ascii_lowercase() != EXPECTED_ACCOUNT_TYPE
            {
                errors.push(format!(
                    "MT5_EXPECT_ACCOUNT_TYPE='{}' is not supported: this bridge executes on \
                     Deriv MT5 *demo* accounts only",
                    expected.trim()
                ));
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> BridgeConfig {
        BridgeConfig {
            node4_ws_url: "wss://example.invalid/ws".into(),
            node4_token: Some("bridge-token".into()),
            ea_bind_addr: "127.0.0.1".into(),
            ea_port: 5055,
            ea_token: Some("ea-token".into()),
            expected_login: Some(123_456),
            account_password_set: true,
            requested_symbol: "XAUUSD".into(),
            symbol_map: BTreeMap::new(),
            default_volume_lots: 0.01,
            order_timeout_ms: 15_000,
            request_timeout_ms: 5_000,
            account_max_age_ms: 5_000,
            max_quote_age_ms: 3_000,
            history_file: PathBuf::from("data/mt5_history.jsonl"),
            history_page_size: 100,
            push_interval_ms: 2_000,
            max_deviation_points: 20,
            magic: 330_033,
            trading_enabled_on_start: true,
            halt_on_ea_disconnect: true,
            sim_terminal: false,
            allow_other_symbol: false,
        }
    }

    #[test]
    fn defaults_validate_cleanly() {
        assert!(base().validate().is_empty());
    }

    #[test]
    fn missing_node4_token_is_a_hard_error() {
        let mut cfg = base();
        cfg.node4_token = None;
        let errors = cfg.validate();
        assert!(errors.iter().any(|e| e.contains("MT5_BRIDGE_TOKEN")));
    }

    #[test]
    fn password_without_login_is_a_hard_error() {
        let mut cfg = base();
        cfg.expected_login = None;
        let errors = cfg.validate();
        assert!(errors.iter().any(|e| e.contains("MT5_LOGIN")));
    }

    #[test]
    fn non_xauusd_symbol_is_rejected_unless_explicit() {
        let mut cfg = base();
        cfg.requested_symbol = "EURUSD".into();
        assert!(!cfg.validate().is_empty());

        cfg.allow_other_symbol = true;
        assert!(cfg.validate().is_empty());
    }

    #[test]
    fn malformed_symbol_map_entries_are_reported() {
        let mut cfg = base();
        cfg.symbol_map.insert("XAUUSD".into(), String::new());
        let errors = cfg.validate();
        assert!(errors.iter().any(|e| e.contains("MT5_SYMBOL_MAP")));
    }

    #[test]
    fn ea_token_missing_disables_writes() {
        let mut cfg = base();
        cfg.ea_token = None;
        assert!(!cfg.ea_write_enabled());
        assert!(cfg.ea_write_disabled_reason().is_some());
        // Read-only mode is a warning, not a startup error: monitoring without
        // trading is a legitimate (safe) deployment.
        assert!(cfg.validate().is_empty());
    }

    #[test]
    fn resolves_symbol_verbatim_then_by_explicit_mapping_then_fails() {
        let cfg = base();
        let available = vec!["XAUUSD.a".to_string(), "EURUSD".to_string()];

        // No mapping -> fail closed, no suffix guessing.
        let err = cfg.resolve_symbol("XAUUSD", &available).unwrap_err();
        assert!(err.contains("MT5_SYMBOL_MAP"));

        let mut mapped = base();
        mapped.symbol_map.insert("XAUUSD".into(), "XAUUSD.a".into());
        assert_eq!(
            mapped.resolve_symbol("XAUUSD", &available).unwrap(),
            "XAUUSD.a"
        );

        // Exact match wins over mapping.
        assert_eq!(
            mapped.resolve_symbol("EURUSD", &available).unwrap(),
            "EURUSD"
        );

        // Mapping to a symbol the terminal does not offer is an error.
        let mut broken = base();
        broken.symbol_map.insert("XAUUSD".into(), "GOLD".into());
        assert!(broken.resolve_symbol("XAUUSD", &available).is_err());
    }
}
