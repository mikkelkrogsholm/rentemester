use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, IntercompanyMutation, MAX_INTERCOMPANY_BODY, intercompany_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "intercompany"
            | "group_reconciliation"
            | "apply_group_manifest"
            | "propose_intercompany_mapping"
            | "approve_intercompany_mapping"
            | "revoke_intercompany_mapping"
    )
}
pub fn tools() -> Vec<Value> {
    let mut tools = vec![
        tool(
            "intercompany",
            "Read current authorized V1 manifest/hash and visible mapping states; current server rights decide visibility, no hidden reference counts or local financial effects.",
            json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false}),
            true,
        ),
        tool(
            "group_reconciliation",
            "Global read at explicit asOf YYYY-MM-DD. Preserve exact native decimal-string balances/differences/sourceRefs.amount, blockers and source snapshots. No company selector, local money math, FX, inferred ownership or consolidation.",
            json!({"type":"object","required":["asOf"],"properties":{"asOf":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"}},"additionalProperties":false}),
            true,
        ),
    ];
    for (name, action) in [
        ("apply_group_manifest", IntercompanyMutation::Manifest),
        (
            "propose_intercompany_mapping",
            IntercompanyMutation::Propose,
        ),
        (
            "approve_intercompany_mapping",
            IntercompanyMutation::Approve,
        ),
        ("revoke_intercompany_mapping", IntercompanyMutation::Revoke),
    ] {
        let mut body = native_body(intercompany_body_schema(action));
        body["maxLength"] = json!(MAX_INTERCOMPANY_BODY);
        tools.push(tool(name,"Exact native256KiB body once, inner/outer confirm:true, ordinary1MiB frame. Current all-reference authority, source/account/date/currency validation and independent authenticated approval remain server-owned. Approve/revoke use exact canonical mappingId/mappingHash. No actor/keys/defaults/retries, ledger posting, FX or consolidation. Reconcile unknown outcomes with intercompany/group_reconciliation reads before further explicit action.",json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    tools
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Current {
    company: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Global {
    as_of: String,
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
    if name == "intercompany" {
        let r: Current =
            serde_json::from_value(args).map_err(|_| "intercompany requires only company")?;
        return Ok(api.intercompany(&r.company).await);
    }
    if name == "group_reconciliation" {
        let r: Global = serde_json::from_value(args)
            .map_err(|_| "group_reconciliation requires only explicit asOf (no company)")?;
        return Ok(api.group_reconciliation(&r.as_of).await);
    }
    let r: Write = serde_json::from_value(args)
        .map_err(|_| "intercompany mutation requires only company/body/confirm")?;
    let action = match name {
        "apply_group_manifest" => IntercompanyMutation::Manifest,
        "propose_intercompany_mapping" => IntercompanyMutation::Propose,
        "approve_intercompany_mapping" => IntercompanyMutation::Approve,
        _ => IntercompanyMutation::Revoke,
    };
    Ok(api
        .mutate_intercompany(&r.company, action, r.body.into_bytes(), r.confirm)
        .await)
}
