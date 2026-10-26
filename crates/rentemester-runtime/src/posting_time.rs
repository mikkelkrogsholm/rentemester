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
/// Recipient-local date from trusted UTC milliseconds and an explicit validated IANA zone.
pub fn recipient_date(now_ms: i64, zone: &str) -> Result<String, AuthError> {
    rentemester_engine::tasks::reminders::recipient_date(now_ms, zone)
        .map_err(|_| AuthError::InvalidClock)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recipients_use_explicit_iana_dates_across_dst_and_different_days() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-03-28T23:30:00Z")
            .unwrap()
            .timestamp_millis();
        assert_eq!(
            recipient_date(now, "Europe/Copenhagen").unwrap(),
            "2026-03-29"
        );
        assert_eq!(
            recipient_date(now, "America/New_York").unwrap(),
            "2026-03-28"
        );
        assert!(recipient_date(now, "invalid-zone").is_err());
    }
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
