//! Positive fixed-six rate values. Quote/currency identity and journal policy are caller context.
use crate::{Error, decimal, money::Money};
use num_bigint::{BigInt, Sign};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FxRate(BigInt);
impl FxRate {
    pub fn parse(value: &str) -> Result<Self, Error> {
        let invalid = || {
            Error::new(
                "invalid_rate",
                "FX rate must round to a positive bounded six-decimal value",
            )
        };
        let rate = decimal::parse_scaled(value, 6).map_err(|_| invalid())?;
        if rate.sign() != Sign::Plus {
            return Err(invalid());
        }
        Ok(Self(rate))
    }
    pub fn parse_canonical(value: &str) -> Result<Self, Error> {
        let invalid = || {
            Error::new(
                "invalid_rate",
                "rate must be canonical positive six-decimal string",
            )
        };
        let rate = decimal::parse_canonical(value, 6).map_err(|_| invalid())?;
        if rate.sign() != Sign::Plus {
            return Err(invalid());
        }
        Ok(Self(rate))
    }
    pub fn decimal(&self) -> String {
        decimal::format_scaled(&self.0, 6)
    }
    pub fn apply(&self, amount: &Money) -> Money {
        // Fixed nonzero denominator; positivity and decimal parsing are established at construction.
        Money(
            decimal::round_div(&(&amount.0 * &self.0), &BigInt::from(1000000u32))
                .expect("fixed nonzero rate scale"),
        )
    }
}
impl Serialize for FxRate {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.decimal())
    }
}
impl<'de> Deserialize<'de> for FxRate {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse_canonical(&s).map_err(serde::de::Error::custom)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_quantization_stages_and_exact_fx_halves() {
        assert_eq!(
            Money::multiply_decimal("1.005", "1.5").unwrap().decimal(),
            "1.51"
        );
        assert_eq!(
            FxRate::parse("1.5")
                .unwrap()
                .apply(&Money::parse("1.005").unwrap())
                .decimal(),
            "1.52"
        );
        assert_eq!(
            FxRate::parse("0.1")
                .unwrap()
                .apply(&Money::parse("1.15").unwrap())
                .decimal(),
            "0.12"
        );
        assert_eq!(
            FxRate::parse("0.1")
                .unwrap()
                .apply(&Money::parse("-1.15").unwrap())
                .decimal(),
            "-0.12"
        );
        assert_eq!(FxRate::parse("0.0000005").unwrap().decimal(), "0.000001");
        assert_eq!(FxRate::parse("7.4603855").unwrap().decimal(), "7.460386");
        assert_eq!(
            Money::parse("1.00")
                .unwrap()
                .percent("0.5")
                .unwrap()
                .decimal(),
            "0.01"
        );
        assert_eq!(
            Money::parse("-1.00")
                .unwrap()
                .percent("0.5")
                .unwrap()
                .decimal(),
            "-0.01"
        );
        assert_eq!(
            Money::parse("-1.00")
                .unwrap()
                .percent("-0.5")
                .unwrap()
                .decimal(),
            "0.01"
        );
    }
    #[test]
    fn exact_huge_values_and_rate_canonical_boundaries() {
        let huge = "999999999999999999999999999999.99";
        assert_eq!(
            Money::multiply_decimal(huge, "10").unwrap().decimal(),
            "9999999999999999999999999999999.90"
        );
        assert_eq!(
            FxRate::parse("10")
                .unwrap()
                .apply(&Money::parse(huge).unwrap())
                .decimal(),
            "9999999999999999999999999999999.90"
        );
        for value in [
            "0",
            "-1",
            "-0",
            "0.0000004",
            "NaN",
            "Infinity",
            "1e401",
            " 1",
        ] {
            assert_eq!(FxRate::parse(value).unwrap_err().code, "invalid_rate");
        }
        for value in [
            "1",
            "01.000000",
            "+1.000000",
            "1e0",
            "1.0000000",
            "-0.000000",
            "0.000000",
        ] {
            assert!(FxRate::parse_canonical(value).is_err());
        }
        assert_eq!(
            FxRate::parse_canonical("1.000000").unwrap().decimal(),
            "1.000000"
        );
        for value in ["1", "01.00", "+1.00", "1e0", "1.000", "-0.00", " 1.00"] {
            assert!(Money::parse_canonical(value).is_err());
        }
        assert_eq!(Money::parse_canonical("0.00").unwrap().decimal(), "0.00");
    }
}
