//! Written for the headless Linux host, which supersedes the Windows/Wine-desktop
//! assumption: "ready" means the processes this container owns are up (Xvfb, the
//! terminal under Wine, the bridge) and the Expert Advisor has dialled into the
//! bridge's loopback port — never that a desktop looks healthy to a human.
//! The host's observable state: what `/health`, `/readyz` and `/diagnostics`
//! report, and what "ready" means for this container.
//!
//! Everything here is non-secret by construction: the fields are pids, stage
//! names, counters and the *view* Node 4 has of the bridge. Nothing from
//! `Config` that carries a credential is ever copied into this struct (asserted
//! by tests in `config.rs` and `state.rs`).

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|delta| delta.as_millis() as i64)
        .unwrap_or(0)
}

/// What Node 4 says about this bridge, when it is reachable.
#[derive(Debug, Clone, Default)]
pub struct Node4View {
    pub reachable: bool,
    pub configured: Option<bool>,
    pub connected: Option<bool>,
    pub authorized: Option<bool>,
    pub ea_connected: Option<bool>,
    pub ea_mode: Option<String>,
    pub halted: Option<bool>,
    pub halt_reason: Option<String>,
    pub login: Option<i64>,
    pub account_type: Option<String>,
    pub orders_sent: Option<u64>,
    pub orders_filled: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HostStatus {
    pub started_at_ms: i64,
    pub stage: String,
    pub stages_done: Vec<String>,
    pub last_error: Option<String>,
    pub install_dir: Option<String>,
    pub wine_version: Option<String>,
    pub display_ready: bool,
    pub prefix_ready: bool,
    pub mt5_installed: bool,
    pub ea_compiled: bool,
    pub startup_config_written: bool,
    pub terminal_starts: u64,
    pub bridge_starts: u64,
    pub terminal_pid: Option<u32>,
    pub bridge_pid: Option<u32>,
    pub terminal_uptime_secs: Option<u64>,
    /// Established loopback connections to the bridge's EA port: the local
    /// signal that an Expert Advisor is attached and running.
    pub ea_connections: usize,
    pub node4: Node4View,
    /// Total resident memory of this process plus the processes it started.
    pub memory_mb: Option<u64>,
    pub restarts_last_hour: u32,
    pub trading_enabled: bool,
}

impl Default for HostStatus {
    fn default() -> Self {
        Self {
            started_at_ms: now_ms(),
            stage: "starting".into(),
            stages_done: Vec::new(),
            last_error: None,
            install_dir: None,
            wine_version: None,
            display_ready: false,
            prefix_ready: false,
            mt5_installed: false,
            ea_compiled: false,
            startup_config_written: false,
            terminal_starts: 0,
            bridge_starts: 0,
            terminal_pid: None,
            bridge_pid: None,
            terminal_uptime_secs: None,
            ea_connections: 0,
            node4: Node4View::default(),
            memory_mb: None,
            restarts_last_hour: 0,
            trading_enabled: false,
        }
    }
}

impl HostStatus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record progress on a boot stage (idempotent, order preserving).
    pub fn advance(&mut self, stage: &str) {
        self.stage = stage.to_string();
        if !self.stages_done.iter().any(|done| done == stage) {
            self.stages_done.push(stage.to_string());
        }
    }

    pub fn record_error(&mut self, context: &str, error: &str) {
        self.last_error = Some(format!("{context}: {error}"));
    }

    /// Is this container serving a working terminal, end to end?
    ///
    /// Three local facts must hold (display, terminal process, bridge process)
    /// plus the EA loopback connection. Node 4's own view is used only as a
    /// *negative* signal: when Node 4 is reachable and says the EA is not
    /// connected (or the account is not demo), readiness is refused even though
    /// the local processes look healthy.
    pub fn ready(&self) -> Result<(), Vec<String>> {
        let mut reasons = Vec::new();
        if !self.display_ready {
            reasons.push("virtual display is not up".to_string());
        }
        if !self.prefix_ready {
            reasons.push("Wine prefix is not initialised".to_string());
        }
        if !self.mt5_installed {
            reasons.push("MetaTrader 5 terminal is not installed".to_string());
        }
        if !self.ea_compiled {
            reasons.push("the bridge Expert Advisor is not compiled".to_string());
        }
        if !self.startup_config_written {
            reasons.push("terminal startup configuration is missing".to_string());
        }
        if self.terminal_pid.is_none() {
            reasons.push("terminal64.exe is not running".to_string());
        }
        if self.bridge_pid.is_none() {
            reasons.push("mt5-bridge is not running".to_string());
        }
        if self.ea_connections == 0 {
            reasons.push("no Expert Advisor is attached to the bridge port".to_string());
        }
        if self.node4.reachable {
            if self.node4.ea_connected == Some(false) {
                reasons.push("Node 4 reports the EA link as down".to_string());
            }
            if let Some(error) = self.node4.error.as_deref() {
                reasons.push(format!("Node 4 reports: {error}"));
            }
        }
        if reasons.is_empty() {
            Ok(())
        } else {
            Err(reasons)
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        ((now_ms() - self.started_at_ms).max(0) / 1000) as u64
    }

    pub fn to_json(&self) -> Value {
        json!({
            "service": "mt5-host",
            "stage": self.stage,
            "stages_done": self.stages_done,
            "uptime_secs": self.uptime_secs(),
            "last_error": self.last_error,
            "install_dir": self.install_dir,
            "wine_version": self.wine_version,
            "checks": json!({
                "display_ready": self.display_ready,
                "prefix_ready": self.prefix_ready,
                "mt5_installed": self.mt5_installed,
                "ea_compiled": self.ea_compiled,
                "startup_config_written": self.startup_config_written,
            }),
            "processes": json!({
                "terminal_pid": self.terminal_pid,
                "terminal_uptime_secs": self.terminal_uptime_secs,
                "terminal_starts": self.terminal_starts,
                "bridge_pid": self.bridge_pid,
                "bridge_starts": self.bridge_starts,
                "ea_connections": self.ea_connections,
                "restarts_last_hour": self.restarts_last_hour,
            }),
            "node4": json!({
                "reachable": self.node4.reachable,
                "configured": self.node4.configured,
                "connected": self.node4.connected,
                "authorized": self.node4.authorized,
                "ea_connected": self.node4.ea_connected,
                "ea_mode": self.node4.ea_mode,
                "halted": self.node4.halted,
                "halt_reason": self.node4.halt_reason,
                "login": self.node4.login,
                "account_type": self.node4.account_type,
                "orders_sent": self.node4.orders_sent,
                "orders_filled": self.node4.orders_filled,
                "error": self.node4.error,
            }),
            "trading_enabled": self.trading_enabled,
            "memory_mb": self.memory_mb,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json as json_value;

    fn healthy() -> HostStatus {
        let mut status = HostStatus::new();
        status.display_ready = true;
        status.prefix_ready = true;
        status.mt5_installed = true;
        status.ea_compiled = true;
        status.startup_config_written = true;
        status.terminal_pid = Some(11);
        status.bridge_pid = Some(22);
        status.ea_connections = 1;
        status
    }

    #[test]
    fn readiness_requires_the_whole_local_stack() {
        assert!(healthy().ready().is_ok());

        let mut no_ea = healthy();
        no_ea.ea_connections = 0;
        let reasons = no_ea.ready().unwrap_err();
        assert!(reasons.iter().any(|r| r.contains("Expert Advisor")));

        let mut no_terminal = healthy();
        no_terminal.terminal_pid = None;
        assert!(no_terminal.ready().is_err());
    }

    #[test]
    fn node4_opinion_can_only_make_readiness_stricter() {
        let mut status = healthy();
        status.node4.reachable = true;
        status.node4.ea_connected = Some(false);
        let reasons = status.ready().unwrap_err();
        assert!(reasons.iter().any(|r| r.contains("Node 4")));

        // Unreachable Node 4 is not this container's fault.
        let mut offline = healthy();
        offline.node4.reachable = false;
        assert!(offline.ready().is_ok());
    }

    #[test]
    fn the_diagnostics_payload_is_json_and_carries_no_secret_shaped_field() {
        let status = healthy();
        let rendered = status.to_json().to_string();
        let parsed: Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(parsed["service"], json_value!("mt5-host"));
        assert_eq!(parsed["checks"]["mt5_installed"], json_value!(true));
        for forbidden in ["password", "token", "secret", "credential"] {
            assert!(
                !rendered.to_ascii_lowercase().contains(forbidden),
                "/diagnostics leaked a {forbidden}-shaped field"
            );
        }
    }

    #[test]
    fn stages_are_recorded_once_and_in_order() {
        let mut status = HostStatus::new();
        status.advance("display");
        status.advance("prefix");
        status.advance("display");
        assert_eq!(status.stages_done, vec!["display", "prefix"]);
        assert_eq!(status.stage, "display");
    }
}
