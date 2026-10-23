//! Built-in XAUUSD strategy evaluator.
//!
//! Order flow was removed from Node1 together with the exchange aggTrade
//! adapters, the delta/absorption analyzer and the `bubbles` topic. This module
//! is the replacement: the strategy is evaluated from what the market-data
//! service still owns — SiftingIO 15m candles, the volume profiles
//! (PW / PS / CW / swing) and the tick-volume bars — and it emits a sized
//! decision for the demo engine.
//!
//! Everything here is deterministic: `evaluate` takes plain slices plus a
//! timestamp and never reads a clock or the network, so a decision can be
//! replayed exactly in a unit test.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::types::{TickVolumeBar, VpCandle, VpLevels};

pub const SYMBOL: &str = "XAUUSD";
/// One XAUUSD lot is 100 troy ounces: a $1 move is $100 per lot.
pub const CONTRACT_SIZE: f64 = 100.0;
/// How many evaluations `/strategy` keeps for the dashboard's history panel.
const HISTORY_LIMIT: usize = 120;

/// What the strategy wants done. `Flat` means "no new position": the demo
/// engine keeps whatever it already holds until a stop, a target or an
/// opposite signal closes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Buy,
    Sell,
    Flat,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Buy => "buy",
            Action::Sell => "sell",
            Action::Flat => "flat",
        }
    }

    /// +1 for a long, -1 for a short, 0 when flat.
    pub fn sign(self) -> f64 {
        match self {
            Action::Buy => 1.0,
            Action::Sell => -1.0,
            Action::Flat => 0.0,
        }
    }
}

/// Tunables of the evaluator. The defaults are the shipped behaviour; every
/// field is overridable from the environment (see `Config`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyParams {
    pub enabled: bool,
    pub min_confidence: f64,
    pub risk_pct: f64,
    pub atr_period: usize,
    pub atr_stop_mult: f64,
    pub reward_risk: f64,
    pub min_lots: f64,
    pub max_lots: f64,
    pub lot_step: f64,
    pub fast_ema: usize,
    pub slow_ema: usize,
    pub momentum_bars: usize,
    pub tick_pressure_bars: usize,
    pub event_blackout_secs: i64,
    /// Leverage of the account this engine sizes for (the demo engine: 1:1000).
    pub leverage: f64,
    /// Never commit more than this percentage of equity to margin.
    pub max_margin_pct: f64,
}

impl Default for StrategyParams {
    fn default() -> Self {
        Self {
            enabled: true,
            min_confidence: 0.55,
            risk_pct: 0.5,
            atr_period: 14,
            atr_stop_mult: 1.5,
            reward_risk: 2.0,
            min_lots: 0.01,
            max_lots: 1.0,
            lot_step: 0.01,
            fast_ema: 9,
            slow_ema: 21,
            momentum_bars: 8,
            tick_pressure_bars: 8,
            event_blackout_secs: 900,
            leverage: 1000.0,
            max_margin_pct: 50.0,
        }
    }
}

/// One weighted input to the score, kept in the payload so a client can see
/// *why* the engine took (or skipped) a trade instead of only the verdict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalFactor {
    pub name: String,
    /// -1 (bearish) .. +1 (bullish).
    pub value: f64,
    pub weight: f64,
    /// `value * weight`.
    pub contribution: f64,
    pub detail: String,
}

/// Everything the evaluator reads. Borrowed so the caller keeps ownership of
/// the caches it cloned out of the shared state.
pub struct StrategyInput<'a> {
    pub now_ms: i64,
    pub price: f64,
    /// Closed 15m bars, oldest first (the in-progress bar is not included).
    pub candles: &'a [VpCandle],
    pub levels: &'a [VpLevels],
    /// 15m tick-volume bars, oldest first.
    pub tick_bars: &'a [TickVolumeBar],
    pub equity: f64,
    /// Epoch ms of the next high-impact USD event, if the calendar has one.
    pub next_high_impact_event: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyDecision {
    pub ts: i64,
    pub symbol: String,
    pub action: Action,
    /// Weighted score in -1..1; the sign is the direction.
    pub score: f64,
    /// 0..1 — score magnitude discounted by how much the factors disagree.
    pub confidence: f64,
    pub price: f64,
    pub atr: f64,
    /// `trend_up` | `trend_down` | `balance`.
    pub regime: String,
    /// `expanding` | `normal` | `contracting`.
    pub volatility: String,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub reward_risk: Option<f64>,
    /// 0.01 .. 1.00 in 0.01 steps; 0.0 on a flat decision.
    pub lots: f64,
    pub risk_usd: f64,
    pub reasons: Vec<SignalFactor>,
    pub notes: Vec<String>,
    pub next_event_ts: Option<i64>,
}

/// Evaluator plus the recent decisions `/strategy` serves.
pub struct StrategyEngine {
    pub params: StrategyParams,
    last: Option<StrategyDecision>,
    history: VecDeque<StrategyDecision>,
}

impl StrategyEngine {
    pub fn new(params: StrategyParams) -> Self {
        Self {
            params,
            last: None,
            history: VecDeque::with_capacity(HISTORY_LIMIT),
        }
    }

    pub fn last(&self) -> Option<&StrategyDecision> {
        self.last.as_ref()
    }

    /// Evaluate one snapshot and remember it. The returned decision is the
    /// exact payload that goes out on `/strategy` and the `strategy` WS topic.
    pub fn evaluate(&mut self, input: &StrategyInput<'_>) -> StrategyDecision {
        let decision = self.decide(input);
        self.last = Some(decision.clone());
        self.history.push_back(decision.clone());
        while self.history.len() > HISTORY_LIMIT {
            self.history.pop_front();
        }
        decision
    }

    fn decide(&self, input: &StrategyInput<'_>) -> StrategyDecision {
        let p = &self.params;
        let now = input.now_ms;
        let price = input.price;

        let mut decision = StrategyDecision {
            ts: now,
            symbol: SYMBOL.to_string(),
            action: Action::Flat,
            score: 0.0,
            confidence: 0.0,
            price: round2(price),
            atr: 0.0,
            regime: "balance".to_string(),
            volatility: "normal".to_string(),
            stop_loss: None,
            take_profit: None,
            reward_risk: None,
            lots: 0.0,
            risk_usd: 0.0,
            reasons: Vec::new(),
            notes: Vec::new(),
            next_event_ts: input.next_high_impact_event,
        };

        if !p.enabled {
            decision.notes.push("strategy disabled".into());
            return decision;
        }
        if !price.is_finite() || price <= 0.0 {
            decision.notes.push("no price yet".into());
            return decision;
        }

        let Some(atr) = atr_last(input.candles, p.atr_period) else {
            decision.notes.push(format!(
                "insufficient history: need {} closed 15m bars for ATR({})",
                p.atr_period + 1,
                p.atr_period
            ));
            return decision;
        };
        decision.atr = round2(atr);

        // A high-impact print is a coin flip for a 15m trend model: stand
        // aside instead of paying the spread into the release.
        if let Some(event_ts) = input.next_high_impact_event {
            let blackout_ms = p.event_blackout_secs.max(0) * 1000;
            if (event_ts - now).abs() <= blackout_ms {
                decision.notes.push(format!(
                    "high-impact USD event at {} is inside the {}-minute blackout",
                    event_ts,
                    p.event_blackout_secs / 60
                ));
                return decision;
            }
        }

        let closes: Vec<f64> = input.candles.iter().map(|c| c.close).collect();
        let (Some(fast), Some(slow)) =
            (ema_last(&closes, p.fast_ema), ema_last(&closes, p.slow_ema))
        else {
            decision.notes.push(format!(
                "insufficient history: need {} closed bars for EMA({})",
                p.slow_ema, p.slow_ema
            ));
            return decision;
        };

        let atr_slow = atr_last(input.candles, p.atr_period.saturating_mul(3));
        decision.volatility = (match atr_slow {
            Some(slow_atr) if slow_atr > 0.0 && atr > slow_atr * 1.3 => "expanding",
            Some(slow_atr) if slow_atr > 0.0 && atr < slow_atr * 0.7 => "contracting",
            _ => "normal",
        })
        .to_string();

        let mut factors: Vec<SignalFactor> = Vec::new();

        // 1. Trend: EMA(9) vs EMA(21), scaled by volatility.
        let trend_value = ((fast - slow) / (atr * 0.5)).clamp(-1.0, 1.0);
        factors.push(factor(
            "trend",
            trend_value,
            0.30,
            format!(
                "EMA{} {:.2} vs EMA{} {:.2}",
                p.fast_ema, fast, p.slow_ema, slow
            ),
        ));

        // 2. Where price sits inside the weekly session value area.
        if let Some(level) = reference_level(input.levels) {
            let span = (level.vah - level.val).max(atr * 0.25);
            let value = ((price - level.poc) / span).clamp(-1.0, 1.0);
            factors.push(factor(
                "value_location",
                value,
                0.20,
                format!(
                    "{} value area {:.2}-{:.2}, POC {:.2}",
                    level.window, level.val, level.vah, level.poc
                ),
            ));
        } else {
            decision.notes.push("no volume profile yet".into());
        }

        // 3. Rejection / acceptance at the nearest POC / VAH / VAL.
        if let Some(level_price) = nearest_level_price(input.levels, price) {
            let value = if let Some(last) = input.candles.last() {
                let near = (level_price - last.close).abs() <= atr * 0.5;
                if near && last.low < level_price && last.close > level_price {
                    1.0
                } else if near && last.high > level_price && last.close < level_price {
                    -1.0
                } else {
                    0.0
                }
            } else {
                0.0
            };
            factors.push(factor(
                "level_reaction",
                value,
                0.15,
                format!("nearest level {:.2}", level_price),
            ));
        }

        // 4. Momentum: price change over N bars, normalised by ATR.
        if let Some(value) = momentum(&closes, p.momentum_bars, atr) {
            factors.push(factor(
                "momentum",
                value,
                0.15,
                format!("{} bar change normalised by ATR", p.momentum_bars),
            ));
        }

        // 5. Tick-volume pressure (tick rule, not order flow).
        if let Some(value) = tick_pressure(input.tick_bars, p.tick_pressure_bars) {
            factors.push(factor(
                "tick_pressure",
                value,
                0.10,
                format!("up/down tick split over {} bars", p.tick_pressure_bars),
            ));
        }

        // 6. Direction of the most recent confirmed swing leg.
        if let Some(value) = swing_signal(input.levels, price) {
            factors.push(factor("swing_structure", value, 0.10, "swing leg".into()));
        }

        let mut weighted = 0.0;
        let mut weight_sum = 0.0;
        for f in &factors {
            weighted += f.value * f.weight;
            weight_sum += f.weight;
        }
        let score = if weight_sum > 0.0 {
            (weighted / weight_sum).clamp(-1.0, 1.0)
        } else {
            0.0
        };

        let sign = if score >= 0.0 { 1.0 } else { -1.0 };
        let mut counted = 0usize;
        let mut agreeing = 0usize;
        for f in &factors {
            if f.value.abs() >= 0.05 {
                counted += 1;
                if f.value * sign > 0.0 {
                    agreeing += 1;
                }
            }
        }
        let agreement = if counted == 0 {
            0.0
        } else {
            agreeing as f64 / counted as f64
        };
        let confidence = (score.abs() * (0.55 + 0.45 * agreement)).clamp(0.0, 1.0);

        let trend_value = factors
            .iter()
            .find(|f| f.name == "trend")
            .map(|f| f.value)
            .unwrap_or(0.0);
        decision.regime = (if trend_value >= 0.35 {
            "trend_up"
        } else if trend_value <= -0.35 {
            "trend_down"
        } else {
            "balance"
        })
        .to_string();
        decision.score = round4(score);
        decision.confidence = round4(confidence);
        decision.reasons = factors;

        if score.abs() < 0.15 {
            decision.notes.push("no edge: factors cancel out".into());
            return decision;
        }
        if confidence < p.min_confidence {
            decision
                .notes
                .push(format!("confidence {:.2} below threshold", confidence));
            return decision;
        }

        let action = if score > 0.0 {
            Action::Buy
        } else {
            Action::Sell
        };
        let distance = atr * p.atr_stop_mult;
        let (stop, target) = if action == Action::Buy {
            (price - distance, price + distance * p.reward_risk)
        } else {
            (price + distance, price - distance * p.reward_risk)
        };
        let (lots, risk_usd) = size_position(p, input.equity, price, stop);
        if lots <= 0.0 {
            decision
                .notes
                .push("risk-based size rounds below the 0.01 lot minimum".into());
            return decision;
        }

        decision.action = action;
        decision.stop_loss = Some(round2(stop));
        decision.take_profit = Some(round2(target));
        decision.reward_risk = Some(round2(
            (target - price).abs() / (price - stop).abs().max(1e-9),
        ));
        decision.lots = lots;
        decision.risk_usd = round2(risk_usd);
        decision
    }

    /// `/strategy` payload: params, the latest decision and a short history.
    pub fn snapshot(&self) -> serde_json::Value {
        let last = self
            .last
            .as_ref()
            .and_then(|d| serde_json::to_value(d).ok());
        let history: Vec<serde_json::Value> = self
            .history
            .iter()
            .filter_map(|d| serde_json::to_value(d).ok())
            .collect();
        serde_json::json!({
            "symbol": SYMBOL,
            "enabled": self.params.enabled,
            "params": self.params,
            "last": last,
            "history": history,
        })
    }
}

/// Risk-based position size on the 0.01 lot grid, capped by the configured
/// maximum and by a margin budget. The result is rounded *down* to the grid and
/// then clamped to `[min_lots, max_lots]`, so the strategy always emits a size
/// the demo engine can actually trade. Returns `(lots, risk_usd)`, where
/// `risk_usd` is what the emitted size really puts at risk (not the budget).
pub fn size_position(params: &StrategyParams, equity: f64, entry: f64, stop: f64) -> (f64, f64) {
    let distance = (entry - stop).abs();
    if distance <= 0.0 || equity <= 0.0 || entry <= 0.0 {
        return (0.0, 0.0);
    }
    let risk_budget = equity * params.risk_pct / 100.0;
    let loss_per_lot = distance * CONTRACT_SIZE;
    if loss_per_lot <= 0.0 {
        return (0.0, 0.0);
    }
    let mut lots = risk_budget / loss_per_lot;

    // Margin cap: never commit more than `max_margin_pct` of equity.
    let margin_per_lot = entry * CONTRACT_SIZE / params.leverage.max(1.0);
    if margin_per_lot > 0.0 {
        let margin_cap = equity * params.max_margin_pct / 100.0 / margin_per_lot;
        lots = lots.min(margin_cap);
    }

    let lots = round_lots(lots, params.min_lots, params.max_lots, params.lot_step);
    (lots, round2(lots * loss_per_lot))
}

/// Snap a lot size onto the broker grid: floor to `lot_step`, clamp to the
/// configured range, 0.0 when the input is not a positive number.
pub fn round_lots(lots: f64, min_lots: f64, max_lots: f64, lot_step: f64) -> f64 {
    if !lots.is_finite() || lots <= 0.0 {
        return 0.0;
    }
    let step = if lot_step > 0.0 { lot_step } else { 0.01 };
    let min = if min_lots > 0.0 { min_lots } else { step };
    let max = if max_lots > 0.0 {
        max_lots.max(min)
    } else {
        min
    };
    let snapped = ((lots / step).floor() * step * 10_000.0).round() / 10_000.0;
    snapped.clamp(min, max)
}

fn factor(name: &str, value: f64, weight: f64, detail: String) -> SignalFactor {
    SignalFactor {
        name: name.to_string(),
        value: round4(value),
        weight,
        contribution: round4(value * weight),
        detail,
    }
}

/// Average true range over the last `period` bars. The first range uses the
/// previous close, so `period + 1` closed bars are required.
fn atr_last(candles: &[VpCandle], period: usize) -> Option<f64> {
    if period == 0 || candles.len() <= period {
        return None;
    }
    let start = candles.len() - period - 1;
    let slice = &candles[start..];
    let mut sum = 0.0;
    for pair in slice.windows(2) {
        let prev_close = pair[0].close;
        let candle = &pair[1];
        let tr = (candle.high - candle.low)
            .max((candle.high - prev_close).abs())
            .max((candle.low - prev_close).abs());
        sum += tr;
    }
    let atr = sum / period as f64;
    if atr.is_finite() && atr > 0.0 {
        Some(atr)
    } else {
        None
    }
}

fn ema_last(values: &[f64], period: usize) -> Option<f64> {
    if period == 0 || values.len() < period {
        return None;
    }
    let seed: f64 = values[..period].iter().sum::<f64>() / period as f64;
    let k = 2.0 / (period as f64 + 1.0);
    let mut ema = seed;
    for value in &values[period..] {
        ema = value * k + ema * (1.0 - k);
    }
    Some(ema)
}

fn momentum(closes: &[f64], bars: usize, atr: f64) -> Option<f64> {
    if bars == 0 || closes.len() <= bars || atr <= 0.0 || !atr.is_finite() {
        return None;
    }
    let last = *closes.last()?;
    let reference = closes[closes.len() - 1 - bars];
    Some(((last - reference) / (atr * 1.5)).clamp(-1.0, 1.0))
}

fn tick_pressure(bars: &[TickVolumeBar], lookback: usize) -> Option<f64> {
    if bars.is_empty() {
        return None;
    }
    let start = bars.len().saturating_sub(lookback.max(1));
    let mut up = 0.0;
    let mut down = 0.0;
    let mut total = 0.0;
    for bar in &bars[start..] {
        up += bar.up_ticks as f64;
        down += bar.down_ticks as f64;
        total += bar.ticks as f64;
    }
    if total <= 0.0 {
        return None;
    }
    Some(((up - down) / total).clamp(-1.0, 1.0))
}

/// The weekly reference profile used for the value-area factor: PW first, then
/// PS, then CW.
fn reference_level(levels: &[VpLevels]) -> Option<&VpLevels> {
    for window in ["PW", "PS", "CW"] {
        if let Some(level) = levels.iter().find(|l| l.window == window && l.vah > l.val) {
            return Some(level);
        }
    }
    levels.iter().find(|l| l.vah > l.val)
}

fn nearest_level_price(levels: &[VpLevels], price: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    for level in levels {
        for value in [level.poc, level.vah, level.val] {
            if !value.is_finite() || value <= 0.0 {
                continue;
            }
            best = match best {
                Some(current) if (current - price).abs() <= (value - price).abs() => Some(current),
                _ => Some(value),
            };
        }
    }
    best
}

/// Most recent confirmed swing leg, as a directional bias.
fn swing_signal(levels: &[VpLevels], price: f64) -> Option<f64> {
    let swing = levels.iter().find(|l| l.window.starts_with("SWING"))?;
    if swing.direction.eq_ignore_ascii_case("bearish") {
        Some(if price <= swing.poc { -1.0 } else { -0.5 })
    } else {
        Some(if price >= swing.poc { 1.0 } else { 0.5 })
    }
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candles(n: usize, start: f64, step: f64) -> Vec<VpCandle> {
        (0..n)
            .map(|i| {
                let i = i as f64;
                let open = start + i * step;
                let close = open + step * 0.8;
                VpCandle {
                    time: 1_700_000_000_000 + (i as i64) * 900_000,
                    open,
                    high: close + 0.5,
                    low: open - 0.5,
                    close,
                    volume: 100.0,
                    source: "synthetic".into(),
                }
            })
            .collect()
    }

    fn levels(direction: &str) -> Vec<VpLevels> {
        vec![VpLevels {
            window: "PW".into(),
            poc: 3350.0,
            vah: 3400.0,
            val: 3300.0,
            start: 1_700_000_000_000,
            end: 1_700_600_000_000,
            timestamp: 1_700_600_000_000,
            direction: direction.into(),
            swing_high: None,
            swing_low: None,
            sunday_open: None,
            meta: None,
        }]
    }

    fn tick_bars(bullish: bool) -> Vec<TickVolumeBar> {
        (0..10)
            .map(|i| TickVolumeBar {
                time: 1_700_000_000_000 + i * 900_000,
                ticks: 400,
                up_ticks: if bullish { 180 } else { 80 },
                down_ticks: if bullish { 80 } else { 180 },
                flat_ticks: 140,
                close: 3400.0,
                last_tick: 1_700_000_800_000,
                ticks_per_sec: 1.0,
                closed: true,
                source: "sifting".into(),
            })
            .collect()
    }

    fn input<'a>(
        candles: &'a [VpCandle],
        levels: &'a [VpLevels],
        ticks: &'a [TickVolumeBar],
        price: f64,
    ) -> StrategyInput<'a> {
        StrategyInput {
            now_ms: 1_700_700_000_000,
            price,
            candles,
            levels,
            tick_bars: ticks,
            equity: 10_000.0,
            next_high_impact_event: None,
        }
    }

    #[test]
    fn atr_matches_the_hand_computed_true_range() {
        // Three candles, flat open/close: TR = high - low = 2.0 each.
        let bars: Vec<VpCandle> = (0..3)
            .map(|i| VpCandle {
                time: i * 900_000,
                open: 3400.0,
                high: 3401.0,
                low: 3399.0,
                close: 3400.0,
                volume: 1.0,
                source: "t".into(),
            })
            .collect();
        let atr = atr_last(&bars, 2).expect("atr");
        assert!((atr - 2.0).abs() < 1e-9, "atr = {atr}");
        assert!(atr_last(&bars, 3).is_none(), "period + 1 bars are required");
    }

    #[test]
    fn ema_matches_a_hand_computed_series() {
        // Seed = SMA(3) of 1,2,3 = 2; k = 0.5 -> 2*0.5 + 4*0.5 = 3.
        let ema = ema_last(&[1.0, 2.0, 3.0, 4.0], 3).expect("ema");
        assert!((ema - 3.0).abs() < 1e-9, "ema = {ema}");
    }

    #[test]
    fn insufficient_history_is_flat() {
        let bars = candles(5, 3300.0, 1.0);
        let levels = levels("bullish");
        let ticks = tick_bars(true);
        let mut engine = StrategyEngine::new(StrategyParams::default());
        let decision = engine.evaluate(&input(&bars, &levels, &ticks, 3305.0));
        assert_eq!(decision.action, Action::Flat);
        assert_eq!(decision.lots, 0.0);
        assert!(!decision.notes.is_empty());
    }

    #[test]
    fn strong_uptrend_buys_with_a_bounded_size() {
        let bars = candles(120, 3300.0, 1.0);
        let levels = levels("bullish");
        let ticks = tick_bars(true);
        let price = bars.last().unwrap().close;
        let mut engine = StrategyEngine::new(StrategyParams::default());
        let decision = engine.evaluate(&input(&bars, &levels, &ticks, price));

        assert_eq!(decision.action, Action::Buy, "{decision:?}");
        assert!(decision.confidence >= 0.55, "{decision:?}");
        assert!(
            (0.01..=1.0).contains(&decision.lots),
            "lots = {}",
            decision.lots
        );
        assert!(
            (decision.lots / 0.01 - (decision.lots / 0.01).round()).abs() < 1e-6,
            "lots must sit on the 0.01 grid: {}",
            decision.lots
        );
        let stop = decision.stop_loss.unwrap();
        let target = decision.take_profit.unwrap();
        assert!(stop < price && target > price);
        assert!(decision.risk_usd > 0.0);
    }

    #[test]
    fn strong_downtrend_sells() {
        let bars = candles(120, 3420.0, -1.0);
        let levels = levels("bearish");
        let ticks = tick_bars(false);
        let price = bars.last().unwrap().close;
        let mut engine = StrategyEngine::new(StrategyParams::default());
        let decision = engine.evaluate(&input(&bars, &levels, &ticks, price));
        assert_eq!(decision.action, Action::Sell, "{decision:?}");
        assert!(decision.stop_loss.unwrap() > price);
        assert!(decision.take_profit.unwrap() < price);
    }

    #[test]
    fn a_high_impact_event_forces_the_engine_to_stand_aside() {
        let bars = candles(120, 3300.0, 1.0);
        let levels = levels("bullish");
        let ticks = tick_bars(true);
        let price = bars.last().unwrap().close;
        let mut engine = StrategyEngine::new(StrategyParams::default());
        let mut snapshot = input(&bars, &levels, &ticks, price);
        snapshot.next_high_impact_event = Some(snapshot.now_ms + 60_000);
        let decision = engine.evaluate(&snapshot);
        assert_eq!(decision.action, Action::Flat);
        assert!(decision.notes.iter().any(|n| n.contains("blackout")));
    }

    #[test]
    fn sizing_never_exceeds_one_lot_or_leaves_the_grid() {
        let params = StrategyParams::default();
        // A very large equity would imply thousands of lots; the cap wins.
        let (lots, _) = size_position(&params, 1_000_000_000.0, 3400.0, 3397.3);
        assert_eq!(lots, 1.0);
        // A tiny risk budget still trades the 0.01 minimum the desk asked for,
        // and reports the risk that actually comes with it.
        let (lots, risk) = size_position(&params, 10.0, 3400.0, 3397.3);
        assert_eq!(lots, 0.01);
        assert!((risk - 2.7).abs() < 0.01, "risk = {risk}");
        // A normal account lands on the 0.01 grid inside the range.
        let (lots, risk) = size_position(&params, 10_000.0, 3400.0, 3397.3);
        assert!((0.01..=1.0).contains(&lots));
        assert!((lots * 2.7 * 100.0 - risk).abs() < 0.01);
    }

    #[test]
    fn tick_pressure_uses_the_tick_rule_split() {
        let bullish = tick_pressure(&tick_bars(true), 10).unwrap();
        let bearish = tick_pressure(&tick_bars(false), 10).unwrap();
        assert!((bullish - 0.25).abs() < 1e-9, "{bullish}");
        assert!((bearish + 0.25).abs() < 1e-9, "{bearish}");
        assert!(tick_pressure(&[], 10).is_none());
    }

    #[test]
    fn evaluations_are_recorded_for_the_rest_endpoint() {
        let bars = candles(120, 3300.0, 1.0);
        let levels = levels("bullish");
        let ticks = tick_bars(true);
        let price = bars.last().unwrap().close;
        let mut engine = StrategyEngine::new(StrategyParams::default());
        engine.evaluate(&input(&bars, &levels, &ticks, price));
        engine.evaluate(&input(&bars, &levels, &ticks, price));
        let snapshot = engine.snapshot();
        assert_eq!(snapshot["symbol"], "XAUUSD");
        assert!(snapshot["last"]["action"].is_string());
        assert_eq!(snapshot["history"].as_array().unwrap().len(), 2);
    }
}
