//! Original source planning contracts, including the source half of the single comparison overload.
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "budget_set" | "budget_list" | "budget_forecast" | "liquidity_forecast_13_week"
    )
}
pub fn comparison_schema() -> Value {
    json!({"type":"object","required":["company","from","to"],"properties":{"company":crate::tools::company_schema(),"from":{"type":"string","minLength":1},"to":{"type":"string","minLength":1}},"additionalProperties":false})
}
pub fn tools() -> Vec<Value> {
    ["budget_set","budget_list","budget_forecast","liquidity_forecast_13_week"].into_iter().map(|name|{
        let read=name!="budget_set";let mut p=json!({"company":crate::tools::company_schema()});let mut required=vec!["company"];
        match name {
            "budget_set"=>{p["accountNo"]=json!({"type":"string","minLength":1});p["period"]=json!({"type":"string","minLength":1});p["amount"]=json!({"type":"number","minimum":0});p["notes"]=json!({"type":"string"});p["confirm"]=json!({"type":"boolean"});required.extend(["accountNo","period","amount"]);}
            "budget_list"=>{for k in ["period","accountNo"] {p[k]=json!({"type":"string"});}}
            _=>{p["startDate"]=json!({"type":"string","minLength":1});required.push("startDate");if name=="budget_forecast" {p["months"]=json!({"type":"integer","minimum":1,"maximum":18});required.push("months");}else{p["weeks"]=json!({"type":"integer","minimum":1,"maximum":13});}}
        }
        let mut t=crate::tools::tool(name,"Original source budget/cash planning. Set appends one revision and requires confirm:true; reads never generate invoices or post journals. Server owns source provenance, base/scenario arithmetic and current authority. No automatic retries.",json!({"type":"object","properties":p,"required":required,"additionalProperties":false}),read);
        t["annotations"]=json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read,"openWorldHint":false});
        t["outputSchema"]=json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"errors":{"type":"array","items":{"type":"string"}},"data":{"type":"object"},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}},"additionalProperties":false});t
    }).collect()
}
fn result(v: Value) -> Value {
    json!({"content":[{"type":"text","text":v.to_string()}],"structuredContent":v,"isError":v["ok"]!=true})
}
fn wrap(mut core: Value) -> Value {
    let o = core.as_object_mut().unwrap();
    let ok = o.remove("ok").unwrap();
    let errors = o.remove("errors").unwrap();
    let rules = o.remove("appliedRules");
    let mut v = json!({"ok":ok,"errors":errors});
    if let Some(r) = rules {
        v["appliedRules"] = r;
    }
    if !core.as_object().unwrap().is_empty() {
        v["data"] = core;
    }
    result(v)
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let schema = if name == "budget_vs_actual" {
        comparison_schema()
    } else {
        tools()
            .into_iter()
            .find(|t| t["name"] == name)
            .ok_or("unknown budget tool")?["inputSchema"]
            .clone()
    };
    if !rentemester_client::validate_original_mileage_shape(&args, &schema) {
        return Err("invalid original budget arguments");
    }
    if name == "budget_set" && args["confirm"] != true {
        return Ok(result(
            json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true required for budget set"]}),
        ));
    }
    let c = args["company"].as_str().ok_or("company required")?;
    let outcome = match name {
        "budget_set" => {
            let mut b = args.clone();
            let o = b.as_object_mut().unwrap();
            o.remove("company");
            o.remove("confirm");
            api.original_budget_set(
                c,
                &serde_json::to_vec(&b).map_err(|_| "invalid budget JSON")?,
                true,
            )
            .await
        }
        "budget_list" => {
            api.original_budget_list(c, args["period"].as_str(), args["accountNo"].as_str())
                .await
        }
        "budget_vs_actual" => {
            api.original_budget_vs_actual(
                c,
                args["from"].as_str().unwrap(),
                args["to"].as_str().unwrap(),
            )
            .await
        }
        "budget_forecast" => {
            api.original_budget_forecast(
                c,
                args["startDate"].as_str().unwrap(),
                &args["months"].to_string(),
            )
            .await
        }
        _ => {
            let weeks = args.get("weeks").map(Value::to_string);
            api.original_budget_forecast_13_week(
                c,
                args["startDate"].as_str().unwrap(),
                weeks.as_deref(),
            )
            .await
        }
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
