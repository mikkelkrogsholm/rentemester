use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, HostedRecurringMutation as Op, MAX_HOSTED_RECURRING_BODY,
    hosted_recurring_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "hosted_recurring_plan" | "consent_hosted_recurring" | "revoke_hosted_recurring"
    )
}
pub fn tools() -> Vec<Value> {
    let mut tools = vec![tool(
        "hosted_recurring_plan",
        "Current human CompanyAdmin reads complete immutable source/schedule/configured executor/authority and exact planHash. No query/body/confirmation, activation, dispatch or worker-health inference.",
        json!({"type":"object","required":["company","templateId"],"properties":{"company":company_schema(),"templateId":{"type":"integer","minimum":1,"maximum":9007199254740991u64}},"additionalProperties":false}),
        true,
    )];
    for (name, action) in [
        ("consent_hosted_recurring", Op::Consent),
        ("revoke_hosted_recurring", Op::Revoke),
    ] {
        let mut body = native_body(hosted_recurring_body_schema(action));
        body["maxLength"] = json!(MAX_HOSTED_RECURRING_BODY);
        tools.push(tool(name,"Current human CompanyAdmin; exact reviewed native16KiB body, inner+outer confirm:true. Consent binds configured serviceAccountId/credentialId/current planHash and explicit reason, never executor secret. Revoke uses exact expectedStateHash/reason. No implicit dispatch, client driver/timer, inferred worker health, grants, schedule math/defaults/keys or retry. Unknown requires current status/hosted-plan reconciliation; actual operator worker configuration is separate.",json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    tools
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Plan {
    company: String,
    template_id: i64,
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
    if name == "hosted_recurring_plan" {
        let v: Plan = serde_json::from_value(args)
            .map_err(|_| "hosted plan accepts company/templateId only")?;
        if !(1..=9_007_199_254_740_991).contains(&v.template_id) {
            return Err("positive exact templateId required");
        }
        return Ok(api.hosted_recurring_plan(&v.company, v.template_id).await);
    }
    let v: Write = serde_json::from_value(args)
        .map_err(|_| "hosted mutation accepts company/body/confirm only")?;
    if v.body.len() > MAX_HOSTED_RECURRING_BODY {
        return Err("hosted body exceeds native16KiB");
    }
    let op = match name {
        "consent_hosted_recurring" => Op::Consent,
        "revoke_hosted_recurring" => Op::Revoke,
        _ => return Err("unknown hosted operation"),
    };
    Ok(api
        .mutate_hosted_recurring(&v.company, op, v.body.into_bytes(), v.confirm)
        .await)
}
