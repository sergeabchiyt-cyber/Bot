//! XAUUSD market clock.
//!
//! The week and session anchor is **18:00 `America/New_York`**, DST-aware
//! (UTC−4 EDT / UTC−5 EST). Gold trades Sunday 18:00 → Friday 17:00 New York;
//! the stretch from Friday 17:00 to Sunday 18:00 is the weekend close.
//!
//! Node 1 only reports the clock (`market_open`, `session`) — it never derives
//! anything from it beyond freshness, because the weekly profiles are Node 3's
//! job.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Timelike, Utc, Weekday};
use chrono_tz::America::New_York;
use chrono_tz::Tz;

/// Sessions open at 18:00 New York.
const ANCHOR_HOUR: u32 = 18;
/// Gold stops trading for the week at 17:00 New York on Friday.
const WEEK_CLOSE_HOUR: u32 = 17;

/// One 18:00 → 18:00 New York session, in epoch milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    /// Session open (18:00 New York).
    pub open_ms: i64,
    /// Session close (the next 18:00 New York).
    pub close_ms: i64,
    /// False on the Friday-evening → Sunday-evening weekend window.
    pub market_open: bool,
}

fn epoch_to_new_york(now_ms: i64) -> DateTime<Tz> {
    let utc = match DateTime::<Utc>::from_timestamp_millis(now_ms) {
        Some(dt) => dt,
        None => DateTime::<Utc>::from_timestamp_millis(0).expect("the Unix epoch is representable"),
    };
    utc.with_timezone(&New_York)
}

/// 18:00 New York on `date`, in epoch milliseconds.
///
/// Returns `None` only for a date outside chrono's representable range. The
/// 18:00 anchor is never ambiguous or skipped (US DST changes at 02:00), but
/// `earliest()` keeps that assumption out of the code path.
fn anchor_ms(date: NaiveDate, tz: Tz) -> Option<i64> {
    let naive = date.and_hms_opt(ANCHOR_HOUR, 0, 0)?;
    tz.from_local_datetime(&naive)
        .earliest()
        .map(|dt| dt.timestamp_millis())
}

/// True while the XAUUSD session is open: Sunday 18:00 → Friday 17:00 NY.
///
/// The daily 17:00–18:00 New York maintenance hour is not carved out here:
/// the contract's closed-market window is the weekend, and a one-hour gap on a
/// weekday is reported by `freshness` (the feed simply goes quiet) rather than
/// by a hard session flag.
pub fn market_open(now_ms: i64) -> bool {
    let now = epoch_to_new_york(now_ms);
    match now.weekday() {
        Weekday::Sat => false,
        Weekday::Fri => now.hour() < WEEK_CLOSE_HOUR,
        Weekday::Sun => now.hour() >= ANCHOR_HOUR,
        Weekday::Mon | Weekday::Tue | Weekday::Wed | Weekday::Thu => true,
    }
}

/// The session containing `now_ms`: the most recent 18:00 New York anchor at or
/// before it, and the next one after that.
pub fn current_session(now_ms: i64) -> Session {
    let today = epoch_to_new_york(now_ms).date_naive();
    let mut open_ms = None;
    // Three days of lookback covers every case: the anchor is at most 24 h
    // before `now`, and a DST shift only moves it by an hour.
    for back in 0..=3 {
        let date = today - Duration::days(back);
        if let Some(candidate) = anchor_ms(date, New_York)
            && candidate <= now_ms
        {
            open_ms = Some(candidate);
            break;
        }
    }
    let open_ms = open_ms.unwrap_or(now_ms);
    let open_date = epoch_to_new_york(open_ms).date_naive();
    let close_ms = anchor_ms(open_date + Duration::days(1), New_York)
        .unwrap_or(open_ms.saturating_add(86_400_000));
    Session {
        open_ms,
        close_ms,
        market_open: market_open(now_ms),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ny(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
        New_York
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .single()
            .expect("unambiguous New York wall time")
            .timestamp_millis()
    }

    #[test]
    fn the_week_opens_sunday_1800_new_york() {
        assert!(!market_open(ny(2026, 10, 25, 17, 59)));
        assert!(market_open(ny(2026, 10, 25, 18, 0)));
        assert!(market_open(ny(2026, 10, 26, 9, 0)));
    }

    #[test]
    fn the_week_closes_friday_1700_new_york() {
        assert!(market_open(ny(2026, 10, 23, 16, 59)));
        assert!(!market_open(ny(2026, 10, 23, 17, 0)));
        assert!(!market_open(ny(2026, 10, 24, 12, 0)), "Saturday is closed");
        assert!(
            !market_open(ny(2026, 10, 25, 12, 0)),
            "Sunday before 18:00 is closed"
        );
    }

    #[test]
    fn the_anchor_follows_daylight_saving_time() {
        // Monday 2026-10-26 is EDT (UTC−4): 18:00 NY == 22:00 UTC.
        let edt = current_session(ny(2026, 10, 26, 20, 0));
        assert_eq!(edt.open_ms, ny(2026, 10, 26, 18, 0));
        assert_eq!(edt.open_ms % 3_600_000, 0);
        assert_eq!(epoch_to_new_york(edt.open_ms).hour(), 18);

        // Monday 2026-11-09 is EST (UTC−5): 18:00 NY == 23:00 UTC.
        let est = current_session(ny(2026, 11, 9, 20, 0));
        assert_eq!(est.open_ms, ny(2026, 11, 9, 18, 0));
        assert_eq!(epoch_to_new_york(est.open_ms).hour(), 18);
        assert_ne!(
            edt.open_ms % 86_400_000,
            est.open_ms % 86_400_000,
            "the UTC anchor must move by an hour across the DST switch"
        );
    }

    #[test]
    fn a_session_spans_the_dst_change_without_a_fixed_24_hours() {
        // US DST starts on 2026-03-08: the session that opens on the 7th is
        // one hour short.
        let spring = current_session(ny(2026, 3, 8, 5, 0));
        assert_eq!(spring.open_ms, ny(2026, 3, 7, 18, 0));
        assert_eq!(spring.close_ms - spring.open_ms, 23 * 3_600_000);

        // ...and the session that contains the November fall-back is one hour
        // long. 2026-11-01 is the switch.
        let autumn = current_session(ny(2026, 11, 1, 5, 0));
        assert_eq!(autumn.close_ms - autumn.open_ms, 25 * 3_600_000);
    }

    #[test]
    fn sessions_are_contiguous_and_ordered() {
        let mut now = ny(2026, 10, 19, 0, 0);
        for _ in 0..40 {
            let session = current_session(now);
            assert!(session.open_ms <= now);
            assert!(now < session.close_ms);
            let next = current_session(session.close_ms);
            assert_eq!(next.open_ms, session.close_ms);
            now += 12 * 3_600_000;
        }
    }

    #[test]
    fn reopening_monday_counts_as_open() {
        // Sunday 18:00 open -> Monday 18:00 close is one session.
        let sunday = current_session(ny(2026, 10, 25, 18, 0));
        assert_eq!(sunday.open_ms, ny(2026, 10, 25, 18, 0));
        assert_eq!(sunday.close_ms, ny(2026, 10, 26, 18, 0));
        assert!(sunday.market_open);
    }
}
