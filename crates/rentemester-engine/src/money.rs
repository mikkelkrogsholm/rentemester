use crate::Error;
use num_bigint::{BigInt, Sign};
use serde::{Serialize, Serializer};

/// Exact integer øre. Floating point is confined to the legacy storage adapter.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Money(pub(crate) BigInt);
impl Money {
    /// Decimal/exponent notation, rounded once, halfway away from zero.
    /// Resource limits bound parsing work; integer arithmetic never wraps.
    pub fn parse(value: &str) -> Result<Self, Error> {
        let invalid = || Error::new("invalid_input", "invalid or out-of-range decimal amount");
        if value.is_empty() || value.len() > 2048 || !value.is_ascii() {
            return Err(invalid());
        }
        let (negative, unsigned) = if let Some(x) = value.strip_prefix('-') {
            (true, x)
        } else {
            (false, value.strip_prefix('+').unwrap_or(value))
        };
        let mut parts = unsigned.split(['e', 'E']);
        let coefficient = parts.next().ok_or_else(invalid)?;
        let exponent = parts
            .next()
            .map(|x| x.parse::<i32>())
            .transpose()
            .map_err(|_| invalid())?
            .unwrap_or(0);
        if parts.next().is_some() || !(-400..=400).contains(&exponent) {
            return Err(invalid());
        }
        let mut dot = false;
        let mut scale = 0i32;
        let mut digits = String::new();
        for c in coefficient.chars() {
            if c == '.' && !dot {
                dot = true;
            } else if c.is_ascii_digit() {
                digits.push(c);
                if dot {
                    scale += 1;
                }
            } else {
                return Err(invalid());
            }
        }
        if digits.is_empty() {
            return Err(invalid());
        }
        let mut integer = BigInt::parse_bytes(digits.as_bytes(), 10).ok_or_else(invalid)?;
        let shift = scale - exponent - 2;
        if shift > 0 {
            let divisor = BigInt::from(10u8).pow(shift as u32);
            let remainder = &integer % &divisor;
            integer /= &divisor;
            if remainder * 2u8 >= divisor {
                integer += 1u8;
            }
        } else {
            integer *= BigInt::from(10u8).pow((-shift) as u32);
        }
        if negative {
            integer = -integer;
        }
        Ok(Self(integer))
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
        let negative = self.is_negative();
        let digits = self.0.magnitude().to_str_radix(10);
        let padded = if digits.len() < 3 {
            format!("{digits:0>3}")
        } else {
            digits
        };
        let split = padded.len() - 2;
        format!(
            "{}{}.{}",
            if negative { "-" } else { "" },
            &padded[..split],
            &padded[split..]
        )
    }
}
impl Serialize for Money {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.decimal())
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
