//! Original bank journey transport; proof, scores and identities remain server-owned.
use crate::{ApiClient, Error, company_path};
use reqwest::Method;
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug)]
pub enum BankOriginalMutation {
    LinkJournal,
    CorrectionApply,
}
pub fn bank_original_body_schema(operation: BankOriginalMutation) -> Value {
    let id = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    match operation {
        BankOriginalMutation::LinkJournal => {
            json!({"type":"object","properties":{"confirm":{"const":true},"bankTransactionId":id,"journalEntryId":id,"matchMethod":{"type":"string","enum":["exact-date-amount","settlement-lag-amount","source-reference","manual-review"]},"sourceReference":{"type":"string"},"note":{"type":"string"}},"required":["confirm","bankTransactionId","journalEntryId","matchMethod"],"additionalProperties":false})
        }
        BankOriginalMutation::CorrectionApply => {
            let mut schema = crate::bank_correction_body_schema();
            schema["properties"]["expectedPrincipal"] = json!({"type":"object","properties":{"kind":{"type":"string","enum":["user","service-account"]},"subjectId":{"type":"string","minLength":1}},"required":["kind","subjectId"],"additionalProperties":false});
            schema
        }
    }
}
fn envelope(value: Value, mutation: bool) -> Result<Value, Error> {
    let valid = value["ok"].is_boolean()
        && value["errors"]
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
        && value.get("code").is_none_or(Value::is_string)
        && value
            .get("appliedRules")
            .is_none_or(|v| v.as_array().is_some_and(|a| a.iter().all(Value::is_string)))
        && if value["ok"] == true {
            value["data"].is_object() && value["errors"].as_array().is_some_and(Vec::is_empty)
        } else {
            value.get("data").is_none() && value["errors"].as_array().is_some_and(|a| !a.is_empty())
        };
    if valid {
        Ok(value)
    } else {
        Err(if mutation {
            Error::OutcomeUnknown
        } else {
            Error::Protocol
        })
    }
}
fn positive(id: i64) -> bool {
    (1..=9007199254740991).contains(&id)
}
impl ApiClient {
    pub async fn bank_original_suggestions(
        &self,
        company: &str,
        bank: Option<i64>,
        max: Option<u64>,
    ) -> Result<Value, Error> {
        if bank.is_some_and(|n| !positive(n)) || max.is_some_and(|n| n == 0 || n > 9007199254740991)
        {
            return Err(Error::InvalidInput(
                "positive safe bankTransactionId/max required",
            ));
        }
        let bank = bank.map(|n| n.to_string());
        let max = max.map(|n| n.to_string());
        let mut query = vec![];
        if let Some(v) = bank.as_deref() {
            query.push(("bankTransactionId", v));
        }
        if let Some(v) = max.as_deref() {
            query.push(("max", v));
        }
        envelope(
            self.request(
                Method::GET,
                &company_path(company, "bank/original/suggestions")?,
                &query,
                None,
            )
            .await?,
            false,
        )
    }
    pub async fn bank_original_correction_plan(
        &self,
        company: &str,
        bank: i64,
        replacement: i64,
    ) -> Result<Value, Error> {
        if !positive(bank) || !positive(replacement) {
            return Err(Error::InvalidInput(
                "positive safe bank/replacement identifiers required",
            ));
        }
        let bank = bank.to_string();
        let replacement = replacement.to_string();
        envelope(
            self.request(
                Method::GET,
                &company_path(company, "bank/original/correction-plan")?,
                &[
                    ("bankTransactionId", &bank),
                    ("replacementJournalEntryId", &replacement),
                ],
                None,
            )
            .await?,
            false,
        )
    }
    /// Exactly one confirmed request, no automatic retries. Preserve raw reason/key/assertion.
    pub async fn bank_original_mutation(
        &self,
        company: &str,
        operation: BankOriginalMutation,
        body: Vec<u8>,
    ) -> Result<Value, Error> {
        if body.len() > 16 * 1024 {
            return Err(Error::InvalidInput("bank original mutation exceeds16KiB"));
        }
        crate::domain::body(&body, &bank_original_body_schema(operation))?;
        let action = match operation {
            BankOriginalMutation::LinkJournal => "link-journal",
            BankOriginalMutation::CorrectionApply => "correction-apply",
        };
        envelope(
            self.request(
                Method::POST,
                &company_path(company, &format!("bank/original/{action}"))?,
                &[],
                Some(body),
            )
            .await?,
            true,
        )
    }
}
