use rentemester_client::{
    ApiClient, BookkeepingBatchMutation as Op, MAX_BOOKKEEPING_BATCH_BODY,
    bookkeeping_batch_body_schema, bookkeeping_batch_scope_schema,
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
        "bookkeeping-batch-plan"
            | "bookkeeping-batch-status"
            | "bookkeeping-batch-persist"
            | "bookkeeping-batch-approve"
            | "bookkeeping-batch-apply"
    )
}
fn op(s: &str) -> Option<Op> {
    match s {
        "bookkeeping-batch-persist" => Some(Op::Persist),
        "bookkeeping-batch-approve" => Some(Op::Approve),
        "bookkeeping-batch-apply" => Some(Op::Apply),
        _ => None,
    }
}
pub fn help() -> Value {
    json!({"scopeSchema":bookkeeping_batch_scope_schema(),"bodySchemas":{"bookkeeping-batch-persist":bookkeeping_batch_body_schema(Op::Persist),"bookkeeping-batch-approve":bookkeeping_batch_body_schema(Op::Approve),"bookkeeping-batch-apply":bookkeeping_batch_body_schema(Op::Apply)},"usage":"All commands require --server, private current-MFA --cookie-file, --company. Plan GET requires --accounting-from/--accounting-to/--bank-from/--bank-to; status GET requires exactly one --id or --run-key (unknown persist recovery). Reads reject confirm and create no facts. Mutations exact native32KiB stdin JSON plus --confirm, no scope/id flags. Persist binds caller runKey and reviewed expectedPlanHash; approve/apply bind runId+planHash. Native atomic_v1 requires distinct reviewer and full rollback on refusal. Legacy unversioned partial batches inspect-only. Current authority/source/transfer fence and final checks are server-owned. No math, keys/hashes/defaults, automatic retry or local files. Unknown write outcomes require status read before another manual action; canonical known refusal JSON preserved."})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let mutation = op(command);
    if preview || confirmed != mutation.is_some() {
        return Err(invalid("batch writes require --confirm; reads forbid it"));
    }
    let dates = [
        ("--accounting-from", "accountingFrom"),
        ("--accounting-to", "accountingTo"),
        ("--bank-from", "bankFrom"),
        ("--bank-to", "bankTo"),
    ];
    if flags.keys().any(|k| {
        let common = ["--server", "--cookie-file", "--company"].contains(k);
        let status = command == "bookkeeping-batch-status" && ["--id", "--run-key"].contains(k);
        let plan = command == "bookkeeping-batch-plan" && dates.iter().any(|(f, _)| f == k);
        !(common || status || plan)
    }) {
        return Err(invalid("command-specific batch scope/selectors required"));
    }
    let company = *flags
        .get("--company")
        .ok_or_else(|| invalid("explicit --company required"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private current-MFA --cookie-file required"))?;
    let mut scope = json!({});
    let mut id = 0;
    if command == "bookkeeping-batch-plan" {
        for (f, k) in dates {
            scope[k] =
                json!(flags.get(f).ok_or_else(|| invalid(
                    "four explicit accounting/bank scope dates required"
                ))?);
        }
    }
    if command == "bookkeeping-batch-status"
        && flags.contains_key("--id") == flags.contains_key("--run-key")
    {
        return Err(invalid("status requires exactly one --id or --run-key"));
    }
    if command == "bookkeeping-batch-status" && flags.contains_key("--id") {
        id = flags
            .get("--id")
            .ok_or_else(|| invalid("explicit run --id required"))?
            .parse::<i64>()
            .map_err(|_| invalid("positive exact run ID required"))?;
    }
    let mut bytes = vec![];
    if mutation.is_some() {
        std::io::stdin()
            .take(MAX_BOOKKEEPING_BATCH_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read bounded native batch JSON"))?;
    }
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("HTTP runtime unavailable"))?;
    let value = rt
        .block_on(async {
            match mutation {
                Some(op) => {
                    api.mutate_bookkeeping_batch(company, op, bytes, confirmed)
                        .await
                }
                None if command == "bookkeeping-batch-plan" => {
                    api.bookkeeping_batch_plan(company, &scope).await
                }
                None if flags.contains_key("--run-key") => {
                    api.bookkeeping_batch_status_by_key(company, flags["--run-key"])
                        .await
                }
                None => api.bookkeeping_batch_status(company, id).await,
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}
