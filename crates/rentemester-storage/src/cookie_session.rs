//! Exact token resolution and time policy inside the held control writer transaction.
use rentemester_engine::Error;
use rusqlite::{MAIN_DB, OptionalExtension, Transaction, TransactionState};
use time::{Date, Month, PrimitiveDateTime, Time};
#[derive(Clone, Copy)]
pub enum SessionRequirement {
    Current,
    Fresh,
}
pub struct SessionIdentity {
    pub subject: String,
    pub session_id: String,
}
fn denied() -> Error {
    Error::new("invalid_credentials", "current session refused")
}
pub(crate) fn timestamp(value: &str) -> Result<i64, Error> {
    let bytes = value.as_bytes();
    if bytes.len() != 24
        || !bytes.iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            10 => *b == b'T',
            13 | 16 => *b == b':',
            19 => *b == b'.',
            23 => *b == b'Z',
            _ => b.is_ascii_digit(),
        })
    {
        return Err(denied());
    }
    let number = |a: usize, b: usize| value[a..b].parse::<u16>().map_err(|_| denied());
    let year = i32::from(number(0, 4)?);
    if year < 1 {
        return Err(denied());
    }
    let date = Date::from_calendar_date(
        year,
        Month::try_from(number(5, 7)? as u8).map_err(|_| denied())?,
        number(8, 10)? as u8,
    )
    .map_err(|_| denied())?;
    let time = Time::from_hms_milli(
        number(11, 13)? as u8,
        number(14, 16)? as u8,
        number(17, 19)? as u8,
        number(20, 23)?,
    )
    .map_err(|_| denied())?;
    i64::try_from(
        PrimitiveDateTime::new(date, time)
            .assume_utc()
            .unix_timestamp_nanos()
            / 1_000_000,
    )
    .map_err(|_| denied())
}
/// No refresh, cache or writes. The same control lock must cover policy and company commit.
pub fn resolve(
    control: &Transaction<'_>,
    token: &str,
    now: i64,
    requirement: SessionRequirement,
) -> Result<SessionIdentity, Error> {
    if control
        .transaction_state(Some(MAIN_DB))
        .map_err(|_| denied())?
        != TransactionState::Write
    {
        return Err(Error::new(
            "control_fence_required",
            "control main writer lock required",
        ));
    }
    crate::control_migration::validate_current_control(control)
        .map_err(|_| Error::new("invalid_control", "current control admission refused"))?;
    let row: Option<(String, String, String, String)> = control
        .query_row(
            "SELECT id,userId,createdAt,expiresAt FROM main.session WHERE token=?1",
            [token],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(|_| denied())?;
    let (session_id, subject, created, expires) = row.ok_or_else(denied)?;
    let created = timestamp(&created)?;
    let expires = timestamp(&expires)?;
    if !(0..=253_402_300_799_999).contains(&now)
        || subject.trim().is_empty()
        || session_id.trim().is_empty()
        || subject.chars().any(char::is_control)
        || session_id.chars().any(char::is_control)
        || created > now
        || expires <= now
        || expires <= created
        || expires - created > 43_200_000
        || matches!(requirement, SessionRequirement::Fresh) && now - created >= 600_000
    {
        return Err(denied());
    }
    Ok(SessionIdentity {
        subject,
        session_id,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timestamps_are_canonical_not_coerced() {
        assert_eq!(timestamp("1970-01-01T00:00:00.000Z").unwrap(), 0);
        assert_eq!(
            timestamp("2026-01-01T00:00:00.000Z").unwrap(),
            1_767_225_600_000
        );
        assert_eq!(
            timestamp("2000-02-29T23:59:59.999Z").unwrap(),
            951_868_799_999
        );
        for bad in [
            "1900-02-29T00:00:00.000Z",
            "2026-01-01",
            "2026-01-01T00:00:60.000Z",
            "2026-01-01T24:00:00.000Z",
            "2026-01-01T00:00:00.000+00:00",
            "0000-01-01T00:00:00.000Z",
            "2026-01-01T00:00:00.000z",
            "2026-01-01T00:00:00.000Zx",
        ] {
            assert!(timestamp(bad).is_err(), "{bad}");
        }
    }
}
