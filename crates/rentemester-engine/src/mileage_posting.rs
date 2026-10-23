//! Explicit company configuration only; never selects statutory or latest rates.
use crate::{Error, fiscal_year::Date, mileage};
pub const RULE: &str = "NATIVE-MILEAGE-POST-1";
pub fn refused(message: impl Into<String>) -> Error {
    Error::new("mileage_posting_refused", message)
}
pub fn text(value: &str, max: usize) -> Result<(), Error> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(refused("Required bounded explicit text"));
    }
    Ok(())
}
pub fn version(from: &str, to: &str, rate: &str) -> Result<(), Error> {
    let a = Date::parse(from).map_err(|_| refused("Invalid validFrom"))?;
    let b = Date::parse(to).map_err(|_| refused("Invalid validTo"))?;
    if a > b || !crate::decimal::positive(rate).map_err(|_| refused("Invalid decimal rate"))? {
        return Err(refused("Invalid rate or reversed date interval"));
    }
    Ok(())
}
pub fn matches(
    from: &str,
    to: &str,
    vehicle: &str,
    date: &str,
    trip_vehicle: &str,
    rate: &str,
    stored_rate: &str,
) -> Result<(), Error> {
    version(from, to, rate)?;
    Date::parse(date).map_err(|_| refused("Invalid trip date"))?;
    if date < from
        || date > to
        || vehicle != trip_vehicle
        || !mileage::same_rate(rate, stored_rate).map_err(|_| refused("Invalid stored rate"))?
    {
        return Err(refused(
            "Selected version does not match immutable trip date, vehicle or supplied rate",
        ));
    }
    Ok(())
}
#[cfg(test)]
mod mileage_posting_tests {
    use super::*;
    #[test]
    fn mileage_posting_explicit_boundaries() {
        for date in ["2026-01-01", "2026-12-31"] {
            matches("2026-01-01", "2026-12-31", "car", date, "car", "2.00", "2").unwrap();
        }
        for date in ["2025-12-31", "2027-01-01", "2026-02-30"] {
            assert!(matches("2026-01-01", "2026-12-31", "car", date, "car", "2", "2").is_err());
        }
        assert!(version("2026-12-31", "2026-01-01", "2").is_err());
        assert!(version("2026-01-01", "2026-12-31", "0").is_err());
        assert_eq!(
            crate::mileage::calculate("12.35", "2.00")
                .unwrap()
                .1
                .decimal(),
            "24.70"
        );
    }
}
