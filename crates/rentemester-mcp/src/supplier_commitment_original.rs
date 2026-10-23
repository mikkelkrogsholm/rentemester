//! Source supplier commitment contracts over the frozen original SDK boundary.
//! Native source-bound proposals are deliberately not aliases for pure TS hashes/schedules.
use rentemester_client::{
    ApiClient, Error, SupplierCommitmentOriginalOperation as Op,
    original_supplier_commitment_schema, validate_original_supplier_commitment,
};
use serde_json::{Value, json};

fn operation(name: &str) -> Option<Op> {
    Some(match name {
        "supplier_commitment_plan" => Op::Plan,
        "supplier_commitment_apply" => Op::Apply,
        "supplier_commitment_list" => Op::List,
        "supplier_commitment_change" => Op::Change,
        "supplier_commitment_match" => Op::Match,
        "supplier_commitment_matches" => Op::Matches,
        "supplier_commitment_alerts" => Op::Alerts,
        _ => return None,
    })
}
pub fn accepts(name: &str) -> bool {
    operation(name).is_some()
}
fn writes(op: Op) -> bool {
    matches!(op, Op::Apply | Op::Change | Op::Match)
}
pub fn tools() -> Vec<Value> {
    ["supplier_commitment_plan", "supplier_commitment_apply", "supplier_commitment_list",
     "supplier_commitment_change", "supplier_commitment_match", "supplier_commitment_matches",
     "supplier_commitment_alerts"].into_iter().map(|name| {
        let op = operation(name).unwrap();
        let write = writes(op);
        let mut schema = original_supplier_commitment_schema(op);
        schema["properties"]["company"] = crate::tools::company_schema();
        schema["required"].as_array_mut().unwrap().push(json!("company"));
        if write {
            // Source confirmation is optional at discovery, but true is mandatory before HTTP.
            schema["properties"]["confirm"] = json!({"type":"boolean"});
            schema["required"].as_array_mut().unwrap().retain(|v| v != "confirm");
        }
        let mut tool = crate::tools::tool(name,
            "Original supplier commitments using the current native source-bound proposal/hash and nextDate+90 inclusive schedule, NOT offline pure TS hash parity. Plan requires an active local vendor and immutable document evidence; vendorSnapshot asserts the current vendor name only. Review the exact proposal before explicit approval. Writes require confirm:true/current CompanyDraftWrite; reads CompanyRead. Canonical match facts are resolved server-side from evidence kind/id, never caller amounts. Reasoned pause/end preserves history and removes future forecast occurrences. No payable, journal, payment, supplier message, automatic retry or identity fallback. Reconcile uncertain writes through canonical reads. Native nested matches remain native_supplier_commitment_matches.",
            schema, !write);
        tool["annotations"] = json!({"readOnlyHint":!write,"destructiveHint":false,"idempotentHint":!write,"openWorldHint":false});
        tool["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"errors":{"type":"array","items":{"type":"string"}},"data":{"type":"object"},"code":{"type":"string"}},"additionalProperties":false});
        tool
    }).collect()
}
fn result(value: Value) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":value["ok"]!=true})
}
fn wrap(mut core: Value) -> Value {
    let fields = core.as_object_mut().unwrap(); // SDK validates the flat core response.
    let ok = fields.remove("ok").unwrap();
    let errors = fields.remove("errors").unwrap_or_else(|| json!([]));
    let mut envelope = json!({"ok":ok,"errors":errors});
    if ok == true {
        envelope["data"] = core;
    } else {
        let text = envelope["errors"]
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
            || text.contains("periode er lukket")
        {
            Some("PERIOD_CLOSED")
        } else if text.contains("forudsætning ikke opfyldt")
            || text.contains("precondition not met")
            || text.contains("precondition missing")
            || text.contains("must be posted")
            || text.contains("skal være bogf")
            || text.contains("skal bogf")
        {
            Some("PRECONDITION_MISSING")
        } else if text.contains("does not exist")
            || text.contains("findes ikke")
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
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let op = operation(name).ok_or("unknown supplier commitment tool")?;
    let mut object = args
        .as_object()
        .ok_or("expected supplier commitment arguments")?
        .clone();
    let company = object
        .remove("company")
        .and_then(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned))
        .ok_or("company required")?;
    let confirmed = object.get("confirm") == Some(&json!(true));
    if writes(op) {
        if object.get("confirm").is_some_and(|v| !v.is_boolean()) {
            return Err("confirm must be boolean");
        }
        object.insert("confirm".into(), json!(true));
    }
    let input = Value::Object(object);
    validate_original_supplier_commitment(op, &input)
        .map_err(|_| "original supplier commitment input schema differs")?;
    if writes(op) && !confirmed {
        return Ok(result(
            json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":[format!("confirm: true required for write tool {name}")]}),
        ));
    }
    Ok(
        match api
            .original_supplier_commitment(&company, op, &input, None)
            .await
        {
            Ok(core) => wrap(core),
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
        },
    )
}
