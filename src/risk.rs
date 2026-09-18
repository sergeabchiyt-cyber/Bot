use crate::config::RiskCfg;
use crate::types::{Candle, Side};

/// Wilder ATR over closed candles.
pub fn atr(cs: &[Candle], p: usize) -> f64 {
    if cs.len() <= p { return 0.0; }
    let tr = |i: usize| {
        let (x, y) = (&cs[i], &cs[i - 1]);
        (x.h - x.l).max((x.h - y.c).abs()).max((x.l - y.c).abs())
    };
    let mut a = (cs.len() - p..cs.len()).map(tr).sum::<f64>() / p as f64;
    for i in (cs.len() - p)..cs.len() { a = (a * (p - 1) as f64 + tr(i)) / p as f64; }
    a
}

/// SL/TP distances clamped to pip bounds, scaled by ATR. Returns (sl_price, tp_price).
pub fn plan(side: Side, entry: f64, atr: f64, c: &RiskCfg) -> (f64, f64) {
    let sl = (atr * c.atr_sl_mult).clamp(c.sl_pips_min * c.pip, c.sl_pips_max * c.pip);
    let tp = (sl * c.rr).clamp(c.tp_pips_min * c.pip, c.tp_pips_max * c.pip);
    match side {
        Side::Long  => (entry - sl, entry + tp),
        Side::Short => (entry + sl, entry - tp),
    }
}

pub fn size(equity: f64, c: &RiskCfg, sl_dist: f64) -> f64 {
    if sl_dist <= 0.0 { return 0.0; }
    equity * c.risk_pct / 100.0 / sl_dist
}
