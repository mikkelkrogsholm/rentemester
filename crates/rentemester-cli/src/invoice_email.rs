use rentemester_client::{
    ApiClient, InvoiceEmailMutation, MAX_INVOICE_EMAIL_BODY, invoice_email_body_schema,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn accepts(command: &str) -> bool {
    matches!(
        command,
        "invoice-email-status"
            | "invoice-email-send"
            | "invoice-email-reconcile"
            | "invoice-email-retry"
    )
}
pub fn help() -> Value {
    json!({"commands":["invoice-email-status","invoice-email-send","invoice-email-reconcile","invoice-email-retry"],"policy":"Explicit company and current human CompanyExternalSend for mutations, CompanyRead for status. Status GET never sends. Writes exact native16KiB stdin JSON with inner+outer confirm; immutable stored PDF, explicit recipient. Issue/post/send separate. Known409 unknown and422 refused preserve canonical delivery as ok:false; network/5xx outcome_unknown requires canonical status and external evidence. Reconciliation never sends; only separate reviewed retry after definitely not accepted may send. No SMTP credentials, default recipient, generated key/hash or automatic retry.","bodySchemas":{"invoice-email-send":invoice_email_body_schema(InvoiceEmailMutation::Send),"invoice-email-reconcile":invoice_email_body_schema(InvoiceEmailMutation::Reconcile),"invoice-email-retry":invoice_email_body_schema(InvoiceEmailMutation::Retry)}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = command == "invoice-email-status";
    if confirmed == read
        || preview
        || flags
            .keys()
            .any(|key| !(["--server", "--cookie-file"].contains(key) || *key == "--company"))
    {
        return Err(invalid(
            "command-specific selectors required; readonly has no confirmation and writes require exact confirmed stdin body",
        ));
    }
    let company = *flags
        .get("--company")
        .ok_or_else(|| invalid("explicit --company required"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let path = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private MFA --cookie-file required"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(path))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = vec![];
    if !read {
        std::io::stdin()
            .take(MAX_INVOICE_EMAIL_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact native body"))?;
    }
    let runtime =
        tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let result = runtime
        .block_on(async {
            match command {
                "invoice-email-status" => api.invoice_email_status(company).await,
                "invoice-email-send" => api.invoice_email_send(company, bytes, confirmed).await,
                "invoice-email-reconcile" => {
                    api.invoice_email_reconcile(company, bytes, confirmed).await
                }
                "invoice-email-retry" => api.invoice_email_retry(company, bytes, confirmed).await,
                _ => Err(rentemester_client::Error::InvalidInput(
                    "unknown invoice email operation",
                )),
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}
