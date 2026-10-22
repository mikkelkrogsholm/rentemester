//! Three original expense tools. Native expense aliases are intentionally untouched.
use rentemester_client::{
    ApiClient, Error, original_expense_book_schema, validate_original_expense_book,
};
use serde_json::{Value, json};
const NAMES: [&str; 3] = [
    "expense_vat_preflight",
    "expense_vat_preflight_apply",
    "expense_book",
];
pub fn accepts(name: &str) -> bool {
    NAMES.contains(&name)
}
pub fn tools() -> Vec<Value> {
    NAMES.into_iter().map(|name| {
        let read = name == NAMES[0];
        let mut schema = if name == NAMES[2] { original_expense_book_schema(true) } else {
            json!({"type":"object","properties":{"documentId":{"type":"integer","minimum":1,"maximum":9007199254740991u64}},"required":["documentId"],"additionalProperties":false})
        };
        schema["properties"]["company"] = json!({"type":"string","minLength":1});
        schema["required"].as_array_mut().unwrap().push(json!("company"));
        if !read { schema["properties"]["confirm"] = json!({"type":"boolean"}); }
        let description = match name {
            "expense_vat_preflight" => "Read-only purchase VAT preflight. Derived region, required validation, cache freshness and safe evidence; never writes or contacts a provider.",
            "expense_vat_preflight_apply" => "Obtain required EU VAT evidence with confirm:true. Retains safe durable evidence and resumable exception when blocked. write-reversible; no automatic retry.",
            _ => "Book expense from retained original and imported bank transaction. Optional caller idempotencyKey; no generated key or automatic retry. write-irreversible.",
        };
        let mut tool = crate::tools::tool(name,description,schema,read);
        tool["annotations"] = json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read,"openWorldHint":name==NAMES[1]});
        tool["outputSchema"] = json!({"type":"object","properties":{"ok":{"type":"boolean"},"data":{"type":"object"},"errors":{"type":"array","items":{"type":"string"}},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}},"required":["ok","errors"],"additionalProperties":false});
        tool
    }).collect()
}
fn result(value: Value) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":value["ok"]!=true})
}
fn core(mut value: Value) -> Value {
    let ok = value["ok"] == true;
    let errors = value
        .as_object_mut()
        .unwrap()
        .remove("errors")
        .unwrap_or(json!([]));
    let rules = value.as_object_mut().unwrap().remove("appliedRules");
    value.as_object_mut().unwrap().remove("ok");
    let mut envelope = json!({"ok":ok,"errors":errors});
    if let Some(rules) = rules
        && rules.as_array().is_some_and(|a| !a.is_empty())
    {
        envelope["appliedRules"] = rules;
    }
    if ok {
        envelope["data"] = value;
    } else if let Some(code) = value.get("code") {
        envelope["code"] = code.clone();
    }
    // Source wrapCoreResult deliberately omits success data on failures, including preflight.
    result(envelope)
}
fn failure(error: Error) -> Value {
    let code = match error {
        Error::InvalidInput(_) => "invalid_input",
        Error::Refused { .. } => "server_refused",
        Error::Transport => "transport",
        Error::Protocol => "protocol",
        Error::OutcomeUnknown => "outcome_unknown",
    };
    result(json!({"ok":false,"errors":[error.to_string()],"code":code}))
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let read = name == NAMES[0];
    let book = name == NAMES[2];
    let mut object = args
        .as_object()
        .ok_or("flat expense arguments required")?
        .clone();
    let company = object
        .remove("company")
        .and_then(|v| v.as_str().map(str::to_owned))
        .filter(|s| !s.trim().is_empty())
        .ok_or("nonempty company required")?;
    let confirm = if read { None } else { object.remove("confirm") };
    if confirm.as_ref().is_some_and(|v| !v.is_boolean()) {
        return Err("confirm must be boolean");
    }
    let input = Value::Object(object);
    if book {
        validate_original_expense_book(&input, true).map_err(|_| "invalid strict expense input")?;
    } else if input.as_object().unwrap().len() != 1
        || input["documentId"]
            .as_i64()
            .is_none_or(|id| !(1..=9_007_199_254_740_991).contains(&id))
    {
        return Err("only positive safe documentId accepted; no caller actor/principal");
    }
    if !read && confirm != Some(json!(true)) {
        return Ok(result(
            json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":[format!("confirm: true required for write tool {name}")]}),
        ));
    }
    let response = if book {
        api.original_expense_book(&company, &input, true, None)
            .await
    } else if read {
        api.original_expense_vat_preflight(&company, input["documentId"].as_i64().unwrap())
            .await
    } else {
        api.original_expense_vat_preflight_apply(
            &company,
            input["documentId"].as_i64().unwrap(),
            true,
            None,
        )
        .await
    };
    Ok(match response {
        Ok(value) => core(value),
        Err(error) => failure(error),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_wrap_core_result_failure_omits_data_and_text_matches() {
        let value = core(
            json!({"ok":false,"documentId":3,"errors":["fresh EU VAT validation evidence is required"],"evidence":[],"exception":{"id":2,"status":"open"}}),
        );
        assert!(value["structuredContent"].get("data").is_none());
        assert_eq!(value["isError"], true);
        assert_eq!(
            serde_json::from_str::<Value>(value["content"][0]["text"].as_str().unwrap()).unwrap(),
            value["structuredContent"]
        );
    }
    #[test]
    fn source_success_retains_safe_evidence_and_key_receipt() {
        let receipt = json!({"replayed":true,"receiptId":2,"createdAt":"synthetic-created","expiresAt":"synthetic-expiry"});
        let value = core(
            json!({"ok":true,"errors":[],"appliedRules":["source-rule"],"entryId":8,"idempotency":receipt}),
        );
        assert_eq!(value["structuredContent"]["data"]["idempotency"], receipt);
        assert_eq!(
            value["structuredContent"]["appliedRules"],
            json!(["source-rule"])
        );
        assert_eq!(
            serde_json::from_str::<Value>(value["content"][0]["text"].as_str().unwrap()).unwrap(),
            value["structuredContent"]
        );
    }
}
