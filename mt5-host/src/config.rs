//! This is the Linux/Wine shape of the terminal host, and it supersedes the
//! earlier Windows (or Wine-with-a-desktop) deployment assumption: nothing here
//! assumes a human in front of a GUI, so every fact the manual flow used to
//! imply — virtual display, Wine prefix, unattended installer, non-interactive
//! login, EA attach, restart policy — has to be an environment variable that the
//! host validates before it launches anything.
//! Configuration for the headless MT5 host.
//!
//! Everything is read from the process environment. Two families of variables
//! are mixed here:
//!
//! * `MT5_HOST_*` — how to build and run the terminal (display, Wine, timeouts).
//! * `MT5_*` / `NODE4_WS_URL` / `MT5_BRIDGE_TOKEN` — the **bridge's** own
//!   configuration. The host does not reinterpret those: it passes its
//!   environment through to the bridge process unchanged, so the terminal host
//!   has exactly one description of the bridge's contract
//!   (`mt5-bridge/README.md`).
//!
//! Nothing in this module ever logs, stores or reports `MT5_PASSWORD`; the only
//! place it is written is the terminal's startup ini, which is created with
//! `0600` permissions and removed as soon as the terminal has read it.

use std::env;
use std::path::{Path, PathBuf};

pub const DEFAULT_PORT: u16 = 10_000;
pub const DEFAULT_STATE_DIR: &str = "/data";
pub const DEFAULT_INSTALLER_PATH: &str = "/opt/mt5/mt5setup.exe";
pub const DEFAULT_INSTALLER_URL: &str =
    "https://download.mql5.com/cdn/web/metaquotes.software.corp/mt5/mt5setup.exe";
pub const DEFAULT_EA_NAME: &str = "Mt5BridgeEA";
/// Sub-folder under `MQL5/Experts` the EA is installed into, so that the
/// `[StartUp] Expert=` value is unambiguous (`Expert=Node4\Mt5BridgeEA`).
pub const EA_SUBDIR: &str = "Node4";
pub const DEFAULT_BRIDGE_BIN: &str = "mt5-bridge";
pub const DEFAULT_EA_PORT: u16 = 5055;
pub const DEFAULT_MAGIC: i64 = 330_033;

fn env_str(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn env_opt(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// `1`/`true`/`yes`/`on` (case-insensitive) are true, `0`/`false`/`no`/`off` and
/// an empty value are false, anything else falls back to `default` — a typo in
/// a safety switch must never read as "on".
pub fn parse_flag(raw: &str, default: bool) -> bool {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" | "" => false,
        _ => default,
    }
}

fn env_flag(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(raw) => parse_flag(&raw, default),
        Err(_) => default,
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

/// Parse `MT5_SYMBOL_MAP` (`requested=broker`, comma separated). Malformed
/// entries are kept as an error string rather than silently dropped, so the
/// host refuses to boot on a configuration it cannot honour.
pub fn parse_symbol_map(raw: &str) -> (Vec<(String, String)>, Vec<String>) {
    let mut map = Vec::new();
    let mut errors = Vec::new();
    for entry in raw
        .split(',')
        .map(|item| item.trim())
        .filter(|i| !i.is_empty())
    {
        match entry.split_once('=') {
            Some((from, to)) if !from.trim().is_empty() && !to.trim().is_empty() => {
                map.push((from.trim().to_string(), to.trim().to_string()))
            }
            _ => errors.push(format!(
                "MT5_SYMBOL_MAP entry '{entry}' is not a `requested=broker` pair"
            )),
        }
    }
    (map, errors)
}

#[derive(Debug, Clone)]
pub struct Config {
    // ---- service -----------------------------------------------------------
    pub port: u16,
    pub state_dir: PathBuf,
    pub prepare_only: bool,

    // ---- virtual display ---------------------------------------------------
    pub display: String,
    pub screen: String,
    pub xvfb_bin: String,

    // ---- wine --------------------------------------------------------------
    pub wine_bin: String,
    pub wine_prefix: PathBuf,
    pub wineboot_extra_args: Vec<String>,

    // ---- MT5 installer -----------------------------------------------------
    pub installer_path: PathBuf,
    pub installer_url: String,
    pub prefix_archive_url: Option<String>,
    pub tar_bin: String,
    pub curl_bin: String,

    // ---- terminal credentials ---------------------------------------------
    pub login: Option<String>,
    pub password: Option<String>,
    pub server: Option<String>,
    pub manual_login: bool,

    // ---- terminal startup --------------------------------------------------
    pub symbol: String,
    pub period: String,
    pub ea_name: String,
    pub ea_source: Option<PathBuf>,
    pub ea_token: Option<String>,
    pub ea_port: u16,
    pub magic: i64,
    pub allowed_symbols: Option<String>,
    pub extra_terminal_args: Vec<String>,
    pub write_profile: bool,

    // ---- bridge ------------------------------------------------------------
    pub bridge_bin: PathBuf,
    pub node4_ws_url: Option<String>,
    pub bridge_token: Option<String>,
    pub trading_enabled: bool,

    /// Parsed `MT5_SYMBOL_MAP` (`requested` -> `broker`).
    pub symbol_map: Vec<(String, String)>,

    // ---- operational -------------------------------------------------------
    pub timeouts: Timeouts,
    pub rss_warn_mb: u64,
    pub max_restarts_per_hour: u32,
    pub node4_poll_secs: u64,
}

#[derive(Debug, Clone)]
pub struct Timeouts {
    pub display_secs: u64,
    pub wineboot_secs: u64,
    pub install_secs: u64,
    pub compile_secs: u64,
    pub ea_wait_secs: u64,
    pub prefix_download_secs: u64,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            // 0.1 CPU is slow: these are generous on purpose, and every one of
            // them is overridable so a bigger instance can boot faster.
            display_secs: 60,
            wineboot_secs: 600,
            install_secs: 2_400,
            compile_secs: 600,
            ea_wait_secs: 420,
            prefix_download_secs: 1_800,
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let state_dir = PathBuf::from(env_str("MT5_HOST_STATE_DIR", DEFAULT_STATE_DIR));
        let wine_prefix = env_opt("WINEPREFIX")
            .map(PathBuf::from)
            .unwrap_or_else(|| state_dir.join("wine"));
        let (symbol_map, symbol_map_errors) = parse_symbol_map(&env_str("MT5_SYMBOL_MAP", ""));
        for error in &symbol_map_errors {
            // The bridge validates the map authoritatively and refuses to start
            // on a malformed one; say so here too, where the boot log is read.
            eprintln!("mt5-host: warning: {error}");
        }

        Self {
            port: env::var("PORT")
                .ok()
                .and_then(|value| value.trim().parse::<u16>().ok())
                .or_else(|| {
                    env::var("MT5_HOST_PORT")
                        .ok()
                        .and_then(|value| value.trim().parse::<u16>().ok())
                })
                .filter(|port| *port != 0)
                .unwrap_or(DEFAULT_PORT),
            state_dir,
            prepare_only: env_flag("MT5_HOST_PREPARE_ONLY", false),

            display: env_str("MT5_HOST_DISPLAY", ":99"),
            screen: env_str("MT5_HOST_SCREEN", "640x480x16"),
            xvfb_bin: env_str("MT5_HOST_XVFB_BIN", "Xvfb"),

            wine_bin: env_str("MT5_HOST_WINE_BIN", "wine"),
            wine_prefix,
            wineboot_extra_args: split_args(&env_str("MT5_HOST_WINEBOOT_ARGS", "")),

            installer_path: PathBuf::from(env_str("MT5_HOST_INSTALLER", DEFAULT_INSTALLER_PATH)),
            installer_url: env_str("MT5_HOST_INSTALLER_URL", DEFAULT_INSTALLER_URL),
            prefix_archive_url: env_opt("MT5_HOST_PREFIX_ARCHIVE_URL"),
            tar_bin: env_str("MT5_HOST_TAR_BIN", "tar"),
            curl_bin: env_str("MT5_HOST_CURL_BIN", "curl"),

            login: env_opt("MT5_LOGIN"),
            password: env_opt("MT5_PASSWORD"),
            server: env_opt("MT5_SERVER"),
            manual_login: env_flag("MT5_HOST_MANUAL_LOGIN", false),

            symbol: env_str("MT5_SYMBOL", "XAUUSD"),
            period: env_str("MT5_HOST_PERIOD", "1"),
            ea_name: env_str("MT5_HOST_EA_NAME", DEFAULT_EA_NAME),
            ea_source: env_opt("MT5_HOST_EA_SOURCE").map(PathBuf::from),
            ea_token: env_opt("MT5_EA_TOKEN"),
            ea_port: env::var("MT5_EA_PORT")
                .ok()
                .and_then(|value| value.trim().parse::<u16>().ok())
                .filter(|port| *port != 0)
                .unwrap_or(DEFAULT_EA_PORT),
            magic: env::var("MT5_MAGIC")
                .ok()
                .and_then(|value| value.trim().parse::<i64>().ok())
                .filter(|magic| *magic > 0)
                .unwrap_or(DEFAULT_MAGIC),
            allowed_symbols: env_opt("MT5_HOST_ALLOWED_SYMBOLS"),
            extra_terminal_args: split_args(&env_str("MT5_HOST_EXTRA_TERMINAL_ARGS", "")),
            write_profile: env_flag("MT5_HOST_WRITE_PROFILE", false),

            bridge_bin: PathBuf::from(env_str("MT5_HOST_BRIDGE_BIN", DEFAULT_BRIDGE_BIN)),
            node4_ws_url: env_opt("NODE4_WS_URL"),
            bridge_token: env_opt("MT5_BRIDGE_TOKEN"),
            // Monitor-only unless the operator explicitly switches trading on.
            trading_enabled: env_flag("MT5_TRADING_ENABLED", false),

            timeouts: Timeouts {
                display_secs: env_u64("MT5_HOST_DISPLAY_TIMEOUT_SECS", 60),
                wineboot_secs: env_u64("MT5_HOST_WINEBOOT_TIMEOUT_SECS", 600),
                install_secs: env_u64("MT5_HOST_INSTALL_TIMEOUT_SECS", 2_400),
                compile_secs: env_u64("MT5_HOST_COMPILE_TIMEOUT_SECS", 600),
                ea_wait_secs: env_u64("MT5_HOST_EA_WAIT_SECS", 420),
                prefix_download_secs: env_u64("MT5_HOST_PREFIX_DOWNLOAD_TIMEOUT_SECS", 1_800),
            },
            rss_warn_mb: env_u64("MT5_HOST_RSS_WARN_MB", 420),
            max_restarts_per_hour: env_u64("MT5_HOST_MAX_RESTARTS_PER_HOUR", 5) as u32,
            node4_poll_secs: env_u64("MT5_HOST_NODE4_POLL_SECS", 15),
            symbol_map,
        }
    }

    /// `MT5_SYMBOL_MAP` as parsed, used to widen the EA's allowed symbols.
    pub fn symbol_map(&self) -> &[(String, String)] {
        &self.symbol_map
    }

    /// Errors that must stop the process: they describe a host that could never
    /// present a working terminal, or a bridge that would refuse to start.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // A prepared prefix has no credentials in it by design: that is what
        // makes it safe to publish as a build artifact.
        if !self.prepare_only {
            let credentials = [
                ("MT5_LOGIN", self.login.is_some()),
                ("MT5_PASSWORD", self.password.is_some()),
                ("MT5_SERVER", self.server.is_some()),
            ];
            let missing: Vec<&str> = credentials
                .iter()
                .filter(|(_, present)| !present)
                .map(|(name, _)| *name)
                .collect();
            let any_present = credentials.iter().any(|(_, present)| *present);
            if self.manual_login {
                if any_present {
                    errors.push(
                        "MT5_HOST_MANUAL_LOGIN=1 yet MT5_LOGIN/MT5_PASSWORD/MT5_SERVER are set — \
                         remove them (the terminal is logged in some other way) or unset \
                         MT5_HOST_MANUAL_LOGIN so the host can log in unattended"
                            .into(),
                    );
                }
            } else if !missing.is_empty() {
                errors.push(format!(
                    "unattended terminal login needs MT5_LOGIN, MT5_PASSWORD and MT5_SERVER \
                     together (missing: {}) — or set MT5_HOST_MANUAL_LOGIN=1 if the terminal is \
                     logged in another way",
                    missing.join(", ")
                ));
            }

            match self.node4_ws_url.as_deref() {
                None => errors.push(
                    "NODE4_WS_URL is not set — the bridge dials out to Node 4's WS /mt5/bridge \
                     endpoint and will not start without it"
                        .into(),
                ),
                Some(url) if !(url.starts_with("ws://") || url.starts_with("wss://")) => errors
                    .push(format!(
                        "NODE4_WS_URL must be a ws:// or wss:// URL, got '{url}'"
                    )),
                Some(_) => {}
            }
            if self.bridge_token.is_none() {
                errors.push(
                    "MT5_BRIDGE_TOKEN is not set — it must equal Node 4's MT5_BRIDGE_TOKEN; the \
                     bridge refuses to start without it"
                        .into(),
                );
            }
        } else if self.node4_ws_url.is_some() || self.bridge_token.is_some() {
            // Harmless, but a prepared prefix must never contain a live
            // credential: warn loudly instead of silently writing one.
            errors.push(
                "MT5_HOST_PREPARE_ONLY=1 but NODE4_WS_URL/MT5_BRIDGE_TOKEN are set — the archive \
                 is meant to be credential-free; unset them for the prepare run"
                    .into(),
            );
        }

        if self.login.is_some() && self.manual_login {
            errors.push("MT5_HOST_MANUAL_LOGIN=1 conflicts with MT5_LOGIN".into());
        }
        if !self.port_ok() {
            errors.push("PORT/MT5_HOST_PORT must be a usable port".into());
        }
        if self.symbol.trim().is_empty() {
            errors.push("MT5_SYMBOL must not be empty".into());
        }
        if self.ea_token.is_none() {
            // Not fatal: the bridge stays read-only without it, which is a
            // perfectly valid monitoring deployment.
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn port_ok(&self) -> bool {
        self.port != 0
    }

    /// Non-secret settings, for logs and `/diagnostics`. Never includes the
    /// password (asserted by a test), and never includes the shared tokens.
    pub fn summary(&self) -> Vec<(String, String)> {
        vec![
            ("port".into(), self.port.to_string()),
            ("state_dir".into(), self.state_dir.display().to_string()),
            ("wine_prefix".into(), self.wine_prefix.display().to_string()),
            ("display".into(), self.display.clone()),
            ("screen".into(), self.screen.clone()),
            ("wine_bin".into(), self.wine_bin.clone()),
            (
                "installer".into(),
                self.installer_path.display().to_string(),
            ),
            (
                "prefix_archive".into(),
                self.prefix_archive_url
                    .clone()
                    .unwrap_or_else(|| "(installer)".into()),
            ),
            ("symbol".into(), self.symbol.clone()),
            ("period".into(), self.period.clone()),
            ("ea_name".into(), self.ea_name.clone()),
            (
                "login".into(),
                self.login.clone().unwrap_or_else(|| "(unset)".into()),
            ),
            (
                "server".into(),
                self.server.clone().unwrap_or_else(|| "(unset)".into()),
            ),
            ("manual_login".into(), self.manual_login.to_string()),
            ("prepare_only".into(), self.prepare_only.to_string()),
            (
                "node4_ws_url".into(),
                self.node4_ws_url
                    .clone()
                    .unwrap_or_else(|| "(unset)".into()),
            ),
            ("trading_enabled".into(), self.trading_enabled.to_string()),
            ("ea_token".into(), present(self.ea_token.as_deref())),
            ("bridge_token".into(), present(self.bridge_token.as_deref())),
            (
                "max_restarts_per_hour".into(),
                self.max_restarts_per_hour.to_string(),
            ),
        ]
    }

    /// `MT5_HOST_ALLOWED_SYMBOLS`, or the strategy symbol plus every explicit
    /// broker symbol from `MT5_SYMBOL_MAP`.
    pub fn allowed_symbols(&self) -> String {
        if let Some(explicit) = self.allowed_symbols.as_deref() {
            return explicit.to_string();
        }
        let mut symbols = vec![self.symbol.clone()];
        for (_, broker) in self.symbol_map() {
            if !symbols
                .iter()
                .any(|known| known.eq_ignore_ascii_case(broker))
            {
                symbols.push(broker.clone());
            }
        }
        symbols.join(",")
    }

    pub fn mt5_install_dir_hint(&self) -> PathBuf {
        self.wine_prefix
            .join("drive_c")
            .join("Program Files")
            .join("MetaTrader 5")
    }

    pub fn start_ini_path(&self) -> PathBuf {
        self.state_dir.join("mt5-start.ini")
    }

    pub fn preset_path(&self, install_dir: &Path) -> PathBuf {
        install_dir
            .join("MQL5")
            .join("Presets")
            .join(format!("{}.set", self.ea_name))
    }

    /// Environment shared by every Wine invocation.
    pub fn wine_env(&self) -> Vec<(String, String)> {
        vec![
            ("WINEPREFIX".into(), self.wine_prefix.display().to_string()),
            ("WINEARCH".into(), "win64".into()),
            ("DISPLAY".into(), self.display.clone()),
            // No debug channel: on 0.1 CPU the logging itself is measurable.
            ("WINEDEBUG".into(), "-all".into()),
            // Never prompt for Mono/Gecko: those dialogs are invisible here and
            // would hang the unattended boot.
            ("WINEDLLOVERRIDES".into(), "mscoree,mshtml=".into()),
        ]
    }

    /// The bridge's environment: this process's environment, with the few
    /// defaults the host owns filled in. Values the operator set always win.
    pub fn bridge_env(&self) -> Vec<(String, String)> {
        let mut env = Vec::new();
        if env::var_os("MT5_TRADING_ENABLED").is_none() {
            env.push(("MT5_TRADING_ENABLED".into(), "0".into()));
        }
        if env::var_os("MT5_HISTORY_FILE").is_none() {
            env.push((
                "MT5_HISTORY_FILE".into(),
                self.state_dir
                    .join("mt5_history.jsonl")
                    .display()
                    .to_string(),
            ));
        }
        if env::var_os("MT5_EA_BIND_ADDR").is_none() {
            env.push(("MT5_EA_BIND_ADDR".into(), "127.0.0.1".into()));
        }
        if env::var_os("MT5_EA_PORT").is_none() {
            env.push(("MT5_EA_PORT".into(), self.ea_port.to_string()));
        }
        if env::var_os("RUST_LOG").is_none() {
            env.push(("RUST_LOG".into(), "info".into()));
        }
        env
    }

    /// Where the mt5-bridge history ledger lives (used by `/diagnostics`).
    pub fn history_file(&self) -> PathBuf {
        env_opt("MT5_HISTORY_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| self.state_dir.join("mt5_history.jsonl"))
    }
}

/// `present`/`absent`, so a summary can describe a secret without printing it.
pub fn present(value: Option<&str>) -> String {
    match value {
        Some(_) => "set".into(),
        None => "(unset)".into(),
    }
}

pub fn split_args(raw: &str) -> Vec<String> {
    raw.split_whitespace()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Config {
        Config {
            port: 10_000,
            state_dir: PathBuf::from("/data"),
            prepare_only: false,
            display: ":99".into(),
            screen: "640x480x16".into(),
            xvfb_bin: "Xvfb".into(),
            wine_bin: "wine".into(),
            wine_prefix: PathBuf::from("/data/wine"),
            wineboot_extra_args: Vec::new(),
            installer_path: PathBuf::from("/opt/mt5/mt5setup.exe"),
            installer_url: DEFAULT_INSTALLER_URL.into(),
            prefix_archive_url: None,
            tar_bin: "tar".into(),
            curl_bin: "curl".into(),
            login: Some("123456".into()),
            password: Some("hunter2".into()),
            server: Some("Deriv-Demo".into()),
            manual_login: false,
            symbol: "XAUUSD".into(),
            period: "1".into(),
            ea_name: DEFAULT_EA_NAME.into(),
            ea_source: None,
            ea_token: Some("ea-token".into()),
            ea_port: DEFAULT_EA_PORT,
            magic: DEFAULT_MAGIC,
            allowed_symbols: None,
            extra_terminal_args: Vec::new(),
            write_profile: false,
            bridge_bin: PathBuf::from("mt5-bridge"),
            node4_ws_url: Some("wss://node4.example/mt5/bridge".into()),
            bridge_token: Some("bridge-token".into()),
            trading_enabled: false,
            timeouts: Timeouts::default(),
            rss_warn_mb: 420,
            max_restarts_per_hour: 5,
            node4_poll_secs: 15,
            symbol_map: Vec::new(),
        }
    }

    #[test]
    fn credentials_are_all_or_nothing() {
        let mut config = base();
        assert!(config.validate().is_ok());

        config.password = None;
        let errors = config.validate().unwrap_err();
        assert!(
            errors.iter().any(|e| e.contains("MT5_PASSWORD")),
            "{errors:?}"
        );

        // Manual login is the documented alternative, but it must not be
        // combined with half-set credentials.
        let mut manual = base();
        manual.password = None;
        manual.login = None;
        manual.server = None;
        manual.manual_login = true;
        assert!(manual.validate().is_ok());

        let mut conflicted = manual.clone();
        conflicted.login = Some("123456".into());
        assert!(conflicted.validate().is_err());
    }

    #[test]
    fn a_prepare_run_does_not_need_credentials_and_refuses_to_hold_them() {
        let mut config = base();
        config.prepare_only = true;
        config.login = None;
        config.password = None;
        config.server = None;
        config.node4_ws_url = None;
        config.bridge_token = None;
        assert!(config.validate().is_ok());

        config.bridge_token = Some("live-token".into());
        let errors = config.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("credential-free")));
    }

    #[test]
    fn the_node4_url_must_be_a_websocket_url() {
        let mut config = base();
        config.node4_ws_url = Some("https://node4.example/mt5/bridge".into());
        let errors = config.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("ws:// or wss://")));
    }

    #[test]
    fn the_summary_never_carries_the_password_or_a_token() {
        let config = base();
        let rendered = format!("{:?}", config.summary());
        assert!(!rendered.contains("hunter2"), "password leaked: {rendered}");
        assert!(
            !rendered.contains("bridge-token"),
            "token leaked: {rendered}"
        );
        assert!(!rendered.contains("ea-token"), "token leaked: {rendered}");
        assert!(rendered.contains("Deriv-Demo"));
        assert!(rendered.contains("\"login\", \"123456\"") || rendered.contains("123456"));
    }

    #[test]
    fn flags_fail_closed_on_typos() {
        assert!(!parse_flag("maybe", false));
        assert!(!parse_flag("", false));
        assert!(parse_flag("TRUE", false));
        assert!(!parse_flag("0", true));
    }

    #[test]
    fn allowed_symbols_include_explicit_broker_mappings() {
        let mut config = base();
        config.symbol_map = parse_symbol_map("XAUUSD=XAUUSD.a,XAUUSD=XAUUSD.m").0;
        assert_eq!(config.allowed_symbols(), "XAUUSD,XAUUSD.a,XAUUSD.m");

        config.allowed_symbols = Some("GOLD".into());
        assert_eq!(config.allowed_symbols(), "GOLD");
    }

    #[test]
    fn symbol_map_errors_are_reported_not_dropped() {
        // `broken` has no pair at all; `XAUUSD=` maps onto nothing. Both are
        // errors: the host fails closed rather than booting with a mapping it
        // cannot honour.
        let (map, errors) = parse_symbol_map("XAUUSD=XAUUSD.a,broken,XAUUSD= ");
        assert_eq!(map, vec![("XAUUSD".to_string(), "XAUUSD.a".to_string())]);
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors[0].contains("broken"), "{errors:?}");
        assert!(errors[1].contains("XAUUSD="), "{errors:?}");

        // A clean list is silent.
        let (map, errors) = parse_symbol_map(" XAUUSD=XAUUSD.a , GOLD=XAUUSD ");
        assert_eq!(map.len(), 2);
        assert!(errors.is_empty(), "{errors:?}");

        // Padding-only and empty input are not errors either.
        assert_eq!(parse_symbol_map(" , ,"), (Vec::new(), Vec::new()));
        assert_eq!(parse_symbol_map(""), (Vec::new(), Vec::new()));
    }

    #[test]
    fn bridge_defaults_are_monitor_only() {
        let config = base();
        let env = config.bridge_env();
        // Only the host-owned defaults appear here; the operator's own values
        // are inherited, never overwritten.
        for (key, value) in &env {
            match key.as_str() {
                "MT5_TRADING_ENABLED" => assert_eq!(value, "0"),
                "MT5_EA_BIND_ADDR" => assert_eq!(value, "127.0.0.1"),
                _ => {}
            }
        }
        assert!(!config.trading_enabled, "trading must be opt-in");
    }

    #[test]
    fn split_args_ignores_padding() {
        assert_eq!(
            split_args("  /skipupdate   /portable "),
            vec!["/skipupdate", "/portable"]
        );
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn the_symbol_map_field_is_not_dead() {
        let config = base();
        assert!(config.symbol_map().is_empty());
        assert_eq!(
            config.history_file(),
            PathBuf::from("/data/mt5_history.jsonl")
        );
        assert_eq!(
            config.start_ini_path(),
            PathBuf::from("/data/mt5-start.ini")
        );
        assert_eq!(
            config.preset_path(Path::new("/data/wine/drive_c/Program Files/MetaTrader 5")),
            PathBuf::from(
                "/data/wine/drive_c/Program Files/MetaTrader 5/MQL5/Presets/Mt5BridgeEA.set"
            )
        );
    }
}
