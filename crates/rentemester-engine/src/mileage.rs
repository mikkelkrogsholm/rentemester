//! Documentation arithmetic only: kilometres are rounded to hundredths, rate is user supplied.
use crate::{Error, decimal, money::Money};
pub const RULE: &str = "DK-MILEAGE-LOG-001";
pub fn calculate(kilometers: &str, rate: &str) -> Result<(Money, Money), Error> {
    let km = Money::parse(kilometers)?;
    if km.is_negative() || km == Money::default() || !decimal::positive(rate)? {
        return Err(Error::new(
            "invalid_input",
            "kilometers and ratePerKm must be positive (kilometers at hundredth precision)",
        ));
    }
    let amount = Money::multiply_decimal(&km.decimal(), rate)?;
    Ok((km, amount))
}
/// Existing source advisory. This never supplies or changes a calculation rate.
pub fn same_rate(left: &str, right: &str) -> Result<bool, Error> {
    Ok(decimal::compare(left, right)? == std::cmp::Ordering::Equal)
}
pub fn warnings(rate: &str) -> Result<Vec<String>, Error> {
    if decimal::compare(rate, "3.94")? != std::cmp::Ordering::Greater {
        return Ok(vec![]);
    }
    Ok(vec![format!(
        "Den angivne sats {rate} kr/km overstiger Skatterådets højeste maksimum for 2026 (3,94 kr/km for bil ≤20.000 km). Maks. for bil over 20.000 km er 2,28 kr/km og for cykel/knallert 0,64 kr/km. En sats over maksimum kan gøre godtgørelsen skattepligtig — bekræft satsgrundlaget. Rentemester ændrer ikke satsen (kun en advarsel)."
    )])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rounds_kilometers_then_multiplies_user_rate_exactly() {
        let (km, amount) = calculate("12.345", "3.791").unwrap();
        assert_eq!(km.decimal(), "12.35");
        assert_eq!(amount.decimal(), "46.82");
        assert_eq!(calculate("1", "1.005").unwrap().1.decimal(), "1.01");
        assert_eq!(calculate("1", "0.0001").unwrap().1.decimal(), "0.00");
        for (km, rate) in [
            ("0", "1"),
            ("-1", "1"),
            ("0.001", "1"),
            ("1", "0"),
            ("1", "-1"),
            ("NaN", "1"),
            ("1", "inf"),
        ] {
            assert!(calculate(km, rate).is_err());
        }
        assert!(warnings("3.94").unwrap().is_empty());
        assert_eq!(warnings("9").unwrap().len(), 1);
        assert_eq!(warnings("3.940001").unwrap().len(), 1);
    }
}
