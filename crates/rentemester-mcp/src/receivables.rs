//! Exact invoice settlement/credit/refund tools over the shared HTTP client.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, Error, ReceivablesMutation, receivables_body_schema};
use serde::Deserialize;
use serde_json::{Value, json};
fn operation(name: &str) -> Option<ReceivablesMutation> {
    match name {
        "settle_invoice" => Some(ReceivablesMutation::Settle),
        "credit_invoice" => Some(ReceivablesMutation::Credit),
        "refund_invoice" => Some(ReceivablesMutation::Refund),
        _ => None,
    }
}
pub fn accepts(name: &str) -> bool {
    name == "invoice_pdf" || operation(name).is_some()
}
pub fn tools() -> Vec<Value> {
    let mut tools:Vec<Value> = [
 ("settle_invoice",ReceivablesMutation::Settle,"Apply one positive imported DKK bank receipt to an active posted invoice, up to its open principal. Original booked debtor and current compatible bank mapping are used. Exactly one bankTransactionId or unambiguous reference; optional amount/paymentDate must equal source row. No overpayment/claims/bad-debt flow"),
 ("credit_invoice",ReceivablesMutation::Credit,"Issue and post a credit note against original accounts of an active standard domestic25%DKK invoice. grossAmount is a partial gross credit; omit/null for the server-established remaining full credit. Requires issueDate and reason; no client VAT/remaining-balance arithmetic"),
 ("refund_invoice",ReceivablesMutation::Refund,"Apply one outgoing imported DKK bank row to an invoice's refundable credit balance, using original debtor/current bank mapping. Exactly one bankTransactionId or unambiguous reference; optional amount/refundDate must equal source row. Cannot refund beyond established credit balance")
 ].into_iter().map(|(name,op,description)|tool(name,&format!("{description}. Current company.ledger.post permission required. Exact native body forwarded once with explicit confirmation; no server idempotency keys or automatic retries. On outcome_unknown inspect invoices/bank/documents/journal before further action; a read is not permission to resend a partial credit."),json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":native_body(receivables_body_schema(op)),"confirm":{"const":true}},"additionalProperties":false}),false)).collect();
    tools.push(tool("invoice_pdf","Read the immutable issued invoice original by positive document id from invoices/documents. Requires current company.documents.read permission. Returns {pdf:{contentType,bytes}} with exact PDF byte integers; creates no local files or invoice/ledger effects. No body or confirmation.",json!({"type":"object","required":["company","id"],"properties":{"company":company_schema(),"id":{"type":"integer","minimum":1}},"additionalProperties":false}),true));
    tools
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PdfRead {
    company: String,
    id: i64,
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
    if name == "invoice_pdf" {
        let input: PdfRead = serde_json::from_value(args)
            .map_err(|_| "invoice_pdf requires only company and positive document id")?;
        if input.id <= 0 {
            return Err("invoice_pdf requires positive document id");
        }
        return Ok(api.invoice_pdf(&input.company, input.id).await);
    }
    let operation = operation(name).ok_or("unknown receivables tool")?;
    let input: Write = serde_json::from_value(args).map_err(
        |_| "receivables tools require company, exact native body string and confirmation",
    )?;
    Ok(api
        .receivables_mutation(
            &input.company,
            operation,
            input.body.into_bytes(),
            input.confirm,
        )
        .await)
}
