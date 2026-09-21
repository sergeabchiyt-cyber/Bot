use anyhow::Result;
use chrono::{Datelike, TimeZone, Utc};
use chrono_tz::America::New_York;
use wickra::{Candle, ValueArea};

use crate::types::VpLevels;

pub struct VolumeProfileEngine {
    pub pw_levels: Option<VpLevels>,
    pub ps_levels: Option<VpLevels>,
    pub cw_levels: Option<VpLevels>,
}

impl VolumeProfileEngine {
    pub fn new() -> Self {
        Self { pw_levels: None, ps_levels: None, cw_levels: None }
    }

    pub fn compute_window(
        &self,
        candles: &[Candle],
        window_label: &str,
        bins: usize,
        va_pct: f64,
    ) -> Result<VpLevels> {
        let mut va = ValueArea::new(candles.len().max(1), bins, va_pct)
            .map_err(|e| anyhow::anyhow!("ValueArea init: {:?}", e))?;

        let mut last = None;
        for c in candles {
            last = va.update(c.clone());
        }
        let out = last.ok_or_else(|| anyhow::anyhow!("ValueArea produced no output"))?;
        Ok(VpLevels {
            window: window_label.into(),
            poc: out.poc,
            vah: out.vah,
            val: out.val,
            timestamp: Utc::now().timestamp_millis(),
        })
    }

    pub fn update_pw(&mut self, candles: &[Candle]) -> Result<()> {
        self.pw_levels = Some(self.compute_window(candles, "PW", 100, 0.70)?);
        Ok(())
    }

    pub fn update_ps(&mut self, candles: &[Candle]) -> Result<()> {
        self.ps_levels = Some(self.compute_window(candles, "PS", 100, 0.70)?);
        Ok(())
    }

    pub fn update_cw(&mut self, candles: &[Candle]) -> Result<()> {
        self.cw_levels = Some(self.compute_window(candles, "CW", 100, 0.70)?);
        Ok(())
    }

    pub fn all_levels(&self) -> Vec<&VpLevels> {
        [self.pw_levels.as_ref(), self.ps_levels.as_ref(), self.cw_levels.as_ref()]
            .into_iter()
            .flatten()
            .collect()
    }

    /// Sunday 18:00 UTC-4 week boundary
    pub fn week_start_utc4() -> i64 {
        let now = Utc::now().with_timezone(&New_York);
        let days_since_sunday = now.weekday().num_days_from_sunday() as i64;
        let sunday = now - chrono::Duration::days(days_since_sunday);
        let start = New_York
            .with_ymd_and_hms(sunday.year(), sunday.month(), sunday.day(), 18, 0, 0)
            .single()
            .expect("valid Sunday 18:00 local time");
        start.timestamp_millis()
    }
}