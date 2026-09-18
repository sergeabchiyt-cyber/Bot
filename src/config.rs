use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct Cfg {
    pub server: ServerCfg, pub tz: TzCfg, pub feed: FeedCfg,
    pub vp: VpCfg, pub signal: SignalCfg, pub risk: RiskCfg, pub exec: ExecCfg,
}
#[derive(Clone, Deserialize)] pub struct ServerCfg { pub bind: String }
#[derive(Clone, Deserialize)] pub struct TzCfg { pub offset_hours: i32, pub day_roll: String, pub week_open: String }
#[derive(Clone, Deserialize)] pub struct FeedCfg { pub kind: String, pub oanda_url: String, pub instrument: String, pub price: String, pub csv_dir: String }
#[derive(Clone, Deserialize)] pub struct VpCfg { pub row: f64, pub cw_from_weekday: u8 }
#[derive(Clone, Deserialize)] pub struct SignalCfg {
    pub tf: String, pub atr_tf: String, pub atr_period: usize, pub vol_min: f64,
    pub retest_tol_pips: f64, pub retest_window: u32, pub level_cooldown_h: i64,
}
#[derive(Clone, Deserialize)] pub struct RiskCfg {
    pub pip: f64, pub sl_pips_min: f64, pub sl_pips_max: f64, pub tp_pips_min: f64, pub tp_pips_max: f64,
    pub atr_sl_mult: f64, pub rr: f64, pub paper_equity: f64, pub risk_pct: f64,
}
#[derive(Clone, Deserialize)] pub struct ExecCfg { pub mode: String, pub alert_telegram: bool }

pub fn load(path: &str) -> Cfg {
    toml::from_str(&std::fs::read_to_string(path).expect("config.toml")).expect("config parse")
}
