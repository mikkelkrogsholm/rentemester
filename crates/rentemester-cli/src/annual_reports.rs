//! Explicit read selectors, with no body, confirmation or local artifact writes.
use rentemester_client::{ApiClient, valid_annual_bounds};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::collections::BTreeMap;
type Failure = (u8, Error);
fn invalid(message: &str) -> Failure {
    (2, Error::new("invalid_input", message))
}
pub fn run(
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview_reviewed: bool,
) -> Result<Value, Failure> {
    if confirmed
        || preview_reviewed
        || flags.keys().any(|k| {
            ![
                "--server",
                "--cookie-file",
                "--company",
                "--fiscal-year-start",
                "--fiscal-year-end",
            ]
            .contains(k)
        })
    {
        return Err(invalid(
            "annual-report is read-only; only server/cookie-file/company/fiscal-year-start/fiscal-year-end flags are accepted",
        ));
    }
    let start = flags
        .get("--fiscal-year-start")
        .ok_or_else(|| invalid("annual-report requires --fiscal-year-start YYYY-MM-DD"))?;
    let end = flags
        .get("--fiscal-year-end")
        .ok_or_else(|| invalid("annual-report requires --fiscal-year-end YYYY-MM-DD"))?;
    if !valid_annual_bounds(start, end) {
        return Err(invalid(
            "annual-report requires ascending real ISO fiscal dates",
        ));
    }
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("annual-report requires explicit --company"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server origin required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("annual-report requires --cookie-file"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api =
        ApiClient::new(server, allow_http, Some(cookie)).map_err(crate::remote::client_error)?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let response = runtime
        .block_on(api.annual_report(company, start, end))
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":response.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":response}))
}
