use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, BookkeepingBatchMutation as Op, Error, MAX_BOOKKEEPING_BATCH_BODY,
    bookkeeping_batch_body_schema, bookkeeping_batch_scope_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "bookkeeping_batch_plan"
            | "bookkeeping_batch_status"
            | "bookkeeping_batch_persist"
            | "bookkeeping_batch_approve"
            | "bookkeeping_batch_apply"
    )
}
pub fn tools() -> Vec<Value> {
    let mut out = vec![
        tool(
            "bookkeeping_batch_plan",
            "Read-only complete native source plan; explicit accounting/bank date ranges. No persistence or inferred readiness.",
            json!({"type":"object","properties":{"company":company_schema(),"scope":bookkeeping_batch_scope_schema()},"required":["company","scope"],"additionalProperties":false}),
            true,
        ),
        tool(
            "bookkeeping_batch_status",
            "Read immutable revisions/approval/receipts/checks by explicit runId OR exact runKey using current company read authority; no retry/apply.",
            json!({"type":"object","properties":{"company":company_schema(),"runId":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"runKey":{"type":"string","minLength":1,"maxLength":1024}},"required":["company"],"oneOf":[{"required":["runId"]},{"required":["runKey"]}],"additionalProperties":false}),
            true,
        ),
    ];
    for (name, op) in [
        ("bookkeeping_batch_persist", Op::Persist),
        ("bookkeeping_batch_approve", Op::Approve),
        ("bookkeeping_batch_apply", Op::Apply),
    ] {
        let mut body = native_body(bookkeeping_batch_body_schema(op));
        body["maxLength"] = json!(MAX_BOOKKEEPING_BATCH_BODY);
        out.push(tool(name,"Exact native32KiB JSON with inner+outer confirm. Persist requires caller runKey and reviewed expectedPlanHash; approve/apply runId+planHash. Current principal/independent approval/atomic_v1 source checks/fences remain server-owned. Legacy partial history inspect-only. No generated keys/hashes/defaults/math, silent skip, local files or automatic retry. Unknown outcome requires canonical status before manual action; known refusal JSON preserved.",json!({"type":"object","properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"required":["company","body","confirm"],"additionalProperties":false}),false));
    }
    out
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    company: String,
    scope: Value,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Status {
    company: String,
    run_id: Option<i64>,
    run_key: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    company: String,
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "bookkeeping_batch_plan" {
        let v: Plan =
            serde_json::from_value(args).map_err(|_| "batch plan accepts company/scope only")?;
        return Ok(api.bookkeeping_batch_plan(&v.company, &v.scope).await);
    }
    if name == "bookkeeping_batch_status" {
        let v: Status =
            serde_json::from_value(args).map_err(|_| "batch status accepts company/runId only")?;
        return match (v.run_id, v.run_key) {
            (Some(id), None) => Ok(api.bookkeeping_batch_status(&v.company, id).await),
            (None, Some(key)) => Ok(api.bookkeeping_batch_status_by_key(&v.company, &key).await),
            _ => Err("status requires exactly one runId or runKey"),
        };
    }
    let v: Write = serde_json::from_value(args)
        .map_err(|_| "batch mutation accepts company/body/confirm only")?;
    if v.body.len() > MAX_BOOKKEEPING_BATCH_BODY {
        return Err("native batch body exceeds32KiB");
    }
    let op = match name {
        "bookkeeping_batch_persist" => Op::Persist,
        "bookkeeping_batch_approve" => Op::Approve,
        "bookkeeping_batch_apply" => Op::Apply,
        _ => return Err("unknown batch operation"),
    };
    Ok(api
        .mutate_bookkeeping_batch(&v.company, op, v.body.into_bytes(), v.confirm)
        .await)
}
