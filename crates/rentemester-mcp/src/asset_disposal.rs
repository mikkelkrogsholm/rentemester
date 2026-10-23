use rentemester_client::{ApiClient, Error, asset_disposal_schema};
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "asset_disposal_preview" | "asset_dispose" | "asset_disposal_receipt"
    )
}
pub fn tools() -> Vec<Value> {
    ["asset_disposal_preview","asset_dispose","asset_disposal_receipt"].into_iter().map(|name|{
 let receipt=name=="asset_disposal_receipt"; let apply=name=="asset_dispose";
 let mut schema=if receipt {json!({"type":"object","properties":{"key":{"type":"string","minLength":8,"maxLength":160}},"required":["key"],"additionalProperties":false})}else{asset_disposal_schema(apply)};
 schema["properties"]["company"]=crate::tools::company_schema();schema["properties"]["assetId"]=json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
 schema["required"].as_array_mut().unwrap().extend([json!("company"),json!("assetId")]);
 crate::tools::tool(name,"Capitalized asset derecognition, not sale/VAT or immediate write-off. Review server preview then explicitly confirm the same intent/hash/key. Never retry unknown writes; explicitly read receipt under current authority.",schema,!apply)
}).collect()
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let mut fields = args.as_object().ok_or("object required")?.clone();
    let company = fields
        .remove("company")
        .and_then(|v| v.as_str().map(str::to_owned))
        .filter(|s| !s.trim().is_empty())
        .ok_or("company required")?;
    let id = fields
        .remove("assetId")
        .and_then(|v| v.as_i64())
        .filter(|n| (1..=9007199254740991).contains(n))
        .ok_or("positive safe assetId required")?;
    let result = match name {
        "asset_disposal_preview" => {
            api.asset_disposal_preview(&company, id, &Value::Object(fields))
                .await
        }
        "asset_dispose" => {
            api.asset_dispose(&company, id, &Value::Object(fields), true)
                .await
        }
        "asset_disposal_receipt" => {
            let key = fields
                .remove("key")
                .and_then(|v| v.as_str().map(str::to_owned))
                .ok_or("key required")?;
            if !fields.is_empty() {
                return Err("unknown receipt field");
            }
            api.asset_disposal_receipt(&company, id, &key).await
        }
        _ => return Err("unknown disposal tool"),
    };
    let v = match result {
        Ok(v) => v,
        Err(e) => {
            let code = match e {
                Error::OutcomeUnknown => "outcome_unknown",
                Error::InvalidInput(_) => "invalid_input",
                Error::Refused { .. } => "server_refused",
                Error::Protocol => "protocol",
                Error::Transport => "transport",
            };
            json!({"ok":false,"code":code,"errors":[e.to_string()]})
        }
    };
    Ok(
        json!({"content":[{"type":"text","text":v.to_string()}],"structuredContent":v,"isError":v["ok"]!=true}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn asset_disposal_tools_are_additive_and_strict() {
        let list = tools();
        assert_eq!(list.len(), 3);
        for tool in list {
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        }
        assert!(!accepts("asset_write_off"));
        assert!(accepts("asset_dispose"));
    }
    #[test]
    fn asset_disposal_mcp_requires_confirm_without_network() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
        let api = ApiClient::new("http://127.0.0.1:1", true, None).unwrap();
        let v=call(&api,"asset_dispose",json!({"company":"synthetic","assetId":3,"transactionDate":"2026-10-20","documentId":7,"reason":"Retired","expectedPlanHash":"a".repeat(64),"idempotencyKey":"synthetic-key"})).await.unwrap();
        assert_eq!(v["structuredContent"]["code"], "invalid_input");
        });
    }
}
