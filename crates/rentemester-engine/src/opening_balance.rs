//! Explicit operator-supplied cutover. No archive amount/date inference or posting authority.
use crate::{
    Error,
    currency::Currency,
    fiscal_year::Date,
    identifiers::AccountNo,
    manual_journal::{JournalLine, ManualJournal},
    money::Money,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Line {
    pub account_no: AccountNo,
    #[serde(default)]
    pub debit_amount: Money,
    #[serde(default)]
    pub credit_amount: Money,
    pub text: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub cut_over_date: Date,
    pub lines: Vec<Line>,
    pub note: Option<String>,
    #[serde(default)]
    pub source_archive_import_ids: Vec<i64>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Apply {
    pub input: Input,
    pub plan_hash: String,
    pub idempotency_key: String,
    pub confirm: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchiveCitation {
    pub import_id: i64,
    pub source_hash: String,
    pub parser_contract: String,
    pub declared_currency: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceBasis {
    pub kind: String,
    pub currency: String,
    pub source_archives: Vec<ArchiveCitation>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record {
    pub cut_over_date: Date,
    pub journal_entry_id: i64,
    pub journal_entry_no: String,
    pub journal_entry_hash: String,
    pub plan_hash: String,
    pub source_basis: SourceBasis,
    pub created_at: String,
    pub actor: String,
}
pub fn refused(message: impl Into<String>) -> Error {
    Error::new("opening_balance_refused", message)
}
impl Input {
    pub fn validate(&self) -> Result<(), Error> {
        let bytes = serde_json::to_vec(self).map_err(|_| refused("opening input unavailable"))?;
        if bytes.len() > 65536
            || self.lines.len() < 2
            || self.note.as_ref().is_some_and(|n| n.len() > 4096)
            || self.source_archive_import_ids.len() > 32
        {
            return Err(refused(
                "opening input exceeds bounds or requires two lines",
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        if self
            .source_archive_import_ids
            .iter()
            .any(|id| *id <= 0 || *id > 9_007_199_254_740_991 || !seen.insert(*id))
        {
            return Err(refused(
                "archive citations must be unique positive safe identifiers",
            ));
        }
        Ok(())
    }
    pub fn request(&self, currency: &str) -> Result<ManualJournal, Error> {
        self.validate()?;
        let note = self.note.as_deref().unwrap_or("").trim();
        Ok(ManualJournal {
            transaction_date: self.cut_over_date,
            text: format!(
                "Primobalance pr. {}{}",
                self.cut_over_date,
                if note.is_empty() {
                    String::new()
                } else {
                    format!(" — {note}")
                }
            ),
            currency: Currency::parse(currency)?,
            document_id: None,
            source_bank_transaction_id: None,
            amount_foreign: None,
            amount_dkk: None,
            fx_rate_to_dkk: None,
            created_by_program: None,
            lines: self
                .lines
                .iter()
                .map(|l| JournalLine {
                    account_no: l.account_no.clone(),
                    debit_amount: l.debit_amount.clone(),
                    credit_amount: l.credit_amount.clone(),
                    vat_code: None,
                    text: Some(
                        l.text
                            .clone()
                            .unwrap_or_else(|| format!("Primobalance {}", l.account_no.as_str())),
                    ),
                })
                .collect(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn exact_wire_defaults_and_no_inferred_source_values() {
        let i:Input=serde_json::from_value(json!({"cutOverDate":"2024-07-01","lines":[{"accountNo":"A","debitAmount":"100.01"},{"accountNo":"E","creditAmount":"100.01"}]})).unwrap();
        assert_eq!(
            serde_json::to_value(&i).unwrap()["lines"][0]["creditAmount"],
            "0.00"
        );
        let request = i.request("DKK").unwrap();
        assert_eq!(request.transaction_date.to_string(), "2024-07-01");
        assert!(request.created_by_program.is_none());
        assert!(request.document_id.is_none());
        assert_eq!(request.lines[0].debit_amount.decimal(), "100.01");
        assert!(
            serde_json::from_value::<Input>(
                json!({"cutOverDate":"2024-07-01","lines":[],"actor":"user:fake"})
            )
            .is_err()
        );
    }
    #[test]
    fn invalid_source_citations_refuse() {
        let mut i:Input=serde_json::from_value(json!({"cutOverDate":"2024-07-01","lines":[{"accountNo":"A","debitAmount":"1.00"},{"accountNo":"E","creditAmount":"1.00"}],"sourceArchiveImportIds":[1,1]})).unwrap();
        assert!(i.validate().is_err());
        i.source_archive_import_ids = vec![-1];
        assert!(i.validate().is_err());
    }
}
