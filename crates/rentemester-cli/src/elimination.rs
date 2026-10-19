use rentemester_client::{
    ApiClient, EliminationMutation, MAX_ELIMINATION_BODY, elimination_body_schema,
    elimination_plan_schema,
};
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
        "eliminations"
            | "group-eliminations"
            | "plan-elimination"
            | "propose-elimination"
            | "approve-elimination"
            | "reject-elimination"
            | "apply-elimination"
            | "reverse-elimination"
    )
}
pub fn help() -> Value {
    json!({"usage":"eliminations --company ANCHOR; group-eliminations --as-of YYYY-MM-DD (no company); plan-elimination --company ANCHOR < exact-input.json (read-only, NO --confirm); propose-elimination/approve-elimination/reject-elimination/apply-elimination/reverse-elimination --company ANCHOR --confirm < exact-native.json","policy":"Native256KiB exact bytes once; explicit server/private MFA cookie. Plan is read-only without confirm; propose requires exact reviewed expectedPayloadHash. Lifecycle requires eliminationId/payloadHash and inner+outer confirmation; independent reviewer and current all-company rights/source freshness are server-owned. Preserve full V1/V2 evidence and decimal strings. No amount math, actor/IDs/keys/defaults or retries. Read current eliminations/group-eliminations after uncertain writes before further explicit action. No ledger postings.","bodySchemas":{"plan":elimination_plan_schema(),"propose":elimination_body_schema(EliminationMutation::Propose),"lifecycle":elimination_body_schema(EliminationMutation::Approve)}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirm: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let global = command == "group-eliminations";
    let plan = command == "plan-elimination";
    let read = global || command == "eliminations";
    let write = !read && !plan;
    if preview
        || confirm != write
        || flags.keys().any(|k| !match *k {
            "--server" | "--cookie-file" => true,
            "--as-of" => global,
            "--company" => !global,
            _ => false,
        })
    {
        return Err(invalid(
            "elimination selectors/confirmation differ from command; see remote help",
        ));
    }
    let company = if global {
        None
    } else {
        Some(
            *flags
                .get("--company")
                .ok_or_else(|| invalid("explicit company required"))?,
        )
    };
    let date = if global {
        Some(
            *flags
                .get("--as-of")
                .ok_or_else(|| invalid("explicit as-of required"))?,
        )
    } else {
        None
    };
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private MFA cookie file required"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let action = match command {
        "propose-elimination" => EliminationMutation::Propose,
        "approve-elimination" => EliminationMutation::Approve,
        "reject-elimination" => EliminationMutation::Reject,
        "apply-elimination" => EliminationMutation::Apply,
        _ => EliminationMutation::Reverse,
    };
    let mut bytes = vec![];
    if !read {
        std::io::stdin()
            .take(MAX_ELIMINATION_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact elimination body"))?;
    }
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("HTTP runtime unavailable"))?;
    let value = rt
        .block_on(async {
            if global {
                api.group_eliminations(date.unwrap()).await
            } else if read {
                api.eliminations(company.unwrap()).await
            } else if plan {
                api.elimination_plan(company.unwrap(), bytes).await
            } else {
                api.mutate_elimination(company.unwrap(), action, bytes, confirm)
                    .await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":value}))
}
