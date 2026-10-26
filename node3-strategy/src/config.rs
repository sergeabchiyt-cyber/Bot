use std::env;

use crate::ny::parse_trading_weekday;
use crate::store::interval_ms;
use crate::vp::histogram::{ProfileModel, RowMode, DEFAULT_BIN_SIZE, DEFAULT_ROWS};
use crate::vp::timeframe::{parse_label, LowerTf};
use crate::vp::windows::{PsMode, WindowSettings};

/// Only anchor this crate supports: Sunday 18:00 New York, DST-aware.
pub const WEEK_ANCHOR: &str = "sunday-1800-America/New_York";
/// Bars kept per resolution. The brief requires at least 35 days.
pub const MIN_RETENTION_DAYS: i64 = 35;

#[derive(Clone, Debug)]
pub struct Config {
    /// Node 1 market-data and volume-profile stream.
    pub node1_ws_url: String,
    /// Node 3 HTTP, public diagnostics WS, and private Node 4 WS port.
    pub port: u16,
    /// Shared service credential required on the private `/execution` socket.
    /// This is not a broker credential and must never be exposed to Node 2.
    pub node4_shared_token: Option<String>,
    /// How long a trade intent remains executable after Node 3 emits it.
    pub signal_ttl_secs: u64,
    /// Bounded in-memory replay queue for intents awaiting a Node 4 report.
    pub max_pending_intents: usize,

    /// Fixed New York calendar anchor. Only `WEEK_ANCHOR` is supported.
    pub week_anchor: String,
    /// Trading weekday on which CW first publishes (`Mon` default, or `Wed`).
    pub cw_first_publish: String,
    pub ps_mode: PsMode,
    /// Value-area share of volume, stored as a fraction (`VA_PCT=70` -> 0.70).
    pub va_pct: f64,
    pub vp_rows: usize,
    pub vp_row_mode: RowMode,
    pub vp_bin_size: f64,
    pub vp_lower_tf: LowerTf,
    /// Days of closed bars kept in memory. Never below `MIN_RETENTION_DAYS`.
    pub retention_days: i64,
    /// Node 1 candle interval driving break and retest evaluation.
    pub strategy_interval: String,

    /// Volume gate threshold. 0 disables the gate (decision D6, the default).
    pub volume_threshold: f64,
    pub sl_min_pips: f64,
    pub sl_max_pips: f64,
    pub tp_min_pips: f64,
    pub tp_max_pips: f64,
    pub rr_min: f64,
    pub rr_max: f64,
    /// Stop distance as a multiple of ATR(14), in pips, before clamping.
    pub sl_atr_mult: f64,
    /// Target distance as a multiple of ATR(14), in pips, before clamping.
    pub tp_atr_mult: f64,

    /// Problems found while reading the environment. Logged at startup.
    pub warnings: Vec<String>,
}

fn env_str(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_opt(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        _ => None,
    }
}

fn env_f64(key: &str, default: f64) -> f64 {
    env::var(key)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

fn env_u16(key: &str, default: u16) -> u16 {
    env::var(key)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

fn env_i64(key: &str, default: i64) -> i64 {
    env::var(key)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

impl Default for Config {
    fn default() -> Self {
        Self {
            node1_ws_url: "wss://engine-southeastasia-sng-main.onrender.com/ws".into(),
            port: 10_000,
            node4_shared_token: None,
            signal_ttl_secs: 120,
            max_pending_intents: 128,
            week_anchor: WEEK_ANCHOR.into(),
            cw_first_publish: "Mon".into(),
            ps_mode: PsMode::WeekOpen,
            va_pct: 0.70,
            vp_rows: DEFAULT_ROWS,
            vp_row_mode: RowMode::Rows,
            vp_bin_size: DEFAULT_BIN_SIZE,
            vp_lower_tf: LowerTf::Tv,
            retention_days: MIN_RETENTION_DAYS,
            strategy_interval: "15m".into(),
            volume_threshold: 0.0,
            sl_min_pips: 200.0,
            sl_max_pips: 300.0,
            tp_min_pips: 600.0,
            tp_max_pips: 800.0,
            rr_min: 2.0,
            rr_max: 3.0,
            sl_atr_mult: 1.0,
            tp_atr_mult: 2.0,
            warnings: Vec::new(),
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let defaults = Self::default();
        let mut warnings = Vec::new();

        let week_anchor = env_str("WEEK_ANCHOR", WEEK_ANCHOR);
        if week_anchor != WEEK_ANCHOR {
            warnings.push(format!(
                "WEEK_ANCHOR '{week_anchor}' is not supported; using {WEEK_ANCHOR}"
            ));
        }

        let cw_raw = env_str("CW_FIRST_PUBLISH_WEEKDAY", &defaults.cw_first_publish);
        let cw_first_publish = match parse_trading_weekday(&cw_raw) {
            Some(_) => cw_raw.trim().to_string(),
            None => {
                warnings.push(format!(
                    "CW_FIRST_PUBLISH_WEEKDAY '{cw_raw}' is not a weekday; using Mon"
                ));
                "Mon".into()
            }
        };

        let va_raw = env_f64("VA_PCT", defaults.va_pct * 100.0);
        let va_pct = if (5.0..=99.0).contains(&va_raw) {
            va_raw / 100.0
        } else {
            warnings.push(format!("VA_PCT {va_raw} outside 5-99; using 70"));
            defaults.va_pct
        };

        let rows_raw = env_u64("VP_ROWS", defaults.vp_rows as u64);
        let vp_rows = if (2..=5_000).contains(&rows_raw) {
            rows_raw as usize
        } else {
            warnings.push(format!(
                "VP_ROWS {rows_raw} outside 2-5000; using {DEFAULT_ROWS}"
            ));
            DEFAULT_ROWS
        };

        let vp_bin_size = env_f64("VP_BIN_SIZE", defaults.vp_bin_size);
        let vp_bin_size = if vp_bin_size.is_finite() && vp_bin_size > 0.0 {
            vp_bin_size
        } else {
            warnings.push(format!(
                "VP_BIN_SIZE {vp_bin_size} must be positive; using {DEFAULT_BIN_SIZE}"
            ));
            DEFAULT_BIN_SIZE
        };

        let vp_row_mode = RowMode::parse(&env_str("VP_ROW_MODE", "rows"));

        let lower_raw = env_str("VP_LOWER_TF", "tv");
        let vp_lower_tf = if lower_raw.trim().eq_ignore_ascii_case("tv") {
            LowerTf::Tv
        } else if parse_label(&lower_raw).is_some() {
            LowerTf::parse(&lower_raw)
        } else {
            warnings.push(format!(
                "VP_LOWER_TF '{lower_raw}' not recognised; using tv"
            ));
            LowerTf::Tv
        };

        let retention_raw = env_i64("RETAIN_DAYS", MIN_RETENTION_DAYS);
        let retention_days = retention_raw.max(MIN_RETENTION_DAYS);
        if retention_raw < MIN_RETENTION_DAYS {
            warnings.push(format!(
                "RETAIN_DAYS {retention_raw} is below {MIN_RETENTION_DAYS}; using {MIN_RETENTION_DAYS}"
            ));
        }

        let strategy_interval = env_str("STRATEGY_INTERVAL", &defaults.strategy_interval);
        if interval_ms(&strategy_interval).is_none() {
            warnings.push(format!(
                "STRATEGY_INTERVAL '{strategy_interval}' is not one of 1m, 15m, 1h; using 15m"
            ));
        }
        let strategy_interval = if interval_ms(&strategy_interval).is_some() {
            strategy_interval
        } else {
            "15m".into()
        };

        let mut config = Self {
            node1_ws_url: env_str("NODE1_WS_URL", &defaults.node1_ws_url),
            port: env_u16("PORT", defaults.port),
            node4_shared_token: env_opt("NODE4_SHARED_TOKEN"),
            signal_ttl_secs: env_u64("SIGNAL_TTL_SECS", defaults.signal_ttl_secs),
            max_pending_intents: env_u64("MAX_PENDING_INTENTS", defaults.max_pending_intents as u64)
                as usize,
            week_anchor: WEEK_ANCHOR.into(),
            cw_first_publish,
            ps_mode: PsMode::parse(&env_str("PS_MODE", "week_open")),
            va_pct,
            vp_rows,
            vp_row_mode,
            vp_bin_size,
            vp_lower_tf,
            retention_days,
            strategy_interval,
            volume_threshold: env_f64("VOLUME_THRESHOLD", defaults.volume_threshold),
            sl_min_pips: env_f64("SL_MIN_PIPS", defaults.sl_min_pips),
            sl_max_pips: env_f64("SL_MAX_PIPS", defaults.sl_max_pips),
            tp_min_pips: env_f64("TP_MIN_PIPS", defaults.tp_min_pips),
            tp_max_pips: env_f64("TP_MAX_PIPS", defaults.tp_max_pips),
            rr_min: env_f64("RR_MIN", defaults.rr_min),
            rr_max: env_f64("RR_MAX", defaults.rr_max),
            sl_atr_mult: env_f64("SL_ATR_MULT", defaults.sl_atr_mult),
            tp_atr_mult: env_f64("TP_ATR_MULT", defaults.tp_atr_mult),
            warnings,
        };

        // Keep malformed environment values from producing unsafe or unbounded
        // strategy output. Node 4 still validates every intent independently.
        config.signal_ttl_secs = config.signal_ttl_secs.clamp(5, 3_600);
        config.max_pending_intents = config.max_pending_intents.clamp(1, 10_000);
        config.volume_threshold = if config.volume_threshold.is_finite() {
            config.volume_threshold.max(0.0)
        } else {
            0.0
        };
        config.sl_min_pips = config.sl_min_pips.max(1.0);
        config.sl_max_pips = config.sl_max_pips.max(config.sl_min_pips);
        config.tp_min_pips = config.tp_min_pips.max(1.0);
        config.tp_max_pips = config.tp_max_pips.max(config.tp_min_pips);
        config.rr_min = config.rr_min.max(0.1);
        config.rr_max = config.rr_max.max(config.rr_min);
        config.sl_atr_mult = config.sl_atr_mult.max(0.01);
        config.tp_atr_mult = config.tp_atr_mult.max(0.01);
        config
    }

    pub fn node4_link_configured(&self) -> bool {
        self.node4_shared_token
            .as_deref()
            .is_some_and(|token| !token.trim().is_empty())
    }

    /// The volume gate is on only when a positive threshold is configured.
    pub fn volume_filter_enabled(&self) -> bool {
        self.volume_threshold > 0.0
    }

    /// Profile construction settings shared by every window.
    pub fn window_settings(&self) -> WindowSettings {
        WindowSettings {
            model: ProfileModel {
                row_mode: self.vp_row_mode,
                rows: self.vp_rows,
                bin_size: self.vp_bin_size,
                tick_size: crate::vp::histogram::DEFAULT_TICK_SIZE,
                va_pct: self.va_pct,
            },
            lower_tf: self.vp_lower_tf,
            cw_first_weekday: parse_trading_weekday(&self.cw_first_publish).unwrap_or(1),
            ps_mode: self.ps_mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_contain_no_broker_or_execution_configuration() {
        let config = Config::default();
        assert!(!config.node4_link_configured());
        assert_eq!(config.signal_ttl_secs, 120);
        assert_eq!(config.max_pending_intents, 128);
    }

    #[test]
    fn volume_gate_is_off_by_default() {
        let config = Config::default();
        assert_eq!(config.volume_threshold, 0.0);
        assert!(!config.volume_filter_enabled());
    }

    #[test]
    fn window_settings_follow_the_brief_defaults() {
        let settings = Config::default().window_settings();
        assert_eq!(settings.cw_first_weekday, 1, "CW defaults to Monday");
        assert_eq!(settings.ps_mode, PsMode::WeekOpen);
        assert_eq!(settings.model.rows, 128);
        assert!((settings.model.va_pct - 0.70).abs() < 1e-12);
    }
}
