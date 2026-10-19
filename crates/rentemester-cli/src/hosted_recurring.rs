use rentemester_client::{
    ApiClient, HostedRecurringMutation, MAX_HOSTED_RECURRING_BODY, hosted_recurring_body_schema,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn accepts(command: &str) -> bool {
    matches!(
        command,
        "hosted-recurring-plan" | "consent-hosted-recurring" | "revoke-hosted-recurring"
    )
}
pub fn help() -> Value {
    json!({"commands":["hosted-recurring-plan", "consent-hosted-recurring", "revoke-hosted-recurring"],"policy":"Current human CompanyAdmin; review full immutable template, configured executor, exact authority/plan hash. Hosted plan GET is read-only and has only --company/--id. Writes exact native16KiB stdin JSON with inner+outer confirm:true, explicit reason and identities; NEVER a service secret. Consent only records the grant; no dispatch, timer, client driver, inferred worker health or implicit permissions. Explicit operator server worker configuration remains separate. No automatic retries; reconcile current status/hosted plan after uncertain outcome.","bodySchemas":{"consent-hosted-recurring":hosted_recurring_body_schema(HostedRecurringMutation::Consent),"revoke-hosted-recurring":hosted_recurring_body_schema(HostedRecurringMutation::Revoke)}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = command == "hosted-recurring-plan";
    if confirmed == read
        || preview
        || flags.keys().any(|key| {
            !(["--server", "--cookie-file"].contains(key)
                || *key == "--company"
                || read && *key == "--id")
        })
    {
        return Err(invalid(
            "command-specific selectors required; readonly has no confirmation and writes require exact confirmed stdin body",
        ));
    }
    let company = *flags
        .get("--company")
        .ok_or_else(|| invalid("explicit --company required"))?;
    let id = if read {
        flags
            .get("--id")
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| (1..=9_007_199_254_740_991).contains(n))
            .ok_or_else(|| invalid("positive exact --id required"))?
    } else {
        0
    };
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let path = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private MFA --cookie-file required"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(path))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = vec![];
    if !read {
        std::io::stdin()
            .take(MAX_HOSTED_RECURRING_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact native body"))?;
    }
    let runtime =
        tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let result = runtime
        .block_on(async {
            if read {
                api.hosted_recurring_plan(company, id).await
            } else {
                let action = match command {
                    "consent-hosted-recurring" => HostedRecurringMutation::Consent,
                    "revoke-hosted-recurring" => HostedRecurringMutation::Revoke,
                    _ => return Err(rentemester_client::Error::InvalidInput("unknown operation")),
                };
                api.mutate_hosted_recurring(company, action, bytes, confirmed)
                    .await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}
