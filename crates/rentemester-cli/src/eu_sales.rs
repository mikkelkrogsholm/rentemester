//! Canonical dated EU sales and explicit provider validation, without local fallback.
use rentemester_client::ApiClient;
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub fn accepts(name: &str) -> bool {
    matches!(name, "eu-sales" | "vies-validate")
}
pub fn help() -> Value {
    json!({"commands":["eu-sales","vies-validate"],"usage":"remote eu-sales --company SLUG --from YYYY-MM-DD --to YYYY-MM-DD; remote vies-validate --company SLUG --vat-or-cvr EUVAT --confirm. Both require --server and --cookie-file. VIES calls operator-configured provider once, retains observation, never accepts caller validity. EU report is read-only preparation, not a tax submission."})
}
pub fn run(
    name: &str,
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
                "exact EU sales dates or VAT number with explicit confirmation required",
            ),
        )
    };
    let write = name == "vies-validate";
    let allowed = if write {
        vec!["--server", "--cookie-file", "--company", "--vat-or-cvr"]
    } else {
        vec!["--server", "--cookie-file", "--company", "--from", "--to"]
    };
    if !accepts(name) || confirmed != write || preview || flags.keys().any(|k| !allowed.contains(k))
    {
        return Err(invalid());
    }
    let required = |key| flags.get(key).copied().ok_or_else(invalid);
    let company = required("--company")?;
    let selectors = if write {
        (required("--vat-or-cvr")?, "")
    } else {
        (required("--from")?, required("--to")?)
    };
    let cookie =
        rentemester_client::read_cookie_file(std::path::Path::new(required("--cookie-file")?))
            .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(required("--server")?, allow_http, Some(cookie))
        .map_err(crate::remote::client_error)?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "HTTP runtime unavailable")))?;
    let value = runtime
        .block_on(async {
            if write {
                api.vies_validate(company, selectors.0).await
            } else {
                api.eu_sales(company, selectors.0, selectors.1).await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unconfirmed_lookup_and_report_writes() {
        assert!(run("vies-validate", &BTreeMap::new(), false, false, false).is_err());
        assert!(run("eu-sales", &BTreeMap::new(), false, true, false).is_err());
    }
}
