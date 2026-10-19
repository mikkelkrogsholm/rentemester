//! Explicit native supplier-planning commands; no intent or replay keys generated.
use rentemester_client::{ApiClient, CommitmentMutation as Mutation};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(message: &str) -> Failure {
    (2, Error::new("invalid_input", message))
}
pub fn accepts(command: &str) -> bool {
    matches!(
        command,
        "supplier-commitments"
            | "supplier-commitment-matches"
            | "plan-supplier-commitment"
            | "approve-supplier-commitment"
            | "change-supplier-commitment"
            | "match-supplier-commitment"
    )
}
pub fn help() -> Value {
    json!({"commands":["supplier-commitments","supplier-commitment-matches","plan-supplier-commitment","approve-supplier-commitment","change-supplier-commitment","match-supplier-commitment"],"usage":"All require explicit --company, --server and --cookie-file. supplier-commitments optionally accepts --start-date YYYY-MM-DD; server defaults to trusted Danish date. No year/id/status/account selectors. Matches read accepts no selector. Plan reads exact native stdin JSON, is read-only and forbids --confirm. Approve/change/match require bare --confirm plus confirm:true in exact native stdin body. Approval needs reviewed plan.payloadHash and original commitment; change needs current expectedPayloadHash plus reason. Match uses canonical kind+id and scheduled date; never supply factual amount/date/party overrides. Native approval supports caller-supplied optional idempotencyKey; no key is invented or injected. One attempt, no retry. Reconcile outcome_unknown with current commitments, matches, history/hash/original key before action. Forecast coverage remains incomplete; missing bank cash is null and unsupported FX excluded. No journals/payables/payments created.","bodySchemas":{"plan":rentemester_client::commitment_plan_schema(),"approve":rentemester_client::commitment_body_schema(Mutation::Apply),"change":rentemester_client::commitment_body_schema(Mutation::Change),"match":rentemester_client::commitment_body_schema(Mutation::Match)}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview_reviewed: bool,
) -> Result<Value, Failure> {
    let mutation = match command {
        "approve-supplier-commitment" => Some(Mutation::Apply),
        "change-supplier-commitment" => Some(Mutation::Change),
        "match-supplier-commitment" => Some(Mutation::Match),
        _ => None,
    };
    if confirmed != mutation.is_some() || preview_reviewed {
        return Err(invalid(
            "supplier writes require bare --confirm; read/plan never accepts confirmation or preview-reviewed",
        ));
    }
    if flags.keys().any(|k| {
        !(["--server", "--cookie-file", "--company"].contains(k)
            || *k == "--start-date" && command == "supplier-commitments")
    }) {
        return Err(invalid(
            "only supplier-commitments read accepts --start-date; other selectors are unavailable for commitment commands",
        ));
    }
    if flags
        .get("--start-date")
        .is_some_and(|s| !rentemester_client::valid_commitment_start(s))
    {
        return Err(invalid(
            "supplier-commitments --start-date requires real YYYY-MM-DD date",
        ));
    }
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("supplier commitment command requires explicit --company"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server origin required"))?;
    let cookie = flags.get("--cookie-file").ok_or_else(|| {
        invalid("authenticated supplier commitment command requires --cookie-file")
    })?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api =
        ApiClient::new(server, allow_http, Some(cookie)).map_err(crate::remote::client_error)?;
    let body = if mutation.is_some() || command == "plan-supplier-commitment" {
        let mut bytes = vec![];
        std::io::stdin()
            .lock()
            .take(262145)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact native commitment body"))?;
        Some(bytes)
    } else {
        None
    };
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let response = runtime
        .block_on(async {
            if let Some(operation) = mutation {
                api.mutate_supplier_commitment(company, operation, body.unwrap(), confirmed)
                    .await
            } else {
                match command {
                    "supplier-commitments" => {
                        api.supplier_commitments(company, flags.get("--start-date").copied())
                            .await
                    }
                    "supplier-commitment-matches" => api.supplier_commitment_matches(company).await,
                    _ => api.plan_supplier_commitment(company, body.unwrap()).await,
                }
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":response.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":response}))
}
