//! Exact source evidence lifecycle; independent human review and journal/head authority stay server-side.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, DispositionAction as Action, Error, MAX_DISPOSITION_BODY, disposition_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
fn action(name: &str) -> Option<Action> {
    Some(match name.strip_suffix("_group_disposition")? {
        "plan" => Action::Plan,
        "propose" => Action::Propose,
        "approve" => Action::Approve,
        "link" => Action::Link,
        "settle" => Action::Settle,
        "supersede" => Action::Supersede,
        "reopen" => Action::Reopen,
        _ => return None,
    })
}
pub fn accepts(name: &str) -> bool {
    name == "group_disposition_status" || action(name).is_some()
}
pub fn tools() -> Vec<Value> {
    let mut tools = vec![tool(
        "group_disposition_status",
        "Global current-authorized exact-ID disposition status/history/candidates/exceptions. Optional asOf affects overdue evaluation only. No company selector, confirmation, body or mutation. Preserve original actor/history and exact heads. Settled balanced is false.",
        json!({"type":"object","properties":{"id":{"type":"string"},"asOf":{"type":"string"}},"required":["id"],"additionalProperties":false}),
        true,
    )];
    for op in [
        Action::Plan,
        Action::Propose,
        Action::Approve,
        Action::Link,
        Action::Settle,
        Action::Supersede,
        Action::Reopen,
    ] {
        let mut body = native_body(disposition_body_schema(op));
        body["maxLength"] = json!(MAX_DISPOSITION_BODY);
        let mut schema = json!({"type":"object","properties":{"company":company_schema(),"body":body},"required":["company","body"],"additionalProperties":false});
        if op.is_write() {
            schema["properties"]["confirm"] = json!({"const":true});
            schema["required"]
                .as_array_mut()
                .unwrap()
                .push(json!("confirm"));
        }
        tools.push(tool(&format!("{}_group_disposition",op.suffix()),if op.is_write(){"Explicit evidence lifecycle action, exact native32KiB inner/outer confirmed JSON and anchor endpoint company. Current authority on both companies, stable independent approval and exact actual entry/head proofs; server owns state-specific replay. Never posts/pays/eliminates or refreshes heads. Propose the normalized returned plan disposition including its ID; omitted IDs preserve server generation but a lost reply loses direct recovery. Unknown outcome: read canonical exact-ID status before another action; never generate IDs/keys/hashes, rewrite amounts or automatically retry."}else{"Read-only POST plan, exact native32KiB JSON without confirmation. Return complete normalized disposition, server-assigned ID/hash and authorized actual journal candidates. Adopt returned ID and normalized disposition for explicit proposal; no ledger effects or client arithmetic."},schema,!op.is_write()));
    }
    tools
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Status {
    id: String,
    as_of: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
    body: String,
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
    if name == "group_disposition_status" {
        let v: Status = serde_json::from_value(args)
            .map_err(|_| "status requires only id and optional asOf")?;
        return Ok(api
            .group_disposition_status(&v.id, v.as_of.as_deref())
            .await);
    }
    let op = action(name).ok_or("unsupported disposition action")?;
    let (company, body, confirmed) = if op.is_write() {
        let v: Write = serde_json::from_value(args)
            .map_err(|_| "write requires only company, exact body and confirm:true")?;
        (v.company, v.body, v.confirm)
    } else {
        let v: Read = serde_json::from_value(args)
            .map_err(|_| "plan requires only company and exact body without confirmation")?;
        (v.company, v.body, false)
    };
    if body.len() > MAX_DISPOSITION_BODY {
        return Err("disposition body exceeds native32KiB");
    }
    Ok(api
        .intercompany_disposition(&company, op, body.into_bytes(), confirmed)
        .await)
}
