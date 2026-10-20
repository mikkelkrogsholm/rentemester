//! Existing bank and supplier-bill endpoints; accounting/evidence stays on server.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object, optional_text},
};
use reqwest::Method;
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug)]
pub enum BankMutation {
    CreateAccount,
    ImportStatement,
    RegisterPayable,
    PayPayable(i64),
    MatchExpense,
}
pub fn bank_body_schema(operation: BankMutation) -> Value {
    let id = json!({"type":"integer","minimum":1});
    let date = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
    let account = json!({"type":"string","minLength":1,"maxLength":128});
    let treatment = json!({"type":["string","null"],"enum":["standard","exempt","non_deductible",null],"description":"Server verifies company VAT registration and immutable document amounts"});
    match operation {
        BankMutation::CreateAccount => object(
            json!({"name":{"type":"string","minLength":1,"maxLength":256},"slug":optional_text(),"bankName":optional_text(),"registrationNo":optional_text(),"accountNo":optional_text(),"iban":optional_text(),"bic":optional_text(),"accountOwner":optional_text(),"customerNo":optional_text(),"currency":optional_text(),"ledgerAccountNo":optional_text()}),
            &["name"],
        ),
        BankMutation::ImportStatement => {
            let mut schema = object(
                json!({"confirm":{"const":true},"csvContent":{"type":"string","minLength":1,"maxLength":26214400,"description":"UTF-8 CSV text; use csvBase64 instead to retain original raw bytes/BOM/legacy encoding. Generic comma/semicolon/tab CSV requires date,text,amount and unambiguous ISO dates. Optional currency,reference,booking_date,balance_after,amount_dkk,fx_rate_to_dkk. Server validates and archives exact supplied source."},"csvBase64":{"type":"string","minLength":4,"maxLength":34952536,"description":"Canonical standard base64 of original CSV bytes, at most25MiB decoded. Exactly one of csvBase64 or csvContent. Generic profile accepts UTF-8/BOM; named danske-bank also supports declared Windows1252 fallback. No client decoding or encoding inference."},"account":{"type":["string","null"],"maxLength":128,"description":"Existing active bank account id or slug from bank_accounts; ledger mapping constrains later payment account"},"profile":{"type":["string","null"],"enum":[null,"","danske-bank"],"description":"Omit, null or empty for generic UTF-8/BOM CSV; danske-bank selects declared semicolon/day-month-year CSV with UTF-8 or Windows1252 fallback"}}),
                &["confirm"],
            );
            schema["oneOf"] = json!([
                {"required":["csvContent"],"not":{"required":["csvBase64"]}},
                {"required":["csvBase64"],"not":{"required":["csvContent"]}}
            ]);
            schema
        }
        BankMutation::RegisterPayable => object(
            json!({"documentId":id,"billDate":date,"dueDate":date,"expenseAccountNo":account,"vatTreatment":treatment,"vendorId":{"type":["integer","null"],"minimum":1},"note":optional_text(),"confirm":{"const":true}}),
            &[
                "documentId",
                "billDate",
                "dueDate",
                "expenseAccountNo",
                "confirm",
            ],
        ),
        BankMutation::PayPayable(_) => object(
            json!({"bankTransactionId":id,"paymentDate":optional_text(),"amount":{"type":["number","null"],"exclusiveMinimum":0,"description":"If supplied, equals full magnitude of outgoing DKK bank row; server prevents overpayment"},"paymentAccountNo":optional_text(),"note":optional_text(),"confirm":{"const":true}}),
            &["bankTransactionId", "confirm"],
        ),
        BankMutation::MatchExpense => object(
            json!({"documentId":id,"bankTransactionId":id,"expenseAccountNo":account,"vatTreatment":treatment,"paymentAccountNo":optional_text(),"transactionDate":optional_text(),"text":optional_text(),"confirm":{"const":true}}),
            &[
                "documentId",
                "bankTransactionId",
                "expenseAccountNo",
                "confirm",
            ],
        ),
    }
}
const MAX_BANK_IMPORT_BODY: usize = 35 * 1024 * 1024;
const MAX_BANK_ORIGINAL: usize = 25 * 1024 * 1024;

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}
// Canonical spelling and decoded bound without allocating or interpreting source bytes.
fn bounded_base64(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_BANK_ORIGINAL.div_ceil(3) * 4 || !bytes.len().is_multiple_of(4) {
        return false;
    }
    let padding = if bytes.ends_with(b"==") {
        2
    } else if bytes.ends_with(b"=") {
        1
    } else {
        0
    };
    let payload = &bytes[..bytes.len() - padding];
    if !payload.iter().all(|b| base64_value(*b).is_some()) {
        return false;
    }
    let tail = base64_value(*payload.last().unwrap()).unwrap();
    bytes.len() / 4 * 3 - padding <= MAX_BANK_ORIGINAL
        && match padding {
            2 => tail & 15 == 0,
            1 => tail & 3 == 0,
            _ => true,
        }
}
fn import_body(bytes: &[u8]) -> Result<(), Error> {
    if bytes.len() > MAX_BANK_IMPORT_BODY {
        return Err(Error::InvalidInput(
            "bank import exceeds native35MiB JSON bound",
        ));
    }
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| Error::InvalidInput("expected native bank import JSON object"))?;
    // domain::body has a256KiB cap and does not implement oneOf/not. Enforce the
    // discoverable import union directly; do not weaken other domain mutations.
    let allowed = ["confirm", "csvContent", "csvBase64", "account", "profile"];
    if value
        .as_object()
        .is_none_or(|fields| fields.keys().any(|key| !allowed.contains(&key.as_str())))
        || value["confirm"] != true
        || value
            .get("account")
            .is_some_and(|v| !v.is_null() && v.as_str().is_none_or(|s| s.len() > 128))
        || value
            .get("profile")
            .is_some_and(|v| ![json!(null), json!(""), json!("danske-bank")].contains(v))
    {
        return Err(Error::InvalidInput(
            "native bank import shape differs from discoverable schema",
        ));
    }
    match (value.get("csvContent"), value.get("csvBase64")) {
        (Some(content), None)
            if content
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= MAX_BANK_ORIGINAL) =>
        {
            Ok(())
        }
        (None, Some(content)) if content.as_str().is_some_and(bounded_base64) => Ok(()),
        _ => Err(Error::InvalidInput(
            "exactly one bounded csvContent or canonical csvBase64 required",
        )),
    }
}

impl ApiClient {
    pub async fn bank_accounts(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "bank-accounts")?, &[])
            .await
    }
    pub async fn bank(&self, company: &str, year: Option<&str>) -> Result<Value, Error> {
        if year.is_some_and(|s| s.len() > 128) {
            return Err(Error::InvalidInput("bank year selector too long"));
        }
        let query = year.map(|y| vec![("year", y)]).unwrap_or_default();
        self.get(&company_path(company, "bank")?, &query).await
    }
    pub async fn payables(
        &self,
        company: &str,
        status: Option<&str>,
        as_of: Option<&str>,
    ) -> Result<Value, Error> {
        if status.is_some_and(|s| !["open", "paid", "overdue", "all"].contains(&s))
            || as_of.is_some_and(|s| s.len() > 32)
        {
            return Err(Error::InvalidInput("invalid payable status/asOf filter"));
        }
        let mut query = vec![];
        if let Some(s) = status {
            query.push(("status", s));
        }
        if let Some(s) = as_of {
            query.push(("asOf", s));
        }
        self.get(&company_path(company, "payables")?, &query).await
    }
    pub async fn expense_booking_options(
        &self,
        company: &str,
        document_id: i64,
    ) -> Result<Value, Error> {
        if document_id <= 0 {
            return Err(Error::InvalidInput("positive document id required"));
        }
        self.get(
            &company_path(company, &format!("documents/{document_id}/booking-options"))?,
            &[],
        )
        .await
    }
    /// Exact body, explicit confirmation, one attempt. No generic idempotency keys.
    /// On uncertainty reconcile bank/payables/journal/documents before further action.
    pub async fn bank_mutation(
        &self,
        company: &str,
        operation: BankMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit bank/payable mutation confirmation required",
            ));
        }
        if matches!(operation, BankMutation::ImportStatement) {
            import_body(&bytes)?;
        } else {
            body(&bytes, &bank_body_schema(operation))?;
        }
        let path = match operation {
            BankMutation::CreateAccount => "bank-accounts".to_owned(),
            BankMutation::ImportStatement => "bank/import".into(),
            BankMutation::RegisterPayable => "payables".into(),
            BankMutation::PayPayable(id) => {
                if id <= 0 {
                    return Err(Error::InvalidInput("positive payable id required"));
                }
                format!("payables/{id}/pay")
            }
            BankMutation::MatchExpense => {
                if bytes.len() > 16 * 1024 {
                    return Err(Error::InvalidInput(
                        "expense match exceeds native16384-byte body limit",
                    ));
                }
                "documents/book-expense".into()
            }
        };
        self.request(
            Method::POST,
            &company_path(company, &path)?,
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
    fn bank_import_union_canonical_bytes_and_existing_admission_are_enforced() {
        for value in [
            json!({"csvContent":"date;text;amount\n2026-01-01;Synthetic;-1", "confirm":true}),
            json!({"csvBase64":"5vjlIIA=", "account":null,"profile":"danske-bank","confirm":true}),
            json!({"csvBase64":"Zg==", "account":"synthetic","confirm":true}),
        ] {
            assert!(import_body(&serde_json::to_vec(&value).unwrap()).is_ok());
        }
        for value in [
            json!({"confirm":true}),
            json!({"csvContent":"x","csvBase64":"eA==","confirm":true}),
            json!({"csvContent":null,"confirm":true}),
            json!({"csvBase64":null,"confirm":true}),
            json!({"csvContent":1,"confirm":true}),
            json!({"csvBase64":1,"confirm":true}),
            json!({"csvContent":"","confirm":true}),
            json!({"csvBase64":"","confirm":true}),
            json!({"csvBase64":"eA==","confirm":false}),
            json!({"csvBase64":"eA==","confirm":true,"actor":"forged"}),
            json!({"csvBase64":"eA==","confirm":true,"idempotencyKey":"forged"}),
            json!({"csvBase64":"eA==","confirm":true,"profile":"Danske-Bank"}),
            json!({"csvBase64":"eA==","confirm":true,"account":1}),
        ] {
            assert!(
                import_body(&serde_json::to_vec(&value).unwrap()).is_err(),
                "{value}"
            );
        }
        for encoded in [
            "Zg", "Zh==", "Zm9=", "Zg===", "Zg==\n", "Zg--", "Zg__", "====", "=AAA", "AA=A", "å===",
        ] {
            assert!(!bounded_base64(encoded), "{encoded}");
        }
        assert!(bounded_base64("Zm8="));
        assert!(bounded_base64("Zm9v"));
        assert!(!bounded_base64(
            &"A".repeat(MAX_BANK_ORIGINAL.div_ceil(3) * 4)
        ));
        assert!(import_body(&vec![b' '; MAX_BANK_IMPORT_BODY + 1]).is_err());
        let oversized = json!({"csvContent":"x".repeat(MAX_BANK_ORIGINAL + 1),"confirm":true});
        assert!(import_body(&serde_json::to_vec(&oversized).unwrap()).is_err());
    }
}
