//! Versioned closed-history source values; never a live-ledger migration.
use crate::{Error, money::Money};
use num_bigint::BigInt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
mod csv;
#[cfg(test)]
mod tests;
pub const FORMAT: &str = "rentemester-native-closed-history/1";
pub const PACKAGE: &str = "rentemester-dinero-archive-source/1";
pub fn refused(message: impl Into<String>) -> Error {
    Error::new("archive_import_refused", message)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveDecimal(BigInt);
impl ArchiveDecimal {
    pub fn parse_csv(raw: &str) -> Result<Self, Error> {
        let raw = raw.trim();
        let s = raw.replace(',', ".");
        let unsigned = s.strip_prefix('-').unwrap_or(&s);
        let mut parts = unsigned.split('.');
        let whole = parts.next().unwrap_or("");
        let fraction = parts.next();
        if whole.is_empty()
            || s.len() > 2048
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || parts.next().is_some()
            || fraction.is_some_and(|v| {
                v.is_empty() || v.len() > 6 || !v.bytes().all(|b| b.is_ascii_digit())
            })
        {
            return Err(refused("invalid signed six-decimal amount"));
        }
        let n = crate::decimal::parse_scaled(&s, 6)
            .map_err(|_| refused("invalid signed six-decimal amount"))?;
        Self::bounded(n)
    }
    pub fn parse_canonical(s: &str) -> Result<Self, Error> {
        Self::bounded(
            crate::decimal::parse_canonical(s, 6)
                .map_err(|_| refused("noncanonical archive amount"))?,
        )
    }
    fn bounded(n: BigInt) -> Result<Self, Error> {
        let limit = BigInt::from(9_007_199_254_740_991i64) * 1_000_000u32;
        if n.magnitude() > limit.magnitude() {
            return Err(refused("archive amount exceeds supported magnitude"));
        }
        Ok(Self(n))
    }
    pub fn decimal(&self) -> String {
        crate::decimal::format_scaled(&self.0, 6)
    }
    pub fn money(&self) -> Money {
        Money::parse(&self.decimal()).expect("bounded canonical six decimal")
    }
    /// Numeric storage guard reuses the exact decimal parser, including scientific shortest float spellings.
    pub fn matches_numeric_decimal(&self, s: &str) -> bool {
        crate::decimal::parse_scaled(s, 6).is_ok_and(|n| n == self.0)
            && Money::parse(s).is_ok_and(|n| n == self.money())
    }
}
impl Serialize for ArchiveDecimal {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.decimal())
    }
}
impl<'de> Deserialize<'de> for ArchiveDecimal {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::parse_canonical(&String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}
#[derive(Clone, Debug)]
pub struct CsvFile {
    pub path: String,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Posting {
    pub line_no: u32,
    pub account_no: String,
    pub account_name: String,
    pub transaction_date: String,
    pub voucher: String,
    pub voucher_type: String,
    pub text: String,
    pub vat_type: String,
    pub amount: ArchiveDecimal,
    pub running_balance: Option<ArchiveDecimal>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Balance {
    pub line_no: u32,
    pub account_no: String,
    pub account_name: String,
    pub amount: ArchiveDecimal,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Year {
    pub fiscal_year: u16,
    pub postings: Vec<Posting>,
    pub balances: Vec<Balance>,
    pub has_balances: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParsedArchive {
    pub parser_contract: String,
    pub declared_currency: String,
    pub excluded_latest_year: u16,
    pub years: Vec<Year>,
}
impl ParsedArchive {
    pub fn canonical(&self) -> Result<String, Error> {
        serde_json::to_string(self).map_err(|_| refused("canonical archive unavailable"))
    }
}
pub fn canonical_path(path: &str) -> Result<(u16, bool), Error> {
    let Some((year, name)) = path.split_once('/') else {
        return Err(refused("unsupported archive source path"));
    };
    if year.len() != 4 || !year.bytes().all(|b| b.is_ascii_digit()) {
        return Err(refused("unsupported archive source year"));
    }
    let year = year
        .parse::<u16>()
        .map_err(|_| refused("invalid archive year"))?;
    if !(1..=9999).contains(&year) {
        return Err(refused("invalid archive year"));
    }
    match name {
        "Posteringer.csv" => Ok((year, false)),
        "SaldoBalance.csv" => Ok((year, true)),
        _ => Err(refused("source paths must use canonical year/CSV names")),
    }
}
pub fn parse(files: &[CsvFile], currency: &str) -> Result<ParsedArchive, Error> {
    if files.len() > 64
        || files.is_empty()
        || currency.len() != 3
        || !currency.bytes().all(|b| b.is_ascii_uppercase())
    {
        return Err(refused("invalid source inventory or declared currency"));
    }
    crate::currency::Currency::parse(currency)?;
    let mut inventory = BTreeMap::new();
    let mut total = 0usize;
    for file in files {
        let key = canonical_path(&file.path)?;
        total = total
            .checked_add(file.bytes.len())
            .ok_or_else(|| refused("source size"))?;
        if file.bytes.len() > 16 * 1024 * 1024
            || total > 18 * 1024 * 1024
            || inventory.insert(key, file).is_some()
        {
            return Err(refused("duplicate or oversized archive source"));
        }
    }
    let years: Vec<_> = inventory
        .keys()
        .filter_map(|(y, b)| (!b).then_some(*y))
        .collect();
    if !(2..=32).contains(&years.len()) || inventory.keys().any(|(y, _)| !years.contains(y)) {
        return Err(refused(
            "at least two posting years required; balances require their posting source",
        ));
    }
    let excluded = *years.last().unwrap();
    let mut result = vec![];
    let mut count = 0usize;
    for year in years {
        let postings = csv::postings(&inventory[&(year, false)].bytes)?;
        let has_balances = inventory.contains_key(&(year, true));
        let balances = if let Some(file) = inventory.get(&(year, true)) {
            csv::balances(&file.bytes)?
        } else {
            vec![]
        };
        count += postings.len() + balances.len();
        if count > 100000 {
            return Err(refused("archive row limit exceeded"));
        }
        if year != excluded {
            result.push(Year {
                fiscal_year: year,
                postings,
                balances,
                has_balances,
            });
        }
    }
    Ok(ParsedArchive {
        parser_contract: FORMAT.into(),
        declared_currency: currency.into(),
        excluded_latest_year: excluded,
        years: result,
    })
}
