use crate::config::{Cfg, RiskCfg};
use crate::types::{Position, Side, Signal};

pub struct Paper { pub equity: f64, pub positions: Vec<Position>, next_id: u64 }

impl Paper {
    pub fn new(rc: &RiskCfg) -> Self { Self { equity: rc.paper_equity, positions: Vec::new(), next_id: 1 } }

    pub fn submit(&mut self, s: &Signal) -> Position {
        let p = Position { id: self.next_id, side: s.side, entry: s.entry, sl: s.sl, tp: s.tp,
                           size: s.size, open_t: s.t, close_t: None, exit: None, pnl: None };
        self.next_id += 1;
        self.positions.push(p.clone());
        p
    }

    /// Mark open positions against current bid/ask. Returns positions closed this tick.
    pub fn on_price(&mut self, t: i64, bid: f64, ask: f64) -> Vec<Position> {
        let mut closed = Vec::new();
        for p in self.positions.iter_mut() {
            if p.close_t.is_some() { continue; }
            let px = if p.side == Side::Long { bid } else { ask };
            let exit = match p.side {
                Side::Long  => if px <= p.sl { Some(p.sl) } else if px >= p.tp { Some(p.tp) } else { None },
                Side::Short => if px >= p.sl { Some(p.sl) } else if px <= p.tp { Some(p.tp) } else { None },
            };
            if let Some(x) = exit {
                let dir = if p.side == Side::Long { 1.0 } else { -1.0 };
                p.exit = Some(x); p.close_t = Some(t);
                p.pnl = Some((x - p.entry) * dir * p.size);
                self.equity += p.pnl.unwrap();
                closed.push(p.clone());
            }
        }
        closed
    }
}

pub async fn telegram(cfg: &Cfg, text: &str) {
    let (tok, chat) = match (std::env::var("TELEGRAM_TOKEN").ok(), std::env::var("TELEGRAM_CHAT").ok()) {
        (Some(t), Some(c)) => (t, c),
        _ => { tracing::warn!("TELEGRAM_TOKEN/TELEGRAM_CHAT unset, alert dropped"); return; }
    };
    let url = format!("https://api.telegram.org/bot{tok}/sendMessage");
    let _ = reqwest::Client::new().post(url)
        .json(&serde_json::json!({ "chat_id": chat, "text": text }))
        .send().await;
}

pub fn alert_text(s: &Signal) -> String {
    format!("XAUUSD {} @ {:.3}\nlevel: {}\nSL {:.3} | TP {:.3}\nsize {:.2} | vol {:.0}",
            if s.side == Side::Long { "LONG" } else { "SHORT" }, s.entry, s.level, s.sl, s.tp, s.size, s.vol)
}
