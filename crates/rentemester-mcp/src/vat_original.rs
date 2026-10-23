//! Original read-only VAT contracts; OSS retains its single existing registration.
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(name, "vat_report" | "vat_eu_sales_list" | "vat_oss_report")
}
pub fn tool(name: &str) -> Value {
    let mut t = crate::tools::tool(
        name,
        "Read-only source VAT projection. Current authority and source assurance required; EU sales retains external-lookup permission but invokes no provider. No submission or period changes.",
        json!({"type":"object","required":["company","from","to"],"properties":{"company":{"type":"string","minLength":1},"from":{"type":"string","minLength":1},"to":{"type":"string","minLength":1}},"additionalProperties":false}),
        true,
    );
    t["annotations"] = json!({"readOnlyHint":true,"idempotentHint":true,"destructiveHint":false,"openWorldHint":false});
    t["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"errors":{"type":"array","items":{"type":"string"}},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"},"data":{"type":"object"}},"additionalProperties":false});
    t
}
pub fn tools() -> Vec<Value> {
    ["vat_report", "vat_eu_sales_list"]
        .into_iter()
        .map(tool)
        .collect()
}
fn result(v: Value) -> Value {
    json!({"content":[{"type":"text","text":v.to_string()}],"structuredContent":v,"isError":v["ok"]!=true})
}
pub fn wrap(mut core: Value) -> Value {
    let o = core.as_object_mut().unwrap();
    let ok = o.remove("ok").unwrap();
    let errors = o.remove("errors").unwrap();
    let rules = o.remove("appliedRules");
    let code = o.remove("code");
    let mut v = json!({"ok":ok,"errors":errors});
    if let Some(r) = rules {
        v["appliedRules"] = r;
    }
    if let Some(c) = code {
        v["code"] = c;
    }
    if v["ok"] == true {
        v["data"] = core;
    } else if v["errors"] == json!(["selskabet er ikke momsregistreret"]) {
        v["code"] = json!("NOT_VAT_REGISTERED");
    }
    result(v)
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    if !rentemester_client::validate_original_mileage_shape(&args, &tool(name)["inputSchema"]) {
        return Err("exact nonempty company/from/to required");
    }
    let c = args["company"].as_str().unwrap();
    let from = args["from"].as_str().unwrap();
    let to = args["to"].as_str().unwrap();
    let outcome = match name {
        "vat_report" => api.original_vat_report(c, from, to).await,
        "vat_eu_sales_list" => api.original_vat_eu_sales_list(c, from, to).await,
        _ => api.original_vat_oss_report(c, from, to).await,
    };
    Ok(match outcome {
        Ok(v) => wrap(v),
        Err(e) => {
            let code = match e {
                Error::InvalidInput(_) => "invalid_input",
                Error::Transport => "transport",
                Error::Refused { .. } => "server_refused",
                Error::Protocol => "protocol",
                Error::OutcomeUnknown => "outcome_unknown",
            };
            result(json!({"ok":false,"code":code,"errors":[e.to_string()]}))
        }
    })
}
