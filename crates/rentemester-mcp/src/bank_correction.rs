//! Server-owned canonical correction plan and principal-bound durable evidence receipt.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, Error, MAX_BANK_CORRECTION_BODY, bank_correction_body_schema};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let mut body = native_body(bank_correction_body_schema());
    body["maxLength"] = json!(MAX_BANK_CORRECTION_BODY);
    vec![tool(
        "bank_reconciliation_correction",
        "Append canonical evidence correction only; actual reversal/replacement accounting must already exist. CompanyLedgerPost; current principal/source/original/archive checks. Exact native 16KiB body with caller-supported idempotencyKey and inner/outer confirm:true. Raw reason/key preserved, never generated/trimmed. No automatic retries. On outcome_unknown inspect canonical bank/history first and preserve exact body/key; only explicit same-principal action can recover durable receipt. Changed payload/key/stale proof refuse. Returns full correction/idempotent/receipt; creates no journals/payments.",
        json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),
        false,
    )]
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Read {
    company: String,
    bank_transaction_id: i64,
    replacement_journal_entry_id: i64,
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
    if name == "bank_reconciliation_correction_plan" {
        let input:Read=serde_json::from_value(args).map_err(|_|"correction plan requires only company, bankTransactionId and replacementJournalEntryId")?;
        if ![
            input.bank_transaction_id,
            input.replacement_journal_entry_id,
        ]
        .iter()
        .all(|n| (1..=9_007_199_254_740_991).contains(n))
        {
            return Err("positive exact bank/replacement identifiers required");
        }
        return Ok(api
            .bank_reconciliation_correction_plan(
                &input.company,
                input.bank_transaction_id,
                input.replacement_journal_entry_id,
            )
            .await);
    }
    let input: Write = serde_json::from_value(args).map_err(
        |_| "correction requires only company, exact native body string and confirmation",
    )?;
    if input.body.len() > MAX_BANK_CORRECTION_BODY {
        return Err("correction body exceeds native 16KiB");
    }
    Ok(api
        .correct_bank_reconciliation(&input.company, input.body.into_bytes(), input.confirm)
        .await)
}
