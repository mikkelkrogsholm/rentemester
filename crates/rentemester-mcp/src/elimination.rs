use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, EliminationMutation, Error, MAX_ELIMINATION_BODY, elimination_body_schema,
    elimination_plan_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "eliminations"
            | "group_eliminations"
            | "plan_elimination"
            | "propose_elimination"
            | "approve_elimination"
            | "reject_elimination"
            | "apply_elimination"
            | "reverse_elimination"
    )
}
pub fn tools() -> Vec<Value> {
    let mut tools = vec![
        tool(
            "eliminations",
            "Read current authorized elimination states, full immutable V1/V2 payload/hash/status. Server current company visibility applies.",
            json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false}),
            true,
        ),
        tool(
            "group_eliminations",
            "Read actual current applied elimination evidence at exact explicit asOf date; no carryforward, company selector, local math or invented evidence.",
            json!({"type":"object","required":["asOf"],"properties":{"asOf":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"}},"additionalProperties":false}),
            true,
        ),
    ];
    let mut plan = native_body(elimination_plan_schema());
    plan["maxLength"] = json!(MAX_ELIMINATION_BODY);
    tools.push(tool("plan_elimination","Read-only POST exact native Input, no confirm and no mutation audit. Returns actual frozen payload/payloadHash for explicit review; source freshness, positions and exact money stay server-owned. No retry.",json!({"type":"object","required":["company","body"],"properties":{"company":company_schema(),"body":plan},"additionalProperties":false}),true));
    for (name, action) in [
        ("propose_elimination", EliminationMutation::Propose),
        ("approve_elimination", EliminationMutation::Approve),
        ("reject_elimination", EliminationMutation::Reject),
        ("apply_elimination", EliminationMutation::Apply),
        ("reverse_elimination", EliminationMutation::Reverse),
    ] {
        let mut body = native_body(elimination_body_schema(action));
        body["maxLength"] = json!(MAX_ELIMINATION_BODY);
        tools.push(tool(name,"Exact native256KiB body once with inner/outer confirm:true. Propose cites reviewed expectedPayloadHash; lifecycle cites eliminationId/payloadHash. Server enforces independent approval, current all-company authority and source freshness. Preserve all V1/V2 evidence and decimal strings. No actor/IDs/keys/defaults, money calculations, ledger effects or automatic retry. Reconcile uncertain writes with eliminations/group_eliminations before further explicit action.",json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),false));
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
struct Plan {
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
    if name == "eliminations" {
        let r: Current =
            serde_json::from_value(args).map_err(|_| "eliminations requires only company")?;
        return Ok(api.eliminations(&r.company).await);
    }
    if name == "group_eliminations" {
        let r: Global = serde_json::from_value(args)
            .map_err(|_| "group_eliminations requires only explicit asOf")?;
        return Ok(api.group_eliminations(&r.as_of).await);
    }
    if name == "plan_elimination" {
        let r: Plan = serde_json::from_value(args)
            .map_err(|_| "plan_elimination requires only company/body, no confirm")?;
        return Ok(api.elimination_plan(&r.company, r.body.into_bytes()).await);
    }
    let r: Write = serde_json::from_value(args)
        .map_err(|_| "elimination mutation requires only company/body/confirm")?;
    let action = match name {
        "propose_elimination" => EliminationMutation::Propose,
        "approve_elimination" => EliminationMutation::Approve,
        "reject_elimination" => EliminationMutation::Reject,
        "apply_elimination" => EliminationMutation::Apply,
        _ => EliminationMutation::Reverse,
    };
    Ok(api
        .mutate_elimination(&r.company, action, r.body.into_bytes(), r.confirm)
        .await)
}
