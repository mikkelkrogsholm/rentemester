//! Source asset names coexist with the four native aliases.
use rentemester_client::{
    ApiClient, Error, original_asset_schema, validate_original_asset, validate_original_asset_actor,
};
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "asset_register" | "asset_depreciate" | "asset_write_off" | "asset_register_report"
    )
}
pub fn tools() -> Vec<Value> {
    ["asset_register","asset_depreciate","asset_write_off","asset_register_report"].into_iter().map(|name| {
        let read=name=="asset_register_report";
        let mut schema=if read {json!({"type":"object","properties":{},"required":[],"additionalProperties":false})} else {original_asset_schema(match name {"asset_register"=>"register","asset_depreciate"=>"depreciate",_=>"write-off"})};
        schema["properties"]["company"]=crate::tools::company_schema();
        schema["required"].as_array_mut().unwrap().push(json!("company"));
        if !read {
            schema["properties"]["confirm"]=json!({"type":"boolean"});
            schema["properties"]["expectedActor"]=json!({"type":"string","minLength":1,"maxLength":160,"description":"Equality assertion of current credential identity only, never authority"});
        }
        if name=="asset_depreciate" {schema["properties"]["assetId"]=json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});schema["required"].as_array_mut().unwrap().push(json!("assetId"));}
        let mut tool=crate::tools::tool(name,"Original asset lifecycle via shared server. Numeric DKK and source field names only. Registration declares metadata, creates no journal; physical original and proven unallocated capitalisation required (intentionally stricter compatibility). Server defaults5800/5810/5820 and confirmed bank payment role; no account creation. Depreciation requires explicit period/date, no amount/next. Write-off requires explicit advisor/user thresholdRuleSource and BOOLEAN confirmImmediateWriteOff; false is a legitimate business refusal. Existing acquisition-year guardrail: through2024 33100,2025 34400,2026 onward36000 DKK, not new legal/tax/VAT advice. Outer confirm:true authorizes all writes, expectedActor asserts current credential only. Current company/period/evidence controls apply; no keys or retries. Unknown writes require canonical report/journal reconciliation.",schema,read);
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
    let mut fields = args.as_object().ok_or("expected asset object")?.clone();
    let company = fields
        .remove("company")
        .and_then(|v| {
            v.as_str()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
        })
        .ok_or("company required")?;
    let outcome = if name == "asset_register_report" {
        if !fields.is_empty() {
            return Err("report only accepts company");
        }
        api.original_asset_register_report(&company).await
    } else {
        if !accepts(name) {
            return Err("unknown asset tool");
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
        validate_original_asset_actor(actor.as_deref())
            .map_err(|_| "invalid expectedActor assertion")?;
        let id = if name == "asset_depreciate" {
            Some(
                fields
                    .remove("assetId")
                    .and_then(|v| v.as_i64())
                    .filter(|n| (1..=9_007_199_254_740_991).contains(n))
                    .ok_or("positive safe assetId required")?,
            )
        } else {
            None
        };
        let input = Value::Object(fields);
        validate_original_asset(
            &input,
            match name {
                "asset_register" => "register",
                "asset_depreciate" => "depreciate",
                _ => "write-off",
            },
        )
        .map_err(|_| "invalid asset schema: null/unknown/identity/key/type fields refused")?;
        if !confirmed {
            return Ok(result(
                json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true required for asset writes"]}),
            ));
        }
        if let Some(id) = id {
            api.original_asset_depreciate(&company, id, &input, true, actor.as_deref())
                .await
        } else if name == "asset_write_off" {
            api.original_asset_write_off(&company, &input, true, actor.as_deref())
                .await
        } else {
            api.original_asset_register(&company, &input, true, actor.as_deref())
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
