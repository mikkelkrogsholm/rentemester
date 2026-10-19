//! Exact positive recognition slices and anchored, clamped calendar-month dates.
use crate::{Error, asset, calendar::valid_iso_date, fiscal_year::Date, money::Money};
pub fn period(
    total: &Money,
    periods: u32,
    first: &str,
    step: u32,
    index: u32,
) -> Result<(String, Money), Error> {
    Date::parse(first)?;
    if step == 0 || step > 1200 {
        return Err(Error::new("accrual_refused", "month step must be 1–1200"));
    }
    let amount = asset::amount(total, periods, index)?;
    let year = first[..4].parse::<u32>().unwrap();
    let month = first[5..7].parse::<u32>().unwrap();
    let mut day = first[8..].parse::<u32>().unwrap();
    let offset = year * 12 + month - 1 + (index - 1) * step;
    if offset / 12 > 9999 {
        return Err(Error::new(
            "accrual_refused",
            "schedule exceeds supported calendar",
        ));
    }
    let date = loop {
        let value = format!("{:04}-{:02}-{day:02}", offset / 12, offset % 12 + 1);
        if valid_iso_date(&value) {
            break value;
        }
        day -= 1;
    };
    Ok((date, amount))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn anchored_schedule_and_final_remainder() {
        let t = Money::parse("1000").unwrap();
        let s = (1..=3)
            .map(|i| period(&t, 3, "2024-01-31", 1, i).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            s.iter().map(|v| v.0.as_str()).collect::<Vec<_>>(),
            ["2024-01-31", "2024-02-29", "2024-03-31"]
        );
        assert_eq!(s[2].1.decimal(), "333.34");
        assert!(period(&t, 3, "9999-12-31", 1, 2).is_err());
        assert!(period(&t, 3, "2026-02-30", 1, 1).is_err());
        assert!(period(&t, 3, "2026-01-01", 0, 1).is_err());
    }
}
