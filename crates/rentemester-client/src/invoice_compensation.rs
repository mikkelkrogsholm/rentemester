//! Exact human-operated compensation requests; financial policy remains server-owned.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};
pub const MAX_INVOICE_COMPENSATION_BODY: usize = 16 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvoiceCompensationOperation {
    Calculate,
    Register,
    Post,
}
pub fn invoice_compensation_body_schema(op: InvoiceCompensationOperation) -> Value {
    let id = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    let date = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
    match op {
        InvoiceCompensationOperation::Calculate => object(
            json!({"invoiceDocumentId":id,"asOfDate":date,"compensationAmountDkk":{"type":["number","null"]}}),
            &["invoiceDocumentId", "asOfDate"],
        ),
        InvoiceCompensationOperation::Register => object(
            json!({"invoiceDocumentId":id,"asOfDate":date,"compensationAmountDkk":{"type":["number","null"]},"note":{"type":["string","null"],"maxLength":4096},"confirm":{"const":true}}),
            &["invoiceDocumentId", "asOfDate", "confirm"],
        ),
        InvoiceCompensationOperation::Post => object(
            json!({"invoiceDocumentId":id,"transactionDate":{"type":["string","null"],"pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},"receivableAccountNo":{"type":"string","minLength":1,"maxLength":128},"compensationIncomeAccountNo":{"type":"string","minLength":1,"maxLength":128},"confirm":{"const":true}}),
            &[
                "invoiceDocumentId",
                "receivableAccountNo",
                "compensationIncomeAccountNo",
                "confirm",
            ],
        ),
    }
}
// Deserialize actual fields as well as validating the discoverable schema, so
// duplicate/unknown/ill-typed fields cannot be silently normalized by the SDK.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Calculate {
    invoice_document_id: i64,
    as_of_date: String,
    compensation_amount_dkk: Option<serde_json::Number>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Register {
    invoice_document_id: i64,
    as_of_date: String,
    compensation_amount_dkk: Option<serde_json::Number>,
    note: Option<String>,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Post {
    invoice_document_id: i64,
    transaction_date: Option<String>,
    receivable_account_no: String,
    compensation_income_account_no: String,
    confirm: bool,
}
fn invalid() -> Error {
    Error::InvalidInput("invalid exact invoice compensation body")
}
fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let parsed = (|| {
        let y = value[..4].parse::<i32>().ok()?;
        if y < 1 {
            return None;
        }
        let m = time::Month::try_from(value[5..7].parse::<u8>().ok()?).ok()?;
        time::Date::from_calendar_date(y, m, value[8..].parse::<u8>().ok()?).ok()
    })();
    parsed.is_some()
}
fn validate(op: InvoiceCompensationOperation, bytes: &[u8], confirmed: bool) -> Result<(), Error> {
    if bytes.len() > MAX_INVOICE_COMPENSATION_BODY
        || (op != InvoiceCompensationOperation::Calculate && !confirmed)
    {
        return Err(invalid());
    }
    body(bytes, &invoice_compensation_body_schema(op))?;
    let valid = match op {
        InvoiceCompensationOperation::Calculate => {
            let i: Calculate = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            let _ = i.compensation_amount_dkk;
            (1..=9007199254740991).contains(&i.invoice_document_id) && valid_date(&i.as_of_date)
        }
        InvoiceCompensationOperation::Register => {
            let i: Register = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            let _ = i.compensation_amount_dkk;
            (1..=9007199254740991).contains(&i.invoice_document_id)
                && valid_date(&i.as_of_date)
                && i.confirm
                && i.note.as_ref().is_none_or(|s| s.len() <= 4096)
        }
        InvoiceCompensationOperation::Post => {
            let i: Post = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            let account = |s: &str| {
                !s.trim().is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)
            };
            (1..=9007199254740991).contains(&i.invoice_document_id)
                && i.confirm
                && i.transaction_date.as_deref().is_none_or(valid_date)
                && account(&i.receivable_account_no)
                && account(&i.compensation_income_account_no)
        }
    };
    if valid { Ok(()) } else { Err(invalid()) }
}
impl ApiClient {
    /// Send exact bytes once. Calculation is read-only. Unknown writes require
    /// canonical claim/journal reads and human resolution, never automatic retry.
    pub async fn invoice_compensation(
        &self,
        company: &str,
        op: InvoiceCompensationOperation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        validate(op, &bytes, confirmed)?;
        if self.service_key.is_some() {
            return Err(Error::InvalidInput(
                "invoice compensation requires a current human MFA cookie",
            ));
        }
        let action = match op {
            InvoiceCompensationOperation::Calculate => "calculate",
            InvoiceCompensationOperation::Register => "register",
            InvoiceCompensationOperation::Post => "post",
        };
        self.request(
            Method::POST,
            &company_path(company, &format!("invoices/compensation/{action}"))?,
            &[],
            Some(bytes),
        )
        .await
        .map_err(|error| {
            if op == InvoiceCompensationOperation::Calculate && error == Error::OutcomeUnknown {
                Error::Transport
            } else {
                error
            }
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_body_requires_both_write_confirmations_and_rejects_authority_duplicates_and_invalid_dates()
     {
        let calc = br#"{"invoiceDocumentId":7,"asOfDate":"2026-01-01"}"#;
        assert!(validate(InvoiceCompensationOperation::Calculate, calc, false).is_ok());
        let registered = br#"{"invoiceDocumentId":7,"asOfDate":"2026-01-01","confirm":true}"#;
        assert!(validate(InvoiceCompensationOperation::Register, registered, true).is_ok());
        assert!(validate(InvoiceCompensationOperation::Register, registered, false).is_err());
        for raw in [
            r#"{"invoiceDocumentId":7,"asOfDate":"2026-02-30"}"#,
            r#"{"invoiceDocumentId":7,"invoiceDocumentId":8,"asOfDate":"2026-01-01"}"#,
            r#"{"invoiceDocumentId":7,"asOfDate":"2026-01-01","actor":"forged"}"#,
            r#"{"invoiceDocumentId":7,"asOfDate":"2026-01-01","confirm":true}"#,
        ] {
            assert!(
                validate(
                    InvoiceCompensationOperation::Calculate,
                    raw.as_bytes(),
                    false
                )
                .is_err()
            );
        }
        assert!(validate(InvoiceCompensationOperation::Post,br#"{"invoiceDocumentId":7,"receivableAccountNo":" ","compensationIncomeAccountNo":"I","confirm":true}"#,true).is_err());
    }
}
