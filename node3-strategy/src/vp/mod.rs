//! Volume-profile engine: FRVP histograms, value area, and window policy.
//!
//! Node 3 owns all profile maths. Node 1's `/levels` is never read. Levels are
//! rebuilt from closed bars in the local store.

pub mod histogram;
pub mod timeframe;
pub mod windows;

use serde::{Deserialize, Serialize};

pub use histogram::ProfileRow;
use windows::BuiltProfile;

/// A profile as published on `/vp` and the `vp` WebSocket topic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VpAudit {
    pub window: String,
    pub start: i64,
    pub end: i64,
    pub poc: f64,
    pub vah: f64,
    pub val: f64,
    pub poc_row: usize,
    pub vah_row: usize,
    pub val_row: usize,
    pub row_mode: String,
    pub row_height: f64,
    pub rows: usize,
    pub range_low: f64,
    pub range_high: f64,
    pub va_pct: f64,
    pub total_volume: f64,
    pub input_interval: String,
    pub input_bars: usize,
    pub histogram: Vec<ProfileRow>,
}

/// Convert a built profile into its published form.
pub fn audit_from_profile(window: &str, profile: &BuiltProfile) -> VpAudit {
    let hist = &profile.histogram;
    VpAudit {
        window: window.to_string(),
        start: profile.start,
        end: profile.end,
        poc: hist.poc(),
        vah: hist.vah(),
        val: hist.val(),
        poc_row: hist.poc_index,
        vah_row: hist.vah_index,
        val_row: hist.val_index,
        row_mode: hist.model.row_mode.as_str().to_string(),
        row_height: hist.row_height,
        rows: hist.rows.len(),
        range_low: hist.range_low,
        range_high: hist.range_high,
        va_pct: hist.model.va_pct,
        total_volume: hist.total_volume,
        input_interval: profile.input_interval.to_string(),
        input_bars: profile.input_bars,
        histogram: hist.rows.clone(),
    }
}

#[cfg(test)]
mod golden {
    //! Golden fixtures recorded from TradingView-parity Node 1 output. Each
    //! fixture is checked offline against the histogram maths:
    //! PoC, VAH, and VAL rows; row height and count; total volume.

    use super::histogram::{row_grid, value_area, ProfileModel, RowMode};
    use serde_json::Value;

    const PW_FIXTURE: &str = include_str!("../../tests/fixtures/vp-PW.json");
    const PS_FIXTURE: &str = include_str!("../../tests/fixtures/vp-PS.json");
    const CW_FIXTURE: &str = include_str!("../../tests/fixtures/vp-CW.json");

    fn number(doc: &Value, key: &str) -> f64 {
        doc[key]
            .as_f64()
            .unwrap_or_else(|| panic!("fixture field {key} is not a number"))
    }

    fn index(doc: &Value, key: &str) -> usize {
        doc[key]
            .as_u64()
            .unwrap_or_else(|| panic!("fixture field {key} is not an index")) as usize
    }

    fn check_golden(raw: &str, expected_window: &str) {
        let doc: Value = serde_json::from_str(raw).expect("fixture JSON");
        assert_eq!(doc["window"].as_str(), Some(expected_window));
        let rows = doc["histogram"].as_array().expect("histogram rows");
        let volumes: Vec<f64> = rows
            .iter()
            .map(|row| row["volume"].as_f64().expect("row volume"))
            .collect();

        // 1. Value area over the recorded row volumes reproduces the recorded rows.
        let (poc, val, vah) = value_area(&volumes, number(&doc, "va_pct")).expect("value area");
        assert_eq!(poc, index(&doc, "poc_row"), "{expected_window} PoC row");
        assert_eq!(val, index(&doc, "val_row"), "{expected_window} VAL row");
        assert_eq!(vah, index(&doc, "vah_row"), "{expected_window} VAH row");

        // 2. Recorded prices agree with the rows that carry them, within one row.
        let row_height = number(&doc, "row_height");
        assert!((rows[poc]["price"].as_f64().expect("price") - number(&doc, "poc")).abs() < 1e-9);
        assert!((rows[vah]["high"].as_f64().expect("high") - number(&doc, "vah")).abs() < 1e-9);
        assert!((rows[val]["low"].as_f64().expect("low") - number(&doc, "val")).abs() < 1e-9);
        assert!(row_height > 0.0);

        // 3. The row grid for the recorded range reproduces the row height and count.
        let model = ProfileModel {
            row_mode: RowMode::Rows,
            rows: index(&doc, "rows"),
            ..ProfileModel::default()
        };
        let (grid_lo, height, count) = row_grid(
            number(&doc, "range_low"),
            number(&doc, "range_high"),
            &model,
        )
        .expect("row grid");
        assert!((height - row_height).abs() < 1e-9, "{expected_window} row height");
        assert_eq!(count, rows.len(), "{expected_window} row count");
        assert!((grid_lo - number(&doc, "range_low")).abs() < 1e-9);

        // 4. Total volume matches within rounding noise.
        let total: f64 = volumes.iter().sum();
        assert!((total - number(&doc, "total_volume")).abs() < 1e-3);
    }

    #[test]
    fn pw_golden_profile_matches_tradingview_parity_fixture() {
        check_golden(PW_FIXTURE, "PW");
    }

    #[test]
    fn ps_golden_profile_matches_tradingview_parity_fixture() {
        // The PS fixture is Wed 21 Oct 18:00 -> Thu 22 Oct 18:00. It validates
        // PoC/VA on a 1m session. The Thu->Fri week-open case is covered
        // synthetically in `windows`, because no recorded Thu->Fri golden exists.
        check_golden(PS_FIXTURE, "PS");
    }

    #[test]
    fn cw_golden_profile_matches_tradingview_parity_fixture() {
        check_golden(CW_FIXTURE, "CW");
    }
}
