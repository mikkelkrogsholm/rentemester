use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, ConsolidationMutation, Error, consolidation_body_schema};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "consolidation"
            | "group_report_profiles"
            | "group_consolidated_report"
            | "propose_consolidation_profile"
            | "approve_consolidation_profile"
            | "revoke_consolidation_profile"
    )
}
pub fn tools() -> Vec<Value> {
    let date = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
    let mut tools = vec![
        tool(
            "consolidation",
            "Read canonical visible native consolidation profiles/hashes/states and active metadata at explicit asOf. Current server scope is authoritative.",
            json!({"type":"object","required":["company","asOf"],"properties":{"company":company_schema(),"asOf":date},"additionalProperties":false}),
            true,
        ),
        tool(
            "group_report_profiles",
            "Global active consolidation-profile metadata at explicit asOf; no company default/selector or inferred coverage.",
            json!({"type":"object","required":["asOf"],"properties":{"asOf":date},"additionalProperties":false}),
            true,
        ),
        tool(
            "group_consolidated_report",
            "Global read-only source consolidation report with explicit profileId/from/asOf. Preserve full JSON, source proof, blockers/redaction and DECIMAL STRINGS; no local financial math/FX or inferred authority.",
            json!({"type":"object","required":["profileId","from","asOf"],"properties":{"profileId":{"type":"string","minLength":1},"from":date,"asOf":date},"additionalProperties":false}),
            true,
        ),
    ];
    for (name, action) in [
        (
            "propose_consolidation_profile",
            ConsolidationMutation::Propose,
        ),
        (
            "approve_consolidation_profile",
            ConsolidationMutation::Approve,
        ),
        (
            "revoke_consolidation_profile",
            ConsolidationMutation::Revoke,
        ),
    ] {
        let mut body = native_body(consolidation_body_schema(action));
        body["maxLength"] = json!(action.max_body());
        tools.push(tool(name,"Exact native reviewed inner/outer-confirm body ONCE: proposal1MiB (ONLY this tool frame2MiB+64KiB), approve/revoke32KiB (ordinary1MiB frame). Server requires current workspace/all-configured-company authority and distinct authenticated approval of exact profileHash. Revoke uses only profileId/confirm. No actor/defaults/keys/retries/calculations. Unknown outcomes require canonical current/profile/report reads before further explicit action.",json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    tools
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Current {
    company: String,
    as_of: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Profiles {
    as_of: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Report {
    profile_id: String,
    from: String,
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
    match name {
        "consolidation" => {
            let r: Current = serde_json::from_value(args)
                .map_err(|_| "consolidation requires only company/asOf")?;
            Ok(api.consolidation(&r.company, &r.as_of).await)
        }
        "group_report_profiles" => {
            let r: Profiles = serde_json::from_value(args)
                .map_err(|_| "group_report_profiles requires only explicit asOf (no company)")?;
            Ok(api.group_report_profiles(&r.as_of).await)
        }
        "group_consolidated_report" => {
            let r:Report=serde_json::from_value(args).map_err(|_|"group_consolidated_report requires only explicit profileId/from/asOf (no company)")?;
            Ok(api
                .group_consolidated_report(&r.profile_id, &r.from, &r.as_of)
                .await)
        }
        _ => {
            let r: Write = serde_json::from_value(args)
                .map_err(|_| "consolidation mutation requires only company/body/confirm")?;
            let action = match name {
                "propose_consolidation_profile" => ConsolidationMutation::Propose,
                "approve_consolidation_profile" => ConsolidationMutation::Approve,
                _ => ConsolidationMutation::Revoke,
            };
            Ok(api
                .mutate_consolidation(&r.company, action, r.body.into_bytes(), r.confirm)
                .await)
        }
    }
}
