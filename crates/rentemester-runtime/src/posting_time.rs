//! Trusted UTC milliseconds to Danish posting horizon; never host TZ or caller date.
use crate::auth::AuthError;
use chrono::{Datelike, Days};
pub fn latest_date(now_ms: i64, max_future_days: u32) -> Result<String, AuthError> {
    let date = chrono::DateTime::from_timestamp_millis(now_ms)
        .ok_or(AuthError::InvalidClock)?
        .with_timezone(&chrono_tz::Europe::Copenhagen)
        .date_naive()
        .checked_add_days(Days::new(u64::from(max_future_days)))
        .ok_or(AuthError::InvalidClock)?;
    if !(0..=9999).contains(&date.year()) {
        return Err(AuthError::InvalidClock);
    }
    Ok(date.format("%Y-%m-%d").to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn danish_midnight_dst_and_configured_horizon_are_trusted_arithmetic() {
        for (utc, days, expected) in [
            ("2026-01-01T22:59:59Z", 0, "2026-01-01"),
            ("2026-01-01T23:00:00Z", 0, "2026-01-02"),
            ("2026-07-01T22:00:00Z", 0, "2026-07-02"),
            ("2026-03-29T00:59:59Z", 45, "2026-05-13"),
            ("2026-03-29T01:00:00Z", 45, "2026-05-13"),
            ("2026-10-25T01:00:00Z", 0, "2026-10-25"),
        ] {
            let n = chrono::DateTime::parse_from_rfc3339(utc)
                .unwrap()
                .timestamp_millis();
            assert_eq!(latest_date(n, days).unwrap(), expected);
        }
        assert!(latest_date(i64::MAX, 45).is_err());
        assert!(latest_date(0, u32::MAX).is_err());
    }
}
