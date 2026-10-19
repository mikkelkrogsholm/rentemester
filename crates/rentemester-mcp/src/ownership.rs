use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, MAX_OWNERSHIP_BODY, MAX_OWNERSHIP_PROPOSE_BODY, OwnershipMutation,
    ownership_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "ownership"
            | "ownership_history"
            | "propose_ownership"
            | "review_ownership"
            | "apply_ownership"
    )
}
pub fn tools() -> Vec<Value> {
    let date = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
    let mut tools = vec![
        tool(
            "ownership",
            "Read current authorized source graph at explicit asOf; preserve full facts/partial/consolidation advisory. Server owns effective intervals and all calculations; never changes accounting or v1 consolidation manifest.",
            json!({"type":"object","required":["company","asOf"],"properties":{"company":company_schema(),"asOf":date},"additionalProperties":false}),
            true,
        ),
        tool(
            "ownership_history",
            "Read authorized append-only snapshot/provenance/review history, optional exact snapshotId/asOf and source projection. Use canonical hashes/state after unknown writes; no inferred review authority.",
            json!({"type":"object","required":["company"],"properties":{"company":company_schema(),"snapshotId":{"type":"string","minLength":1,"maxLength":80},"asOf":date},"additionalProperties":false}),
            true,
        ),
    ];
    for (name, action) in [
        ("propose_ownership", OwnershipMutation::Propose),
        ("review_ownership", OwnershipMutation::Review),
        ("apply_ownership", OwnershipMutation::Apply),
    ] {
        let mut body = native_body(ownership_body_schema(action));
        body["maxLength"] = json!(if matches!(action, OwnershipMutation::Propose) {
            MAX_OWNERSHIP_PROPOSE_BODY
        } else {
            MAX_OWNERSHIP_BODY
        });
        tools.push(tool(name,"Exact native body once, inner/outer confirm, current all-endpoint authority. Propose2MiB/2048facts, named proposal frame4MiB+64KiB; review/apply32KiB/all other non-upload frames1MiB. Review requires distinct authenticated principal; apply exact server snapshotHash/diffHash only after approval. Later reviewed fact may explicitly end prior fact; omissions never revoke. No client math, legal classification, generated keys/hashes, automatic retry, journals or consolidation changes. Reconcile unknown results through ownership_history/asOf reads.",json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    tools
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Read {
    company: String,
    as_of: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct History {
    company: String,
    snapshot_id: Option<String>,
    as_of: Option<String>,
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
    if name == "ownership" {
        let r: Read =
            serde_json::from_value(args).map_err(|_| "ownership requires only company/asOf")?;
        return Ok(api.ownership(&r.company, &r.as_of).await);
    }
    if name == "ownership_history" {
        let r: History = serde_json::from_value(args)
            .map_err(|_| "ownership_history requires company/snapshotId?/asOf?")?;
        return Ok(api
            .ownership_history(&r.company, r.snapshot_id.as_deref(), r.as_of.as_deref())
            .await);
    }
    let r: Write = serde_json::from_value(args)
        .map_err(|_| "ownership mutation requires only company/body/confirm")?;
    let action = match name {
        "propose_ownership" => OwnershipMutation::Propose,
        "review_ownership" => OwnershipMutation::Review,
        _ => OwnershipMutation::Apply,
    };
    Ok(api
        .mutate_ownership(&r.company, action, r.body.into_bytes(), r.confirm)
        .await)
}
