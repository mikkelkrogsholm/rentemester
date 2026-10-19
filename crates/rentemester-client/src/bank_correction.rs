//! Evidence-only correction over a server-proven reversal/replacement chain and durable caller key.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_BANK_CORRECTION_BODY: usize = 16 * 1024;
pub fn bank_correction_body_schema() -> Value {
    let id = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    object(
        json!({"bankTransactionId":id,"replacementJournalEntryId":id,"expectedReconciliationId":{"type":"string","minLength":1,"maxLength":128,"description":"Exact currently reviewed reconciliationId from server plan"},"planHash":{"type":"string","minLength":64,"maxLength":64,"description":"Exact reviewed server correction planHash, never computed here"},"reason":{"type":"string","minLength":1,"description":"Exact raw reason is part of the keyed payload; server stores trimmed nonblank reason ≤1000 Unicode scalar chars"},"idempotencyKey":{"type":"string","minLength":1,"description":"Caller supplied, supported durable operation key. Server permits ≤128 UTF-16 code units, nonblank ECMAScript whitespace; no charset restriction. Never trimmed, generated, replaced or automatically retried."},"confirm":{"const":true}}),
        &[
            "bankTransactionId",
            "replacementJournalEntryId",
            "expectedReconciliationId",
            "planHash",
            "reason",
            "idempotencyKey",
            "confirm",
        ],
    )
}
impl ApiClient {
    pub async fn bank_reconciliation_correction_plan(
        &self,
        company: &str,
        bank_id: i64,
        replacement_id: i64,
    ) -> Result<Value, Error> {
        if ![bank_id, replacement_id]
            .iter()
            .all(|n| (1..=9_007_199_254_740_991).contains(n))
        {
            return Err(Error::InvalidInput(
                "positive exact bank/replacement journal identifiers required",
            ));
        }
        self.get(
            &company_path(company, "bank/reconciliation-correction-plan")?,
            &[
                ("bankTransactionId", &bank_id.to_string()),
                ("replacementJournalEntryId", &replacement_id.to_string()),
            ],
        )
        .await
    }
    /// Exact body/key once. Explicit replay may recover the same principal's durable receipt under
    /// current rights/source/original checks. Unknown outcomes require read-first reconciliation.
    pub async fn correct_bank_reconciliation(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit correction confirmation required",
            ));
        }
        if bytes.len() > MAX_BANK_CORRECTION_BODY {
            return Err(Error::InvalidInput(
                "bank correction exceeds native 16KiB body limit",
            ));
        }
        body(&bytes, &bank_correction_body_schema())?;
        self.request(
            Method::POST,
            &company_path(company, "bank/reconciliation-correction")?,
            &[],
            Some(bytes),
        )
        .await
    }
}
