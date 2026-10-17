//! Pure Gregorian date validation and fiscal retention deadline calculation.
use crate::Error;
pub fn valid_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let y = s[..4].parse::<u32>().unwrap();
    let m = s[5..7].parse::<u32>().unwrap();
    let d = s[8..].parse::<u32>().unwrap();
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if y.is_multiple_of(400) || (y.is_multiple_of(4) && !y.is_multiple_of(100)) {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    d > 0 && d <= days
}

/// Last day of the fiscal end month five years after the fiscal end year.
/// Existing stored deadlines are outside this calculation's authority.
pub fn retention_deadline(basis: &str, fiscal_start_month: u32) -> Result<String, Error> {
    let basis =
        basis.trim_matches(|c: char| c != '\u{0085}' && (c.is_whitespace() || c == '\u{FEFF}'));
    if !valid_iso_date(basis) {
        return Err(Error::new("invalid_ledger", "invalid retention basis date"));
    }
    let year = basis[..4].parse::<u32>().unwrap();
    let month = basis[5..7].parse::<u32>().unwrap();
    let start = if (1..=12).contains(&fiscal_start_month) {
        fiscal_start_month
    } else {
        1
    };
    let target = year + u32::from(start != 1 && month >= start) + 5;
    if target > 9999 {
        return Err(Error::new(
            "invalid_ledger",
            "retention deadline exceeds YYYY-MM-DD range",
        ));
    }
    let end_month = if start == 1 { 12 } else { start - 1 };
    let mut day = 31;
    while !valid_iso_date(&format!("{target:04}-{end_month:02}-{day:02}")) {
        day -= 1;
    }
    Ok(format!("{target:04}-{end_month:02}-{day:02}"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deadlines_use_real_target_calendar_and_trimmed_low_years() {
        for (basis, start, expected) in [
            ("2024-02-29", 3, "2029-02-28"),
            ("2023-03-01", 3, "2029-02-28"),
            ("2027-02-28", 3, "2032-02-29"),
            (" 2026-01-01 ", 1, "2031-12-31"),
            ("0001-07-01", 7, "0007-06-30"),
            ("0000-01-01", 1, "0005-12-31"),
        ] {
            assert_eq!(retention_deadline(basis, start).unwrap(), expected);
        }
        assert!(retention_deadline("9999-01-01", 1).is_err());
        assert!(retention_deadline("2023-02-29", 1).is_err());
    }
}
