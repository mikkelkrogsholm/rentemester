//! Existing source tool names, source envelopes, shared original HTTP transport.
use crate::tools::{company_schema, tool};
use rentemester_client::{ApiClient, Error};
use serde_json::{Value,json};
pub fn accepts(s: &str) -> bool { matches!(s,"exceptions_list"|"exception_resolve") }
pub fn tools() -> Vec<Value> {
    let mut tools = vec![
        tool("exceptions_list","List exceptions in descending ID order; open and unarchived by default.",json!({"type":"object","required":["company"],"properties":{"company":company_schema(),"status":{"type":"string","enum":["open","resolved","all"]},"includeArchived":{"type":"boolean"}},"additionalProperties":false}),true),
        tool("exception_resolve","Review one exception. Requires confirm:true. No accounting repair or automatic retry; server-owned identity.",json!({"type":"object","required":["company","id"],"properties":{"company":company_schema(),"id":{"type":"integer","minimum":1,"maximum":9_007_199_254_740_991_i64},"note":{"type":"string"},"confirm":{"type":"boolean"}},"additionalProperties":false}),false)
    ];
    for t in &mut tools { t["annotations"]["destructiveHint"] = json!(false); t["annotations"]["openWorldHint"] = json!(false); }
    tools
}
fn result(value: Value) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":value["ok"]!=true})
}
fn wrap(mut core: Value) -> Value {
    let fields = core.as_object_mut().unwrap();
    let ok = fields.remove("ok").unwrap();
    let errors = fields.remove("errors").unwrap();
    let rules = fields.remove("appliedRules");
    let code = fields.remove("code");
    let mut envelope = json!({"ok":ok,"errors":errors});
    if let Some(rules) = rules { envelope["appliedRules"] = rules; }
    if let Some(code) = code { envelope["code"] = code; }
    if ok == true { envelope["data"] = core; }
    result(envelope)
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value,&'static str> {
    let o = args.as_object().ok_or("expected exceptions arguments object")?;
    let write = name == "exception_resolve";
    if !accepts(name) || o.keys().any(|k| !if write { ["company","id","note","confirm"].contains(&k.as_str()) } else { ["company","status","includeArchived"].contains(&k.as_str()) }) { return Err("unknown exceptions argument"); }
    let company = args["company"].as_str().filter(|s| !s.is_empty()).ok_or("company required")?;
    if write {
        if !args["id"].as_i64().is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n))
            || args.get("note").is_some_and(|n| n.as_str().is_none_or(|s| s.len() > 4096))
            || args.get("confirm").is_some_and(|c| !c.is_boolean()) { return Err("invalid exception_resolve arguments"); }
        if args["confirm"] != true { return Ok(result(json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true required for write tool exception_resolve"]}))); }
    } else if args.get("status").is_some_and(|s| !matches!(s.as_str(),Some("open"|"resolved"|"all"))) || args.get("includeArchived").is_some_and(|v| !v.is_boolean()) { return Err("invalid exceptions_list arguments"); }
    let outcome = if write {
        let mut body = json!({});
        if let Some(note) = args.get("note") { body["note"] = note.clone(); }
        api.exceptions_original_resolve(company,args["id"].as_i64().unwrap(),serde_json::to_vec(&body).map_err(|_| "invalid note")?,true).await
    } else { api.exceptions_original_list(company,args.get("status").and_then(Value::as_str),args["includeArchived"] == true).await };
    Ok(match outcome {
        Ok(core) => wrap(core),
        Err(e) => {
            let code = match e { Error::InvalidInput(_) => "invalid_input", Error::Refused{..} => "server_refused", Error::Transport => "transport", Error::Protocol => "protocol", Error::OutcomeUnknown => "outcome_unknown" };
            result(json!({"ok":false,"code":code,"errors":[e.to_string()]}))
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn exception_source_confirm_and_null_note() {
        let api = ApiClient::new("http://127.0.0.1:9",true,None).unwrap();
        for confirm in [None,Some(false)] {
            let mut args = json!({"company":"synthetic","id":1});
            if let Some(c) = confirm { args["confirm"] = json!(c); }
            let out = call(&api,"exception_resolve",args).await.unwrap();
            assert_eq!(out["structuredContent"],json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true required for write tool exception_resolve"]}));
            assert_eq!(serde_json::from_str::<Value>(out["content"][0]["text"].as_str().unwrap()).unwrap(),out["structuredContent"]);
        }
        for extra in [json!({"note":null}),json!({"confirm":"true"}),json!({"actor":"user:caller"})] {
            let mut args = json!({"company":"synthetic","id":1,"confirm":true});
            args.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            assert!(call(&api,"exception_resolve",args).await.is_err());
        }
    }
    #[test]
    fn exception_source_annotations_and_envelopes() {
        let t = tools();
        assert_eq!(t[0]["annotations"],json!({"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}));
        assert_eq!(t[1]["annotations"],json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}));
        assert_eq!(wrap(json!({"ok":true,"resolved":false,"errors":[]}))["structuredContent"],json!({"ok":true,"data":{"resolved":false},"errors":[]}));
        assert!(wrap(json!({"ok":false,"resolved":false,"errors":["actual"],"code":"ACTUAL"}))["structuredContent"].get("data").is_none());
    }
}
