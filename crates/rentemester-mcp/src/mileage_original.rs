//! Original mileage tools; files belong to the local stdio host, never HTTP.
use rentemester_client::{ApiClient, Error, original_mileage_log_schema, publish_original_mileage};
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "mileage_log" | "mileage_list" | "mileage_report" | "mileage_export"
    )
}
pub fn tools() -> Vec<Value> {
    ["mileage_log","mileage_list","mileage_report","mileage_export"].into_iter().map(|name|{
 let read=matches!(name,"mileage_list"|"mileage_report");
 let mut p=json!({"company":crate::tools::company_schema()});let mut required=vec!["company"];
 if name=="mileage_log" {let mut input=original_mileage_log_schema();
 for k in ["tripDate","purpose","fromLocation","toLocation","vehicle","driver","rateBasis"] {input["properties"][k]=json!({"type":"string","minLength":1,"maxLength":65536});}
 for k in ["kilometers","ratePerKm"] {input["properties"][k]=json!({"type":"number","exclusiveMinimum":0});input["required"].as_array_mut().unwrap().push(json!(k));}
 for k in ["rateSource","notes"] {input["properties"][k]=json!({"type":"string"});}
 p["input"]=input;required.push("input");}
 if name=="mileage_report"||name=="mileage_export"{for k in ["from","to"] {p[k]=json!({"type":"string","minLength":1});required.push(k);}}
 if name=="mileage_export" {p["outputDir"]=json!({"type":"string","minLength":1,"description":"Directory on the local stdio MCP host, not the remote Rust HTTP server. Fixed JSON/CSV files are replaced per-file atomically."});required.push("outputDir");}
 if !read {p["confirm"]=json!({"type":"boolean"});}
 let mut tool=crate::tools::tool(name,"Original mileage documentation only; no ledger/VAT posting. Caller supplies rate and basis. Log/export require confirm:true. Never automatically retry writes; local export failure may leave prepared server audit.",json!({"type":"object","properties":p,"required":required,"additionalProperties":false}),read);
 tool["annotations"]=json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read,"openWorldHint":false});
 tool["outputSchema"]=json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"errors":{"type":"array","items":{"type":"string"}},"data":{"type":"object"},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}},"additionalProperties":false});tool
 }).collect()
}
fn result(v: Value) -> Value {
    json!({"content":[{"type":"text","text":v.to_string()}],"structuredContent":v,"isError":v["ok"]!=true})
}
fn wrap(mut core: Value) -> Value {
    let fields = core.as_object_mut().unwrap();
    let ok = fields.remove("ok").unwrap();
    let errors = fields.remove("errors").unwrap();
    let rules = fields.remove("appliedRules");
    let mut v = json!({"ok":ok,"errors":errors});
    if let Some(r) = rules {
        v["appliedRules"] = r;
    }
    // The source envelope carries all remaining domain fields, including failed-range detail.
    if !core.as_object().unwrap().is_empty() {
        v["data"] = core;
    }
    result(v)
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let schema = tools()
        .into_iter()
        .find(|t| t["name"] == name)
        .ok_or("unknown mileage tool")?["inputSchema"]
        .clone();
    if !rentemester_client::validate_original_mileage_shape(&args, &schema) {
        return Err("invalid mileage arguments");
    }
    let company = args["company"].as_str().ok_or("company required")?;
    if matches!(name, "mileage_log" | "mileage_export") && args["confirm"] != true {
        return Ok(result(
            json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true required for mileage writes"]}),
        ));
    }
    let outcome = match name {
        "mileage_log" => {
            api.original_mileage_log(
                company,
                &serde_json::to_vec(&args["input"]).map_err(|_| "invalid mileage input")?,
                true,
            )
            .await
        }
        "mileage_list" => api.original_mileage_list(company).await,
        "mileage_report" => {
            api.original_mileage_report(
                company,
                args["from"].as_str().unwrap(),
                args["to"].as_str().unwrap(),
            )
            .await
        }
        "mileage_export" => {
            let b = serde_json::to_vec(&json!({"from":args["from"],"to":args["to"]})).unwrap();
            match api.original_mileage_export(company, &b, true).await {
                Ok(v) if v["ok"] == true => match publish_original_mileage(
                    &v,
                    std::path::Path::new(args["outputDir"].as_str().unwrap()),
                ) {
                    Ok(v) => Ok(v),
                    Err(e) => {
                        return Ok(result(
                            json!({"ok":false,"code":"local_export_failed","errors":[e.to_string()]}),
                        ));
                    }
                },
                other => other,
            }
        }
        _ => return Err("unknown mileage tool"),
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
