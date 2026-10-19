use rentemester_client::{ApiClient, ConsolidationMutation, consolidation_body_schema};
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
        "consolidation"
            | "group-report-profiles"
            | "group-consolidated-report"
            | "propose-consolidation-profile"
            | "approve-consolidation-profile"
            | "revoke-consolidation-profile"
    )
}
pub fn help() -> Value {
    json!({"usage":"consolidation --company ANCHOR --as-of YYYY-MM-DD; group-report-profiles --as-of YYYY-MM-DD; group-consolidated-report --profile-id ID --from YYYY-MM-DD --as-of YYYY-MM-DD; propose/approve/revoke-consolidation-profile --company ANCHOR --confirm < exact-native.json","policy":"No company selector for global reads. Exact inner-confirm body once: proposal1MiB, approve/revoke32KiB. Server owns all configured-company authority, independent authenticated approval of exact profileHash, dates/currency/account coverage, source proofs, eliminations and decimal-string figures. No actor/keys/default dates/calculations/retry. Unknown writes require canonical consolidation/profile/report reads before further explicit action.","bodySchemas":{"propose":consolidation_body_schema(ConsolidationMutation::Propose),"approve":consolidation_body_schema(ConsolidationMutation::Approve),"revoke":consolidation_body_schema(ConsolidationMutation::Revoke)}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let current = command == "consolidation";
    let report = command == "group-consolidated-report";
    let global = report || command == "group-report-profiles";
    let read = current || global;
    if preview
        || confirm == read
        || flags.keys().any(|k| !match *k {
            "--server" | "--cookie-file" => true,
            "--company" => !global,
            "--as-of" => read,
            "--from" | "--profile-id" => report,
            _ => false,
        })
    {
        return Err(invalid(
            "consolidation selectors/confirmation do not match command; see remote help",
        ));
    }
    let required = |key| {
        flags
            .get(key)
            .copied()
            .ok_or_else(|| invalid("explicit consolidation selector required"))
    };
    let company = if !global {
        Some(required("--company")?)
    } else {
        None
    };
    let as_of = if read {
        Some(required("--as-of")?)
    } else {
        None
    };
    let from = if report {
        Some(required("--from")?)
    } else {
        None
    };
    let profile = if report {
        Some(required("--profile-id")?)
    } else {
        None
    };
    let server = required("--server")?;
    let cookie =
        rentemester_client::read_cookie_file(std::path::Path::new(required("--cookie-file")?))
            .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let action = match command {
        "propose-consolidation-profile" => ConsolidationMutation::Propose,
        "approve-consolidation-profile" => ConsolidationMutation::Approve,
        _ => ConsolidationMutation::Revoke,
    };
    let mut bytes = vec![];
    if !read {
        std::io::stdin()
            .take(action.max_body() as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact consolidation body"))?;
    }
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("HTTP runtime unavailable"))?;
    let value = rt
        .block_on(async {
            if report {
                api.group_consolidated_report(profile.unwrap(), from.unwrap(), as_of.unwrap())
                    .await
            } else if global {
                api.group_report_profiles(as_of.unwrap()).await
            } else if current {
                api.consolidation(company.unwrap(), as_of.unwrap()).await
            } else {
                api.mutate_consolidation(company.unwrap(), action, bytes, confirm)
                    .await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":value}))
}
