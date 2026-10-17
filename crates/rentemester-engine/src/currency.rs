//! Canonical currency syntax only; configured eligibility is caller policy.
use crate::Error;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Currency(String);
impl Currency {
    pub fn parse(value: &str) -> Result<Self, Error> {
        if value.len() != 3 || !value.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(Error::new(
                "invalid_currency",
                "currency must be three uppercase ASCII letters",
            ));
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for Currency {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        Self::parse(&value)
    }
}
impl From<Currency> for String {
    fn from(value: Currency) -> Self {
        value.0
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn currency_is_canonical_syntax_not_a_registry_or_normalizer() {
        for value in ["DKK", "USD", "EUR", "XYZ"] {
            assert_eq!(Currency::parse(value).unwrap().as_str(), value);
        }
        for value in [
            "",
            "dkk",
            " DKK",
            "DKK ",
            "DK",
            "DKKK",
            "123",
            "D€K",
            "ＤＫＫ",
            "DK\0",
        ] {
            assert_eq!(Currency::parse(value).unwrap_err().code, "invalid_currency");
        }
    }
}
