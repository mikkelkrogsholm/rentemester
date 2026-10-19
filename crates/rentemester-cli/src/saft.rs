//! Explicit audited export, stdout JSON binary envelope, no filesystem writes.
use rentemester_client::{ApiClient, MAX_SAFT_BODY};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn run(
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    if !confirm
        || preview
        || flags
            .keys()
            .any(|k| !["--server", "--cookie-file", "--company"].contains(k))
    {
        return Err(invalid(
            "saft-export requires --confirm, server/cookie-file/company and exact native stdin body only",
        ));
    }
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("explicit --company required"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private --cookie-file required"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = vec![];
    std::io::stdin()
        .take(MAX_SAFT_BODY as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("could not read SAF-T body"))?;
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let result = rt
        .block_on(api.saft_export(company, bytes, confirm))
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":true,"data":result}))
}
