//! One current company read under a human MFA session.
use rentemester_client::ApiClient;
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::collections::BTreeMap;
type Failure = (u8, Error);
fn invalid() -> Failure {
    (
        2,
        Error::new(
            "invalid_input",
            "backup-governance requires only --company SLUG, --server and private --cookie-file; no confirmation, body, time, service key or authority flags",
        ),
    )
}
pub fn help() -> Value {
    json!({"usage":"remote backup-governance --company SLUG --server HTTPS_ORIGIN --cookie-file PRIVATE_COOKIE","scope":"Current human CompanyAdmin. Read current local retained backup plus matching recorded placement and historical remote observation evidence. No provider request or mutation. Operator offsite declarations are not independently verified and recovery is not tested."})
}
pub fn run(
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    if confirm
        || preview
        || flags
            .keys()
            .any(|k| !["--server", "--cookie-file", "--company"].contains(k))
    {
        return Err(invalid());
    }
    let required = |key| flags.get(key).copied().ok_or_else(invalid);
    let company = required("--company")?;
    if company.is_empty()
        || !company
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(invalid());
    }
    let server = required("--server")?;
    let cookie_path = required("--cookie-file")?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie_path))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let runtime = tokio::runtime::Runtime::new().map_err(|_| invalid())?;
    let data = runtime
        .block_on(api.backup_governance(company))
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":true,"data":data}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_read_scope_before_cookie_access() {
        let base = BTreeMap::from([
            ("--server", "https://example.invalid"),
            ("--cookie-file", "/missing-cookie"),
            ("--company", "synthetic"),
        ]);
        for field in [
            "--as-of",
            "--actor",
            "--service-key-file",
            "--operation",
            "--body",
        ] {
            let mut flags = base.clone();
            flags.insert(field, "forged");
            assert_eq!(run(&flags, false, false, false).unwrap_err().0, 2)
        }
        assert_eq!(run(&base, false, true, false).unwrap_err().0, 2);
        assert_eq!(run(&base, false, false, true).unwrap_err().0, 2);
        let help = crate::remote::run(&["--help".into()]).unwrap();
        assert!(
            help["commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "backup-governance")
        );
    }
}
