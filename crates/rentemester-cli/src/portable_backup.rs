//! Confirmed remote workspace backup; no stdin envelope or local artifact writes.
use rentemester_client::ApiClient;
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub fn run(
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    let invalid = || {
        (
            2,
            Error::new(
                "invalid_input",
                "workspace-backup requires --server, --cookie-file and bare --confirm only; no company/year/id/body/path/key selectors or stdin body",
            ),
        )
    };
    if !confirm
        || preview
        || flags
            .keys()
            .any(|k| !["--server", "--cookie-file"].contains(k))
    {
        return Err(invalid());
    }
    let server = flags.get("--server").ok_or_else(invalid)?;
    let cookie = flags.get("--cookie-file").ok_or_else(invalid)?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let rt = tokio::runtime::Runtime::new().map_err(|_| {
        (
            2,
            Error::new("invalid_input", "could not start HTTP runtime"),
        )
    })?;
    let data = rt
        .block_on(api.workspace_backup(confirm))
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":true,"data":data}))
}
