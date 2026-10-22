//! Pure shared engine validation: no company, allocation, provider, PDF or ledger access.
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tool() -> Value {
    let mut tool = crate::tools::tool(
        "invoice_validate",
        "Validate the supplied original invoice payload offline using the shared pure engine. Amounts are decimal currency units, vatRate is fractional (0.25 =25%). No company, confirmation or effects.",
        json!({"type":"object","required":["payload"],"properties":{"payload":{"type":"object"}},"additionalProperties":false}),
        true,
    );
    tool["annotations"] = json!({"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false});
    tool
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    payload: Value,
}
pub fn call(args: Value) -> Result<Value, &'static str> {
    let input: Input =
        serde_json::from_value(args).map_err(|_| "only payload required; no company or confirm")?;
    if !input.payload.is_object() {
        return Err("payload must be object");
    }
    let data = rentemester_engine::invoice_validation::validate(&input.payload);
    let mut value = json!({"ok":data["ok"]==true,"errors":data.get("errors").cloned().unwrap_or(json!([])),"data":data});
    if let Some(rules) = data.get("appliedRules") {
        value["appliedRules"] = rules.clone();
    }
    Ok(
        json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":value["ok"]!=true}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pure_tool_rejects_authority_and_calls_shared_verdict() {
        for extra in ["company", "confirm", "actor"] {
            let mut args = json!({"payload":{}});
            args[extra] = json!("forged");
            assert!(call(args).is_err());
        }
        let expected = rentemester_engine::invoice_validation::validate(&json!({}));
        let result = call(json!({"payload":{}})).unwrap();
        assert_eq!(result["structuredContent"]["data"], expected);
        assert_eq!(result["isError"], true);
    }
}
