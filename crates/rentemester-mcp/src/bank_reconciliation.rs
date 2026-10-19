//! Reviewed link-only bank reconciliation, with server-supplied proof and current authority.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, MAX_BANK_RECONCILIATION_BODY, bank_reconciliation_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let mut body = native_body(bank_reconciliation_body_schema());
    body["maxLength"] = json!(MAX_BANK_RECONCILIATION_BODY);
    vec![
        tool(
            "bank_reconciliation_candidates",
            "Read actual current plans for one imported bank row. Requires CompanyRead. Inspect sourceHash/transactionHash, mapped bank movement, journal/document proof and server planHash; select explicitly, never invent a hash. No effects. Preserves server correctionAvailable capability without adding correction operations.",
            json!({"type":"object","required":["company","bankTransactionId"],"properties":{"company":company_schema(),"bankTransactionId":{"type":"integer","minimum":1,"maximum":9007199254740991u64}},"additionalProperties":false}),
            true,
        ),
        tool(
            "bank_reconciliation_link",
            "Link imported bank evidence to an existing journal after reviewing exact current candidate proof. CompanyLedgerPost required. Exact native body once (16KiB); inner and outer confirm:true. Server rechecks archived statement/current facts; no new journals/payables/payments. No correction, invented keys/hash/actor or automatic retry. Duplicate/stale definite refusals; on outcome_unknown read canonical bank reconciliationId/proof and candidates before action, never blindly resend.",
            json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),
            false,
        ),
    ]
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Read {
    company: String,
    bank_transaction_id: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    company: String,
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "bank_reconciliation_candidates" {
        let input:Read=serde_json::from_value(args).map_err(|_|"reconciliation candidates require only company and positive exact bankTransactionId")?;
        if !(1..=9_007_199_254_740_991).contains(&input.bank_transaction_id) {
            return Err("positive exact bankTransactionId required");
        }
        return Ok(api
            .bank_reconciliation_candidates(&input.company, input.bank_transaction_id)
            .await);
    }
    let input: Write = serde_json::from_value(args)
        .map_err(|_| "reconciliation link requires company, exact body string and confirmation")?;
    if input.body.len() > MAX_BANK_RECONCILIATION_BODY {
        return Err("reconciliation body exceeds native16KiB");
    }
    Ok(api
        .link_bank_journal(&input.company, input.body.into_bytes(), input.confirm)
        .await)
}
