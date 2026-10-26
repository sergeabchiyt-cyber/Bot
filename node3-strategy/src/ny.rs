//! New York trading calendar: the 18:00 America/New_York session anchors.
//!
//! The trading week opens Sunday 18:00 and closes Friday 18:00. Every daily
//! session runs 18:00 -> next 18:00. Offsets are DST-aware (UTC-4 EDT,
//! UTC-5 EST).
//!
//! The US DST rules are encoded directly, so the crate needs no timezone
//! database dependency. Rules in force since 2007 are second Sunday of March
//! to first Sunday of November; 1987-2006 used first Sunday of April to last
//! Sunday of October. Both switch at 02:00 local. Session closes sit at 18:00
//! local, never inside a transition hour, so the date-based offset is exact
//! for every anchor. The unit tests cross-check the offsets against reference
//! instants.

use chrono::{Datelike, NaiveDate, Weekday};
#[cfg(test)]
use chrono::{TimeZone, Utc};

pub const HOUR_MS: i64 = 3_600_000;
pub const DAY_MS: i64 = 24 * HOUR_MS;
/// Hour (New York local time) at which sessions close and open.
pub const SESSION_CLOSE_HOUR: i64 = 18;
/// chrono day number of 1970-01-01 (chrono counts 0001-01-01 as day 1).
const EPOCH_CE_DAYS: i64 = 719_163;

fn days_from_date(date: NaiveDate) -> i64 {
    i64::from(date.num_days_from_ce()) - EPOCH_CE_DAYS
}

fn date_from_days(days: i64) -> NaiveDate {
    let ce = i32::try_from(days + EPOCH_CE_DAYS).expect("date within chrono range");
    NaiveDate::from_num_days_from_ce_opt(ce).expect("date within chrono range")
}

/// The `nth` (1-based) `weekday` of a month.
fn nth_weekday(year: i32, month: u32, weekday: Weekday, nth: u32) -> NaiveDate {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid month");
    let lead = (weekday.num_days_from_monday() + 7 - first.weekday().num_days_from_monday()) % 7;
    NaiveDate::from_ymd_opt(year, month, 1 + lead + 7 * (nth - 1)).expect("valid weekday")
}

/// The last `weekday` of a month.
fn last_weekday(year: i32, month: u32, weekday: Weekday) -> NaiveDate {
    let next_month_start = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .expect("valid month");
    let mut day = next_month_start.pred_opt().expect("valid date");
    while day.weekday() != weekday {
        day = day.pred_opt().expect("valid date");
    }
    day
}

/// First and last calendar date on which New York observes daylight time.
/// Dates before the first day are never daylight; the window is empty.
fn dst_window(year: i32) -> (NaiveDate, NaiveDate) {
    if year >= 2007 {
        (
            nth_weekday(year, 3, Weekday::Sun, 2),
            nth_weekday(year, 11, Weekday::Sun, 1),
        )
    } else if year >= 1987 {
        (
            nth_weekday(year, 4, Weekday::Sun, 1),
            last_weekday(year, 10, Weekday::Sun),
        )
    } else {
        let jan_first = NaiveDate::from_ymd_opt(year, 1, 1).expect("valid year");
        (jan_first, jan_first)
    }
}

/// UTC offset in milliseconds for New York local time on a calendar date.
/// Valid for any instant of that date at or after 18:00 local.
fn offset_on_date(date: NaiveDate) -> i64 {
    let (start, end) = dst_window(date.year());
    if date >= start && date < end {
        -4 * HOUR_MS
    } else {
        -5 * HOUR_MS
    }
}

/// UTC offset in milliseconds for New York at an exact instant (handles the
/// 02:00 transition hours).
pub fn offset_ms_at(utc_ms: i64) -> i64 {
    let standard_local = utc_ms - 5 * HOUR_MS;
    let year = date_from_days(standard_local.div_euclid(DAY_MS)).year();
    let (start, end) = dst_window(year);
    // 02:00 EST is 07:00 UTC on the start day; 02:00 EDT is 06:00 UTC on the end day.
    let start_utc = days_from_date(start) * DAY_MS + 7 * HOUR_MS;
    let end_utc = days_from_date(end) * DAY_MS + 6 * HOUR_MS;
    if utc_ms >= start_utc && utc_ms < end_utc {
        -4 * HOUR_MS
    } else {
        -5 * HOUR_MS
    }
}

/// The New York calendar date of a UTC instant.
pub fn local_date(utc_ms: i64) -> NaiveDate {
    date_from_days((utc_ms + offset_ms_at(utc_ms)).div_euclid(DAY_MS))
}

/// UTC milliseconds of the 18:00 New York close on `date`.
pub fn close_on(date: NaiveDate) -> i64 {
    days_from_date(date) * DAY_MS + SESSION_CLOSE_HOUR * HOUR_MS - offset_on_date(date)
}

/// Date whose 18:00 close is the most recent one at or before `now_ms`.
pub fn latest_close_date(now_ms: i64) -> NaiveDate {
    let today = local_date(now_ms);
    if close_on(today) <= now_ms {
        today
    } else {
        date_from_days(days_from_date(today) - 1)
    }
}

/// The Sunday whose 18:00 week open is the most recent one at or before `now_ms`.
pub fn week_open_date(now_ms: i64) -> NaiveDate {
    let latest = latest_close_date(now_ms);
    let back = i64::from(latest.weekday().num_days_from_sunday());
    date_from_days(days_from_date(latest) - back)
}

/// Shift a calendar date by whole days.
pub fn shift_days(date: NaiveDate, days: i64) -> NaiveDate {
    date_from_days(days_from_date(date) + days)
}

/// A close on Saturday or Sunday is a weekend bucket, not a trading close.
pub fn is_weekend_close(date: NaiveDate) -> bool {
    matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

/// Walk back from `date` to the last weekday (Mon-Fri) close.
pub fn last_trading_close_date(mut date: NaiveDate) -> NaiveDate {
    while is_weekend_close(date) {
        date = shift_days(date, -1);
    }
    date
}

/// ISO-style weekday rank: Monday = 1 ... Sunday = 7.
pub fn weekday_rank(date: NaiveDate) -> u32 {
    date.weekday().number_from_monday()
}

/// Display name for a weekday rank (1 = Mon ... 7 = Sun).
pub fn weekday_name(rank: u32) -> &'static str {
    match rank {
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        6 => "Sat",
        _ => "Sun",
    }
}

/// Parse a trading weekday (`Mon`..`Fri`, case-insensitive) into its rank.
/// Weekend days are rejected because they never publish a daily close.
pub fn parse_trading_weekday(raw: &str) -> Option<u32> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "mon" | "monday" => Some(1),
        "tue" | "tues" | "tuesday" => Some(2),
        "wed" | "wednesday" => Some(3),
        "thu" | "thur" | "thurs" | "thursday" => Some(4),
        "fri" | "friday" => Some(5),
        _ => None,
    }
}

/// UTC milliseconds of a naive UTC date-time (test and fixture helper).
#[cfg(test)]
pub fn utc_ms(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
    let naive = NaiveDate::from_ymd_opt(year, month, day)
        .and_then(|date| date.and_hms_opt(hour, minute, 0))
        .expect("valid UTC date-time");
    Utc.from_utc_datetime(&naive).timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_constant_matches_chrono() {
        let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch");
        assert_eq!(days_from_date(epoch), 0);
    }

    #[test]
    fn session_close_is_1800_new_york_in_edt_and_est() {
        // EDT (UTC-4): 18:00 local is 22:00 UTC.
        assert_eq!(
            close_on(NaiveDate::from_ymd_opt(2025, 6, 8).expect("date")),
            utc_ms(2025, 6, 8, 22, 0)
        );
        // EST (UTC-5): 18:00 local is 23:00 UTC.
        assert_eq!(
            close_on(NaiveDate::from_ymd_opt(2025, 12, 8).expect("date")),
            utc_ms(2025, 12, 8, 23, 0)
        );
    }

    #[test]
    fn dst_transitions_switch_at_two_am_local() {
        // 2025 spring forward: Sun 9 Mar, 02:00 EST = 07:00 UTC.
        assert_eq!(offset_ms_at(utc_ms(2025, 3, 9, 6, 59)), -5 * HOUR_MS);
        assert_eq!(offset_ms_at(utc_ms(2025, 3, 9, 7, 0)), -4 * HOUR_MS);
        // 2025 fall back: Sun 2 Nov, 02:00 EDT = 06:00 UTC.
        assert_eq!(offset_ms_at(utc_ms(2025, 11, 2, 5, 59)), -4 * HOUR_MS);
        assert_eq!(offset_ms_at(utc_ms(2025, 11, 2, 6, 0)), -5 * HOUR_MS);
        // 2026 windows.
        assert_eq!(dst_window(2026).0.to_string(), "2026-03-08");
        assert_eq!(dst_window(2026).1.to_string(), "2026-11-01");
    }

    #[test]
    fn sunday_1800_open_is_the_week_anchor_on_both_sides_of_the_close() {
        // Sunday 2025-06-08 17:00 NY (21:00 UTC): still the previous week.
        let before = utc_ms(2025, 6, 8, 21, 0);
        assert_eq!(
            week_open_date(before),
            NaiveDate::from_ymd_opt(2025, 6, 1).expect("date")
        );
        // Sunday 2025-06-08 19:00 NY (23:00 UTC): the new week has opened.
        let after = utc_ms(2025, 6, 8, 23, 0);
        assert_eq!(
            week_open_date(after),
            NaiveDate::from_ymd_opt(2025, 6, 8).expect("date")
        );
        // Wednesday noon NY maps to the Sunday just gone.
        assert_eq!(
            week_open_date(utc_ms(2025, 6, 11, 16, 0)),
            NaiveDate::from_ymd_opt(2025, 6, 8).expect("date")
        );
    }

    #[test]
    fn trading_close_walk_back_skips_weekend_closes() {
        let saturday = NaiveDate::from_ymd_opt(2025, 6, 14).expect("date");
        let sunday = NaiveDate::from_ymd_opt(2025, 6, 15).expect("date");
        let friday = NaiveDate::from_ymd_opt(2025, 6, 13).expect("date");
        assert_eq!(last_trading_close_date(saturday), friday);
        assert_eq!(last_trading_close_date(sunday), friday);
        assert_eq!(last_trading_close_date(friday), friday);
    }

    #[test]
    fn weekday_parsing_accepts_only_trading_days() {
        assert_eq!(parse_trading_weekday("Mon"), Some(1));
        assert_eq!(parse_trading_weekday(" wed "), Some(3));
        assert_eq!(parse_trading_weekday("Friday"), Some(5));
        assert_eq!(parse_trading_weekday("Sun"), None);
        assert_eq!(parse_trading_weekday("someday"), None);
        assert_eq!(weekday_name(3), "Wed");
    }

    #[test]
    fn offsets_match_hand_derived_us_dst_rules() {
        // Hand-derived from the US DST rules (second Sunday of March 02:00 local
        // to first Sunday of November 02:00 local since 2007; first Sunday of
        // April to last Sunday of October before that). These were NOT checked
        // against a timezone database. The DST boundary values were also checked
        // out of band against a reference implementation, but that is not part
        // of this repository's tests.
        let samples = [
            (utc_ms(2026, 3, 8, 6, 59), -5 * HOUR_MS),
            (utc_ms(2026, 3, 8, 7, 0), -4 * HOUR_MS),
            (utc_ms(2026, 10, 22, 22, 0), -4 * HOUR_MS),
            (utc_ms(2026, 11, 1, 5, 59), -4 * HOUR_MS),
            (utc_ms(2026, 11, 1, 6, 0), -5 * HOUR_MS),
            (utc_ms(2007, 3, 11, 7, 0), -4 * HOUR_MS),
            (utc_ms(1999, 7, 1, 12, 0), -4 * HOUR_MS),
            (utc_ms(1999, 1, 1, 12, 0), -5 * HOUR_MS),
        ];
        for (instant, expected) in samples {
            assert_eq!(offset_ms_at(instant), expected, "instant {instant}");
        }
    }
}
