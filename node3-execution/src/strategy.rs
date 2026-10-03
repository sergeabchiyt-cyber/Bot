use std::collections::{HashMap, VecDeque};
use tracing::{debug, info};
use crate::types::{VpCandle, VpLevels};

const PROXIMITY_DOLLARS: f64 = 0.50;

#[derive(Debug, Clone)]
pub struct LevelTarget {
    pub name: String,
    pub price: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakState {
    None,
    BrokenAbove,
    BrokenBelow,
}

pub struct StrategyEngine {
    pub levels: HashMap<String, LevelTarget>,
    pub candle_history: VecDeque<VpCandle>,
    pub break_states: HashMap<String, BreakState>,
    pub last_triggered_candle: i64,
}

impl StrategyEngine {
    pub fn new() -> Self {
        Self {
            levels: HashMap::new(),
            candle_history: VecDeque::with_capacity(50),
            break_states: HashMap::new(),
            last_triggered_candle: 0,
        }
    }

    pub fn update_levels(&mut self, lvl: &VpLevels) {
        match lvl.window.as_str() {
            "PW" => {
                self.levels.insert("PW_POC".into(), LevelTarget { name: "PW PoC".into(), price: lvl.poc });
                self.levels.insert("PW_VAH".into(), LevelTarget { name: "PW VaH".into(), price: lvl.vah });
                self.levels.insert("PW_VAL".into(), LevelTarget { name: "PW VaL".into(), price: lvl.val });
                info!("Updated PW Levels: PoC={:.2}, VaH={:.2}, VaL={:.2}", lvl.poc, lvl.vah, lvl.val);
            }
            "PS" => {
                self.levels.insert("PS_POC".into(), LevelTarget { name: "PS PoC".into(), price: lvl.poc });
                info!("Updated PS Level: PoC={:.2}", lvl.poc);
            }
            "CW" => {
                self.levels.insert("CW_POC".into(), LevelTarget { name: "CW PoC".into(), price: lvl.poc });
                self.levels.insert("CW_VAH".into(), LevelTarget { name: "CW VaH".into(), price: lvl.vah });
                self.levels.insert("CW_VAL".into(), LevelTarget { name: "CW VaL".into(), price: lvl.val });
                info!("Updated CW Levels: PoC={:.2}, VaH={:.2}, VaL={:.2}", lvl.poc, lvl.vah, lvl.val);
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

    pub fn evaluate_candle(
        &mut self,
        candle: &VpCandle,
        volume_threshold: f64,
    ) -> Option<(&'static str, f64, String)> {
        self.candle_history.push_back(candle.clone());
        if self.candle_history.len() > 30 {
            self.candle_history.pop_front();
        }

        if candle.time <= self.last_triggered_candle {
            return None;
        }

        let price = candle.close;
        let is_green = candle.close > candle.open;
        let is_red = candle.close < candle.open;
        let vol_ok = candle.volume >= volume_threshold;

        for (id, target) in self.levels.iter() {
            let lvl_price = target.price;
            let state = self.break_states.get(id).copied().unwrap_or(BreakState::None);

            match state {
                BreakState::None => {
                    if is_green && candle.close > lvl_price && candle.open <= lvl_price {
                        debug!("Level {} broken ABOVE by candle at {:.2}", target.name, candle.close);
                        self.break_states.insert(id.clone(), BreakState::BrokenAbove);
                    } else if is_red && candle.close < lvl_price && candle.open >= lvl_price {
                        debug!("Level {} broken BELOW by candle at {:.2}", target.name, candle.close);
                        self.break_states.insert(id.clone(), BreakState::BrokenBelow);
                    }
                }
                BreakState::BrokenAbove => {
                    let retested = candle.low <= lvl_price + PROXIMITY_DOLLARS && candle.close >= lvl_price - PROXIMITY_DOLLARS;
                    if retested {
                        if vol_ok {
                            info!(
                                "BULLISH SETUP CONFIRMED on {}! Retest @ {:.2}, Volume: {:.1} (> {:.1})",
                                target.name, price, candle.volume, volume_threshold
                            );
                            self.break_states.insert(id.clone(), BreakState::None);
                            self.last_triggered_candle = candle.time;
                            return Some(("buy", price, target.name.clone()));
                        } else {
                            debug!(
                                "Retest on {} but volume {:.1} < threshold {:.1}",
                                target.name, candle.volume, volume_threshold
                            );
                        }
                    } else if candle.close < lvl_price - 2.0 {
                        self.break_states.insert(id.clone(), BreakState::None);
                    }
                }
                BreakState::BrokenBelow => {
                    let retested = candle.high >= lvl_price - PROXIMITY_DOLLARS && candle.close <= lvl_price + PROXIMITY_DOLLARS;
                    if retested {
                        if vol_ok {
                            info!(
                                "BEARISH SETUP CONFIRMED on {}! Retest @ {:.2}, Volume: {:.1} (> {:.1})",
                                target.name, price, candle.volume, volume_threshold
                            );
                            self.break_states.insert(id.clone(), BreakState::None);
                            self.last_triggered_candle = candle.time;
                            return Some(("sell", price, target.name.clone()));
                        } else {
                            debug!(
                                "Retest on {} but volume {:.1} < threshold {:.1}",
                                target.name, candle.volume, volume_threshold
                            );
                        }
                    } else if candle.close > lvl_price + 2.0 {
                        self.break_states.insert(id.clone(), BreakState::None);
                    }
                }
            }
        }

        None
    }
}
