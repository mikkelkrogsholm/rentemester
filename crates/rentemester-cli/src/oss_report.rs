//! Read dated OSS base without local tax calculation or submission.
use rentemester_client::ApiClient;
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub fn help() -> Value {
    json!({"usage":"remote oss-report --company SLUG --from YYYY-MM-DD --to YYYY-MM-DD --server ORIGIN --cookie-file PATH. Read-only explicitly classified OSS base and entry count, with underlying VAT report. No destination/rate split or OSS filing. No body, confirmation or actor."})
}
pub fn run(
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    let invalid = || {
        (
            2,
            Error::new(
                "invalid_input",
                "exact company/from/to and server/cookie flags required for read-only OSS report",
            ),
        )
    };
    let allowed = ["--server", "--cookie-file", "--company", "--from", "--to"];
    if confirmed || preview || flags.keys().any(|k| !allowed.contains(k)) {
        return Err(invalid());
    }
    let required = |key| flags.get(key).copied().ok_or_else(invalid);
    let company = required("--company")?;
    let from = required("--from")?;
    let to = required("--to")?;
    let cookie =
        rentemester_client::read_cookie_file(std::path::Path::new(required("--cookie-file")?))
            .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(required("--server")?, allow_http, Some(cookie))
        .map_err(crate::remote::client_error)?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "HTTP runtime unavailable")))?;
    let value = runtime
        .block_on(api.oss_report(company, from, to))
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}
