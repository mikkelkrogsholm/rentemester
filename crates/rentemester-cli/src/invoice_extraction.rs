//! Native extraction compatibility aliases under the existing remote parser.
use rentemester_client::ApiClient;
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::collections::BTreeMap;
type Failure = (u8, Error);
pub const COMMANDS: [&str; 2] = ["extract-invoice", "invoice-extraction"];
pub fn accepts(command: &str) -> bool {
    COMMANDS.contains(&command)
}
pub fn help() -> Value {
    json!({"ok":true,"usage":"remote extract-invoice --server ORIGIN [--allow-loopback-http] --cookie-file PATH --company SLUG --document-id N --confirm; remote invoice-extraction --server ORIGIN [--allow-loopback-http] --cookie-file PATH --company SLUG --document-id N",
        "scope":"Explicit registered-PDF extraction and retained evidence on the selected Rust server. No local fallback, metadata inference or accounting entries. Server session determines actor and current MasterData authority for extraction; reads require document-read authority.",
        "confirmation":"Native boolean --confirm is required only for extract-invoice; no stdin body, provider, actor, path or idempotency key is accepted. Provider is operator-configured; missing provider is a known refusal and does not create an attempt.",
        "recovery":"Sent once with no automatic retries. After outcome_unknown, read remote invoice-extraction before deciding whether to make another explicit attempt. A started attempt may remain unresolved; a needs_resolution response is retained evidence, not a posting.",
        "output":"{ok,data:HTTPenvelope}; canonical extraction fields/conflicts/resolutions remain unchanged, including extraction:null and known {ok:false,code} refusals.",
        "commands":COMMANDS})
}
fn invalid() -> Failure {
    (
        2,
        Error::new(
            "invalid_input",
            "expected remote extract-invoice|invoice-extraction with explicit --server, --cookie-file, --company and positive safe --document-id; boolean --confirm only for extract-invoice",
        ),
    )
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let write = command == "extract-invoice";
    if !accepts(command)
        || confirmed != write
        || preview
        || flags
            .keys()
            .any(|key| !["--server", "--cookie-file", "--company", "--document-id"].contains(key))
    {
        return Err(invalid());
    }
    let required = |key| flags.get(key).copied().ok_or_else(invalid);
    let id = required("--document-id")?;
    if !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid());
    }
    let id = id
        .parse::<i64>()
        .ok()
        .filter(|n| (1..=9_007_199_254_740_991).contains(n))
        .ok_or_else(invalid)?;
    let server = required("--server")?;
    let company = required("--company")?;
    let cookie =
        rentemester_client::read_cookie_file(std::path::Path::new(required("--cookie-file")?))
            .map_err(crate::remote::client_error)?;
    let client =
        ApiClient::new(server, allow_http, Some(cookie)).map_err(crate::remote::client_error)?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let value = runtime
        .block_on(async {
            if write {
                client
                    .extract_document_invoice(company, id, confirmed)
                    .await
            } else {
                client.document_invoice_extraction(company, id).await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}
