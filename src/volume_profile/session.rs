//! 18:00 → 18:00 America/New_York trading sessions.
//!
//! Every session opens at 18:00 NY and closes the next day at 18:00 NY, so a
//! Sunday 18:00 open closes Monday 18:00. PS (previous session) is always the
//! last *closed* session, which lets it roll forward at each close instead of
//! being frozen at its boot-time value. All boundaries are re-anchored in
//! local NY time so they stay at 18:00 across DST changes.

use chrono::{Datelike, Duration, TimeZone, Utc};
use chrono_tz::America::New_York;

use crate::types::VpCandle;

/// Hour (America/New_York) at which the trading session rolls over.
pub const SESSION_CLOSE_HOUR: u32 = 18;

/// How many sessions back we are willing to look for a non-empty previous
/// session. Covers the weekend hole (Fri 18:00 → Sun 18:00) plus holidays.
pub const MAX_SESSION_LOOKBACK: i64 = 5;

/// Most recent 18:00 America/New_York boundary at or before `now_ms`.
/// DST-safe: the wall-clock hour is re-anchored in the local zone.
pub fn last_session_close_utc(now_ms: i64) -> i64 {
    let now = Utc
        .timestamp_millis_opt(now_ms)
        .single()
        .unwrap_or_else(Utc::now);
    let local = now.with_timezone(&New_York);
    let mut close = ny_close_on(local.year(), local.month(), local.day());
    if close > now_ms {
        let prev = local - Duration::days(1);
        close = ny_close_on(prev.year(), prev.month(), prev.day());
    }
    close
}

/// Shift a session-close timestamp by `days` whole calendar days, keeping
/// the boundary pinned to 18:00 local time across DST changes.
fn session_close_shift(close_ms: i64, days: i64) -> i64 {
    let ts = Utc
        .timestamp_millis_opt(close_ms)
        .single()
        .unwrap_or_else(Utc::now)
        .with_timezone(&New_York);
    let shifted = ts + Duration::days(days);
    ny_close_on(shifted.year(), shifted.month(), shifted.day())
}

fn ny_close_on(year: i32, month: u32, day: u32) -> i64 {
    New_York
        .with_ymd_and_hms(year, month, day, SESSION_CLOSE_HOUR, 0, 0)
        .single()
        // 18:00 never falls in a DST gap in New York, but stay total.
        .unwrap_or_else(|| {
            New_York
                .with_ymd_and_hms(year, month, day, SESSION_CLOSE_HOUR + 1, 0, 0)
                .earliest()
                .expect("valid NY session close")
        })
        .with_timezone(&Utc)
        .timestamp_millis()
}

/// Walks back from the most recent session close until it finds a session
/// that actually contains candles (skips the weekend / holiday gap).
pub fn previous_session_slice(
    candles: &[VpCandle],
    now_ms: i64,
) -> (i64, i64, Vec<VpCandle>) {
    let mut end = last_session_close_utc(now_ms);
    let first_end = end;
    let mut first_start = session_close_shift(end, -1);

    for _ in 0..MAX_SESSION_LOOKBACK {
        let start = session_close_shift(end, -1);
        let slice: Vec<VpCandle> = candles
            .iter()
            .filter(|c| c.time >= start && c.time < end)
            .cloned()
            .collect();
        if !slice.is_empty() {
            return (start, end, slice);
        }
        if end == first_end {
            first_start = start;
        }
        end = start;
    }
    (first_start, first_end, Vec::new())
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
    fn session_close_is_1800_new_york() {
        // Wed 2025-06-11 20:00 NY -> close is that same day 18:00 NY.
        let now = ny(2025, 6, 11, 20, 0);
        assert_eq!(last_session_close_utc(now), ny(2025, 6, 11, 18, 0));
        // 17:59 NY is still the *previous* day's session.
        let now = ny(2025, 6, 11, 17, 59);
        assert_eq!(last_session_close_utc(now), ny(2025, 6, 10, 18, 0));
        // Sunday 19:00 NY: the close is the 18:00 week open itself.
        assert_eq!(
            last_session_close_utc(ny(2025, 6, 8, 19, 0)),
            ny(2025, 6, 8, 18, 0)
        );
    }

    #[test]
    fn session_boundary_survives_dst_change() {
        // US DST ends 2025-11-02. 18:00 local on either side must stay 18:00.
        let before = last_session_close_utc(ny(2025, 10, 31, 23, 0));
        let after = last_session_close_utc(ny(2025, 11, 3, 23, 0));
        assert_eq!(before, ny(2025, 10, 31, 18, 0));
        assert_eq!(after, ny(2025, 11, 3, 18, 0));
    }

    #[test]
    fn session_shift_keeps_the_1800_anchor_across_dst() {
        // Shifting the Friday close back lands on 18:00 local each day, even
        // across the Nov 2 DST change.
        let friday = ny(2025, 11, 7, 18, 0);
        assert_eq!(session_close_shift(friday, -1), ny(2025, 11, 6, 18, 0));
        assert_eq!(session_close_shift(friday, -5), ny(2025, 11, 2, 18, 0));
    }
}
