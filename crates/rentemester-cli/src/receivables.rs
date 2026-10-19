//! Remote invoice lifecycle command names/help. The shared client owns every request.
use rentemester_client::{ReceivablesMutation, receivables_body_schema};
use serde_json::{Value, json};
pub fn operation(command: &str) -> Option<ReceivablesMutation> {
    match command {
        "settle-invoice" => Some(ReceivablesMutation::Settle),
        "credit-invoice" => Some(ReceivablesMutation::Credit),
        "refund-invoice" => Some(ReceivablesMutation::Refund),
        _ => None,
    }
}
pub fn help() -> Value {
    json!({"commands":["settle-invoice","credit-invoice","refund-invoice"],"usage":"remote COMMAND --server ORIGIN --cookie-file PRIVATE_COOKIE --company SLUG --confirm < native-body.json","policy":"Current ledger-post permission and body confirm:true required. Settle/refund select exactly one imported bank row by id or unambiguous reference. Optional amount/date must equal source amount/date; native cash flow is DKK. credit-invoice supports standard domestic25%DKK invoice only: grossAmount creates a partial credit; omit it for the remaining full credit. No automatic retries or server idempotency keys. After outcome_unknown inspect invoices/bank/documents/journal; successful reads do not authorize repeating a partial credit. Existing native server owns all accounting and original account identities.","bodySchemas":{"settle":receivables_body_schema(ReceivablesMutation::Settle),"credit":receivables_body_schema(ReceivablesMutation::Credit),"refund":receivables_body_schema(ReceivablesMutation::Refund)}})
}
