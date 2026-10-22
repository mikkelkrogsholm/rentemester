//! Linear depreciation in exact øre, with all rounding remainder in the final period.
use crate::{Error, money::Money};
use num_bigint::BigInt;
pub fn amount(cost: &Money, months: u32, period: u32) -> Result<Money, Error> {
    if months == 0 || months > 12000 || period == 0 || period > months {
        return Err(Error::new(
            "asset_refused",
            "invalid depreciation period or useful life",
        ));
    }
    let base = &cost.0 / BigInt::from(months);
    if base <= BigInt::from(0) {
        return Err(Error::new(
            "asset_refused",
            "cost must support a positive amount in every depreciation period",
        ));
    }
    Ok(Money(if period == months {
        &cost.0 - &base * BigInt::from(months - 1)
    } else {
        base
    }))
}
/// Source schedules permit zero slices; the journal writer still refuses zero postings.
pub fn source_amount(cost: &Money, months: u32, period: u32) -> Result<Money, Error> {
    if months == 0 || months > 12000 || period == 0 || period > months || cost.0 <= BigInt::from(0)
    {
        return Err(Error::new(
            "asset_refused",
            "invalid cost, depreciation period or useful life",
        ));
    }
    let base = &cost.0 / BigInt::from(months);
    Ok(Money(if period == months {
        &cost.0 - &base * BigInt::from(months - 1)
    } else {
        base
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn final_period_carries_exact_remainder() {
        let cost = Money::parse("1000").unwrap();
        let amounts = (1..=3)
            .map(|i| amount(&cost, 3, i).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            amounts.iter().map(Money::decimal).collect::<Vec<_>>(),
            ["333.33", "333.33", "333.34"]
        );
        let mut sum = Money::default();
        for a in amounts {
            sum.add(&a);
        }
        assert_eq!(sum, cost);
        assert!(amount(&Money::parse("0.01").unwrap(), 2, 1).is_err());
        assert!(amount(&cost, 0, 1).is_err());
        assert!(amount(&cost, 3, 4).is_err());
    }
}
