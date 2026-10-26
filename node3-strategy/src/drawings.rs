//! Drawing payloads for the chart (C2 `/drawings` and the `drawing` topic).
//!
//! Node 3 owns the drawing geometry. Node 2 only renders it.
//! Colours and dashing follow Node 2's `js/config.js` (decision D8):
//!   PW  PoC #F0B90B solid, VaH #26A69A solid, VaL #EF5350 solid
//!   PS  PoC #58A6FF solid
//!   CW  PoC #A371F7 dashed, VaH #F778BA dashed, VaL #22D3EE dashed
//! Sunday open is a neutral dotted marker.
//!
//! Level identity is `(window, kind, start, end)`. It is encoded as
//! `WINDOW:kind:start:end` in the drawing `id`, and the same identity keys the
//! strategy's break and retest state.

use std::collections::BTreeMap;

use crate::types::{DrawingItem, DrawingsPayload, PendingWindow, VpLevels};
use crate::vp::VpAudit;

/// Timeframes each level is drawn on: intraday (1H) and daily.
const TIMEFRAMES: [&str; 2] = ["1H", "daily"];

/// Stable identity of a level, shared with drawings and the strategy.
pub fn level_identity(window: &str, kind: &str, start: i64, end: i64) -> String {
    format!("{window}:{kind}:{start}:{end}")
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "poc" => "PoC",
        "vah" => "VaH",
        "val" => "VaL",
        _ => "Sunday Open",
    }
}

/// Colour and line style for a level, from Node 2's palette.
fn style(window: &str, kind: &str) -> (&'static str, &'static str) {
    match (window, kind) {
        (_, "sunday_open") => ("#8B949E", "dotted"),
        ("PW", _) => ("#F0B90B", "solid"),
        ("PS", _) => ("#58A6FF", "solid"),
        ("CW", "poc") => ("#A371F7", "dashed"),
        ("CW", "vah") => ("#F778BA", "dashed"),
        ("CW", _) => ("#22D3EE", "dashed"),
        _ => ("#8B949E", "dotted"),
    }
}

fn frvp_color(window: &str) -> &'static str {
    match window {
        "PW" => "#F0B90B",
        "PS" => "#58A6FF",
        _ => "#A371F7",
    }
}

/// Build the drawing set from the published levels, the FRVP audits, and the
/// windows still pending.
pub fn build(
    levels: &[VpLevels],
    audits: &BTreeMap<String, VpAudit>,
    pending: &[PendingWindow],
    timestamp: i64,
) -> DrawingsPayload {
    let mut items = Vec::new();
    for level in levels {
        let prices = [
            ("poc", Some(level.poc)),
            ("vah", level.vah),
            ("val", level.val),
        ];
        for (kind, price) in prices {
            let Some(price) = price else { continue };
            let (color, line_style) = style(&level.window, kind);
            for timeframe in TIMEFRAMES {
                items.push(DrawingItem {
                    id: level_identity(&level.window, kind, level.start, level.end),
                    kind: "level".into(),
                    window: level.window.clone(),
                    label: format!("{} {}", level.window, kind_label(kind)),
                    timeframe: timeframe.into(),
                    color: color.into(),
                    line_style: line_style.into(),
                    price: Some(price),
                    start: level.start,
                    end: None,
                    ray_from_time: Some(level.start),
                    rows: None,
                });
            }
        }
        if let Some(open) = level.sunday_open {
            let (color, line_style) = style(&level.window, "sunday_open");
            for timeframe in TIMEFRAMES {
                items.push(DrawingItem {
                    id: level_identity(&level.window, "sunday_open", level.start, level.start),
                    kind: "sunday_open".into(),
                    window: level.window.clone(),
                    label: format!("{} Sunday Open", level.window),
                    timeframe: timeframe.into(),
                    color: color.into(),
                    line_style: line_style.into(),
                    price: Some(open),
                    start: level.start,
                    end: None,
                    ray_from_time: Some(level.start),
                    rows: None,
                });
            }
        }
    }
    for (window, audit) in audits {
        items.push(DrawingItem {
            id: level_identity(window, "frvp", audit.start, audit.end),
            kind: "frvp".into(),
            window: window.clone(),
            label: format!("{window} FRVP"),
            timeframe: "1H".into(),
            color: frvp_color(window).into(),
            line_style: "rectangle".into(),
            price: None,
            start: audit.start,
            end: Some(audit.end),
            ray_from_time: None,
            rows: Some(audit.histogram.clone()),
        });
    }
    DrawingsPayload {
        timestamp,
        items,
        pending: pending.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProfileMeta;

    fn meta() -> ProfileMeta {
        ProfileMeta {
            row_mode: "rows".into(),
            row_height: 1.0,
            rows: 128,
            range_low: 4000.0,
            range_high: 4200.0,
            va_pct: 0.7,
            total_volume: 1.0,
            input_interval: "5m".into(),
            input_bars: 1,
        }
    }

    fn level(window: &str, vah: Option<f64>, sunday_open: Option<f64>) -> VpLevels {
        VpLevels {
            window: window.into(),
            poc: 4100.0,
            vah,
            val: vah.map(|v| v - 20.0),
            start: 1_000,
            end: 2_000,
            timestamp: 0,
            sunday_open,
            meta: meta(),
        }
    }

    #[test]
    fn sunday_open_marker_is_emitted_for_pw_when_present() {
        let levels = vec![level("PW", Some(4150.0), Some(4120.0))];
        let payload = build(&levels, &BTreeMap::new(), &[], 0);
        let open: Vec<_> = payload
            .items
            .iter()
            .filter(|item| item.kind == "sunday_open")
            .collect();
        assert_eq!(open.len(), 2, "one per timeframe");
        assert!(open.iter().all(|item| item.price == Some(4120.0)));
        assert_eq!(open[0].id, "PW:sunday_open:1000:1000");
    }

    #[test]
    fn ps_publishes_only_a_poc_ray_with_no_value_area() {
        let levels = vec![level("PS", None, None)];
        let payload = build(&levels, &BTreeMap::new(), &[], 0);
        assert_eq!(payload.items.len(), 2, "PoC on 1H and daily");
        assert!(payload.items.iter().all(|item| item.label == "PS PoC"));
    }

    #[test]
    fn cw_levels_are_dashed_and_keep_their_identity() {
        let levels = vec![level("CW", Some(4150.0), None)];
        let payload = build(&levels, &BTreeMap::new(), &[], 0);
        let poc = payload
            .items
            .iter()
            .find(|item| item.label == "CW PoC")
            .expect("CW PoC");
        assert_eq!(poc.line_style, "dashed");
        assert_eq!(poc.color, "#A371F7");
        assert_eq!(poc.id, "CW:poc:1000:2000");
    }

    #[test]
    fn frvp_rectangles_come_from_the_audits() {
        let mut audits = BTreeMap::new();
        audits.insert(
            "PW".to_string(),
            VpAudit {
                window: "PW".into(),
                start: 1_000,
                end: 2_000,
                poc: 4100.0,
                vah: 4150.0,
                val: 4050.0,
                poc_row: 0,
                vah_row: 0,
                val_row: 0,
                row_mode: "rows".into(),
                row_height: 1.0,
                rows: 0,
                range_low: 4000.0,
                range_high: 4200.0,
                va_pct: 0.7,
                total_volume: 1.0,
                input_interval: "5m".into(),
                input_bars: 1,
                histogram: Vec::new(),
            },
        );
        let payload = build(&[], &audits, &[], 0);
        assert_eq!(payload.items.len(), 1);
        assert_eq!(payload.items[0].line_style, "rectangle");
        assert_eq!(payload.items[0].end, Some(2_000));
    }
}
