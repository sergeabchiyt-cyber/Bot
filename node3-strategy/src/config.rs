use std::env;

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

    // Strategy parameters. Broker selection, account sizing, and credentials
    // deliberately do not exist in Node 3; those belong to Node 4.
    pub volume_threshold: f64,
    pub sl_min_pips: f64,
    pub sl_max_pips: f64,
    pub tp_min_pips: f64,
    pub tp_max_pips: f64,
    pub rr_min: f64,
    pub rr_max: f64,
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

impl Default for Config {
    fn default() -> Self {
        Self {
            node1_ws_url: "wss://engine-southeastasia-sng-main.onrender.com/ws".into(),
            port: 10_000,
            node4_shared_token: None,
            signal_ttl_secs: 120,
            max_pending_intents: 128,
            volume_threshold: 10_500.0,
            sl_min_pips: 200.0,
            sl_max_pips: 300.0,
            tp_min_pips: 600.0,
            tp_max_pips: 800.0,
            rr_min: 2.0,
            rr_max: 3.0,
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let defaults = Self::default();
        let mut config = Self {
            node1_ws_url: env_str("NODE1_WS_URL", &defaults.node1_ws_url),
            port: env_u16("PORT", defaults.port),
            node4_shared_token: env_opt("NODE4_SHARED_TOKEN"),
            signal_ttl_secs: env_u64("SIGNAL_TTL_SECS", defaults.signal_ttl_secs),
            max_pending_intents: env_u64("MAX_PENDING_INTENTS", defaults.max_pending_intents as u64)
                as usize,
            volume_threshold: env_f64("VOLUME_THRESHOLD", defaults.volume_threshold),
            sl_min_pips: env_f64("SL_MIN_PIPS", defaults.sl_min_pips),
            sl_max_pips: env_f64("SL_MAX_PIPS", defaults.sl_max_pips),
            tp_min_pips: env_f64("TP_MIN_PIPS", defaults.tp_min_pips),
            tp_max_pips: env_f64("TP_MAX_PIPS", defaults.tp_max_pips),
            rr_min: env_f64("RR_MIN", defaults.rr_min),
            rr_max: env_f64("RR_MAX", defaults.rr_max),
        };

        // Keep malformed environment values from producing unsafe or unbounded
        // strategy output. Node 4 still validates every intent independently.
        config.signal_ttl_secs = config.signal_ttl_secs.clamp(5, 3_600);
        config.max_pending_intents = config.max_pending_intents.clamp(1, 10_000);
        config.volume_threshold = config.volume_threshold.max(0.0);
        config.sl_min_pips = config.sl_min_pips.max(1.0);
        config.sl_max_pips = config.sl_max_pips.max(config.sl_min_pips);
        config.tp_min_pips = config.tp_min_pips.max(1.0);
        config.tp_max_pips = config.tp_max_pips.max(config.tp_min_pips);
        config.rr_min = config.rr_min.max(0.1);
        config.rr_max = config.rr_max.max(config.rr_min);
        config
    }

    pub fn node4_link_configured(&self) -> bool {
        self.node4_shared_token
            .as_deref()
            .is_some_and(|token| !token.trim().is_empty())
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
}
