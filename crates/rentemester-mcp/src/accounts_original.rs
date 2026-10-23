//! Exact source MCP projections, deliberately distinct from CLI/core receipts.
use rentemester_client::{
    AccountsOriginalOperation as Op, ApiClient, original_accounts_schema,
    validate_original_accounts,
};
use serde_json::{Value, json};
fn operation(name: &str) -> Option<Op> {
    Some(match name {
        "accounts_list" => Op::List,
        "accounts_add" => Op::Add,
        "accounts_roles_status" => Op::RolesStatus,
        "accounts_role_confirm" => Op::RoleConfirm,
        _ => return None,
    })
}
pub fn accepts(name: &str) -> bool {
    operation(name).is_some()
}
pub fn tools() -> Vec<Value> {
    ["accounts_list","accounts_add","accounts_roles_status","accounts_role_confirm"].into_iter().map(|name|{
        let op=operation(name).unwrap();let read=matches!(op,Op::List|Op::RolesStatus);
        let mut s=original_accounts_schema(op);s["properties"]["company"]=json!({"type":"string","minLength":1});
        let required=s["required"].as_array_mut().unwrap();required.retain(|v|v!="confirm");required.push(json!("company"));
        if !read{s["properties"]["confirm"]=json!({"type":"boolean"});}
        if op==Op::Add{s["properties"]["input"]["properties"]["normalBalance"]=json!({"type":"string","enum":["debit","credit"]});}
        let mut t=crate::tools::tool(name,"Original chart and eight required roles. Current server authority; writes require confirm:true. No actor, principal, local policy or automatic retries.",s,read);
        t["annotations"]=json!({"readOnlyHint":read,"idempotentHint":read,"destructiveHint":false,"openWorldHint":false});
        t["outputSchema"]=json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"errors":{"type":"array","items":{"type":"string"}},"data":{"type":"object"},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}},"additionalProperties":false});t
    }).collect()
}
fn result(v: Value) -> Value {
    json!({"content":[{"type":"text","text":v.to_string()}],"structuredContent":v,"isError":v["ok"]!=true})
}
fn project(op: Op, mut v: Value) -> Value {
    if v["ok"] != true {
        return json!({"ok":false,"errors":v["errors"]});
    }
    match op {
        Op::List => {
            json!({"ok":true,"errors":[],"data":{"count":v["count"],"accounts":v["rows"].as_array().unwrap().iter().map(|r|json!({"accountNo":r["account_no"],"name":r["name"],"type":r["type"],"defaultVatCode":r["default_vat_code"]})).collect::<Vec<_>>()}})
        }
        Op::Add => {
            json!({"ok":true,"errors":[],"appliedRules":v["appliedRules"],"data":{"accountNo":v["accountNo"]}})
        }
        Op::RolesStatus => {
            v.as_object_mut().unwrap().remove("ok");
            json!({"ok":true,"errors":[],"data":v})
        }
        Op::RoleConfirm => json!({"ok":true,"errors":[],"data":v}),
    }
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let op = operation(name).ok_or("unknown accounts tool")?;
    let write = matches!(op, Op::Add | Op::RoleConfirm);
    let mut o = args.as_object().ok_or("accounts object required")?.clone();
    let company = o
        .remove("company")
        .and_then(|v| v.as_str().map(str::to_owned))
        .filter(|s| !s.trim().is_empty())
        .ok_or("company required")?;
    if write {
        if o.get("confirm").is_some_and(|v| !v.is_boolean()) {
            return Err("confirm must be boolean");
        }
        if op == Op::Add
            && o.get("input")
                .and_then(|v| v.get("normalBalance"))
                .is_some_and(|v| v.as_str().is_none_or(|s| !matches!(s, "debit" | "credit")))
        {
            return Err("normalBalance must be debit or credit");
        }
        let confirmed = o.get("confirm") == Some(&json!(true));
        o.insert("confirm".into(), json!(true));
        validate_original_accounts(op, &Value::Object(o.clone()))
            .map_err(|_| "invalid accounts input")?;
        if !confirmed {
            return Ok(result(
                json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true required for accounts write"]}),
            ));
        }
    }
    let input = Value::Object(o);
    validate_original_accounts(op, &input).map_err(|_| "invalid accounts input")?;
    Ok(result(
        match api
            .original_accounts(&company, op, &input, None, None)
            .await
        {
            Ok(v) => project(op, v),
            Err(e) => json!({"ok":false,"errors":[e.to_string()]}),
        },
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_distinct_projections() {
        let add = project(
            Op::Add,
            json!({"ok":true,"errors":[],"accountNo":"1790","appliedRules":["DK-CHART-OF-ACCOUNTS-ADD-001"]}),
        );
        assert_eq!(
            add,
            json!({"ok":true,"errors":[],"appliedRules":["DK-CHART-OF-ACCOUNTS-ADD-001"],"data":{"accountNo":"1790"}})
        );
        let core = json!({"ok":true,"resolution":{"role":"bank"}});
        assert_eq!(project(Op::RoleConfirm, core.clone())["data"], core);
        assert_eq!(
            project(
                Op::RoleConfirm,
                json!({"ok":false,"errors":["incompatible"],"status":"incomplete"})
            ),
            json!({"ok":false,"errors":["incompatible"]})
        );
        assert_eq!(
            project(
                Op::List,
                json!({"ok":true,"count":1,"rows":[{"account_no":"1790","name":"Bank","type":"asset","default_vat_code":null}]})
            )["data"]["accounts"][0],
            json!({"accountNo":"1790","name":"Bank","type":"asset","defaultVatCode":null})
        );
    }
}
