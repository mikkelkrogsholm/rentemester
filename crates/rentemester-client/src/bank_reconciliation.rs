//! Link reviewed existing accounting to archived bank evidence. Never creates accounting or proof.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_BANK_RECONCILIATION_BODY: usize = 16 * 1024;
pub fn bank_reconciliation_body_schema() -> Value {
    let id = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    object(
        json!({"bankTransactionId":id,"journalEntryId":id,"planHash":{"type":"string","minLength":64,"maxLength":64,"description":"Exact server planHash from reviewed current candidates; never calculated or invented"},"note":{"type":"string","minLength":1,"maxLength":1000,"description":"Human review note; server requires nonblank trimmed note"},"confirm":{"const":true}}),
        &[
            "bankTransactionId",
            "journalEntryId",
            "planHash",
            "note",
            "confirm",
        ],
    )
}
impl ApiClient {
    pub async fn bank_reconciliation_candidates(
        &self,
        company: &str,
        bank_id: i64,
    ) -> Result<Value, Error> {
        if !(1..=9_007_199_254_740_991).contains(&bank_id) {
            return Err(Error::InvalidInput(
                "positive exact bankTransactionId required",
            ));
        }
        self.get(
            &company_path(company, "bank/reconciliation-candidates")?,
            &[("bankTransactionId", &bank_id.to_string())],
        )
        .await
    }
    /// Exact16KiB native body, once. Server rechecks archived source/current facts and selected proof.
    /// Duplicate/stale refusals are definite; uncertain outcome needs canonical bank/plan reads, never retry.
    pub async fn link_bank_journal(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit bank reconciliation confirmation required",
            ));
        }
        if bytes.len() > MAX_BANK_RECONCILIATION_BODY {
            return Err(Error::InvalidInput(
                "bank reconciliation exceeds native16KiB body limit",
            ));
        }
        body(&bytes, &bank_reconciliation_body_schema())?;
        self.request(
            Method::POST,
            &company_path(company, "bank/reconciliation-link")?,
            &[],
            Some(bytes),
        )
        .await
    }
}
