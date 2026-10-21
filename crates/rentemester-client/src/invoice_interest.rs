//! Exact human-operated interest requests. Rates, dated principal and correction capacity remain server-owned.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};
pub const MAX_INVOICE_INTEREST_BODY: usize = 16 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvoiceInterestOperation {
    Calculate,
    Register,
    Post,
    Correction,
    PostCorrection,
}
impl InvoiceInterestOperation {
    pub const fn is_read_only(self) -> bool {
        matches!(self, Self::Calculate | Self::Correction)
    }
}
pub fn invoice_interest_body_schema(op: InvoiceInterestOperation) -> Value {
    let id = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    let date = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
    let account = json!({"type":"string","minLength":1,"maxLength":128});
    match op {
        InvoiceInterestOperation::Calculate => object(
            json!({"invoiceDocumentId":id,"asOfDate":date,"referenceRatePercent":{"type":["number","null"],"description":"Optional manual reference rate in percent; omission uses the server statutory table, including dated rate changes."}}),
            &["invoiceDocumentId", "asOfDate"],
        ),
        InvoiceInterestOperation::Register => object(
            json!({"invoiceDocumentId":id,"asOfDate":date,"referenceRatePercent":{"type":["number","null"],"description":"Omit to preserve the statutory-table source; do not replace it with a guessed headline rate."},"note":{"type":["string","null"],"maxLength":4096},"confirm":{"const":true}}),
            &["invoiceDocumentId", "asOfDate", "confirm"],
        ),
        InvoiceInterestOperation::Post => object(
            json!({"invoiceDocumentId":id,"claimId":{"type":["integer","null"],"minimum":1,"maximum":9007199254740991u64},"transactionDate":{"type":["string","null"],"pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},"receivableAccountNo":account,"interestIncomeAccountNo":account,"confirm":{"const":true}}),
            &[
                "invoiceDocumentId",
                "receivableAccountNo",
                "interestIncomeAccountNo",
                "confirm",
            ],
        ),
        InvoiceInterestOperation::Correction => {
            object(json!({"invoiceDocumentId":id}), &["invoiceDocumentId"])
        }
        InvoiceInterestOperation::PostCorrection => object(
            json!({"invoiceDocumentId":id,"transactionDate":{"type":["string","null"],"pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},"reason":{"type":"string","minLength":1,"maxLength":4096},"confirm":{"const":true}}),
            &["invoiceDocumentId", "reason", "confirm"],
        ),
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Calculate {
    invoice_document_id: i64,
    as_of_date: String,
    reference_rate_percent: Option<serde_json::Number>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Register {
    invoice_document_id: i64,
    as_of_date: String,
    reference_rate_percent: Option<serde_json::Number>,
    note: Option<String>,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Post {
    invoice_document_id: i64,
    claim_id: Option<i64>,
    transaction_date: Option<String>,
    receivable_account_no: String,
    interest_income_account_no: String,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Correction {
    invoice_document_id: i64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PostCorrection {
    invoice_document_id: i64,
    transaction_date: Option<String>,
    reason: String,
    confirm: bool,
}
fn invalid() -> Error {
    Error::InvalidInput("invalid exact invoice interest body")
}
fn id(value: i64) -> bool {
    (1..=9007199254740991).contains(&value)
}
fn date(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    (|| {
        let y = value[..4].parse::<i32>().ok()?;
        if y < 1 {
            return None;
        }
        let m = time::Month::try_from(value[5..7].parse::<u8>().ok()?).ok()?;
        time::Date::from_calendar_date(y, m, value[8..].parse().ok()?).ok()
    })()
    .is_some()
}
fn account(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}
fn validate(op: InvoiceInterestOperation, bytes: &[u8], confirmed: bool) -> Result<(), Error> {
    if bytes.len() > MAX_INVOICE_INTEREST_BODY || (!op.is_read_only() && !confirmed) {
        return Err(invalid());
    }
    body(bytes, &invoice_interest_body_schema(op))?;
    let valid = match op {
        InvoiceInterestOperation::Calculate => {
            let i: Calculate = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            let _ = i.reference_rate_percent;
            id(i.invoice_document_id) && date(&i.as_of_date)
        }
        InvoiceInterestOperation::Register => {
            let i: Register = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            let _ = i.reference_rate_percent;
            id(i.invoice_document_id)
                && date(&i.as_of_date)
                && i.confirm
                && i.note.as_ref().is_none_or(|s| s.len() <= 4096)
        }
        InvoiceInterestOperation::Post => {
            let i: Post = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            id(i.invoice_document_id)
                && i.claim_id.is_none_or(id)
                && i.transaction_date.as_deref().is_none_or(date)
                && account(&i.receivable_account_no)
                && account(&i.interest_income_account_no)
                && i.confirm
        }
        InvoiceInterestOperation::Correction => {
            let i: Correction = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            id(i.invoice_document_id)
        }
        InvoiceInterestOperation::PostCorrection => {
            let i: PostCorrection = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            id(i.invoice_document_id)
                && i.transaction_date.as_deref().is_none_or(date)
                && !i.reason.trim().is_empty()
                && i.reason.len() <= 4096
                && i.confirm
        }
    };
    if valid { Ok(()) } else { Err(invalid()) }
}
impl ApiClient {
    /// Exact bytes once; read-only calculations never imply a write. Unknown
    /// writes require canonical calculate/correction/claim reads and manual reconciliation.
    pub async fn invoice_interest(
        &self,
        company: &str,
        op: InvoiceInterestOperation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        validate(op, &bytes, confirmed)?;
        if self.service_key.is_some() {
            return Err(Error::InvalidInput(
                "invoice interest requires a current human MFA cookie",
            ));
        }
        let action = match op {
            InvoiceInterestOperation::Calculate => "calculate",
            InvoiceInterestOperation::Register => "register",
            InvoiceInterestOperation::Post => "post",
            InvoiceInterestOperation::Correction => "correction",
            InvoiceInterestOperation::PostCorrection => "post-correction",
        };
        self.request(
            Method::POST,
            &company_path(company, &format!("invoices/interest/{action}"))?,
            &[],
            Some(bytes),
        )
        .await
        .map_err(|e| {
            if op.is_read_only() && e == Error::OutcomeUnknown {
                Error::Transport
            } else {
                e
            }
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn statutory_default_is_optional_and_exact_native_operations_have_both_write_confirmations() {
        for body in [
            r#"{"invoiceDocumentId":7,"asOfDate":"2026-01-05"}"#,
            r#"{"invoiceDocumentId":7,"asOfDate":"2026-01-05","referenceRatePercent":2}"#,
        ] {
            assert!(validate(InvoiceInterestOperation::Calculate, body.as_bytes(), false).is_ok());
        }
        assert!(
            !invoice_interest_body_schema(InvoiceInterestOperation::Calculate)["required"]
                .as_array()
                .unwrap()
                .contains(&json!("referenceRatePercent"))
        );
        for (op, body) in [
            (
                InvoiceInterestOperation::Register,
                r#"{"invoiceDocumentId":7,"asOfDate":"2026-01-05","confirm":true}"#,
            ),
            (
                InvoiceInterestOperation::Post,
                r#"{"invoiceDocumentId":7,"claimId":2,"receivableAccountNo":"R","interestIncomeAccountNo":"I","confirm":true}"#,
            ),
            (
                InvoiceInterestOperation::PostCorrection,
                r#"{"invoiceDocumentId":7,"reason":"Synthetic dated adjustment","confirm":true}"#,
            ),
        ] {
            assert!(validate(op, body.as_bytes(), true).is_ok());
            assert!(validate(op, body.as_bytes(), false).is_err());
            let missing = body.replace(",\"confirm\":true", "");
            assert!(validate(op, missing.as_bytes(), true).is_err());
        }
        assert!(
            validate(
                InvoiceInterestOperation::Correction,
                br#"{"invoiceDocumentId":7}"#,
                false
            )
            .is_ok()
        );
    }
    #[test]
    fn arbitrary_authority_keys_allocations_duplicates_and_bad_dates_are_rejected_without_network()
    {
        for body in [
            r#"{"invoiceDocumentId":7,"asOfDate":"2026-02-30"}"#,
            r#"{"invoiceDocumentId":7,"invoiceDocumentId":8,"asOfDate":"2026-01-05"}"#,
            r#"{"invoiceDocumentId":7,"asOfDate":"2026-01-05","actor":"forged"}"#,
            r#"{"invoiceDocumentId":7,"asOfDate":"2026-01-05","idempotencyKey":"fake"}"#,
            r#"{"invoiceDocumentId":7,"asOfDate":"2026-01-05","referenceRatePercent":"2"}"#,
        ] {
            assert!(validate(InvoiceInterestOperation::Calculate, body.as_bytes(), false).is_err());
        }
        for extra in [
            "receivableAccountNo",
            "interestIncomeAccountNo",
            "amount",
            "claimId",
            "allocation",
        ] {
            let body = format!(
                "{{\"invoiceDocumentId\":7,\"reason\":\"Synthetic\",\"confirm\":true,\"{extra}\":\"forged\"}}"
            );
            assert!(
                validate(
                    InvoiceInterestOperation::PostCorrection,
                    body.as_bytes(),
                    true
                )
                .is_err()
            );
        }
        assert!(validate(InvoiceInterestOperation::Post,br#"{"invoiceDocumentId":7,"claimId":0,"receivableAccountNo":"R","interestIncomeAccountNo":"I","confirm":true}"#,true).is_err());
        assert!(
            validate(
                InvoiceInterestOperation::PostCorrection,
                br#"{"invoiceDocumentId":7,"reason":" ","confirm":true}"#,
                true
            )
            .is_err()
        );
        assert!(
            validate(
                InvoiceInterestOperation::Calculate,
                &vec![b' '; MAX_INVOICE_INTEREST_BODY + 1],
                false
            )
            .is_err()
        );
    }
}
