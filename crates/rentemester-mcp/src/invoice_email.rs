use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, InvoiceEmailMutation as Op, MAX_INVOICE_EMAIL_BODY, invoice_email_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "invoice_email_status"
            | "invoice_email_send"
            | "invoice_email_reconcile"
            | "invoice_email_retry"
    )
}
pub fn tools() -> Vec<Value> {
    let mut result = vec![tool(
        "invoice_email_status",
        "Current CompanyRead canonical invoice-email configuration/history. GET never dispatches or changes issue/post/send state.",
        json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false}),
        true,
    )];
    for (name, action) in [
        ("invoice_email_send", Op::Send),
        ("invoice_email_reconcile", Op::Reconcile),
        ("invoice_email_retry", Op::Retry),
    ] {
        let mut body = native_body(invoice_email_body_schema(action));
        body["maxLength"] = json!(MAX_INVOICE_EMAIL_BODY);
        result.push(tool(name,"Current verified human MFA and CompanyExternalSend. Exact native16KiB JSON, inner+outer confirm, explicit recipient or exact stateHash/messageId/reason; reconciliation additionally external evidence and resolution. Immutable stored PDF only; issue/post/send separate. Known delivery_unknown409/delivery_refused422 retain canonical delivery with ok:false. Reconciliation never sends; retry only after definite nonacceptance. No credentials, invented hashes/defaults/keys, automatic retry or blind resend; unknown requires status/external reconciliation.",json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    result
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
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
    if name == "invoice_email_status" {
        let v: Read =
            serde_json::from_value(args).map_err(|_| "invoice status accepts company only")?;
        return Ok(api.invoice_email_status(&v.company).await);
    }
    let v: Write = serde_json::from_value(args)
        .map_err(|_| "invoice email write accepts company/body/confirm only")?;
    if v.body.len() > MAX_INVOICE_EMAIL_BODY {
        return Err("invoice email body exceeds native16KiB");
    }
    let bytes = v.body.into_bytes();
    Ok(match name {
        "invoice_email_send" => api.invoice_email_send(&v.company, bytes, v.confirm).await,
        "invoice_email_reconcile" => {
            api.invoice_email_reconcile(&v.company, bytes, v.confirm)
                .await
        }
        "invoice_email_retry" => api.invoice_email_retry(&v.company, bytes, v.confirm).await,
        _ => return Err("unknown invoice email operation"),
    })
}
