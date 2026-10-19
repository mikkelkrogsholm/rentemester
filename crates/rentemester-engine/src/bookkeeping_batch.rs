//! Explicit reviewed native atomic batch scope. No accounting calculations or caller identity.
use crate::{Error, fiscal_year::Date};
use serde::{Deserialize, Serialize};
pub const EXECUTION_POLICY: &str = "atomic_v1";
pub fn refused(message: impl Into<String>) -> Error {
    Error::new("bookkeeping_batch_refused", message)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scope {
    pub accounting_from: String,
    pub accounting_to: String,
    pub bank_from: String,
    pub bank_to: String,
    pub company_id: Option<i64>,
}
impl Scope {
    pub fn validate(&self) -> Result<(), Error> {
        if Date::parse(&self.accounting_from)? > Date::parse(&self.accounting_to)?
            || Date::parse(&self.bank_from)? > Date::parse(&self.bank_to)?
        {
            return Err(refused("date ranges must be ordered"));
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Persist {
    pub accounting_from: String,
    pub accounting_to: String,
    pub bank_from: String,
    pub bank_to: String,
    pub company_id: Option<i64>,
    pub run_key: String,
    pub expected_plan_hash: String,
    pub confirm: bool,
}
impl Persist {
    pub fn scope(&self) -> Scope {
        Scope {
            accounting_from: self.accounting_from.clone(),
            accounting_to: self.accounting_to.clone(),
            bank_from: self.bank_from.clone(),
            bank_to: self.bank_to.clone(),
            company_id: self.company_id,
        }
    }
    pub fn validate(&self) -> Result<(), Error> {
        self.scope().validate()?;
        hash(&self.expected_plan_hash)?;
        if !self.confirm
            || self.run_key.trim().is_empty()
            || self.run_key.len() > 1024
            || self.run_key.chars().any(char::is_control)
        {
            return Err(refused("explicit confirmation and bounded runKey required"));
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Exact {
    pub run_id: i64,
    pub plan_hash: String,
    pub confirm: bool,
}
impl Exact {
    pub fn validate(&self) -> Result<(), Error> {
        id(self.run_id)?;
        hash(&self.plan_hash)?;
        if !self.confirm {
            return Err(refused("confirm:true required"));
        }
        Ok(())
    }
}
pub fn id(id: i64) -> Result<(), Error> {
    if !(1..=9_007_199_254_740_991).contains(&id) {
        Err(refused("positive safe integer required"))
    } else {
        Ok(())
    }
}
pub fn hash(s: &str) -> Result<(), Error> {
    if s.len() != 64
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Err(refused("64 lowercase hexadecimal hash required"))
    } else {
        Ok(())
    }
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkbenchInput {
    pub from: String,
    pub to: String,
    pub status: Option<String>,
    pub bank_account_id: Option<i64>,
    pub party_id: Option<String>,
    pub document_quality: Option<String>,
    pub account: Option<String>,
    pub vat_treatment: Option<String>,
    pub dimension: Option<String>,
    pub search: Option<String>,
    pub limit: Option<usize>,
    pub cursor: Option<usize>,
}
impl WorkbenchInput {
    pub fn scope(&self) -> Scope {
        Scope {
            accounting_from: self.from.clone(),
            accounting_to: self.to.clone(),
            bank_from: self.from.clone(),
            bank_to: self.to.clone(),
            company_id: None,
        }
    }
    pub fn validate(&self) -> Result<(), Error> {
        self.scope().validate()?;
        if self.limit.is_some_and(|n| n == 0 || n > 100)
            || self.search.as_ref().is_some_and(|s| s.len() > 512)
            || self
                .document_quality
                .as_deref()
                .is_some_and(|s| !matches!(s, "matched" | "missing"))
        {
            return Err(refused("invalid workbench filter or page size"));
        }
        if let Some(n) = self.bank_account_id {
            id(n)?
        }
        Ok(())
    }
}
/// Source purchase matching confidence in hundredths. Equal amount alone never proves a match.
pub fn match_score(
    bank_text: &str,
    supplier: &str,
    invoice: &str,
    payment: &str,
    near_date: bool,
) -> u32 {
    let bank = bank_text.to_uppercase();
    let bank_tokens = tokens(bank_text);
    let supplier = tokens(supplier);
    let payment = tokens(payment);
    let invoice = invoice.to_uppercase();
    let invoice_hit = !invoice.is_empty() && bank.contains(&invoice);
    let supplier_hits = supplier
        .iter()
        .filter(|s| bank_tokens.contains(s))
        .take(3)
        .count();
    let payment_hit = payment
        .iter()
        .filter(|s| bank_tokens.contains(s))
        .take(2)
        .next()
        .is_some();
    let mut score = 55
        + if invoice_hit { 20 } else { 0 }
        + if payment_hit { 10 } else { 0 }
        + (supplier_hits as u32 * 10).min(20)
        + if near_date { 5 } else { 0 };
    if !invoice_hit && !payment_hit && supplier_hits < 2 && score >= 50 {
        score = 45
    }
    score
}
fn tokens(s: &str) -> Vec<String> {
    let mut out = vec![];
    for token in s
        .to_uppercase()
        .split(|c: char| !c.is_alphanumeric() && c != '-')
    {
        let ignored = "APS IVS AMBA FMBA SMBA GMBH LTD INC PLC LLC DKK EUR USD SEK NOK GBP FOR THE AND VED MED TIL FRA DEN DET";
        if token.encode_utf16().count() >= 3
            && !ignored.split(' ').any(|s| s == token)
            && !out.iter().any(|s| s == token)
        {
            out.push(token.to_owned())
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scope_and_proof_never_amount_only() {
        assert_eq!(
            match_score("Payment", "Synthetic Supplier", "", "", true),
            45
        );
        assert!(
            match_score(
                "Synthetic Supplier INV-57",
                "Synthetic Supplier",
                "INV-57",
                "",
                false
            ) >= 75
        );
        assert_eq!(
            match_score("Syntheticx Supplierx", "Synthetic Supplier", "", "", true),
            45
        );
        assert!(hash(&"A".repeat(64)).is_err());
        assert!(Date::parse("2026-02-30").is_err());
        assert!(
            serde_json::from_str::<Exact>(
                r#"{"runId":1,"planHash":"x","confirm":true,"actor":"u"}"#
            )
            .is_err()
        );
    }
}
