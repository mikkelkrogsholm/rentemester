//! Source accrual names coexist with the four native aliases.
use rentemester_client::{
    ApiClient, Error, original_accrual_schema, validate_original_accrual,
    validate_original_accrual_actor,
};
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "accrual_register" | "accrual_recognize" | "accrual_register_report"
    )
}
pub fn tools() -> Vec<Value> {
    ["accrual_register","accrual_recognize","accrual_register_report"].into_iter().map(|name| {
        let read=name=="accrual_register_report";
        let mut schema=if read {json!({"type":"object","properties":{},"required":[],"additionalProperties":false})} else {original_accrual_schema(name=="accrual_register")};
        schema["properties"]["company"]=crate::tools::company_schema();
        schema["required"].as_array_mut().unwrap().push(json!("company"));
        if !read {
            schema["properties"]["confirm"]=json!({"type":"boolean"});
            schema["properties"]["expectedActor"]=json!({"type":"string","minLength":1,"maxLength":160,"description":"Equality assertion of current credential identity only, never authority"});
        }
        if name=="accrual_recognize" {schema["properties"]["accrualId"]=json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});schema["required"].as_array_mut().unwrap().push(json!("accrualId"));}
        let mut tool=crate::tools::tool(name,"Original accrual lifecycle through shared immutable journals. DKK two-decimal amount; final period carries remainder; period is1-based. Server resolves omitted dates, step1, documented balance accounts and confirmed bank role; never creates accounts. Prepaid/deferred balance-only registration may omit document, but expense/income postings always require evidence; supplied originals always checked. Recognition has no caller amount, date defaults to selected schedule. Current credential/company permission and period/source controls apply. Writes require confirm:true; expectedActor only asserts current identity. No keys, account inference, automatic recognition or retries; unknown writes require canonical report/journal reconciliation.",schema,read);
        tool["annotations"]=json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read,"openWorldHint":false});
        tool["outputSchema"]=json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"errors":{"type":"array","items":{"type":"string"}},"data":{"type":"object"},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}},"additionalProperties":false});
        tool
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
    if ok == true {
        v["data"] = core;
    } else if let Some(c) = core.get("code") {
        v["code"] = c.clone();
    }
    result(v)
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let mut fields = args.as_object().ok_or("expected accrual object")?.clone();
    let company = fields
        .remove("company")
        .and_then(|v| {
            v.as_str()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
        })
        .ok_or("company required")?;
    let outcome = if name == "accrual_register_report" {
        if !fields.is_empty() {
            return Err("report only accepts company");
        }
        api.original_accrual_register_report(&company).await
    } else {
        if !accepts(name) {
            return Err("unknown accrual tool");
        }
        let confirmed = match fields.remove("confirm") {
            None => false,
            Some(v) => v.as_bool().ok_or("confirm must be boolean")?,
        };
        let actor = fields
            .remove("expectedActor")
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or("expectedActor must be string")
            })
            .transpose()?;
        validate_original_accrual_actor(actor.as_deref())
            .map_err(|_| "invalid expectedActor assertion")?;
        let id = if name == "accrual_recognize" {
            Some(
                fields
                    .remove("accrualId")
                    .and_then(|v| v.as_i64())
                    .filter(|n| (1..=9_007_199_254_740_991).contains(n))
                    .ok_or("positive safe accrualId required")?,
            )
        } else {
            None
        };
        let input = Value::Object(fields);
        validate_original_accrual(&input, name == "accrual_register")
            .map_err(|_| "invalid accrual schema: null/unknown/identity/key/type fields refused")?;
        if !confirmed {
            return Ok(result(
                json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true required for accrual writes"]}),
            ));
        }
        if let Some(id) = id {
            api.original_accrual_recognize(&company, id, &input, true, actor.as_deref())
                .await
        } else {
            api.original_accrual_register(&company, &input, true, actor.as_deref())
                .await
        }
    };
    Ok(match outcome {
        Ok(v) => wrap(v),
        Err(e) => {
            let code = match e {
                Error::InvalidInput(_) => "invalid_input",
                Error::Refused { .. } => "server_refused",
                Error::Transport => "transport",
                Error::Protocol => "protocol",
                Error::OutcomeUnknown => "outcome_unknown",
            };
            result(json!({"ok":false,"code":code,"errors":[e.to_string()]}))
        }
    })
}
