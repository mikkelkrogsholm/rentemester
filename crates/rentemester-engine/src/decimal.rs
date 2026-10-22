//! Shared bounded decimal grammar and exact half-away quantization; no floats.
use crate::Error;
use num_bigint::{BigInt, Sign};
struct Decimal {
    coefficient: BigInt,
    scale: i32,
}
impl Decimal {
    fn parse(value: &str) -> Result<Self, Error> {
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
        let mut coefficient = BigInt::parse_bytes(digits.as_bytes(), 10).ok_or_else(invalid)?;
        if negative {
            coefficient = -coefficient;
        }
        Ok(Self {
            coefficient,
            scale: scale - exponent,
        })
    }
    fn quantize(self, target: u8) -> Result<BigInt, Error> {
        let shift = self.scale - i32::from(target);
        if shift > 0 {
            round_div(&self.coefficient, &BigInt::from(10u8).pow(shift as u32))
        } else {
            Ok(self.coefficient * BigInt::from(10u8).pow((-shift) as u32))
        }
    }
}
/// Shift a decimal point without quantization or floating-point conversion.
pub(crate) fn shift(value: &str, power: i32) -> Result<String, Error> {
    let mut value = Decimal::parse(value)?;
    value.scale -= power;
    if value.scale < 0 {
        Ok((value.coefficient * BigInt::from(10u8).pow((-value.scale) as u32)).to_string())
    } else {
        let formatted = format_scaled(&value.coefficient, value.scale as usize);
        if formatted.contains('.') {
            Ok(formatted
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_owned())
        } else {
            Ok(formatted)
        }
    }
}
pub(crate) fn compare(left: &str, right: &str) -> Result<std::cmp::Ordering, Error> {
    let a = Decimal::parse(left)?;
    let b = Decimal::parse(right)?;
    let scale = a.scale.max(b.scale);
    Ok(
        (a.coefficient * BigInt::from(10u8).pow((scale - a.scale) as u32))
            .cmp(&(b.coefficient * BigInt::from(10u8).pow((scale - b.scale) as u32))),
    )
}
pub(crate) fn positive(value: &str) -> Result<bool, Error> {
    Ok(Decimal::parse(value)?.coefficient.sign() == Sign::Plus)
}
pub(crate) fn parse_scaled(value: &str, scale: u8) -> Result<BigInt, Error> {
    Decimal::parse(value)?.quantize(scale)
}
pub(crate) fn multiply(left: &str, right: &str, scale: u8) -> Result<BigInt, Error> {
    let a = Decimal::parse(left)?;
    let b = Decimal::parse(right)?;
    Decimal {
        coefficient: a.coefficient * b.coefficient,
        scale: a.scale + b.scale,
    }
    .quantize(scale)
}
pub(crate) fn round_div(numerator: &BigInt, denominator: &BigInt) -> Result<BigInt, Error> {
    if denominator.sign() == Sign::NoSign {
        return Err(Error::new("division_by_zero", "division by zero"));
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    if (remainder * 2u8).magnitude() >= denominator.magnitude() {
        Ok(quotient
            + if numerator.sign() == denominator.sign() {
                1i8
            } else {
                -1i8
            })
    } else {
        Ok(quotient)
    }
}
/// Canonical integer digits are already a value representation, not raw exponent input.
/// Request byte budgets belong to the caller; computed BigInts must roundtrip.
pub(crate) fn parse_canonical(value: &str, scale: usize) -> Result<BigInt, Error> {
    let invalid = || {
        Error::new(
            "invalid_input",
            "expected canonical fixed-scale decimal string",
        )
    };
    let (negative, unsigned) = value
        .strip_prefix('-')
        .map_or((false, value), |s| (true, s));
    let (whole, fraction) = unsigned.split_once('.').ok_or_else(invalid)?;
    if whole.is_empty()
        || (whole.len() > 1 && whole.starts_with('0'))
        || fraction.len() != scale
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return Err(invalid());
    }
    let digits = format!("{whole}{fraction}");
    let mut integer = BigInt::parse_bytes(digits.as_bytes(), 10).ok_or_else(invalid)?;
    if negative {
        if integer.sign() == Sign::NoSign {
            return Err(invalid());
        }
        integer = -integer;
    }
    Ok(integer)
}
pub(crate) fn format_scaled(value: &BigInt, scale: usize) -> String {
    if scale == 0 {
        return value.to_string();
    }
    let digits = value.magnitude().to_str_radix(10);
    let padded = if digits.len() <= scale {
        format!("{digits:0>width$}", width = scale + 1)
    } else {
        digits
    };
    let split = padded.len() - scale;
    format!(
        "{}{}.{}",
        if value.sign() == Sign::Minus { "-" } else { "" },
        &padded[..split],
        &padded[split..]
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_division_is_half_away_and_rejects_zero() {
        for (n, d, want) in [
            (1, 2, 1),
            (-1, 2, -1),
            (1, -2, -1),
            (-1, -2, 1),
            (4, 3, 1),
            (-4, 3, -1),
            (0, -2, 0),
            (6, -3, -2),
            (8, 3, 3),
            (-8, -3, 3),
        ] {
            assert_eq!(
                round_div(&BigInt::from(n), &BigInt::from(d)).unwrap(),
                BigInt::from(want)
            );
        }
        assert_eq!(
            round_div(&BigInt::from(1), &BigInt::from(0))
                .unwrap_err()
                .code,
            "division_by_zero"
        );
    }
}
