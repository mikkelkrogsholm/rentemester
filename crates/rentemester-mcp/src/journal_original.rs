//! Original numeric journal surface. Authority comes only from current server credentials.
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};

pub fn accepts(name: &str) -> bool {
    matches!(name, "journal_dry_run" | "journal_post" | "journal_reverse")
}
fn object(properties: Value, required: Value) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn string() -> Value {
    json!({"type":"string"})
}
fn nonempty() -> Value {
    json!({"type":"string","minLength":1})
}
fn positive_id() -> Value {
    json!({"type":"integer","minimum":1,"maximum":9007199254740991_u64})
}
fn payload_schema() -> Value {
    let line = object(
        json!({"accountNo":nonempty(),"debitAmount":{"type":"number","minimum":0},"creditAmount":{"type":"number","minimum":0},"vatCode":string(),"text":string()}),
        json!(["accountNo"]),
    );
    object(
        json!({"transactionDate":nonempty(),"text":nonempty(),"documentId":positive_id(),"sourceBankTransactionId":positive_id(),"currency":{"type":"string","description":"Defaults to DKK in the server"},"amountForeign":{"type":"number"},"amountDkk":{"type":"number"},"fxRateToDkk":{"type":"number"},"lines":{"type":"array","minItems":2,"items":line}}),
        json!(["transactionDate", "text", "lines"]),
    )
}
fn schema(name: &str) -> Value {
    let mut properties = if name == "journal_reverse" {
        json!({"company":crate::tools::company_schema(),"date":nonempty(),"reason":nonempty(),"entryId":positive_id(),"entryNo":string(),"matchText":string(),"matchDate":string(),"matchDocumentId":positive_id()})
    } else {
        json!({"company":crate::tools::company_schema(),"payload":payload_schema()})
    };
    if name != "journal_dry_run" {
        properties["confirm"] = json!({"type":"boolean"});
        properties["idempotencyKey"] = json!({"type":"string","minLength":1,"maxLength":128});
    }
    object(
        properties,
        if name == "journal_reverse" {
            json!(["company", "date", "reason"])
        } else {
            json!(["company", "payload"])
        },
    )
}
pub fn tools() -> Vec<Value> {
    ["journal_dry_run", "journal_post", "journal_reverse"].into_iter().map(|name| {
        let read = name == "journal_dry_run";
        let mut tool = crate::tools::tool(name,
            "Original numeric journal, amounts in major units. Dry run uses the same writer and rolls back, reserving nothing. Writes require confirm:true; optional explicit caller key uses native receipts. Never automatically retry an unknown outcome; reconcile canonical journal/receipts. Current cookie/service authority only. Reversal safety correction: positive ID, then trimmed entry number without fallback, then unambiguous exact text with optional filters; never pick newest ambiguous match.", schema(name), read);
        tool["annotations"] = json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read,"openWorldHint":false});
        tool["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"data":{"type":"object"},"errors":{"type":"array","items":{"type":"string"}},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}},"additionalProperties":false});
        tool
    }).collect()
}
// Validate the published structured schema before confirmation or HTTP. Null is not omission.
fn valid(value: &Value, schema: &Value) -> bool {
    match schema["type"].as_str().unwrap() {
        "object" => {
            let Some(fields) = value.as_object() else {
                return false;
            };
            schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .all(|k| fields.contains_key(k.as_str().unwrap()))
                && fields
                    .iter()
                    .all(|(k, v)| schema["properties"].get(k).is_some_and(|s| valid(v, s)))
        }
        "string" => value.as_str().is_some_and(|s| {
            let length = s.encode_utf16().count() as u64;
            schema
                .get("minLength")
                .is_none_or(|v| length >= v.as_u64().unwrap())
                && schema
                    .get("maxLength")
                    .is_none_or(|v| length <= v.as_u64().unwrap())
        }),
        "boolean" => value.is_boolean(),
        "integer" => value.as_u64().is_some_and(|n| {
            n >= schema["minimum"].as_u64().unwrap() && n <= schema["maximum"].as_u64().unwrap()
        }),
        "number" => value.as_f64().is_some_and(|n| {
            n.is_finite()
                && schema
                    .get("minimum")
                    .is_none_or(|min| n >= min.as_f64().unwrap())
        }),
        "array" => value.as_array().is_some_and(|a| {
            a.len() as u64 >= schema["minItems"].as_u64().unwrap()
                && a.iter().all(|v| valid(v, &schema["items"]))
        }),
        _ => false,
    }
}
fn result(value: Value) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":value["ok"]!=true})
}
fn wrap(mut core: Value) -> Value {
    let fields = core.as_object_mut().unwrap();
    let ok = fields.remove("ok").unwrap();
    let errors = fields.remove("errors").unwrap();
    let rules = fields.remove("appliedRules");
    let mut envelope = json!({"ok":ok,"errors":errors});
    if let Some(rules) = rules {
        envelope["appliedRules"] = rules;
    }
    if ok == true {
        envelope["data"] = core;
    } else if let Some(code) = core.get("code") {
        envelope["code"] = code.clone();
    } else {
        let errors = envelope["errors"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        let code = if errors.contains("falls in closed period")
            || errors.contains("falls in reported period")
            || errors.contains("in a closed period")
            || errors.contains("perioden er lukket")
            || errors.contains("periode er lukket")
        {
            Some("PERIOD_CLOSED")
        } else if errors.contains("forudsætning ikke opfyldt")
            || errors.contains("precondition not met")
            || errors.contains("precondition missing")
            || errors.contains("must be posted")
            || errors.contains("skal bogf")
            || errors.contains("skal være bogf")
        {
            Some("PRECONDITION_MISSING")
        } else if errors.contains("does not exist")
            || errors.contains("findes ikke")
            || errors.contains("no such")
            || errors.contains("kunne ikke findes")
        {
            Some("NOT_FOUND")
        } else if errors.contains("ikke momsregistreret") {
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
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    if !accepts(name) || !valid(&args, &schema(name)) {
        return Err("invalid original journal arguments");
    }
    let company = args["company"].as_str().unwrap();
    if company.is_empty()
        || !company
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("invalid company slug");
    }
    if name != "journal_dry_run" && args["confirm"] != true {
        return Ok(result(
            json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":[format!("confirm: true required for write tool {name}")]}),
        ));
    }
    let mut wire = if name == "journal_reverse" {
        let mut wire = args.as_object().unwrap().clone();
        wire.remove("company");
        let date = wire.remove("date").unwrap();
        wire.insert("transactionDate".into(), date);
        Value::Object(wire)
    } else {
        json!({"input":args["payload"]})
    };
    let keyed = args.get("idempotencyKey").is_some();
    if name != "journal_dry_run" {
        wire["confirm"] = json!(true);
        if keyed {
            wire["idempotencyKey"] = args["idempotencyKey"].clone();
        }
    }
    let bytes = serde_json::to_vec(&wire).map_err(|_| "journal serialization failed")?;
    if bytes.len() > 65536 {
        return Err("journal request exceeds65536 bytes");
    }
    let outcome = match (name, keyed) {
        ("journal_dry_run", _) => api.journal_original_dry_run(company, bytes).await,
        ("journal_post", false) => api.journal_original_post(company, bytes, true).await,
        ("journal_post", true) => api.journal_original_keyed_post(company, bytes, true).await,
        (_, false) => api.journal_original_reverse(company, bytes, true).await,
        (_, true) => {
            api.journal_original_keyed_reverse(company, bytes, true)
                .await
        }
    };
    Ok(match outcome {
        Ok(core) => wrap(core),
        Err(error) => {
            let code = match error {
                Error::InvalidInput(_) => "invalid_input",
                Error::Refused { .. } => "server_refused",
                Error::Transport => "transport",
                Error::Protocol => "protocol",
                Error::OutcomeUnknown => "outcome_unknown",
            };
            result(json!({"ok":false,"code":code,"errors":[error.to_string()]}))
        }
    })
}
