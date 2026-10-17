//! Pure canonical fiscal dates. No clock, legal eligibility or persisted identifier rewrite.
use crate::{Error, calendar::valid_iso_date};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Date {
    year: u16,
    month: u8,
    day: u8,
}
impl Date {
    pub fn parse(value: &str) -> Result<Self, Error> {
        if !valid_iso_date(value) {
            return Err(Error::new(
                "invalid_date",
                "date must be canonical Gregorian YYYY-MM-DD",
            ));
        }
        Ok(Self {
            year: value[..4].parse().unwrap(),
            month: value[5..7].parse().unwrap(),
            day: value[8..].parse().unwrap(),
        })
    }
    pub fn new(year: u16, month: u8, day: u8) -> Result<Self, Error> {
        Self::parse(&format!("{year:04}-{month:02}-{day:02}"))
    }
}
impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}
impl TryFrom<String> for Date {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        Self::parse(&value)
    }
}
impl From<Date> for String {
    fn from(value: Date) -> Self {
        value.to_string()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartMonth(u8);
impl StartMonth {
    pub fn new(month: i32) -> Result<Self, Error> {
        if !(1..=12).contains(&month) {
            return Err(Error::new(
                "invalid_start_month",
                "fiscal start month must be1–12",
            ));
        }
        Ok(Self(month as u8))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", try_from = "String")]
pub enum LabelStrategy {
    StartYear,
    EndYear,
    Span,
}
impl TryFrom<&str> for LabelStrategy {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self, Error> {
        match value {
            "start-year" => Ok(Self::StartYear),
            "end-year" => Ok(Self::EndYear),
            "span" => Ok(Self::Span),
            _ => Err(Error::new(
                "invalid_label_strategy",
                "unknown fiscal label strategy",
            )),
        }
    }
}
impl TryFrom<String> for LabelStrategy {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        Self::try_from(value.as_str())
    }
}
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FiscalYear {
    pub start_year: u16,
    pub end_year: u16,
    pub start: Date,
    pub end: Date,
    pub display_label: String,
    pub identifier_label: String,
}
fn year(value: i32) -> Result<u16, Error> {
    if !(0..=9999).contains(&value) {
        return Err(Error::new(
            "date_out_of_range",
            "derived date exceeds supported years0000–9999",
        ));
    }
    Ok(value as u16)
}
fn month_end(year: u16, month: u8) -> Result<Date, Error> {
    for day in (28..=31).rev() {
        if let Ok(date) = Date::new(year, month, day) {
            return Ok(date);
        }
    }
    Err(Error::new("invalid_date", "invalid calendar month"))
}
pub fn for_date(
    date: Date,
    start: StartMonth,
    strategy: LabelStrategy,
) -> Result<FiscalYear, Error> {
    let start_year = year(i32::from(date.year) - i32::from(start.0 != 1 && date.month < start.0))?;
    let end_year = year(i32::from(start_year) + i32::from(start.0 != 1))?;
    let display_label = match strategy {
        LabelStrategy::StartYear => format!("{start_year:04}"),
        LabelStrategy::Span if start.0 != 1 => format!("{start_year:04}/{:02}", end_year % 100),
        _ => format!("{end_year:04}"),
    };
    Ok(FiscalYear {
        start_year,
        end_year,
        start: Date::new(start_year, start.0, 1)?,
        end: month_end(end_year, if start.0 == 1 { 12 } else { start.0 - 1 })?,
        identifier_label: display_label.replace('/', "-"),
        display_label,
    })
}
/// Existing six-calendar-month arithmetic only; eligibility and filing rules belong to the caller.
pub fn annual_report_deadline(end: Date) -> Result<Date, Error> {
    let index = u16::from(end.month) + 5;
    let target_year = year(i32::from(end.year) + i32::from(index / 12))?;
    let target_month = (index % 12 + 1) as u8;
    let last = month_end(target_year, target_month)?;
    if end == month_end(end.year, end.month)? {
        Ok(last)
    } else {
        Date::new(target_year, target_month, end.day.min(last.day))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_dates_reject_rollover_and_lexical_coercion() {
        for s in ["0000-02-29", "0001-01-01", "2000-02-29", "9999-12-31"] {
            assert_eq!(Date::parse(s).unwrap().to_string(), s);
        }
        for s in [
            "2023-02-29",
            "1900-02-29",
            "2100-02-29",
            "2026-04-31",
            "2026-00-01",
            "2026-01-00",
            "2026-13-01",
            "2026-01-32",
            " 2026-01-01",
            "2026-01-01 ",
            "1-01-01",
            "10000-01-01",
            "2026-1-01",
            "２０２６-01-01",
            "2026-01-01T00:00:00Z",
        ] {
            assert_eq!(Date::parse(s).unwrap_err().code, "invalid_date");
        }
        for m in [-1, 0, 13, 100] {
            assert_eq!(StartMonth::new(m).unwrap_err().code, "invalid_start_month");
        }
        assert!(Date::new(10000, 1, 1).is_err());
        assert_eq!(
            LabelStrategy::try_from("unknown").unwrap_err().code,
            "invalid_label_strategy"
        );
    }
    #[test]
    fn fiscal_boundaries_labels_and_checked_years() {
        let july = StartMonth::new(7).unwrap();
        let before = for_date(
            Date::parse("2026-06-30").unwrap(),
            july,
            LabelStrategy::Span,
        )
        .unwrap();
        assert_eq!(
            (
                before.start.to_string(),
                before.end.to_string(),
                before.display_label,
                before.identifier_label
            ),
            (
                "2025-07-01".into(),
                "2026-06-30".into(),
                "2025/26".into(),
                "2025-26".into()
            )
        );
        let low = for_date(
            Date::parse("0099-07-01").unwrap(),
            july,
            LabelStrategy::Span,
        )
        .unwrap();
        assert_eq!(
            (
                low.start.to_string(),
                low.end.to_string(),
                low.display_label
            ),
            ("0099-07-01".into(), "0100-06-30".into(), "0099/00".into())
        );
        for strategy in [
            LabelStrategy::Span,
            LabelStrategy::StartYear,
            LabelStrategy::EndYear,
        ] {
            assert_eq!(
                for_date(
                    Date::parse("0000-02-29").unwrap(),
                    StartMonth::new(1).unwrap(),
                    strategy
                )
                .unwrap()
                .display_label,
                "0000"
            );
        }
        for s in ["0000-01-01", "9999-12-31"] {
            assert_eq!(
                for_date(Date::parse(s).unwrap(), july, LabelStrategy::EndYear)
                    .unwrap_err()
                    .code,
                "date_out_of_range"
            );
        }
    }
    #[test]
    fn six_month_deadline_preserves_month_end_and_clamps_day() {
        for (s, expected) in [
            ("2026-12-31", "2027-06-30"),
            ("2027-06-30", "2027-12-31"),
            ("2026-08-31", "2027-02-28"),
            ("2027-08-31", "2028-02-29"),
            ("2026-02-28", "2026-08-31"),
            ("2026-08-30", "2027-02-28"),
            ("2026-01-15", "2026-07-15"),
            ("0000-02-29", "0000-08-31"),
            ("0099-08-31", "0100-02-28"),
        ] {
            assert_eq!(
                annual_report_deadline(Date::parse(s).unwrap())
                    .unwrap()
                    .to_string(),
                expected
            );
        }
        assert_eq!(
            annual_report_deadline(Date::parse("9999-07-01").unwrap())
                .unwrap_err()
                .code,
            "date_out_of_range"
        );
    }
    #[test]
    fn every_fiscal_start_contains_its_date_and_labels_are_stable() {
        for month in 1..=12 {
            let date = Date::new(2024, month, 15).unwrap();
            for start in 1..=12 {
                for strategy in [
                    LabelStrategy::Span,
                    LabelStrategy::StartYear,
                    LabelStrategy::EndYear,
                ] {
                    let fiscal = for_date(date, StartMonth::new(start).unwrap(), strategy).unwrap();
                    assert!(fiscal.start <= date && date <= fiscal.end);
                    assert_eq!(
                        fiscal.identifier_label,
                        fiscal.display_label.replace('/', "-")
                    );
                    assert_eq!(fiscal.start.month, start as u8);
                }
            }
        }
    }
}
