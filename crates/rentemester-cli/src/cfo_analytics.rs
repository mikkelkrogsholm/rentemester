//! Explicit read-only global CFO query; server owns dates, source evidence and sums.
use rentemester_client::{ApiClient, CfoAnalyticsQuery};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub fn accepts(name: &str) -> bool {
    name == "cfo-analytics"
}
pub fn help() -> Value {
    json!({"commands":["cfo-analytics"],"input":"Read-only --scope company|portfolio|group --from DATE --to DATE; explicit --server/--cookie-file. Optional --company SLUG OR --company-slugs-json JSON_ARRAY; --group-profile-id, --account, --party, --dimension, --currency, --aggregate none|sum, --cursor, --limit1..200. No confirmation/stdin/actor. No local calculations or source defaults. Portfolio is not consolidated; explicit sum only with full server-authorized visibility. Nonempty unsupported archives remain explicit refusal, never zero.","querySchema":rentemester_client::cfo_analytics_schema()})
}
pub fn run(
    _name: &str,
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    let invalid = |s| (2, Error::new("invalid_input", s));
    let allowed = [
        "--server",
        "--cookie-file",
        "--scope",
        "--from",
        "--to",
        "--company",
        "--company-slugs-json",
        "--group-profile-id",
        "--account",
        "--party",
        "--dimension",
        "--currency",
        "--aggregate",
        "--cursor",
        "--limit",
    ];
    if confirmed
        || preview
        || flags.keys().any(|k| !allowed.contains(k))
        || flags.contains_key("--company") && flags.contains_key("--company-slugs-json")
    {
        return Err(invalid(
            "CFO is read-only, exact documented selectors without mixed company flags required",
        ));
    }
    let required = |key| {
        flags
            .get(key)
            .map(|s| s.to_string())
            .ok_or_else(|| invalid("explicit server, cookie, scope, from and to required"))
    };
    let optional = |key| flags.get(key).map(|s| s.to_string());
    let companies = if let Some(value) = flags.get("--company") {
        vec![value.to_string()]
    } else if let Some(value) = flags.get("--company-slugs-json") {
        serde_json::from_str::<Vec<String>>(value)
            .map_err(|_| invalid("company-slugs-json must be a string array"))?
    } else {
        vec![]
    };
    let q = CfoAnalyticsQuery {
        scope: required("--scope")?,
        from: required("--from")?,
        to: required("--to")?,
        company_slugs: companies,
        group_profile_id: optional("--group-profile-id"),
        account: optional("--account"),
        party: optional("--party"),
        dimension: optional("--dimension"),
        currency: optional("--currency"),
        aggregate: optional("--aggregate"),
        cursor: optional("--cursor"),
        limit: flags
            .get("--limit")
            .map(|s| {
                s.parse::<u16>()
                    .map_err(|_| invalid("limit must be integer1..200"))
            })
            .transpose()?,
    };
    let server = required("--server")?;
    let cookie =
        rentemester_client::read_cookie_file(std::path::Path::new(&required("--cookie-file")?))
            .map_err(crate::remote::client_error)?;
    let api =
        ApiClient::new(&server, allow_http, Some(cookie)).map_err(crate::remote::client_error)?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let value = runtime
        .block_on(api.cfo_analytics(&q))
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}
