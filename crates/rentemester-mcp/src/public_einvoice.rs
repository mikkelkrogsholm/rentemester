//! Exact native public-invoice commands, not a provider client or delivery-success inference.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, MAX_PUBLIC_INVOICE_BODY, PublicInvoiceMutation as Action,
    public_invoice_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "public_invoice_config"
            | "public_invoice_submissions"
            | "register_public_invoice"
            | "send_public_invoice"
            | "refresh_public_invoice_status"
    )
}
pub fn tools() -> Vec<Value> {
    let mut tools = vec![];
    for (name, description) in [
        (
            "public_invoice_config",
            "Read safe public-invoice configured/environment/registered/outbound state; no credentials, provider I/O or effects.",
        ),
        (
            "public_invoice_submissions",
            "Read full current authorized canonical public-invoice submissions and history, retaining transmissionId/redeliveryBlocked and source proof. No provider refresh/send effects; use after uncertainty.",
        ),
    ] {
        tools.push(tool(name,description,json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false}),true));
    }
    for (name, action) in [
        ("register_public_invoice", Action::Register),
        ("send_public_invoice", Action::Send),
        ("refresh_public_invoice_status", Action::RefreshStatus),
    ] {
        let mut body = native_body(public_invoice_body_schema(action));
        body["maxLength"] = json!(MAX_PUBLIC_INVOICE_BODY);
        tools.push(tool(name,"Explicit current ExternalSend/fresh-MFA operation; exact native16KiB body once with inner and outer confirm:true. Register accepts {confirm:true}; send/status accept {invoiceDocumentId,confirm:true}. Fixed server/operator configuration only; no credentials, mutable contact overrides, generated keys, automatic retries or posting. HTTP200 is not delivery: prepared means accepted/queued, acknowledged confirmed, failed accepted terminal, uncertain reserved without ID, legacy historical. Preserve transmissionId/redeliveryBlocked and canonical JSON. After transport outcome_unknown read public_invoice_submissions/config before any separate explicit action; never blindly resend.",json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    tools
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
    if matches!(name, "public_invoice_config" | "public_invoice_submissions") {
        let i: Read = serde_json::from_value(args)
            .map_err(|_| "public-invoice read requires only company")?;
        return Ok(if name == "public_invoice_config" {
            api.public_invoice_config(&i.company).await
        } else {
            api.public_invoice_submissions(&i.company).await
        });
    }
    let i: Write = serde_json::from_value(args)
        .map_err(|_| "public-invoice write requires only company/exact body/confirm:true")?;
    let action = match name {
        "register_public_invoice" => Action::Register,
        "send_public_invoice" => Action::Send,
        "refresh_public_invoice_status" => Action::RefreshStatus,
        _ => return Err("unknown public-invoice operation"),
    };
    Ok(api
        .mutate_public_invoice(&i.company, action, i.body.into_bytes(), i.confirm)
        .await)
}
