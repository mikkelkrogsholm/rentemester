//! Exactly the three source posting-rule bindings; no lifecycle/test aliases.
use rentemester_client::{
    ApiClient, Error, PostingRulesOriginalOperation as Op, original_posting_rules_schema,
    validate_original_posting_rules,
};
use serde_json::{Value, json};
const NAMES: [&str; 3] = [
    "posting_rule_propose",
    "posting_rule_approve",
    "posting_rule_explain",
];
pub fn accepts(name: &str) -> bool {
    NAMES.contains(&name)
}
fn operation(name: &str) -> Op {
    match name {
        "posting_rule_propose" => Op::Propose,
        "posting_rule_approve" => Op::Approve,
        _ => Op::Explain,
    }
}
fn schema(name: &str) -> Value {
    let mut s = original_posting_rules_schema(operation(name));
    s["properties"]["company"] = json!({"type":"string","minLength":1});
    if operation(name).write() {
        s["properties"]["confirm"] = json!({"type":"boolean"});
    }
    let r = s["required"].as_array_mut().unwrap();
    r.retain(|k| k != "confirm");
    r.insert(0, json!("company"));
    s
}
pub fn tools() -> Vec<Value> {
    NAMES.into_iter().map(|name|{
    let op=operation(name);
    let mut tool=crate::tools::tool(name,"Original source posting rules. Exact-hash independent approval; confirm:true required for writes before HTTP. Dry runs never grant purchase authority. No automatic retry; reconcile uncertain writes using reads.",schema(name),!op.write());
    tool["annotations"]=json!({"readOnlyHint":!op.write(),"destructiveHint":false,"idempotentHint":op!=Op::Approve,"openWorldHint":false});
    tool
}).collect()
}
fn result(envelope: Value) -> Value {
    json!({"isError":envelope["ok"]!=true,"content":[{"type":"text","text":envelope.to_string()}],"structuredContent":envelope})
}
fn wrap(mut core: Value) -> Value {
    let ok = core.get("ok").is_none_or(|v| v == true);
    let mut envelope = json!({"ok":ok,"errors":core.get("errors").cloned().unwrap_or(json!([]))});
    if ok {
        let o = core.as_object_mut().unwrap();
        o.remove("ok");
        o.remove("errors");
        envelope["data"] = core;
    }
    result(envelope)
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    if !accepts(name) {
        return Err("unknown posting-rule tool");
    }
    let o = args.as_object().ok_or("object required")?;
    let company = args["company"]
        .as_str()
        .filter(|s| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
        .ok_or("invalid company")?;
    let op = operation(name);
    let s = schema(name);
    if o.keys().any(|k| s["properties"].get(k).is_none())
        || args.get("confirm").is_some_and(|v| !v.is_boolean())
    {
        return Err("unknown or invalid posting-rule field");
    }
    let mut body = args.clone();
    body.as_object_mut().unwrap().remove("company");
    // Validate domain shape offline even when the optional discovery confirmation is absent.
    if op.write() {
        body["confirm"] = json!(true);
    }
    validate_original_posting_rules(op, &body)
        .map_err(|_| "invalid original posting-rule arguments")?;
    if op.write() && args["confirm"] != true {
        return Ok(result(
            json!({"ok":false,"errors":[format!("confirm: true required for write tool {name}")],"code":"CONFIRM_REQUIRED"}),
        ));
    }
    Ok(
        match api
            .original_posting_rules(company, op, &body, None, None)
            .await
        {
            Ok(core) => wrap(core),
            Err(error) => {
                let code = match error {
                    Error::InvalidInput(_) => "invalid_input",
                    Error::Refused { .. } => "server_refused",
                    Error::Transport => "transport",
                    Error::Protocol => "protocol",
                    Error::OutcomeUnknown => "outcome_unknown",
                };
                result(json!({"ok":false,"errors":[error.to_string()],"code":code}))
            }
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_source_envelopes() {
        let read = json!({"decision":"human_decision","matched":[],"explanations":[],"reasons":["no approved rule matched"],"dryRun":true});
        assert_eq!(
            wrap(read.clone())["structuredContent"],
            json!({"ok":true,"errors":[],"data":read})
        );
        assert_eq!(
            wrap(json!({"ok":true,"errors":[]}))["structuredContent"],
            json!({"ok":true,"errors":[],"data":{}})
        );
        assert_eq!(
            wrap(json!({"ok":false,"errors":["refused"]}))["structuredContent"],
            json!({"ok":false,"errors":["refused"]})
        );
    }
}
