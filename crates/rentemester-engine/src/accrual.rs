//! Exact nonnegative recognition slices and anchored, clamped calendar-month dates.
use crate::{Error, calendar::valid_iso_date, fiscal_year::Date, money::Money};
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
    if periods == 0 || periods > 12000 || index == 0 || index > periods || total.0 <= 0.into() {
        return Err(Error::new(
            "accrual_refused",
            "invalid amount or recognition period",
        ));
    }
    let base = &total.0 / num_bigint::BigInt::from(periods);
    let amount = Money(if index == periods {
        &total.0 - &base * num_bigint::BigInt::from(periods - 1)
    } else {
        base
    });
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
    fn zero_slice_is_schedule_only_and_asset_rule_stays_positive() {
        let tiny = Money::parse("0.01").unwrap();
        assert_eq!(
            period(&tiny, 2, "2026-01-31", 1, 1).unwrap().1.decimal(),
            "0.00"
        );
        assert_eq!(
            period(&tiny, 2, "2026-01-31", 1, 2).unwrap().1.decimal(),
            "0.01"
        );
        assert!(crate::asset::amount(&tiny, 2, 1).is_err());
        for (n, i) in [(0, 1), (12001, 1), (2, 0), (2, 3)] {
            assert!(period(&tiny, n, "2026-01-31", 1, i).is_err());
        }
        assert!(period(&Money::default(), 2, "2026-01-31", 1, 1).is_err());
    }
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
