//! Server-owned read-only reports; portfolio is never an inferred selected-company request.
use rentemester_client::ApiClient;
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub fn accepts(name: &str) -> bool {
    matches!(name, "portfolio" | "cashflow")
}
pub fn help() -> Value {
    json!({"commands":["portfolio","cashflow"],"input":"remote portfolio [--as-of YYYY-MM-DD] (no company); remote cashflow --company SLUG [--year LABEL]. Explicit --server/--cookie-file; optional --allow-loopback-http. Read-only, no confirm/stdin/provider/retry/local sums; preserve canonical unknown/partial source coverage. Portfolio is not consolidation. Not an alias for TS portfolio_overview."})
}
pub fn run(
    name: &str,
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    let invalid = |s| (2, Error::new("invalid_input", s));
    let allowed = if name == "portfolio" {
        vec!["--server", "--cookie-file", "--as-of"]
    } else {
        vec!["--server", "--cookie-file", "--company", "--year"]
    };
    if confirmed || preview || flags.keys().any(|key| !allowed.contains(key)) {
        return Err(invalid(
            "exact read-only portfolio or cashflow selectors required",
        ));
    }
    let required = |key| {
        flags
            .get(key)
            .copied()
            .ok_or_else(|| invalid("explicit server/cookie and company for cashflow required"))
    };
    let cookie =
        rentemester_client::read_cookie_file(std::path::Path::new(required("--cookie-file")?))
            .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(required("--server")?, allow_http, Some(cookie))
        .map_err(crate::remote::client_error)?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "HTTP runtime unavailable")))?;
    let value = if name == "portfolio" {
        runtime.block_on(api.portfolio(flags.get("--as-of").copied()))
    } else {
        runtime.block_on(api.cashflow(required("--company")?, flags.get("--year").copied()))
    }
    .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn global_portfolio_and_company_cashflow_reject_mixed_or_mutating_selectors() {
        let wrong = BTreeMap::from([("--company", "hidden")]);
        assert!(run("portfolio", &wrong, true, false, false).is_err());
        let wrong = BTreeMap::from([("--as-of", "2025-01-01")]);
        assert!(run("cashflow", &wrong, true, false, false).is_err());
        assert!(run("portfolio", &BTreeMap::new(), true, true, false).is_err());
    }
}
