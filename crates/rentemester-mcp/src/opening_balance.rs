//! Ordinary current-human opening adoption, never local accounting or automatic replay.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, Error, MAX_OPENING_BALANCE_BODY, opening_balance_body_schema};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    [
        "opening_balance",
        "opening_balance_plan",
        "opening_balance_apply",
    ]
    .contains(&name)
}
pub fn tools() -> Vec<Value> {
    let mut tools = vec![tool(
        "opening_balance",
        "Read exact canonical attested opening/current record, including original cutover/journal/hash/actor/source citations. No effects.",
        json!({"type":"object","properties":{"company":company_schema()},"required":["company"],"additionalProperties":false}),
        true,
    )];
    for (name, apply) in [
        ("opening_balance_plan", false),
        ("opening_balance_apply", true),
    ] {
        let mut body = native_body(opening_balance_body_schema(apply));
        body["maxLength"] = json!(MAX_OPENING_BALANCE_BODY);
        let mut schema = json!({"type":"object","properties":{"company":company_schema(),"body":body},"required":["company","body"],"additionalProperties":false});
        if apply {
            schema["properties"]["confirm"] = json!({"const":true});
            schema["required"]
                .as_array_mut()
                .unwrap()
                .push(json!("confirm"));
        }
        tools.push(tool(name,"Explicit opening adoption: readonly plan accepts native64KiB Input WITHOUT confirmation; apply requires exact reviewed input/planHash/caller idempotencyKey/inner+outer confirm:true. Human current Admin+LedgerPost/server source proof and atomic accounting only. No source amount/date inference, local math/actor/key/normalization or automatic retries. Preserve original bytes/key and read canonical opening/journal after unknown before deliberate authorized receipt recovery.",schema,!apply));
    }
    tools
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    company: String,
    body: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Apply {
    company: String,
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    Ok(match name {
        "opening_balance" => {
            let v: Read = serde_json::from_value(args).map_err(|_| "exact company required")?;
            api.opening_balance(&v.company).await
        }
        "opening_balance_plan" => {
            let v: Plan = serde_json::from_value(args)
                .map_err(|_| "exact company and readonly body required; no confirmation")?;
            if v.body.len() > MAX_OPENING_BALANCE_BODY {
                return Err("native64KiB body required");
            }
            api.opening_balance_plan(&v.company, v.body.into_bytes())
                .await
        }
        "opening_balance_apply" => {
            let v: Apply = serde_json::from_value(args)
                .map_err(|_| "exact company, body and confirm:true required")?;
            if !v.confirm || v.body.len() > MAX_OPENING_BALANCE_BODY {
                return Err("explicit confirmation/native64KiB body required");
            }
            api.opening_balance_apply(&v.company, v.body.into_bytes(), v.confirm)
                .await
        }
        _ => return Err("unknown opening tool"),
    })
}
