use crate::Error;
use num_bigint::{BigInt, Sign};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Exact two-decimal amount (DKK øre or legacy foreign centiunits). Currency is explicit caller context.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Money(pub(crate) BigInt);
impl Money {
    /// Decimal/exponent notation, rounded once, halfway away from zero.
    /// Resource limits bound parsing work; integer arithmetic never wraps.
    pub fn parse(value: &str) -> Result<Self, Error> {
        Ok(Self(crate::decimal::parse_scaled(value, 2)?))
    }
    /// Strict wire input; explicit raw parse is the separate quantizing operation.
    pub fn parse_canonical(value: &str) -> Result<Self, Error> {
        Ok(Self(crate::decimal::parse_canonical(value, 2)?))
    }
    /// Multiply raw operands before cent quantization (e.g. price times quantity).
    pub fn multiply_decimal(left: &str, right: &str) -> Result<Self, Error> {
        Ok(Self(crate::decimal::multiply(left, right, 2)?))
    }
    /// Apply a percent quantized to two-decimal basis points to this cent amount.
    pub fn percent(&self, percent: &str) -> Result<Self, Error> {
        let basis_points = crate::decimal::parse_scaled(percent, 2)?;
        Ok(Self(crate::decimal::round_div(
            &(&self.0 * basis_points),
            &BigInt::from(10000u32),
        )?))
    }
    pub fn add(&mut self, value: &Self) {
        self.0 += &value.0;
    }
    pub fn subtract(&self, other: &Self) -> Self {
        Self(&self.0 - &other.0)
    }
    pub fn is_negative(&self) -> bool {
        self.0.sign() == Sign::Minus
    }
    pub fn decimal(&self) -> String {
        crate::decimal::format_scaled(&self.0, 2)
    }
}
impl Serialize for Money {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.decimal())
    }
}

impl<'de> Deserialize<'de> for Money {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse_canonical(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_rounding_and_no_negative_zero() {
        for (input, expected) in [
            ("1.005", "1.01"),
            ("-1.005", "-1.01"),
            ("1.0049", "1.00"),
            ("2.335", "2.34"),
            ("-0.0049", "0.00"),
            ("1e3", "1000.00"),
            ("5e-3", "0.01"),
            ("0.004999999999999999", "0.00"),
            ("999999999999.9949", "999999999999.99"),
        ] {
            assert_eq!(Money::parse(input).unwrap().decimal(), expected, "{input}");
        }
    }
    #[test]
    fn arbitrary_integer_precision_and_bounded_input() {
        let a = Money::parse("999999999999999999999999999999999999.99").unwrap();
        let mut b = a.clone();
        b.add(&Money::parse("0.01").unwrap());
        assert_eq!(b.decimal(), "1000000000000000000000000000000000000.00");
        for input in [
            "", ".", "NaN", "inf", "1e401", "1e-401", "1e2e3", "1.2.3", " 1", "1 ", "1e", "--1",
        ] {
            assert!(Money::parse(input).is_err(), "{input}");
        }
    }
}
