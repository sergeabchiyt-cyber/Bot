use std::collections::HashMap;
use crate::config::{RiskCfg, SignalCfg};
use crate::types::{Candle, Ray, Side, Signal};
use crate::risk;

struct Armed { side: Side, level: f64, label: String, n: u32 }

pub struct Engine { armed: HashMap<String, Armed>, cool: HashMap<String, i64>, prev: Option<Candle> }

impl Engine {
    pub fn new() -> Self { Self { armed: HashMap::new(), cool: HashMap::new(), prev: None } }

    /// Called once per closed M15 candle. `levels` = active rays, `atr` = ATR(atr_tf).
    pub fn on_candle(&mut self, c: Candle, levels: &[Ray], sc: &SignalCfg, rc: &RiskCfg,
                     atr: f64, equity: f64) -> Vec<Signal> {
        let mut out = Vec::new();
        let tol = sc.retest_tol_pips * rc.pip;
        for r in levels {
            let key = r.label.clone();
            if self.cool.get(&key).map_or(false, |t| c.t < *t) { continue; }
            match self.armed.get_mut(&key) {
                None => {
                    if let Some(p) = self.prev {
                        if p.c <= r.price && c.c > r.price && c.v >= sc.vol_min {
                            self.armed.insert(key.clone(), Armed { side: Side::Long, level: r.price, label: key, n: 0 });
                        } else if p.c >= r.price && c.c < r.price && c.v >= sc.vol_min {
                            self.armed.insert(key.clone(), Armed { side: Side::Short, level: r.price, label: key, n: 0 });
                        }
                    }
                }
                Some(a) => {
                    a.n += 1;
                    if a.n > sc.retest_window { self.armed.remove(&key); continue; }
                    let hit = match a.side {
                        Side::Long  => c.l <= a.level + tol && c.c > a.level,
                        Side::Short => c.h >= a.level - tol && c.c < a.level,
                    };
                    if hit && c.v >= sc.vol_min {
                        let (sl, tp) = risk::plan(a.side, c.c, atr, rc);
                        let size = risk::size(equity, rc, (c.c - sl).abs());
                        out.push(Signal { t: c.t, side: a.side, level: a.label.clone(),
                                          entry: c.c, sl, tp, vol: c.v, size });
                        self.armed.remove(&key);
                        self.cool.insert(key, c.t + sc.level_cooldown_h * 3600);
                    }
                }
            }
        }
        self.prev = Some(c);
        out
    }
}
