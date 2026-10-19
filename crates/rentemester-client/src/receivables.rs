//! Native invoice cash/credit operations. No accounting arithmetic or invented replay keys.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug)]
pub enum ReceivablesMutation {
    Settle,
    Credit,
    Refund,
}
impl ReceivablesMutation {
    fn path(self) -> &'static str {
        match self {
            Self::Settle => "invoices/settle",
            Self::Credit => "invoices/credit-note",
            Self::Refund => "invoices/refund",
        }
    }
}
/// Exact native shape; the server validates originals, imported bank evidence, balances,
/// currencies, periods, original account identities and the current principal.
pub fn receivables_body_schema(operation: ReceivablesMutation) -> Value {
    let date = json!({"type":["string","null"],"pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
    let amount = json!({"type":["number","null"],"exclusiveMinimum":0});
    if matches!(operation, ReceivablesMutation::Credit) {
        return object(
            json!({"invoiceDocumentId":{"type":"integer","minimum":1},"issueDate":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},"reason":{"type":"string","minLength":1,"maxLength":2000},"grossAmount":{"type":["number","null"],"exclusiveMinimum":0,"description":"Partial gross DKK credit. Omit/null to credit the server-established remaining amount; no client calculation."},"creditNoteNumber":{"type":["string","null"],"maxLength":256},"confirm":{"const":true}}),
            &["invoiceDocumentId", "issueDate", "reason", "confirm"],
        );
    }
    let mut fields = json!({"invoiceDocumentId":{"type":"integer","minimum":1},"bankTransactionId":{"type":["integer","null"],"minimum":1},"bankTransactionReference":{"type":["string","null"],"minLength":1,"maxLength":1024},"amount":amount,"confirm":{"const":true}});
    fields[if matches!(operation, ReceivablesMutation::Refund) {
        "refundDate"
    } else {
        "paymentDate"
    }] = date;
    let mut schema = object(fields, &["invoiceDocumentId", "confirm"]);
    schema["oneOf"] = json!([
     {"required":["bankTransactionId"],"properties":{"bankTransactionId":{"type":"integer"},"bankTransactionReference":{"type":"null"}}},
     {"required":["bankTransactionReference"],"properties":{"bankTransactionReference":{"type":"string"},"bankTransactionId":{"type":"null"}}}
    ]);
    schema
}
impl ApiClient {
    /// Forward exact caller bytes once. These routes have no server idempotency receipts.
    /// Never retry automatically, particularly a partial credit (which could create another
    /// valid credit). On OutcomeUnknown inspect invoices, imported bank rows, documents and
    /// journal before any explicit further action. Readback is not a generic retry permit.
    pub async fn receivables_mutation(
        &self,
        company: &str,
        operation: ReceivablesMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit receivables confirmation required",
            ));
        }
        body(&bytes, &receivables_body_schema(operation))?;
        if !matches!(operation, ReceivablesMutation::Credit) {
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|_| Error::InvalidInput("native JSON body required"))?;
            let id = value["bankTransactionId"].as_i64().is_some();
            let reference = value["bankTransactionReference"].as_str();
            if reference.is_some_and(|s| s.trim().is_empty()) || id == reference.is_some() {
                return Err(Error::InvalidInput(
                    "exactly one bank transaction id or nonempty reference required",
                ));
            }
        }
        self.request(
            Method::POST,
            &company_path(company, operation.path())?,
            &[],
            Some(bytes),
        )
        .await
    }
}
