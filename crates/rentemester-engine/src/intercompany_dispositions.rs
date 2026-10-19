//! Source-shaped disposition normalization. Evidence only; never accounting instructions.
use crate::{Error, fiscal_year::Date, money::Money};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use unicode_normalization::UnicodeNormalization;
pub fn refused(message: &str) -> Error {
    Error::new("disposition_refused", message)
}
pub fn text(s: &str, max: usize) -> Result<String, Error> {
    let s: String = crate::company_knowledge::trim(s).nfc().collect();
    if s.is_empty() || s.encode_utf16().count() > max {
        return Err(refused("bounded disposition text required"));
    }
    Ok(s)
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Side {
    pub company_slug: String,
    pub role: String,
    pub expected_side: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Disposition {
    #[serde(default)]
    pub disposition_id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub economic_date: String,
    pub amount: Number,
    pub currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settlement_due_date: Option<String>,
    pub party_ids: Vec<String>,
    pub evidence_record_ids: Vec<String>,
    pub left: Side,
    pub right: Side,
}
pub fn refs(values: &[String]) -> Result<Vec<String>, Error> {
    if values.is_empty() || values.len() > 64 {
        return Err(refused("1 through 64 disposition references required"));
    }
    let mut result = values
        .iter()
        .map(|v| text(v, 160))
        .collect::<Result<Vec<_>, _>>()?;
    result.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    if result.windows(2).any(|w| w[0] == w[1]) {
        return Err(refused("disposition references must be unique"));
    }
    Ok(result)
}
impl Disposition {
    pub fn amount_money(&self) -> Result<Money, Error> {
        let raw = self.amount.to_string();
        let amount = Money::parse(&raw)?;
        let (mantissa, exponent) = raw
            .split_once(['e', 'E'])
            .map_or((raw.as_str(), 0), |(m, e)| {
                (m, e.parse::<i32>().unwrap_or(i32::MAX))
            });
        let fraction = mantissa.split_once('.').map_or(0, |(_, f)| f.len() as i32);
        let excess = fraction.saturating_sub(exponent).saturating_sub(2);
        let digits = mantissa
            .chars()
            .filter(char::is_ascii_digit)
            .collect::<String>();
        let exact = excess <= 0
            || (excess as usize <= digits.len()
                && digits[digits.len() - excess as usize..]
                    .bytes()
                    .all(|b| b == b'0'));
        if amount.is_negative() || amount == Money::default() || !exact {
            return Err(refused("positive two-decimal amount required"));
        }
        Ok(amount)
    }
    pub fn normalize(mut self) -> Result<Self, Error> {
        self.disposition_id = text(&self.disposition_id, 80)?;
        if self.disposition_id.encode_utf16().count() < 3 {
            return Err(refused("disposition ID minimum length is three"));
        }
        self.kind = text(&self.kind, 20)?;
        if ![
            "invoice",
            "reimbursement",
            "loan",
            "capital",
            "dividend",
            "repayment",
            "other",
        ]
        .contains(&self.kind.as_str())
        {
            return Err(refused("unsupported disposition type"));
        }
        self.economic_date = text(&self.economic_date, 10)?;
        Date::parse(&self.economic_date)?;
        self.currency = text(&self.currency, 3)?.to_uppercase();
        if self.currency.encode_utf16().count() != 3 {
            return Err(refused("three-character currency required"));
        }
        self.amount_money()?;
        self.settlement_due_date = self
            .settlement_due_date
            .filter(|s| !s.is_empty())
            .map(|s| {
                let d = text(&s, 10)?;
                Date::parse(&d)?;
                Ok(d)
            })
            .transpose()?;
        self.party_ids = refs(&self.party_ids)?;
        self.evidence_record_ids = refs(&self.evidence_record_ids)?;
        for side in [&mut self.left, &mut self.right] {
            side.company_slug = text(&side.company_slug, 120)?;
            side.role = text(&side.role, 80)?;
            side.expected_side = text(&side.expected_side, 20)?;
            if !["receivable", "payable"].contains(&side.expected_side.as_str()) {
                return Err(refused("unsupported disposition side"));
            }
        }
        if self.left.company_slug == self.right.company_slug
            || self.left.expected_side == self.right.expected_side
        {
            return Err(refused(
                "distinct companies and complementary sides required",
            ));
        }
        Ok(self)
    }
}
