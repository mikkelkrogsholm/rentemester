//! Explicit closed-history archive commands; exact sources and keys remain caller-owned.
use rentemester_client::{ApiClient, MAX_ARCHIVE_IMPORT_BODY};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
pub const COMMANDS: [&str; 4] = [
    "archive-import-plan",
    "archive-import-apply",
    "archives",
    "archive-import-original",
];
pub fn accepts(name: &str) -> bool {
    COMMANDS.contains(&name)
}
pub fn help() -> Value {
    json!({"commands":COMMANDS,"input":"Explicit --server/--cookie-file/--company. archive-import-plan reads exact native35MiB stdin JSON {fileName,contentBase64}; archive-import-apply additionally --confirm and exact body planHash/idempotencyKey/confirm:true. Server parses reviewed history only, never live migration. archives optional --fiscal-year YYYY; archive-import-original --import-id ID returns exact original application/json integer bytes. No parsing/math/generatedkeys/automatic retry; unknown apply requires canonical archives/source-hash reconciliation before deliberate exact-key action.","planSchema":rentemester_client::archive_import_body_schema(false),"applySchema":rentemester_client::archive_import_body_schema(true)})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    let invalid = |s| (2, Error::new("invalid_input", s));
    let apply = command == "archive-import-apply";
    let source = command == "archive-import-plan" || apply;
    if confirmed != apply
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k)
                || *k == "--fiscal-year" && command == "archives"
                || *k == "--import-id" && command == "archive-import-original")
        })
    {
        return Err(invalid(
            "exact documented archive selectors; only apply requires confirmation",
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
            .take(MAX_ARCHIVE_IMPORT_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read native archive JSON"))?;
        runtime.block_on(api.archive_import(company, bytes, apply, confirmed))
    } else if command == "archives" {
        let year = flags
            .get("--fiscal-year")
            .map(|y| {
                y.parse::<u16>()
                    .map_err(|_| invalid("four-digit fiscal year required"))
            })
            .transpose()?;
        runtime.block_on(api.archives(company, year))
    } else {
        let id = required("--import-id")?
            .parse::<i64>()
            .map_err(|_| invalid("positive safe import ID required"))?;
        runtime.block_on(api.archive_import_original(company, id))
    }
    .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}
