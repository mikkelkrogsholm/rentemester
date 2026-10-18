//! Deterministic port of the product's VAT cadence, labels and banking-day deadlines.
use crate::{Error, calendar::valid_iso_date};
use time::{Date, Duration, Month, Weekday};

pub struct Window {
    pub start: String,
    pub end: String,
    pub label: String,
    pub deadline: String,
}
fn invalid() -> Error {
    Error::new("invalid_vat_period", "invalid VAT cadence or date range")
}
pub fn date(value: &str) -> Result<Date, Error> {
    if !valid_iso_date(value) {
        return Err(invalid());
    }
    Date::from_calendar_date(
        value[..4].parse().map_err(|_| invalid())?,
        Month::try_from(value[5..7].parse::<u8>().map_err(|_| invalid())?)
            .map_err(|_| invalid())?,
        value[8..].parse().map_err(|_| invalid())?,
    )
    .map_err(|_| invalid())
}
fn iso(d: Date) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), d.month() as u8, d.day())
}
pub fn span(cadence: &str) -> Result<u8, Error> {
    match cadence {
        "month" => Ok(1),
        "quarter" => Ok(3),
        "half-year" => Ok(6),
        _ => Err(invalid()),
    }
}
fn new(year: i32, month: u8, day: u8) -> Result<Date, Error> {
    Date::from_calendar_date(year, Month::try_from(month).map_err(|_| invalid())?, day)
        .map_err(|_| invalid())
}
fn easter(year: i32) -> Result<Date, Error> {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    new(
        year,
        ((h + l - 7 * m + 114) / 31) as u8,
        ((h + l - 7 * m + 114) % 31 + 1) as u8,
    )
}
fn banking(d: Date) -> Result<bool, Error> {
    if matches!(d.weekday(), Weekday::Saturday | Weekday::Sunday)
        || [(1, 1), (12, 25), (12, 26)].contains(&(d.month() as u8, d.day()))
    {
        return Ok(false);
    }
    let distance = (d - easter(d.year())?).whole_days();
    Ok(![-3, -2, 1, 39, 50].contains(&distance))
}
pub fn window(reference: &str, cadence: &str) -> Result<Window, Error> {
    let reference = date(reference)?;
    let year = reference.year();
    let span = span(cadence)?;
    if !(1..=9998).contains(&year) {
        return Err(invalid());
    }
    let first = ((reference.month() as u8 - 1) / span) * span + 1;
    let last = first + span - 1;
    let end = if last == 12 {
        new(year + 1, 1, 1)?
    } else {
        new(year, last + 1, 1)?
    } - Duration::days(1);
    let mut deadline = if cadence == "month" {
        if last == 6 {
            new(year, 8, 17)?
        } else if last == 12 {
            new(year + 1, 1, 25)?
        } else {
            new(year, last + 1, 25)?
        }
    } else {
        let month = last as i32 + 3;
        new(year + (month - 1) / 12, ((month - 1) % 12 + 1) as u8, 1)?
    };
    while !banking(deadline)? {
        deadline = deadline
            .checked_add(Duration::days(1))
            .ok_or_else(invalid)?;
    }
    let label = match cadence {
        "month" => format!(
            "{} {year}",
            [
                "Januar",
                "Februar",
                "Marts",
                "April",
                "Maj",
                "Juni",
                "Juli",
                "August",
                "September",
                "Oktober",
                "November",
                "December"
            ][first as usize - 1]
        ),
        "quarter" => format!("Q{} {year}", (first - 1) / 3 + 1),
        _ => format!("{}. halvår {year}", (first - 1) / 6 + 1),
    };
    Ok(Window {
        start: format!("{year:04}-{first:02}-01"),
        end: iso(end),
        label,
        deadline: iso(deadline),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_keep_registered_cadence_leap_days_and_banking_day_shifts() {
        for (reference, cadence, start, end, deadline) in [
            (
                "2026-10-18",
                "quarter",
                "2026-10-01",
                "2026-12-31",
                "2027-03-01",
            ),
            (
                "2024-02-29",
                "month",
                "2024-02-01",
                "2024-02-29",
                "2024-03-25",
            ),
            (
                "2026-06-01",
                "month",
                "2026-06-01",
                "2026-06-30",
                "2026-08-17",
            ),
            (
                "2026-12-01",
                "month",
                "2026-12-01",
                "2026-12-31",
                "2027-01-25",
            ),
            (
                "2026-06-01",
                "half-year",
                "2026-01-01",
                "2026-06-30",
                "2026-09-01",
            ),
            (
                "2025-02-01",
                "month",
                "2025-02-01",
                "2025-02-28",
                "2025-03-25",
            ),
            (
                "2026-03-01",
                "quarter",
                "2026-01-01",
                "2026-03-31",
                "2026-06-01",
            ),
            (
                "2024-01-01",
                "quarter",
                "2024-01-01",
                "2024-03-31",
                "2024-06-03",
            ),
        ] {
            let w = window(reference, cadence).unwrap();
            assert_eq!(
                (w.start.as_str(), w.end.as_str(), w.deadline.as_str()),
                (start, end, deadline)
            );
        }
        assert!(window("2026-02-30", "quarter").is_err());
        assert!(window("2026-01-01", "invalid").is_err());
        assert!(!banking(new(2026, 4, 2).unwrap()).unwrap());
        assert!(!banking(new(2026, 4, 3).unwrap()).unwrap());
        assert!(!banking(new(2026, 4, 6).unwrap()).unwrap());
    }
}
