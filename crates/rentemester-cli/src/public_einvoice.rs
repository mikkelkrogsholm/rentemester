//! Explicit public invoice provider operations. Credentials and mutable business facts remain server-owned.
use rentemester_client::{
    ApiClient, MAX_PUBLIC_INVOICE_BODY, PublicInvoiceMutation as Action, public_invoice_body_schema,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
pub const COMMANDS: [&str; 5] = [
    "public-invoice-config",
    "public-invoice-submissions",
    "register-public-invoice",
    "send-public-invoice",
    "refresh-public-invoice-status",
];
pub fn accepts(command: &str) -> bool {
    COMMANDS.contains(&command)
}
pub fn help() -> Value {
    json!({"commands":COMMANDS,"usage":"public-invoice-config/public-invoice-submissions --company SLUG; register-public-invoice/send-public-invoice/refresh-public-invoice-status --company SLUG --confirm < native.json. Explicit --server and private MFA --cookie-file required.","policy":"Reads never contact provider or send. Writes exact16KiB JSON with inner+outer confirm:true and current ExternalSend/fresh MFA. Registration body is {confirm:true}; send/status body {invoiceDocumentId,confirm:true}. Credentials, environment and company key are operator-owned, never arguments. Issue, post and network handoff are separate. HTTP200 does not mean delivered: prepared is accepted/queued, acknowledged is confirmed, failed is accepted terminal, uncertain is reserved without an ID; legacy remains historical. Preserve canonical transmissionId/redeliveryBlocked; never automatically resend/retry or invent a key. After outcome_unknown inspect public-invoice-submissions/config before deciding any separate explicit action.","bodySchemas":{"register":public_invoice_body_schema(Action::Register),"send":public_invoice_body_schema(Action::Send),"refreshStatus":public_invoice_body_schema(Action::RefreshStatus)}})
}
fn invalid() -> Failure {
    (
        2,
        Error::new(
            "invalid_input",
            "expected fixed public-invoice command with explicit server/company/private cookie; writes require --confirm and exact bounded stdin JSON, reads accept no confirmation/body/selectors",
        ),
    )
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = matches!(
        command,
        "public-invoice-config" | "public-invoice-submissions"
    );
    if !accepts(command)
        || confirmed == read
        || preview
        || flags
            .keys()
            .any(|k| !["--server", "--cookie-file", "--company"].contains(k))
    {
        return Err(invalid());
    }
    let get = |key| flags.get(key).copied().ok_or_else(invalid);
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(get("--cookie-file")?))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(get("--server")?, allow, Some(cookie))
        .map_err(crate::remote::client_error)?;
    let company = get("--company")?;
    let mut bytes = Vec::new();
    if !read {
        std::io::stdin()
            .take(MAX_PUBLIC_INVOICE_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid())?;
    }
    let runtime = tokio::runtime::Runtime::new().map_err(|_| invalid())?;
    let value = runtime.block_on(async {
        match command {
            "public-invoice-config" => api.public_invoice_config(company).await,
            "public-invoice-submissions" => api.public_invoice_submissions(company).await,
            _ => {
                let action = match command {
                    "register-public-invoice" => Action::Register,
                    "send-public-invoice" => Action::Send,
                    _ => Action::RefreshStatus,
                };
                api.mutate_public_invoice(company, action, bytes, confirmed)
                    .await
            }
        }
    });
    if value == Err(rentemester_client::Error::OutcomeUnknown) {
        return Ok(
            json!({"ok":false,"code":"outcome_unknown","errors":["Public-invoice outcome unknown; inspect canonical submissions before any further action. No automatic replay or new submission key."]}),
        );
    }
    let value = value.map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}
