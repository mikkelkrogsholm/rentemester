//! Nominal exact SQLite references. Zero/negative existing row IDs are not absence.
use crate::Error;
use serde::{Deserialize, Serialize};
macro_rules! row_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(i64);
        impl $name {
            pub fn from_sqlite(value: i64) -> Self {
                Self(value)
            }
            pub fn value(self) -> i64 {
                self.0
            }
            pub fn parse(value: &str) -> Result<Self, Error> {
                let integer = value.parse::<i64>().map_err(|_| {
                    Error::new(
                        "invalid_identifier",
                        "expected canonical signed64 integer string",
                    )
                })?;
                if integer.to_string() != value {
                    return Err(Error::new(
                        "invalid_identifier",
                        "expected canonical signed64 integer string",
                    ));
                }
                Ok(Self(integer))
            }
        }
        impl TryFrom<String> for $name {
            type Error = Error;
            fn try_from(value: String) -> Result<Self, Error> {
                Self::parse(&value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0.to_string()
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}
row_id!(DocumentId);
row_id!(JournalEntryId);
row_id!(BankTransactionId);
row_id!(PeriodId);
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AccountNo(String);
impl AccountNo {
    pub fn parse(value: &str) -> Result<Self, Error> {
        if value.is_empty() {
            return Err(Error::new(
                "invalid_account_no",
                "account reference must not be empty",
            ));
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for AccountNo {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        Self::parse(&value)
    }
}
impl From<AccountNo> for String {
    fn from(value: AccountNo) -> Self {
        value.0
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_ids_and_opaque_accounts() {
        for s in [
            "0",
            "-1",
            "9007199254740993",
            "9223372036854775807",
            "-9223372036854775808",
        ] {
            assert_eq!(DocumentId::parse(s).unwrap().to_string(), s);
        }
        for s in [
            "",
            "+1",
            "01",
            "-0",
            "1.0",
            "1e0",
            " 1",
            "9223372036854775808",
            "-9223372036854775809",
            "１",
        ] {
            assert_eq!(DocumentId::parse(s).unwrap_err().code, "invalid_identifier");
        }
        for s in ["A-1", " 100 ", "æ🦀", "\0"] {
            assert_eq!(AccountNo::parse(s).unwrap().as_str(), s);
        }
        assert!(AccountNo::parse("").is_err());
    }
}
