//! Confirmed established-loss write-off. Financial allocation and authority stay server-owned.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};
pub const MAX_INVOICE_BAD_DEBT_BODY: usize = 16 * 1024;
pub fn invoice_bad_debt_body_schema() -> Value {
    object(
        json!({"invoiceDocumentId":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"writeOffDate":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},"grossAmount":{"type":["number","null"],"exclusiveMinimum":0,"description":"Gross established loss in ORIGINAL invoice currency. Omit for current open principal; claims remain separate."},"expenseAccountNo":{"type":"string","minLength":1,"maxLength":128},"note":{"type":["string","null"],"maxLength":4096},"confirm":{"const":true}}),
        &[
            "invoiceDocumentId",
            "writeOffDate",
            "expenseAccountNo",
            "confirm",
        ],
    )
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WriteOff {
    invoice_document_id: i64,
    write_off_date: String,
    gross_amount: Option<serde_json::Number>,
    expense_account_no: String,
    note: Option<String>,
    confirm: bool,
}
fn invalid() -> Error {
    Error::InvalidInput("invalid exact confirmed invoice bad-debt body")
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

fn validate(bytes: &[u8], confirmed: bool) -> Result<(), Error> {
    if !confirmed || bytes.len() > MAX_INVOICE_BAD_DEBT_BODY {
        return Err(invalid());
    }
    body(bytes, &invoice_bad_debt_body_schema())?;
    let i: WriteOff = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if !(1..=9007199254740991).contains(&i.invoice_document_id)
        || !date(&i.write_off_date)
        || !account(&i.expense_account_no)
        || !i.confirm
        || i.note.as_ref().is_some_and(|s| s.len() > 4096)
        || !i
            .gross_amount
            .as_ref()
            .is_none_or(|n| n.as_f64().is_some_and(|v| v.is_finite() && v > 0.0))
    {
        return Err(invalid());
    }
    Ok(())
}
impl ApiClient {
    /// Send exact confirmed bytes once. Unknown outcomes require canonical invoice/reminderClaims reads and manual reconciliation, never automatic retry.
    pub async fn invoice_bad_debt(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        validate(&bytes, confirmed)?;
        if self.service_key.is_some() {
            return Err(Error::InvalidInput(
                "invoice bad-debt write-off requires a current human MFA cookie",
            ));
        }
        self.request(
            Method::POST,
            &company_path(company, "invoices/bad-debt/write-off")?,
            &[],
            Some(bytes),
        )
        .await
    }
}
/// Original structured payload; selectors and account assertions are resolved only by the server.
pub fn invoice_bad_debt_original_schema() -> Value {
    let mut payload = invoice_bad_debt_body_schema();
    payload["properties"]
        .as_object_mut()
        .unwrap()
        .remove("confirm");
    for key in ["invoiceNumber", "receivableAccountNo", "vatAccountNo"] {
        payload["properties"][key] = json!({"type":"string","minLength":1});
    }
    payload["properties"]["grossAmount"]["type"] = json!("number");
    payload["properties"]["note"]["type"] = json!("string");
    payload["required"] = json!(["writeOffDate", "expenseAccountNo"]);
    payload["anyOf"] = json!([{"required":["invoiceDocumentId"]},{"required":["invoiceNumber"]}]);
    object(
        json!({"payload":payload,"confirm":{"const":true}}),
        &["payload", "confirm"],
    )
}
pub fn validate_invoice_bad_debt_original(bytes: &[u8]) -> Result<(), Error> {
    if bytes.len() > MAX_INVOICE_BAD_DEBT_BODY {
        return Err(invalid());
    }
    body(bytes, &invoice_bad_debt_original_schema())?;
    let value: Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let p = &value["payload"];
    if (p.get("invoiceDocumentId").is_none() && p.get("invoiceNumber").is_none())
        || !date(p["writeOffDate"].as_str().ok_or_else(invalid)?)
        || !account(p["expenseAccountNo"].as_str().ok_or_else(invalid)?)
    {
        return Err(invalid());
    }
    Ok(())
}
impl ApiClient {
    pub async fn invoice_bad_debt_original(
        &self,
        company: &str,
        bytes: Vec<u8>,
    ) -> Result<Value, Error> {
        validate_invoice_bad_debt_original(&bytes)?;
        self.request(
            Method::POST,
            &company_path(company, "invoices/original/write-off-bad-debt")?,
            &[],
            Some(bytes),
        )
        .await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_original_currency_amount_is_optional_and_both_confirmations_required() {
        for raw in [
            r#"{"invoiceDocumentId":7,"writeOffDate":"2026-10-20","expenseAccountNo":"E","confirm":true}"#,
            r#"{"invoiceDocumentId":7,"writeOffDate":"2026-10-20","grossAmount":74.95,"expenseAccountNo":"E","note":"Synthetic documented loss","confirm":true}"#,
        ] {
            assert!(validate(raw.as_bytes(), true).is_ok());
            assert!(validate(raw.as_bytes(), false).is_err());
            assert!(validate(raw.replace(",\"confirm\":true", "").as_bytes(), true).is_err());
        }
    }
    #[test]
    fn rejects_authority_duplicate_dates_accounts_unconfirmed_and_oversize() {
        let valid = json!({"invoiceDocumentId":7,"writeOffDate":"2026-10-20","expenseAccountNo":"E","confirm":true});
        for (key, value) in [
            ("invoiceDocumentId", json!(0)),
            ("writeOffDate", json!("2026-02-30")),
            ("writeOffDate", json!("0000-01-01")),
            ("expenseAccountNo", json!(" ")),
            ("grossAmount", json!(0)),
            ("grossAmount", json!(-1)),
            ("confirm", json!(false)),
            ("actor", json!("forged")),
            ("vatAccountNo", json!("V")),
            ("receivableAccountNo", json!("R")),
            ("idempotencyKey", json!("invented")),
        ] {
            let mut input = valid.clone();
            input[key] = value;
            assert!(
                validate(&serde_json::to_vec(&input).unwrap(), true).is_err(),
                "{key}"
            );
        }
        assert!(validate(br#"{"invoiceDocumentId":7,"invoiceDocumentId":8,"writeOffDate":"2026-10-20","expenseAccountNo":"E","confirm":true}"#,true).is_err());
        assert!(validate(&vec![b' '; MAX_INVOICE_BAD_DEBT_BODY + 1], true).is_err());
    }
}
