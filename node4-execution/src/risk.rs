//! Node 4 risk management — policy validation and account-level state.
//!
//! Node 3 sends an **immutable** `TradeIntent` containing the strategy's
//! ATR-derived stop and target. This module enforces the agreed price bands
//! (SL 200–300 pips, TP 600–800 pips, RR 2–3, 1 pip = $0.01 on XAUUSD) plus
//! the account-level caps. A violation is a **refusal** with a stable
//! `error_code`; Node 4 never silently rewrites a strategy price.
//!
//! `check` runs immediately before any broker write. `record_fill` is called on
//! a broker-confirmed fill and `record_close` when an MT5 position disappears
//! from the bridge's positions snapshot (its last reported profit + swap is the
//! realised PnL), which drives the daily-loss, drawdown and cooldown guards.

use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::intent::{IntentRejection, TradeIntent};

const MS_PER_DAY: i64 = 86_400_000;
/// Absolute tolerance in pips / RR units for floating-point band edges.
const BAND_EPS: f64 = 0.01;

#[derive(Debug, Clone)]
pub struct RiskLimits {
    pub sl_min_pips: f64,
    pub sl_max_pips: f64,
    pub tp_min_pips: f64,
    pub tp_max_pips: f64,
    pub rr_min: f64,
    pub rr_max: f64,
    pub pip_value: f64,
    pub contract_size: f64,
    pub max_risk_usd: Option<f64>,
    pub max_daily_loss_usd: Option<f64>,
    pub max_drawdown_pct: Option<f64>,
    pub max_open_positions: usize,
    pub max_lots: f64,
    pub cooldown_losses: u32,
    pub cooldown_secs: u64,
}

impl RiskLimits {
    pub fn from_config(c: &Config) -> Self {
        Self {
            sl_min_pips: c.risk_sl_min_pips,
            sl_max_pips: c.risk_sl_max_pips,
            tp_min_pips: c.risk_tp_min_pips,
            tp_max_pips: c.risk_tp_max_pips,
            rr_min: c.risk_rr_min,
            rr_max: c.risk_rr_max,
            pip_value: c.risk_pip_value,
            contract_size: c.risk_contract_size,
            max_risk_usd: c.risk_max_risk_usd,
            max_daily_loss_usd: c.risk_max_daily_loss_usd,
            max_drawdown_pct: c.risk_max_drawdown_pct,
            max_open_positions: c.risk_max_open_positions,
            max_lots: c.risk_max_lots,
            cooldown_losses: c.risk_cooldown_losses,
            cooldown_secs: c.risk_cooldown_secs,
        }
    }
}

/// Public, read-only view rendered as `/diagnostics.risk` and `WS /ws`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RiskSnapshot {
    pub enabled: bool,
    pub sl_min_pips: f64,
    pub sl_max_pips: f64,
    pub tp_min_pips: f64,
    pub tp_max_pips: f64,
    pub rr_min: f64,
    pub rr_max: f64,
    pub pip_value: f64,
    pub contract_size: f64,
    pub max_risk_usd: Option<f64>,
    pub max_daily_loss_usd: Option<f64>,
    pub max_drawdown_pct: Option<f64>,
    pub max_open_positions: usize,
    pub max_lots: f64,
    pub cooldown_losses: u32,
    pub cooldown_secs: u64,
    pub open_positions: usize,
    pub exposure_lots: f64,
    pub daily_pnl_usd: f64,
    pub realised_pnl_usd: f64,
    pub current_equity_usd: f64,
    pub peak_equity_usd: f64,
    pub drawdown_pct: f64,
    pub consecutive_losses: u32,
    pub cooldown_until_ms: Option<i64>,
    pub intents_checked: u64,
    pub intents_rejected: u64,
    pub last_rejection_code: Option<String>,
    pub last_rejection_message: Option<String>,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Default)]
struct RiskState {
    day_key: i64,
    open_positions: usize,
    exposure_lots: f64,
    daily_pnl_usd: f64,
    realised_pnl_usd: f64,
    current_equity_usd: f64,
    peak_equity_usd: f64,
    consecutive_losses: u32,
    cooldown_until_ms: Option<i64>,
    intents_checked: u64,
    intents_rejected: u64,
    last_rejection_code: Option<String>,
    last_rejection_message: Option<String>,
    updated_at_ms: i64,
}

impl RiskState {
    fn roll_day(&mut self, now_ms: i64) {
        let day = now_ms / MS_PER_DAY;
        if day != self.day_key {
            self.day_key = day;
            self.daily_pnl_usd = 0.0;
        }
    }
}

/// Pure, state-free price-band checks. A violation is a refusal, never an edit.
fn price_refusal(l: &RiskLimits, intent: &TradeIntent, lots: f64) -> Option<(String, String)> {
    let entry = intent.reference_price;
    let sl_price = (entry - intent.stop_loss).abs();
    let tp_price = (intent.take_profit - entry).abs();
    let sl_pips = sl_price / l.pip_value;
    let tp_pips = tp_price / l.pip_value;
    let implied_rr = if sl_pips > 0.0 { tp_pips / sl_pips } else { 0.0 };
    let risk_usd = sl_price * lots.max(0.0) * l.contract_size;

    if !(l.sl_min_pips - BAND_EPS..=l.sl_max_pips + BAND_EPS).contains(&sl_pips) {
        return Some((
            "risk_sl_out_of_band".into(),
            format!(
                "stop {sl_pips:.1} pips is outside the {:.0}-{:.0} pip band",
                l.sl_min_pips, l.sl_max_pips
            ),
        ));
    }
    if !(l.tp_min_pips - BAND_EPS..=l.tp_max_pips + BAND_EPS).contains(&tp_pips) {
        return Some((
            "risk_tp_out_of_band".into(),
            format!(
                "target {tp_pips:.1} pips is outside the {:.0}-{:.0} pip band",
                l.tp_min_pips, l.tp_max_pips
            ),
        ));
    }
    if !(l.rr_min - BAND_EPS..=l.rr_max + BAND_EPS).contains(&implied_rr) {
        return Some((
            "risk_rr_out_of_band".into(),
            format!(
                "implied RR {implied_rr:.2} is outside {:.1}-{:.1}",
                l.rr_min, l.rr_max
            ),
        ));
    }
    if (implied_rr - intent.risk_reward).abs() > 0.05 * implied_rr.max(1.0) {
        return Some((
            "risk_rr_mismatch".into(),
            format!(
                "declared RR {} disagrees with the prices (implied {implied_rr:.2})",
                intent.risk_reward
            ),
        ));
    }
    if !lots.is_finite() || lots <= 0.0 || lots > l.max_lots + BAND_EPS {
        return Some((
            "risk_lot_limit".into(),
            format!("planned lots {lots} exceed the {:.2} lot cap", l.max_lots),
        ));
    }
    if let Some(cap) = l.max_risk_usd.filter(|cap| risk_usd > *cap + BAND_EPS) {
        return Some((
            "risk_per_trade_cap".into(),
            format!("risk ${risk_usd:.2} exceeds the ${cap:.2} per-trade cap"),
        ));
    }
    None
}

/// Account-state checks, evaluated only when the price bands pass.
fn state_refusal(
    l: &RiskLimits,
    s: &RiskState,
    risk_usd: f64,
    now_ms: i64,
) -> Option<(String, String)> {
    if s.open_positions >= l.max_open_positions {
        return Some((
            "risk_max_open_positions".into(),
            format!(
                "{} positions already open (cap {})",
                s.open_positions, l.max_open_positions
            ),
        ));
    }
    if let Some(until) = s.cooldown_until_ms.filter(|until| *until > now_ms) {
        return Some((
            "risk_cooldown".into(),
            format!("cooling down until UTC ms {until} after consecutive losses"),
        ));
    }
    if let Some(cap) = l
        .max_daily_loss_usd
        .filter(|cap| s.daily_pnl_usd - risk_usd <= -*cap)
    {
        return Some((
            "risk_daily_loss_cap".into(),
            format!(
                "daily PnL ${:.2} plus ${risk_usd:.2} risk breaches the -${cap:.2} cap",
                s.daily_pnl_usd
            ),
        ));
    }
    let drawdown = (s.peak_equity_usd > 0.0)
        .then(|| (s.peak_equity_usd - s.current_equity_usd) / s.peak_equity_usd * 100.0);
    if let Some((dd, cap)) = drawdown
        .zip(l.max_drawdown_pct)
        .filter(|(dd, cap)| *dd > *cap)
    {
        return Some((
            "risk_max_drawdown".into(),
            format!("drawdown {dd:.2}% exceeds the {cap:.2}% cap"),
        ));
    }
    None
}

#[derive(Debug, Clone)]
pub struct RiskManager {
    limits: Arc<RiskLimits>,
    state: Arc<RwLock<RiskState>>,
}

impl RiskManager {
    pub fn new(config: &Config) -> Self {
        Self {
            limits: Arc::new(RiskLimits::from_config(config)),
            state: Arc::new(RwLock::new(RiskState::default())),
        }
    }

    fn read(&self) -> RwLockReadGuard<'_, RiskState> {
        self.state.read().unwrap_or_else(|p| p.into_inner())
    }

    fn write(&self) -> RwLockWriteGuard<'_, RiskState> {
        self.state.write().unwrap_or_else(|p| p.into_inner())
    }

    pub fn snapshot(&self) -> RiskSnapshot {
        let l = self.limits.as_ref();
        let s = self.read();
        let drawdown_pct = if s.peak_equity_usd > 0.0 {
            ((s.peak_equity_usd - s.current_equity_usd) / s.peak_equity_usd * 100.0).max(0.0)
        } else {
            0.0
        };
        RiskSnapshot {
            enabled: true,
            sl_min_pips: l.sl_min_pips,
            sl_max_pips: l.sl_max_pips,
            tp_min_pips: l.tp_min_pips,
            tp_max_pips: l.tp_max_pips,
            rr_min: l.rr_min,
            rr_max: l.rr_max,
            pip_value: l.pip_value,
            contract_size: l.contract_size,
            max_risk_usd: l.max_risk_usd,
            max_daily_loss_usd: l.max_daily_loss_usd,
            max_drawdown_pct: l.max_drawdown_pct,
            max_open_positions: l.max_open_positions,
            max_lots: l.max_lots,
            cooldown_losses: l.cooldown_losses,
            cooldown_secs: l.cooldown_secs,
            open_positions: s.open_positions,
            exposure_lots: s.exposure_lots,
            daily_pnl_usd: s.daily_pnl_usd,
            realised_pnl_usd: s.realised_pnl_usd,
            current_equity_usd: s.current_equity_usd,
            peak_equity_usd: s.peak_equity_usd,
            drawdown_pct,
            consecutive_losses: s.consecutive_losses,
            cooldown_until_ms: s.cooldown_until_ms,
            intents_checked: s.intents_checked,
            intents_rejected: s.intents_rejected,
            last_rejection_code: s.last_rejection_code.clone(),
            last_rejection_message: s.last_rejection_message.clone(),
            updated_at_ms: s.updated_at_ms,
        }
    }

    /// Validate one intent against the price bands and the account state.
    ///
    /// Returns `Ok(())` only when the intent may reach the broker. Every
    /// refusal is counted and exposed in `/diagnostics.risk`.
    pub fn check(
        &self,
        intent: &TradeIntent,
        lots: f64,
        now_ms: i64,
    ) -> Result<(), IntentRejection> {
        let l = self.limits.as_ref();
        let sl_price = (intent.reference_price - intent.stop_loss).abs();
        let risk_usd = sl_price * lots.max(0.0) * l.contract_size;

        // A new UTC day resets the realised-PnL counters before any check.
        {
            let mut s = self.write();
            s.roll_day(now_ms);
        }

        let refusal = {
            let s = self.read();
            price_refusal(l, intent, lots).or_else(|| state_refusal(l, &s, risk_usd, now_ms))
        };

        match refusal {
            Some((code, message)) => {
                {
                    let mut s = self.write();
                    s.intents_rejected = s.intents_rejected.saturating_add(1);
                    s.last_rejection_code = Some(code.clone());
                    s.last_rejection_message = Some(message.clone());
                    s.updated_at_ms = now_ms;
                }
                Err(IntentRejection::new(code, message))
            }
            None => {
                let mut s = self.write();
                s.intents_checked = s.intents_checked.saturating_add(1);
                s.updated_at_ms = now_ms;
                Ok(())
            }
        }
    }

    /// Reserve one open position after a broker-confirmed fill.
    pub fn record_fill(&self, lots: f64, now_ms: i64) {
        let mut s = self.write();
        s.roll_day(now_ms);
        s.open_positions = s.open_positions.saturating_add(1);
        s.exposure_lots = (s.exposure_lots + lots.max(0.0)).min(1_000_000.0);
        s.updated_at_ms = now_ms;
    }

    /// Record a closed position's realised PnL (USD). Drives daily loss,
    /// drawdown and cooldown. `pnl_usd` is the broker's profit + swap.
    pub fn record_close(&self, pnl_usd: f64, now_ms: i64) {
        if !pnl_usd.is_finite() {
            return;
        }
        let mut s = self.write();
        s.roll_day(now_ms);
        s.open_positions = s.open_positions.saturating_sub(1);
        s.realised_pnl_usd += pnl_usd;
        s.daily_pnl_usd += pnl_usd;
        s.current_equity_usd += pnl_usd;
        if s.current_equity_usd > s.peak_equity_usd {
            s.peak_equity_usd = s.current_equity_usd;
        }
        if pnl_usd < 0.0 {
            s.consecutive_losses = s.consecutive_losses.saturating_add(1);
            if s.consecutive_losses >= self.limits.cooldown_losses {
                s.cooldown_until_ms =
                    Some(now_ms.saturating_add(self.limits.cooldown_secs as i64 * 1_000));
                s.consecutive_losses = 0;
            }
        } else {
            s.consecutive_losses = 0;
        }
        s.updated_at_ms = now_ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(sl_pips: f64, tp_pips: f64) -> TradeIntent {
        let entry = 2_650.0;
        TradeIntent {
            schema_version: 1,
            intent_id: "n3-xauusd-1-buy-pw-poc".into(),
            strategy: "vp_break_retest_v1".into(),
            symbol: "XAUUSD".into(),
            side: "buy".into(),
            order_type: "market".into(),
            reference_price: entry,
            stop_loss: entry - sl_pips * 0.01,
            take_profit: entry + tp_pips * 0.01,
            risk_reward: tp_pips / sl_pips,
            level_name: "PW PoC".into(),
            source_candle_time: 1_700_000_000_000,
            created_at: 1_700_000_000_000,
            expires_at: i64::MAX,
        }
    }

    fn manager() -> RiskManager {
        RiskManager::new(&Config::default())
    }

    #[test]
    fn a_valid_atr_scaled_intent_passes() {
        let risk = manager();
        assert!(risk.check(&intent(250.0, 625.0), 0.01, 1_700_000_000_000).is_ok());
        assert_eq!(risk.snapshot().intents_checked, 1);
        assert_eq!(risk.snapshot().intents_rejected, 0);
    }

    #[test]
    fn a_stop_outside_the_200_300_band_is_refused() {
        let risk = manager();
        let err = risk.check(&intent(120.0, 625.0), 0.01, 1_700_000_000_000).unwrap_err();
        assert_eq!(err.code, "risk_sl_out_of_band");
        assert_eq!(risk.snapshot().last_rejection_code.as_deref(), Some("risk_sl_out_of_band"));
    }

    #[test]
    fn a_target_outside_the_600_800_band_is_refused() {
        let risk = manager();
        let err = risk.check(&intent(250.0, 1_400.0), 0.01, 1_700_000_000_000).unwrap_err();
        assert_eq!(err.code, "risk_tp_out_of_band");
    }

    #[test]
    fn the_rr_band_and_the_declared_rr_are_enforced() {
        let risk = manager();
        // 200 pips SL / 800 pips TP = 4R, above the 3R ceiling. Both legs are
        // inside their own bands, so this isolates the RR check.
        let err = risk.check(&intent(200.0, 800.0), 0.01, 1_700_000_000_000).unwrap_err();
        assert_eq!(err.code, "risk_rr_out_of_band");

        let mut lying = intent(250.0, 625.0);
        lying.risk_reward = 9.0;
        let err = risk.check(&lying, 0.01, 1_700_000_000_000).unwrap_err();
        assert_eq!(err.code, "risk_rr_mismatch");
    }

    #[test]
    fn the_per_trade_risk_cap_is_enforced() {
        let mut config = Config::default();
        config.risk_max_risk_usd = Some(10.0);
        let risk = RiskManager::new(&config);
        // $2.50 stop x 0.05 lots x 100 oz = $12.50.
        let err = risk.check(&intent(250.0, 625.0), 0.05, 1_700_000_000_000).unwrap_err();
        assert_eq!(err.code, "risk_per_trade_cap");
    }

    #[test]
    fn the_open_position_cap_is_enforced() {
        let mut config = Config::default();
        config.risk_max_open_positions = 1;
        let risk = RiskManager::new(&config);
        let now = 1_700_000_000_000;
        assert!(risk.check(&intent(250.0, 625.0), 0.01, now).is_ok());
        risk.record_fill(0.01, now);
        let err = risk.check(&intent(250.0, 625.0), 0.01, now).unwrap_err();
        assert_eq!(err.code, "risk_max_open_positions");
    }

    #[test]
    fn consecutive_losses_trigger_a_cooldown() {
        let mut config = Config::default();
        config.risk_cooldown_losses = 2;
        config.risk_cooldown_secs = 60;
        let risk = RiskManager::new(&config);
        let now = 1_700_000_000_000;
        risk.record_fill(0.01, now);
        risk.record_close(-5.0, now);
        risk.record_fill(0.01, now);
        risk.record_close(-5.0, now);
        let err = risk.check(&intent(250.0, 625.0), 0.01, now).unwrap_err();
        assert_eq!(err.code, "risk_cooldown");
        // The cooldown expires after the configured window.
        assert!(risk.check(&intent(250.0, 625.0), 0.01, now + 61_000).is_ok());
    }

    #[test]
    fn the_daily_loss_cap_blocks_the_next_trade() {
        let mut config = Config::default();
        config.risk_max_daily_loss_usd = Some(50.0);
        let risk = RiskManager::new(&config);
        let now = 1_700_000_000_000;
        risk.record_fill(0.01, now);
        risk.record_close(-48.0, now);
        // $2.50 stop x 0.01 lots x 100 oz = $2.50 -> would breach -$50.
        let err = risk.check(&intent(250.0, 625.0), 0.01, now).unwrap_err();
        assert_eq!(err.code, "risk_daily_loss_cap");
    }

    #[test]
    fn a_new_utc_day_resets_the_daily_pnl() {
        let mut config = Config::default();
        config.risk_max_daily_loss_usd = Some(50.0);
        let risk = RiskManager::new(&config);
        let now = 1_700_000_000_000;
        risk.record_fill(0.01, now);
        risk.record_close(-48.0, now);
        assert!(risk.check(&intent(250.0, 625.0), 0.01, now + MS_PER_DAY).is_ok());
    }

    #[test]
    fn the_snapshot_exposes_limits_state_and_rejections() {
        let risk = manager();
        let now = 1_700_000_000_000;
        let _ = risk.check(&intent(250.0, 625.0), 0.01, now);
        risk.record_fill(0.01, now);
        let snap = risk.snapshot();
        let json = serde_json::to_value(&snap).unwrap();
        for key in [
            "sl_min_pips", "sl_max_pips", "tp_min_pips", "tp_max_pips",
            "rr_min", "rr_max", "open_positions", "daily_pnl_usd",
            "intents_checked", "intents_rejected", "last_rejection_code",
        ] {
            assert!(json.get(key).is_some(), "missing risk key {key}");
        }
        assert_eq!(snap.open_positions, 1);
    }
}
