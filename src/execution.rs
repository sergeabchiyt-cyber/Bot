use anyhow::Result;
use crate::config::Config;
use crate::mcp_client::McpClient;
use crate::types::{TradeEvent, VpLevels};

pub struct ExecutionManager {
    pub config: Config,
    pub mcp: McpClient,
    pub atr: f64, // 14-period ATR on 15M
}

impl ExecutionManager {
    pub fn new(config: Config, mcp: McpClient) -> Self {
        Self { config, mcp, atr: 3.0 }
    }

    pub fn compute_sl_tp(&self, entry: f64, side: &str) -> (f64, f64) {
        let atr_pct = (self.atr / entry).clamp(0.001, 0.05);
        // Scale SL between 200-300 pips ($2-$3) based on ATR percentile
        let sl_pips = self.config.sl_min_pips
            + (self.config.sl_max_pips - self.config.sl_min_pips) * atr_pct.min(1.0);
        let tp_pips = self.config.tp_min_pips
            + (self.config.tp_max_pips - self.config.tp_min_pips) * atr_pct.min(1.0);
        // pips = $0.01 move for XAUUSD
        let sl_dist = sl_pips * 0.01;
        let tp_dist = tp_pips * 0.01;
        // Enforce RR between 2.0 and 3.0
        let rr = tp_dist / sl_dist;
        let (final_sl, final_tp) = if rr < self.config.rr_min {
            (sl_dist, sl_dist * self.config.rr_min)
        } else if rr > self.config.rr_max {
            (sl_dist, sl_dist * self.config.rr_max)
        } else {
            (sl_dist, tp_dist)
        };
        match side {
            "buy" => (entry - final_sl, entry + final_tp),
            "sell" => (entry + final_sl, entry - final_tp),
            _ => (entry - final_sl, entry + final_tp),
        }
    }

    pub async fn execute(
        &self,
        side: &str,
        size: f64,
        entry: f64,
        level: &VpLevels,
    ) -> Result<TradeEvent> {
        let (sl, tp) = self.compute_sl_tp(entry, side);
        // MCP call to ChelseaAI
        let order_result = self.mcp.place_order(side, size, sl, tp).await?;
        Ok(TradeEvent {
            trade_id: order_result,
            symbol: "XAUUSD".into(),
            side: side.into(),
            size,
            entry,
            sl,
            tp,
            status: "open".into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
    }
}