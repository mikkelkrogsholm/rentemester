use rentemester_client::{ApiClient, AssetMutation, MAX_ASSET_BODY, asset_body_schema};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "assets" | "next-asset-depreciation" | "register-asset" | "depreciate-asset"
    )
}
pub fn help() -> Value {
    json!({"usage":"assets --company SLUG; next-asset-depreciation --company SLUG --id ASSET_ID; register-asset --company SLUG --confirm < native.json; depreciate-asset additionally --id ASSET_ID. Explicit --server and private --cookie-file required. Native body max256KiB.","policy":"CompanyRead for reads, CompanyLedgerPost for writes. Register requires original purchase evidence and actual capitalisation plus explicit asset(debit), expense(debit), accumulated asset(credit) accounts. Server verifies source hash, current accounts, periods and all depreciation arithmetic. Read next preview and preserve explicit periodIndex when confirming depreciation. Omitted/null periodIndex selects next; blindly repeating could post another period. Posted explicit period refuses. No keys, receipt, automatic runs or retries; on outcome_unknown read assets/next-asset-depreciation/journal/trial-balance before any further action.","bodySchemas":{"register":asset_body_schema(AssetMutation::Register),"depreciate":asset_body_schema(AssetMutation::Depreciate(1))}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = matches!(command, "assets" | "next-asset-depreciation");
    let needs_id = matches!(command, "next-asset-depreciation" | "depreciate-asset");
    if confirmed == read
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k) || needs_id && *k == "--id")
        })
    {
        return Err(invalid(
            "asset reads have no confirmation; register/depreciate require --confirm; next/depreciate require --id only",
        ));
    }
    let id = if needs_id {
        flags
            .get("--id")
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| (1..=9_007_199_254_740_991).contains(n))
            .ok_or_else(|| invalid("positive exact --id required"))?
    } else {
        0
    };
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("assets require --company"))?;
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
    if !read {
        std::io::stdin()
            .take(MAX_ASSET_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact asset body"))?;
    }
    let runtime =
        tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let result = runtime
        .block_on(async {
            match command {
                "assets" => api.assets(company).await,
                "next-asset-depreciation" => api.next_asset_depreciation(company, id).await,
                _ => {
                    api.asset_mutation(
                        company,
                        if command == "register-asset" {
                            AssetMutation::Register
                        } else {
                            AssetMutation::Depreciate(id)
                        },
                        bytes,
                        confirmed,
                    )
                    .await
                }
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}
