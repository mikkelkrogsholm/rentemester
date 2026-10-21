//! Explicit latest-year Dinero commands; exact sources and keys remain caller-owned.
use rentemester_client::{ApiClient, MAX_DINERO_IMPORT_BODY};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
pub const COMMANDS: [&str; 5] = [
    "dinero-import-plan",
    "dinero-import-apply",
    "dinero-imports",
    "dinero-import-original",
    "imported-receivables",
];
pub fn accepts(name: &str) -> bool {
    COMMANDS.contains(&name)
}
pub fn help() -> Value {
    json!({"commands":COMMANDS,"input":"Explicit --server/--cookie-file/--company. dinero-import-plan reads exact native35MiB stdin JSON {fileName,contentBase64}; dinero-import-apply additionally --confirm and exact body planHash/idempotencyKey/confirm:true. Server atomically imports reviewed latest-year journals and source schedules. imported-receivables requires --as-of YYYY-MM-DD; dinero-import-original --attempt-id ID returns exact original application/json integer bytes. No parsing/math/generatedkeys/automatic retry; unknown apply requires canonical dinero-imports/source-hash reconciliation before deliberate exact-key action.","planSchema":rentemester_client::dinero_import_body_schema(false),"applySchema":rentemester_client::dinero_import_body_schema(true)})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    let invalid = |s| (2, Error::new("invalid_input", s));
    let apply = command == "dinero-import-apply";
    let source = command == "dinero-import-plan" || apply;
    if confirmed != apply
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k)
                || *k == "--as-of" && command == "imported-receivables"
                || *k == "--attempt-id" && command == "dinero-import-original")
        })
    {
        return Err(invalid(
            "exact documented dinero selectors; only apply requires confirmation",
        ));
    }
    let required = |k| {
        flags.get(k).copied().ok_or_else(|| {
            invalid("explicit server/cookie/company and documented selector required")
        })
    };
    let server = required("--server")?;
    let company = required("--company")?;
    let cookie =
        rentemester_client::read_cookie_file(std::path::Path::new(required("--cookie-file")?))
            .map_err(crate::remote::client_error)?;
    let api =
        ApiClient::new(server, allow_http, Some(cookie)).map_err(crate::remote::client_error)?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let value = if source {
        let mut bytes = vec![];
        std::io::stdin()
            .take(MAX_DINERO_IMPORT_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read native dinero JSON"))?;
        runtime.block_on(api.dinero_import(company, bytes, apply, confirmed))
    } else if command == "dinero-imports" {
        runtime.block_on(api.dinero_imports(company))
    } else if command == "imported-receivables" {
        runtime.block_on(api.imported_receivables(company, required("--as-of")?))
    } else {
        let id = required("--attempt-id")?
            .parse::<i64>()
            .map_err(|_| invalid("positive safe import ID required"))?;
        runtime.block_on(api.dinero_import_original(company, id))
    }
    .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}
