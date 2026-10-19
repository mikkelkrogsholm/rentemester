//! Explicit contact-only file import, without provider secrets or client parsing.
use rentemester_client::{ApiClient, MAX_CONTACT_IMPORT_BODY};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
pub fn accepts(name: &str) -> bool {
    name == "import-contacts"
}
pub fn help() -> Value {
    json!({"commands":["import-contacts"],"input":"Exact native12MiB stdin JSON {fileName,content,enrichCvr?,confirm:true}; explicit --server/--cookie-file/--company/--confirm. Dinero contacts only. No local CSV parsing/provider configuration/keys/automatic retry. Full partial-row summary and errors retained. Unknown outcome: inspect canonical contacts before a new explicit import; natural-key dedup is not a receipt replay guarantee.","bodySchema":rentemester_client::contact_import_body_schema()})
}
pub fn run(
    _command: &str,
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    let invalid = |s| (2, Error::new("invalid_input", s));
    if !confirmed
        || preview
        || flags
            .keys()
            .any(|k| !["--server", "--cookie-file", "--company"].contains(k))
    {
        return Err(invalid(
            "import-contacts requires --confirm and exact confirmed JSON; no other selectors",
        ));
    }
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let file = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("explicit --cookie-file required"))?;
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("explicit --company required"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(file))
        .map_err(crate::remote::client_error)?;
    let api =
        ApiClient::new(server, allow_http, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(MAX_CONTACT_IMPORT_BODY as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("could not read exact import JSON"))?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let value = runtime
        .block_on(api.import_contacts(company, bytes, confirmed))
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}
