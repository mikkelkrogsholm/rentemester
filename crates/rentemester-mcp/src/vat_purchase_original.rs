//! The two original VAT purchase tools; native VAT aliases remain untouched.
use rentemester_client::{
    ApiClient, Error, original_vat_purchase_schema, validate_original_vat_purchase,
};
use serde_json::{Value, json};
const NAMES: [&str; 2] = [
    "vat_post_eu_service_purchase",
    "vat_post_representation_purchase",
];
pub fn accepts(name: &str) -> bool {
    NAMES.contains(&name)
}
pub fn tools() -> Vec<Value> {
    NAMES.into_iter().enumerate().map(|(index,name)| {
        let schema=json!({"type":"object","properties":{"company":{"type":"string","minLength":1},"payload":original_vat_purchase_schema(index==1),"confirm":{"type":"boolean"}},"required":["company","payload"],"additionalProperties":false});
        let mut tool=crate::tools::tool(name,"Post original VAT purchase with confirm:true. No automatic retry. netAmount is major DKK; bank optional. Representation expenseAccountNo omission requires a unique active expense account explicitly configured with default_vat_code=REPRESENTATION_SPECIAL, never fixed 3070; otherwise supply explicit expenseAccountNo.",schema,false);
        tool["annotations"]=json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false});
        tool["outputSchema"]=json!({"type":"object","properties":{"ok":{"type":"boolean"},"errors":{"type":"array","items":{"type":"string"}},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"},"data":{"type":"object","properties":{"entryId":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"entryNo":{"type":"string"},"entryHash":{"type":"string"}},"required":["entryId","entryNo","entryHash"],"additionalProperties":false}},"required":["ok","errors"],"additionalProperties":false});
        tool
    }).collect()
}
fn result(value: Value) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":value["ok"]!=true})
}
fn core(value: Value) -> Value {
    let mut envelope = json!({"ok":value["ok"],"errors":value["errors"]});
    if value["appliedRules"]
        .as_array()
        .is_some_and(|a| !a.is_empty())
    {
        envelope["appliedRules"] = value["appliedRules"].clone();
    }
    if value["ok"] == true {
        envelope["data"] = json!({"entryId":value["entryId"],"entryNo":value["entryNo"],"entryHash":value["entryHash"]});
    } else {
        let text = value["errors"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        let code = if text.contains("falls in closed period")
            || text.contains("falls in reported period")
            || text.contains("in a closed period")
            || text.contains("perioden er lukket")
        {
            Some("PERIOD_CLOSED")
        } else if text.contains("forudsætning ikke opfyldt")
            || text.contains("precondition not met")
            || text.contains("precondition missing")
            || text.contains("must be posted")
        {
            Some("PRECONDITION_MISSING")
        } else if text.contains("does not exist")
            || text.contains("findes ikke")
            || text.contains("not found")
            || text.contains("no such")
            || text.contains("kunne ikke findes")
        {
            Some("NOT_FOUND")
        } else if text.contains("ikke momsregistreret") {
            Some("NOT_VAT_REGISTERED")
        } else {
            None
        };
        if let Some(code) = code {
            envelope["code"] = json!(code);
        }
    }
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
    let object = args.as_object().ok_or("object arguments required")?;
    if object
        .keys()
        .any(|k| !matches!(k.as_str(), "company" | "payload" | "confirm"))
    {
        return Err("only company/payload/confirm accepted; no actor or program");
    }
    let company = args["company"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or("nonempty company required")?;
    if args.get("confirm").is_some_and(|v| !v.is_boolean()) {
        return Err("confirm must be boolean");
    }
    let input = args.get("payload").ok_or("payload required")?;
    validate_original_vat_purchase(input, name == NAMES[1])
        .map_err(|_| "invalid strict VAT purchase payload")?;
    if args["confirm"] != true {
        return Ok(result(
            json!({"ok":false,"errors":[format!("confirm: true required for write tool {name}")],"code":"CONFIRM_REQUIRED"}),
        ));
    }
    let response = if name == NAMES[0] {
        api.original_vat_eu_service_purchase(company, input, None)
            .await
    } else {
        api.original_vat_representation_purchase(company, input, None)
            .await
    };
    Ok(match response {
        Ok(value) => core(value),
        Err(error) => failure(error),
    })
}
