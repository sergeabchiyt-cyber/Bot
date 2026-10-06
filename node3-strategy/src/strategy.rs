use crate::risk::RiskProjector;
use crate::types::{ScannedLevelSetup, ScanningSnapshot, VpCandle, VpLevels};
use std::collections::{HashMap, VecDeque};
use tracing::{debug, info};

pub const PROXIMITY_DOLLARS: f64 = 0.50;
pub const INVALIDATION_DOLLARS: f64 = 2.00;

#[derive(Debug, Clone)]
pub struct LevelTarget {
    pub name: String,
    pub window: String,
    pub price: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakState {
    None,
    BrokenAbove,
    BrokenBelow,
}

#[derive(Debug, Clone, Default)]
pub struct LevelDiagnosticsMeta {
    pub broken_at: Option<i64>,
    pub broken_price: Option<f64>,
    pub last_note: String,
}

#[derive(Debug, Clone, Default)]
pub struct CandleEvalReport {
    pub triggered: Option<(&'static str, f64, String)>,
    pub breaks_detected: u64,
    pub breaks_invalidated: u64,
    pub retests_rejected_low_volume: u64,
    pub signals_confirmed: u64,
    pub events: Vec<(String, String)>,
}

pub struct StrategyEngine {
    pub levels: HashMap<String, LevelTarget>,
    pub candle_history: VecDeque<VpCandle>,
    pub break_states: HashMap<String, BreakState>,
    pub level_meta: HashMap<String, LevelDiagnosticsMeta>,
    pub last_triggered_candle: i64,
}

impl StrategyEngine {
    pub fn new() -> Self {
        Self {
            levels: HashMap::new(),
            candle_history: VecDeque::with_capacity(50),
            break_states: HashMap::new(),
            level_meta: HashMap::new(),
            last_triggered_candle: 0,
        }
    }

    fn upsert_level(&mut self, id: &str, name: &str, window: &str, price: f64) {
        if !price.is_finite() || price <= 0.0 {
            return;
        }
        self.levels.insert(
            id.to_string(),
            LevelTarget {
                name: name.to_string(),
                window: window.to_string(),
                price,
            },
        );
        self.level_meta
            .entry(id.to_string())
            .or_insert_with(|| LevelDiagnosticsMeta {
                broken_at: None,
                broken_price: None,
                last_note: format!("Watching {name} @ {price:.2} for clean candle break"),
            });
    }

    pub fn update_levels(&mut self, lvl: &VpLevels) {
        match lvl.window.as_str() {
            "PW" => {
                self.upsert_level("PW_POC", "PW PoC", "PW", lvl.poc);
                self.upsert_level("PW_VAH", "PW VaH", "PW", lvl.vah);
                self.upsert_level("PW_VAL", "PW VaL", "PW", lvl.val);
                info!(
                    "Updated PW Levels: PoC={:.2}, VaH={:.2}, VaL={:.2}",
                    lvl.poc, lvl.vah, lvl.val
                );
            }
            "PS" => {
                self.upsert_level("PS_POC", "PS PoC", "PS", lvl.poc);
                info!("Updated PS Level: PoC={:.2}", lvl.poc);
            }
            "CW" => {
                self.upsert_level("CW_POC", "CW PoC", "CW", lvl.poc);
                self.upsert_level("CW_VAH", "CW VaH", "CW", lvl.vah);
                self.upsert_level("CW_VAL", "CW VaL", "CW", lvl.val);
                info!(
                    "Updated CW Levels: PoC={:.2}, VaH={:.2}, VaL={:.2}",
                    lvl.poc, lvl.vah, lvl.val
                );
            }
            _ => {}
        }
    }

    pub fn calculate_atr(&self) -> f64 {
        if self.candle_history.len() < 2 {
            return 3.0;
        }

        let mut tr_sum = 0.0;
        let mut count = 0;

        for pair in self.candle_history.iter().collect::<Vec<_>>().windows(2) {
            let prev = pair[0];
            let curr = pair[1];

            let hl = curr.high - curr.low;
            let hc = (curr.high - prev.close).abs();
            let lc = (curr.low - prev.close).abs();

            let tr = hl.max(hc).max(lc);
            tr_sum += tr;
            count += 1;
        }

        if count > 0 {
            tr_sum / count as f64
        } else {
            3.0
        }
    }

    pub fn evaluate_candle_with_diagnostics(
        &mut self,
        candle: &VpCandle,
        volume_threshold: f64,
    ) -> CandleEvalReport {
        self.candle_history.push_back(candle.clone());
        if self.candle_history.len() > 30 {
            self.candle_history.pop_front();
        }

        let mut report = CandleEvalReport::default();

        if candle.time <= self.last_triggered_candle {
            return report;
        }

        let price = candle.close;
        let is_green = candle.close > candle.open;
        let is_red = candle.close < candle.open;
        let vol_ok = candle.volume >= volume_threshold;

        // Deterministic iteration order by level id
        let mut level_ids: Vec<String> = self.levels.keys().cloned().collect();
        level_ids.sort();

        for id in level_ids {
            let Some(target) = self.levels.get(&id).cloned() else {
                continue;
            };
            let lvl_price = target.price;
            let state = self
                .break_states
                .get(&id)
                .copied()
                .unwrap_or(BreakState::None);

            match state {
                BreakState::None => {
                    if is_green && candle.close > lvl_price && candle.open <= lvl_price {
                        debug!(
                            "Level {} broken ABOVE by candle at {:.2}",
                            target.name, candle.close
                        );
                        self.break_states
                            .insert(id.clone(), BreakState::BrokenAbove);
                        let note = format!(
                            "Broken ABOVE @ {:.2} — awaiting bullish retest of {:.2} (±${:.2})",
                            candle.close, lvl_price, PROXIMITY_DOLLARS
                        );
                        self.level_meta.insert(
                            id.clone(),
                            LevelDiagnosticsMeta {
                                broken_at: Some(candle.time),
                                broken_price: Some(candle.close),
                                last_note: note.clone(),
                            },
                        );
                        report.breaks_detected += 1;
                        report
                            .events
                            .push(("info".into(), format!("{}: {}", target.name, note)));
                    } else if is_red && candle.close < lvl_price && candle.open >= lvl_price {
                        debug!(
                            "Level {} broken BELOW by candle at {:.2}",
                            target.name, candle.close
                        );
                        self.break_states
                            .insert(id.clone(), BreakState::BrokenBelow);
                        let note = format!(
                            "Broken BELOW @ {:.2} — awaiting bearish retest of {:.2} (±${:.2})",
                            candle.close, lvl_price, PROXIMITY_DOLLARS
                        );
                        self.level_meta.insert(
                            id.clone(),
                            LevelDiagnosticsMeta {
                                broken_at: Some(candle.time),
                                broken_price: Some(candle.close),
                                last_note: note.clone(),
                            },
                        );
                        report.breaks_detected += 1;
                        report
                            .events
                            .push(("info".into(), format!("{}: {}", target.name, note)));
                    }
                }
                BreakState::BrokenAbove => {
                    let retested = candle.low <= lvl_price + PROXIMITY_DOLLARS
                        && candle.close >= lvl_price - PROXIMITY_DOLLARS;
                    if retested {
                        if vol_ok {
                            info!(
                                "BULLISH SETUP CONFIRMED on {}! Retest @ {:.2}, Volume: {:.1} (> {:.1})",
                                target.name, price, candle.volume, volume_threshold
                            );
                            self.break_states.insert(id.clone(), BreakState::None);
                            self.last_triggered_candle = candle.time;
                            let note = format!(
                                "BULLISH RETEST CONFIRMED @ {:.2} (vol {:.0} >= {:.0}) — triggering BUY",
                                price, candle.volume, volume_threshold
                            );
                            self.level_meta.insert(
                                id.clone(),
                                LevelDiagnosticsMeta {
                                    broken_at: None,
                                    broken_price: None,
                                    last_note: note.clone(),
                                },
                            );
                            report.signals_confirmed += 1;
                            report
                                .events
                                .push(("signal".into(), format!("{}: {}", target.name, note)));
                            report.triggered = Some(("buy", price, target.name.clone()));
                            return report;
                        } else {
                            debug!(
                                "Retest on {} but volume {:.1} < threshold {:.1}",
                                target.name, candle.volume, volume_threshold
                            );
                            let note = format!(
                                "Retested {:.2} (low {:.2}) but volume {:.0} < {:.0} threshold — waiting",
                                lvl_price, candle.low, candle.volume, volume_threshold
                            );
                            if let Some(meta) = self.level_meta.get_mut(&id) {
                                meta.last_note = note.clone();
                            }
                            report.retests_rejected_low_volume += 1;
                            report
                                .events
                                .push(("warn".into(), format!("{}: {}", target.name, note)));
                        }
                    } else if candle.close < lvl_price - INVALIDATION_DOLLARS {
                        self.break_states.insert(id.clone(), BreakState::None);
                        let note = format!(
                            "Bullish break invalidated (close {:.2} < {:.2})",
                            candle.close,
                            lvl_price - INVALIDATION_DOLLARS
                        );
                        self.level_meta.insert(
                            id.clone(),
                            LevelDiagnosticsMeta {
                                broken_at: None,
                                broken_price: None,
                                last_note: note.clone(),
                            },
                        );
                        report.breaks_invalidated += 1;
                        report
                            .events
                            .push(("warn".into(), format!("{}: {}", target.name, note)));
                    }
                }
                BreakState::BrokenBelow => {
                    let retested = candle.high >= lvl_price - PROXIMITY_DOLLARS
                        && candle.close <= lvl_price + PROXIMITY_DOLLARS;
                    if retested {
                        if vol_ok {
                            info!(
                                "BEARISH SETUP CONFIRMED on {}! Retest @ {:.2}, Volume: {:.1} (> {:.1})",
                                target.name, price, candle.volume, volume_threshold
                            );
                            self.break_states.insert(id.clone(), BreakState::None);
                            self.last_triggered_candle = candle.time;
                            let note = format!(
                                "BEARISH RETEST CONFIRMED @ {:.2} (vol {:.0} >= {:.0}) — triggering SELL",
                                price, candle.volume, volume_threshold
                            );
                            self.level_meta.insert(
                                id.clone(),
                                LevelDiagnosticsMeta {
                                    broken_at: None,
                                    broken_price: None,
                                    last_note: note.clone(),
                                },
                            );
                            report.signals_confirmed += 1;
                            report
                                .events
                                .push(("signal".into(), format!("{}: {}", target.name, note)));
                            report.triggered = Some(("sell", price, target.name.clone()));
                            return report;
                        } else {
                            debug!(
                                "Retest on {} but volume {:.1} < threshold {:.1}",
                                target.name, candle.volume, volume_threshold
                            );
                            let note = format!(
                                "Retested {:.2} (high {:.2}) but volume {:.0} < {:.0} threshold — waiting",
                                lvl_price, candle.high, candle.volume, volume_threshold
                            );
                            if let Some(meta) = self.level_meta.get_mut(&id) {
                                meta.last_note = note.clone();
                            }
                            report.retests_rejected_low_volume += 1;
                            report
                                .events
                                .push(("warn".into(), format!("{}: {}", target.name, note)));
                        }
                    } else if candle.close > lvl_price + INVALIDATION_DOLLARS {
                        self.break_states.insert(id.clone(), BreakState::None);
                        let note = format!(
                            "Bearish break invalidated (close {:.2} > {:.2})",
                            candle.close,
                            lvl_price + INVALIDATION_DOLLARS
                        );
                        self.level_meta.insert(
                            id.clone(),
                            LevelDiagnosticsMeta {
                                broken_at: None,
                                broken_price: None,
                                last_note: note.clone(),
                            },
                        );
                        report.breaks_invalidated += 1;
                        report
                            .events
                            .push(("warn".into(), format!("{}: {}", target.name, note)));
                    }
                }
            }
        }

        report
    }

    pub fn build_scanning_snapshot(
        &self,
        risk: &RiskProjector,
        volume_threshold: f64,
    ) -> ScanningSnapshot {
        let last_candle = self.candle_history.back();
        let last_price = last_candle.map(|c| c.close);
        let current_volume = last_candle.map(|c| c.volume);
        let volume_confirmed = current_volume
            .map(|v| v >= volume_threshold)
            .unwrap_or(false);
        let atr = self.calculate_atr();
        let atr_pips = atr * 100.0;

        let mut setups = Vec::with_capacity(self.levels.len());
        let mut active_setups_count = 0;

        for (id, target) in self.levels.iter() {
            let break_state = self
                .break_states
                .get(id)
                .copied()
                .unwrap_or(BreakState::None);
            let meta = self.level_meta.get(id);

            let (state_str, state_label, pending_side, invalidation_price) = match break_state {
                BreakState::None => (
                    "scanning_break",
                    "Watching for clean candle break",
                    None,
                    None,
                ),
                BreakState::BrokenAbove => {
                    active_setups_count += 1;
                    (
                        "broken_above",
                        "Broken Above — Awaiting Bullish Retest (BUY)",
                        Some("buy".to_string()),
                        Some(target.price - INVALIDATION_DOLLARS),
                    )
                }
                BreakState::BrokenBelow => {
                    active_setups_count += 1;
                    (
                        "broken_below",
                        "Broken Below — Awaiting Bearish Retest (SELL)",
                        Some("sell".to_string()),
                        Some(target.price + INVALIDATION_DOLLARS),
                    )
                }
            };

            let ref_entry = last_price.unwrap_or(target.price);
            let projected_buy = risk.projected_order(
                if break_state == BreakState::BrokenAbove {
                    ref_entry
                } else {
                    target.price
                },
                "buy",
            );
            let projected_sell = risk.projected_order(
                if break_state == BreakState::BrokenBelow {
                    ref_entry
                } else {
                    target.price
                },
                "sell",
            );

            let active_projection = match break_state {
                BreakState::BrokenAbove => Some(projected_buy.clone()),
                BreakState::BrokenBelow => Some(projected_sell.clone()),
                BreakState::None => None,
            };

            let distance_dollars = last_price.map(|p| p - target.price);
            let distance_pips = distance_dollars.map(|d| d.abs() * 100.0);

            let last_note = meta
                .map(|m| m.last_note.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    format!(
                        "Monitoring {} ({:.2}) for break & retest",
                        target.name, target.price
                    )
                });

            setups.push(ScannedLevelSetup {
                id: id.clone(),
                name: target.name.clone(),
                window: target.window.clone(),
                level_price: target.price,
                state: state_str.to_string(),
                state_label: state_label.to_string(),
                pending_side,
                current_price: last_price,
                distance_dollars,
                distance_pips,
                retest_zone_low: target.price - PROXIMITY_DOLLARS,
                retest_zone_high: target.price + PROXIMITY_DOLLARS,
                invalidation_price,
                volume_required: volume_threshold,
                current_volume,
                volume_confirmed,
                projected_buy,
                projected_sell,
                active_projection,
                broken_at: meta.and_then(|m| m.broken_at),
                broken_price: meta.and_then(|m| m.broken_price),
                last_note,
            });
        }

        // Sort armed setups (broken_above / broken_below) first, then by closest distance to current price.
        setups.sort_by(|a, b| {
            let a_armed = a.state != "scanning_break";
            let b_armed = b.state != "scanning_break";
            match b_armed.cmp(&a_armed) {
                std::cmp::Ordering::Equal => {
                    let da = a.distance_pips.unwrap_or(f64::MAX);
                    let db = b.distance_pips.unwrap_or(f64::MAX);
                    da.partial_cmp(&db)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.id.cmp(&b.id))
                }
                ord => ord,
            }
        });

        ScanningSnapshot {
            active_setups_count,
            total_levels_tracked: setups.len(),
            proximity_dollars: PROXIMITY_DOLLARS,
            invalidation_dollars: INVALIDATION_DOLLARS,
            volume_threshold,
            current_volume,
            volume_confirmed,
            last_price,
            atr,
            atr_pips,
            setups,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::risk::RiskProjector;

    #[test]
    fn break_and_retest_populates_scanning_diagnostics_and_triggers_signal() {
        let cfg = Config::from_env();
        let risk = RiskProjector::new(cfg.clone());
        let mut engine = StrategyEngine::new();

        engine.update_levels(&VpLevels {
            window: "PW".into(),
            poc: 2650.0,
            vah: 2660.0,
            val: 2640.0,
            start: 0,
            end: 0,
            timestamp: 1000,
            direction: "neutral".into(),
            swing_high: None,
            swing_low: None,
            sunday_open: None,
        });

        // 1. Bullish break candle across PW PoC (2650.0)
        let c1 = VpCandle {
            time: 1000,
            open: 2649.0,
            high: 2652.0,
            low: 2648.5,
            close: 2651.5,
            volume: 8000.0,
            source: "sifting".into(),
        };
        let r1 = engine.evaluate_candle_with_diagnostics(&c1, 10_500.0);
        assert!(r1.triggered.is_none());
        assert_eq!(r1.breaks_detected, 1);

        let snap1 = engine.build_scanning_snapshot(&risk, 10_500.0);
        assert_eq!(snap1.active_setups_count, 1);
        assert_eq!(snap1.setups[0].id, "PW_POC");
        assert_eq!(snap1.setups[0].state, "broken_above");
        assert_eq!(snap1.setups[0].pending_side.as_deref(), Some("buy"));

        // 2. Retest with low volume (< 10,500) -> rejected, remains armed
        let c2 = VpCandle {
            time: 2000,
            open: 2651.5,
            high: 2651.8,
            low: 2650.1,
            close: 2650.4,
            volume: 9000.0,
            source: "sifting".into(),
        };
        let r2 = engine.evaluate_candle_with_diagnostics(&c2, 10_500.0);
        assert!(r2.triggered.is_none());
        assert_eq!(r2.retests_rejected_low_volume, 1);

        // 3. Retest with volume >= 10,500 -> triggers BUY signal
        let c3 = VpCandle {
            time: 3000,
            open: 2650.4,
            high: 2651.2,
            low: 2649.9,
            close: 2650.3,
            volume: 12_000.0,
            source: "sifting".into(),
        };
        let r3 = engine.evaluate_candle_with_diagnostics(&c3, 10_500.0);
        assert_eq!(r3.signals_confirmed, 1);
        assert_eq!(r3.triggered, Some(("buy", 2650.3, "PW PoC".to_string())));
    }
}
