//! V1 historical evidence and explicit V2 debit/credit polarity. Never a company posting.
use crate::{
    Error,
    fiscal_year::Date,
    intercompany::{Mapping, Position, identifier},
    money::Money,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use unicode_normalization::UnicodeNormalization;
pub fn refused() -> Error {
    Error::new(
        "elimination_refused",
        "elimination operation or source proof refused",
    )
}
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub id: String,
    pub mapping_id: String,
    pub as_of: String,
    pub evidence_refs: Vec<String>,
}
impl Input {
    pub fn normalize(mut self) -> Result<Self, Error> {
        self.id = crate::company_knowledge::trim(&self.id).nfc().collect();
        self.mapping_id = crate::company_knowledge::trim(&self.mapping_id)
            .nfc()
            .collect();
        if !identifier(&self.id)
            || !identifier(&self.mapping_id)
            || Date::parse(&self.as_of).is_err()
            || self.evidence_refs.is_empty()
            || self.evidence_refs.len() > 32
        {
            return Err(refused());
        }
        for s in &mut self.evidence_refs {
            *s = crate::company_knowledge::trim(s).nfc().collect();
            if s.is_empty() || s.encode_utf16().count() > 256 {
                return Err(refused());
            }
        }
        self.evidence_refs
            .sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
        if self.evidence_refs.windows(2).any(|a| a[0] == a[1]) {
            return Err(refused());
        }
        Ok(self)
    }
}
/// Optional fields preserve exact legacy V1 side field order/bytes; V2 requires all three.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Side {
    pub company_slug: String,
    pub account_nos: Vec<String>,
    pub selection_hash: String,
    pub ledger_head_hash: Option<String>,
    pub entry_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credit_ore: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debit_ore: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
}
pub type Left = Side;
pub type Right = Side;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BalancePayload {
    pub version: u8,
    pub id: String,
    pub kind: String,
    pub group_id: String,
    pub mapping_id: String,
    pub mapping_hash: String,
    pub as_of: String,
    pub currency: String,
    pub amount_ore: String,
    pub left: Side,
    pub right: Side,
    pub evidence_refs: Vec<String>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Adjustment {
    pub company_slug: String,
    pub account_nos: Vec<String>,
    pub position: Position,
    pub debit_ore: String,
    pub credit_ore: String,
}
impl BalancePayload {
    pub fn companies(&self) -> BTreeSet<String> {
        BTreeSet::from([
            self.left.company_slug.clone(),
            self.right.company_slug.clone(),
        ])
    }
    pub fn validate(&self) -> Result<(), Error> {
        let i = self.input().normalize()?;
        if i.id != self.id
            || i.mapping_id != self.mapping_id
            || i.evidence_refs != self.evidence_refs
            || !matches!(self.version, 1 | 2)
            || self.kind != "intercompany-balance"
            || !identifier(&self.group_id)
            || !hash(&self.mapping_hash)
            || self.currency.len() != 3
            || !self.currency.bytes().all(|b| b.is_ascii_uppercase())
            || self.amount_ore.is_empty()
            || self.amount_ore.len() > 2048
            || self.amount_ore.starts_with('0')
            || !self.amount_ore.bytes().all(|b| b.is_ascii_digit())
            || self.left.company_slug == self.right.company_slug
        {
            return Err(refused());
        }
        for side in [&self.left, &self.right] {
            let s = &side.company_slug;
            let a = &side.account_nos;
            if s.is_empty()
                || s.len() > 256
                || s.chars().any(char::is_control)
                || a.is_empty()
                || a.len() > 128
                || a.windows(2).any(|a| a[0] >= a[1])
                || a.iter().any(|a| {
                    a.is_empty()
                        || a.len() > 64
                        || !a
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                })
                || !hash(&side.selection_hash)
                || side.ledger_head_hash.as_ref().is_some_and(|h| !hash(h))
                || side.entry_count > 9_007_199_254_740_991
                || ((side.entry_count == 0) != side.ledger_head_hash.is_none())
            {
                return Err(refused());
            }
        }
        if self.version == 1 {
            if self.left.credit_ore.as_deref() != Some(&self.amount_ore)
                || self.right.debit_ore.as_deref() != Some(&self.amount_ore)
                || self.left.debit_ore.is_some()
                || self.right.credit_ore.is_some()
                || self.left.position.is_some()
                || self.right.position.is_some()
            {
                return Err(refused());
            }
        } else {
            if self.left.position.is_none()
                || self.right.position.is_none()
                || self.left.position == self.right.position
            {
                return Err(refused());
            }
            for side in [&self.left, &self.right] {
                let (receivable, payable) = if side.position == Some(Position::Receivable) {
                    (self.amount_ore.as_str(), "0")
                } else {
                    ("0", self.amount_ore.as_str())
                };
                if side.credit_ore.as_deref() != Some(receivable)
                    || side.debit_ore.as_deref() != Some(payable)
                {
                    return Err(refused());
                }
            }
        }
        Ok(())
    }
    /// Compare protocol representations without changing historical evidence or side identity.
    pub fn same_current_snapshot(&self, fresh: &Self) -> Result<bool, Error> {
        if self.adjustments()? != fresh.adjustments()? {
            return Ok(false);
        }
        let mut comparable = fresh.clone();
        if self.version == 1 {
            comparable.version = 1;
            comparable.left.position = None;
            comparable.left.debit_ore = None;
            comparable.right.position = None;
            comparable.right.credit_ore = None;
        }
        Ok(self.canonical()? == comparable.canonical()?)
    }
    pub fn canonical(&self) -> Result<String, Error> {
        self.validate()?;
        serde_json::to_string(self).map_err(|_| refused())
    }
    pub fn input(&self) -> Input {
        Input {
            id: self.id.clone(),
            mapping_id: self.mapping_id.clone(),
            as_of: self.as_of.clone(),
            evidence_refs: self.evidence_refs.clone(),
        }
    }
    /// V1 polarity is historical evidence, NOT a correction of an incompatible mapping.
    /// Consumers must compare this position with the fresh mapping/source side; on conflict reverse/replan.
    pub fn adjustments(&self) -> Result<[Adjustment; 2], Error> {
        self.validate()?;
        let side = |s: &Side, left: bool| {
            let position = if self.version == 1 {
                if left {
                    Position::Receivable
                } else {
                    Position::Payable
                }
            } else {
                s.position.clone().unwrap()
            };
            Adjustment {
                company_slug: s.company_slug.clone(),
                account_nos: s.account_nos.clone(),
                debit_ore: s.debit_ore.clone().unwrap_or_else(|| "0".into()),
                credit_ore: s.credit_ore.clone().unwrap_or_else(|| "0".into()),
                position,
            }
        };
        Ok([side(&self.left, true), side(&self.right, false)])
    }
}
pub fn derive(
    input: &Input,
    mapping: &Mapping,
    mapping_hash: &str,
    row: &Value,
) -> Result<BalancePayload, Error> {
    let i = input.clone().normalize()?;
    if mapping.id != i.mapping_id
        || !mapping.interval.active(&i.as_of)
        || row["status"] != "matched"
        || row["mappingId"] != i.mapping_id
        || row["mappingHash"] != mapping_hash
        || row["left"]["position"]
            != serde_json::to_value(&mapping.left_position).map_err(|_| refused())?
        || row["right"]["position"]
            != serde_json::to_value(&mapping.right_position).map_err(|_| refused())?
    {
        return Err(refused());
    }
    let l = &row["left"];
    let r = &row["right"];
    let amount = Money::parse_canonical(l["balance"].as_str().ok_or_else(refused)?)?;
    let other = Money::parse_canonical(r["balance"].as_str().ok_or_else(refused)?)?;
    if amount.is_negative()
        || amount == Money::default()
        || amount != other
        || l["currency"] != r["currency"]
        || l["companySlug"] != mapping.left_company_slug
        || r["companySlug"] != mapping.right_company_slug
        || l["accountNos"] != serde_json::json!(mapping.left_account_nos)
        || r["accountNos"] != serde_json::json!(mapping.right_account_nos)
    {
        return Err(refused());
    }
    let ore = amount
        .decimal()
        .replace('.', "")
        .trim_start_matches('0')
        .to_owned();
    let make = |s: &Value,
                company: &str,
                accounts: &[String],
                position: &Position|
     -> Result<Side, Error> {
        Ok(Side {
            company_slug: company.into(),
            account_nos: accounts.into(),
            selection_hash: s["sourceSnapshot"]["selectionHash"]
                .as_str()
                .ok_or_else(refused)?
                .into(),
            ledger_head_hash: serde_json::from_value(s["sourceSnapshot"]["ledgerHeadHash"].clone())
                .map_err(|_| refused())?,
            entry_count: s["sourceSnapshot"]["entryCount"]
                .as_u64()
                .ok_or_else(refused)?,
            credit_ore: Some(if *position == Position::Receivable {
                ore.clone()
            } else {
                "0".into()
            }),
            debit_ore: Some(if *position == Position::Payable {
                ore.clone()
            } else {
                "0".into()
            }),
            position: Some(position.clone()),
        })
    };
    let payload = BalancePayload {
        version: 2,
        id: i.id,
        kind: "intercompany-balance".into(),
        group_id: mapping.group_id.clone(),
        mapping_id: i.mapping_id,
        mapping_hash: mapping_hash.into(),
        as_of: i.as_of,
        currency: l["currency"].as_str().ok_or_else(refused)?.into(),
        amount_ore: ore.clone(),
        left: make(
            l,
            &mapping.left_company_slug,
            &mapping.left_account_nos,
            &mapping.left_position,
        )?,
        right: make(
            r,
            &mapping.right_company_slug,
            &mapping.right_account_nos,
            &mapping.right_position,
        )?,
        evidence_refs: i.evidence_refs,
    };
    payload.validate()?;
    Ok(payload)
}
