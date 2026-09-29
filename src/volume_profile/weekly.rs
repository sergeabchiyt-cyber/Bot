//! Trading-week anchors in America/New_York.
//!
//! The week opens Sunday 18:00 NY — the first session open after Friday's
//! 18:00 close — and closes Friday 18:00 NY. PW is the previous full week,
//! held unchanged for the whole current week. CW accumulates from the week
//! open through the last completed 18:00 daily close, so its first snapshot
//! appears when Monday's session closes.

use chrono::{Datelike, Duration, TimeZone, Utc};
use chrono_tz::America::New_York;

use super::session::SESSION_CLOSE_HOUR;

/// Hour (America/New_York) of the Sunday week open.
pub const WEEK_START_HOUR: u32 = 18;

pub fn most_recent_week_start_utc(now_ms: i64) -> i64 {
    let now = Utc
        .timestamp_millis_opt(now_ms)
        .single()
        .unwrap_or_else(Utc::now);
    let local = now.with_timezone(&New_York);
    let days_since_sunday = local.weekday().num_days_from_sunday() as i64;
    let mut sunday = local - Duration::days(days_since_sunday);
    sunday = New_York
        .with_ymd_and_hms(
            sunday.year(),
            sunday.month(),
            sunday.day(),
            WEEK_START_HOUR,
            0,
            0,
        )
        .single()
        .expect("valid Sunday 18:00 local time");
    if sunday.with_timezone(&Utc) > now {
        sunday = sunday - Duration::days(7);
    }
    sunday.with_timezone(&Utc).timestamp_millis()
}

/// Bounds of the previous trading week in UTC millis:
/// `(Sunday 18:00 NY open, Friday 18:00 NY close)` of the week before the
/// current one. Both anchors are resolved in America/New_York local time so
/// they stay on the FX open/close across DST changes.
pub fn previous_week_bounds_utc(now_ms: i64) -> (i64, i64) {
    let week_start = most_recent_week_start_utc(now_ms);
    let this_sunday = Utc
        .timestamp_millis_opt(week_start)
        .single()
        .unwrap_or_else(Utc::now)
        .with_timezone(&New_York);
    let prev_sunday = this_sunday - Duration::days(7);
    let prev_friday = this_sunday - Duration::days(2);
    let start = New_York
        .with_ymd_and_hms(
            prev_sunday.year(),
            prev_sunday.month(),
            prev_sunday.day(),
            WEEK_START_HOUR,
            0,
            0,
        )
        .single()
        .expect("valid Sunday 18:00 local time");
    let end = New_York
        .with_ymd_and_hms(
            prev_friday.year(),
            prev_friday.month(),
            prev_friday.day(),
            SESSION_CLOSE_HOUR,
            0,
            0,
        )
        .single()
        .expect("valid Friday 18:00 local time");
    (
        start.with_timezone(&Utc).timestamp_millis(),
        end.with_timezone(&Utc).timestamp_millis(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ny(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> i64 {
        New_York
            .with_ymd_and_hms(y, m, d, hh, mm, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc)
            .timestamp_millis()
    }

    #[test]
    fn week_start_is_the_sunday_1800_open() {
        // Wednesday noon -> the Sunday just gone, 18:00.
        assert_eq!(
            most_recent_week_start_utc(ny(2025, 6, 11, 12, 0)),
            ny(2025, 6, 8, 18, 0)
        );
        // Sunday 19:00 -> today's 18:00 open.
        assert_eq!(
            most_recent_week_start_utc(ny(2025, 6, 8, 19, 0)),
            ny(2025, 6, 8, 18, 0)
        );
        // Sunday 17:00 -> still the *previous* week (open is at 18:00).
        assert_eq!(
            most_recent_week_start_utc(ny(2025, 6, 8, 17, 0)),
            ny(2025, 6, 1, 18, 0)
        );
    }

    #[test]
    fn previous_week_runs_sunday_open_to_friday_close() {
        let (start, end) = previous_week_bounds_utc(ny(2025, 6, 11, 12, 0));
        assert_eq!(start, ny(2025, 6, 1, 18, 0));
        assert_eq!(end, ny(2025, 6, 6, 18, 0));
    }
}
