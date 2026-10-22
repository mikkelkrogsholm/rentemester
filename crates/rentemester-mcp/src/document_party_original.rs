//! Original document-party tools. Attribution comes exclusively from current
//! server admission; MCP exposes no caller authority or filesystem selectors.
use rentemester_client::{
    ApiClient, DocumentPartyOriginalOperation as Operation, Error, document_party_original_schema,
};
use serde_json::{Value, json};

const NAMES: [&str; 7] = [
    "documents_party_link_plan",
    "documents_party_link_apply",
    "documents_party_link_inspect",
    "documents_party_link_list",
    "documents_party_link_supersede",
    "documents_internal_no_external_party",
    "documents_internal_no_external_party_supersede",
];
pub fn accepts(name: &str) -> bool {
    NAMES.contains(&name)
}
fn operation(name: &str) -> Option<Operation> {
    match name {
        "documents_party_link_plan" => Some(Operation::Plan),
        "documents_party_link_apply" => Some(Operation::Apply),
        "documents_party_link_supersede" => Some(Operation::Supersede),
        "documents_internal_no_external_party" => Some(Operation::Internal),
        "documents_internal_no_external_party_supersede" => Some(Operation::InternalSupersede),
        _ => None,
    }
}
fn read(name: &str) -> bool {
    matches!(
        name,
        "documents_party_link_plan" | "documents_party_link_inspect" | "documents_party_link_list"
    )
}
fn schema(name: &str) -> Value {
    let mut schema = operation(name)
        .map(document_party_original_schema)
        .unwrap_or_else(|| json!({"type":"object","properties":{},"required":[]}));
    let properties = schema["properties"].as_object_mut().unwrap();
    properties.remove("expectedActor");
    properties.remove("expectedPrincipal");
    properties.insert("company".into(), json!({"type":"string","minLength":1}));
    if !read(name) {
        properties.insert("confirm".into(), json!({"type":"boolean"}));
    }
    if name == "documents_party_link_list" {
        properties.insert("status".into(), json!({"type":"string","enum":["linked","unlinked","resolved","internal_no_external_party","unresolved"]}));
    }
    if name == "documents_party_link_inspect" {
        properties.insert(
            "documentId".into(),
            json!({"type":"integer","minimum":1,"maximum":9007199254740991u64}),
        );
    }
    let required = schema["required"].as_array_mut().unwrap();
    required.retain(|v| {
        !matches!(
            v.as_str(),
            Some("confirm" | "expectedActor" | "expectedPrincipal")
        )
    });
    required.insert(0, json!("company"));
    if name == "documents_party_link_inspect" {
        required.push(json!("documentId"));
    }
    schema["additionalProperties"] = json!(false);
    schema
}
pub fn tools() -> Vec<Value> {
    NAMES.into_iter().map(|name| {
        let description = match name {
            "documents_party_link_plan" => "Read-only exact evidence-bound plan for one visible company-scoped party; never fuzzy name matching. Success retains data.ok:true; domain failures are outer failures.",
            "documents_party_link_list" => "Read current original document resolutions, document ID descending, at most 100. linked/unlinked alias resolved/unresolved. No pagination or private registry disclosure.",
            "documents_party_link_inspect" => "Read authorized append-only party-link provenance. JSON columns remain strings; no private idempotency keys.",
            _ => "Confirmed append-only original document-party resolution; never changes originals, VAT or journals. Requires confirm:true before HTTP. Optional idempotencyKey is never invented; no automatic retry. Reconcile unknown outcomes using canonical reads. Identity derives only from current server credentials.",
        };
        let mut tool = crate::tools::tool(name, description, schema(name), read(name));
        tool["annotations"] = json!({"readOnlyHint":read(name),"destructiveHint":false,"idempotentHint":true,"openWorldHint":false});
        tool["outputSchema"] = output_schema(name);
        tool
    }).collect()
}
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn output_schema(name: &str) -> Value {
    let data = match name {
        "documents_party_link_plan" => object(
            json!({"ok":{"const":true},"plan":object(json!({
            "documentId":{"type":"integer"},"documentSha256":{"type":"string"},"documentPayloadSha256":{"type":"string"},
            "partyId":{"type":"string"},"partySnapshot":{"type":"object"},"role":{"type":"string"},"evidence":{"type":"object"},"planHash":{"type":"string"}
        }), &["documentId","documentSha256","documentPayloadSha256","partyId","partySnapshot","role","evidence","planHash"])}),
            &["ok", "plan"],
        ),
        "documents_party_link_list" | "documents_party_link_inspect" => {
            let row = if name == "documents_party_link_list" {
                object(
                    json!({"id":{"type":"integer"},"document_no":{"type":["string","null"]},"sha256_hash":{"type":"string"},"linked":{"enum":[0,1]},"resolution_state":{"enum":["resolved","unresolved","internal_no_external_party"]}}),
                    &[
                        "id",
                        "document_no",
                        "sha256_hash",
                        "linked",
                        "resolution_state",
                    ],
                )
            } else {
                let mut fields = json!({"id":{"type":"integer"}});
                for key in [
                    "party_id",
                    "party_role",
                    "event_type",
                    "evidence_kind",
                    "evidence_json",
                    "document_sha256",
                    "document_payload_sha256",
                    "party_snapshot_json",
                    "plan_hash",
                    "actor",
                    "principal",
                    "created_at",
                ] {
                    fields[key] = json!({"type":"string"});
                }
                fields["reason"] = json!({"type":["string","null"]});
                let keys: Vec<String> = fields.as_object().unwrap().keys().cloned().collect();
                let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
                object(fields, &keys)
            };
            object(json!({"links":{"type":"array","items":row}}), &["links"])
        }
        _ => {
            let supersede = name.ends_with("supersede");
            let mut fields = json!({"id":{"type":if supersede {json!(["integer","null"])} else {json!("integer")}},"idempotent":{"type":"boolean"}});
            let mut required = vec!["id", "idempotent"];
            if name == "documents_party_link_apply" {
                fields["planHash"] = json!({"type":"string"});
                required.push("planHash");
            }
            if name == "documents_internal_no_external_party" {
                fields["decisionHash"] = json!({"type":"string"});
                required.push("decisionHash");
            }
            object(fields, &required)
        }
    };
    object(
        json!({"ok":{"type":"boolean"},"errors":{"type":"array","items":{"type":"string"}},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"},"data":data}),
        &["ok", "errors"],
    )
}
fn valid(value: &Value, schema: &Value) -> bool {
    for union in ["anyOf", "oneOf"] {
        if let Some(choices) = schema[union].as_array() {
            return choices.iter().any(|s| valid(value, s));
        }
    }
    if let Some(expected) = schema.get("const") {
        return value == expected;
    }
    if schema
        .get("enum")
        .and_then(Value::as_array)
        .is_some_and(|choices| !choices.contains(value))
    {
        return false;
    }
    if let Some(types) = schema["type"].as_array() {
        return types.iter().any(|ty| {
            let mut s = schema.clone();
            s["type"] = ty.clone();
            valid(value, &s)
        });
    }
    match schema["type"].as_str() {
        Some("object") => value.as_object().is_some_and(|fields| {
            schema["required"].as_array().is_none_or(|keys| {
                keys.iter()
                    .all(|k| k.as_str().is_some_and(|k| fields.contains_key(k)))
            }) && fields.iter().all(|(key, value)| {
                schema["properties"]
                    .get(key)
                    .is_some_and(|s| valid(value, s))
            })
        }),
        Some("array") => value.as_array().is_some_and(|items| {
            schema["minItems"]
                .as_u64()
                .is_none_or(|n| items.len() as u64 >= n)
                && schema["maxItems"]
                    .as_u64()
                    .is_none_or(|n| items.len() as u64 <= n)
                && items.iter().all(|v| valid(v, &schema["items"]))
        }),
        Some("string") => value.as_str().is_some_and(|s| {
            let n = s.chars().count() as u64;
            schema["minLength"].as_u64().is_none_or(|min| n >= min)
                && schema["maxLength"].as_u64().is_none_or(|max| n <= max)
        }),
        Some("integer") => value.as_u64().is_some_and(|n| {
            n <= 9_007_199_254_740_991
                && schema["minimum"].as_u64().is_none_or(|min| n >= min)
                && schema["maximum"].as_u64().is_none_or(|max| n <= max)
        }),
        Some("number") => value.as_f64().is_some_and(|n| {
            n.is_finite()
                && schema["minimum"].as_f64().is_none_or(|min| n >= min)
                && schema["maximum"].as_f64().is_none_or(|max| n <= max)
        }),
        Some("boolean") => value.is_boolean(),
        Some("null") => value.is_null(),
        None => schema.get("enum").is_some(),
        _ => false,
    }
}

fn result(envelope: Value) -> Value {
    json!({"isError":envelope["ok"]!=true,"content":[{"type":"text","text":envelope.to_string()}],"structuredContent":envelope})
}
fn transport_error(error: Error) -> Value {
    let code = match error {
        Error::InvalidInput(_) => "invalid_input",
        Error::Refused { .. } => "server_refused",
        Error::Transport => "transport",
        Error::Protocol => "protocol",
        Error::OutcomeUnknown => "outcome_unknown",
    };
    result(json!({"ok":false,"code":code,"errors":[error.to_string()]}))
}
fn wrap(name: &str, mut core: Value) -> Value {
    let ok = core["ok"] == true;
    let mut envelope = json!({"ok":ok,"errors":core.get("errors").cloned().unwrap_or(json!([]))});
    if let Some(rules) = core.get("appliedRules") {
        envelope["appliedRules"] = rules.clone();
    }
    if ok {
        if name == "documents_party_link_plan" {
            envelope["data"] = json!({"ok":true,"plan":core["plan"]});
        } else {
            let fields = core.as_object_mut().unwrap();
            for key in ["ok", "errors", "appliedRules"] {
                fields.remove(key);
            }
            if name == "documents_party_link_supersede" {
                fields.remove("planHash");
            }
            envelope["data"] = core;
        }
    } else if let Some(code) = core.get("code") {
        envelope["code"] = code.clone();
    }
    result(envelope)
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    if !accepts(name) || !valid(&args, &schema(name)) {
        return Err("invalid original document-party arguments");
    }
    let company = args["company"].as_str().unwrap();
    if !company
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("invalid company slug");
    }
    for key in ["planHash", "decisionHash"] {
        if args.get(key).is_some_and(|v| {
            !v.as_str()
                .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        }) {
            return Err("invalid document-party hash");
        }
    }
    if args
        .get("reason")
        .is_some_and(|v| v.as_str().is_none_or(|s| s.trim().is_empty()))
    {
        return Err("blank reason");
    }
    if !read(name) && args["confirm"] != true {
        return Ok(result(
            json!({"ok":false,"errors":[format!("confirm: true required for write tool {name}")],"code":"CONFIRM_REQUIRED"}),
        ));
    }
    let outcome = match name {
        "documents_party_link_list" => {
            api.document_party_original_list(company, args["status"].as_str())
                .await
        }
        "documents_party_link_inspect" => {
            api.document_party_original_inspect(company, args["documentId"].as_i64().unwrap())
                .await
        }
        _ => {
            let mut body = args.clone();
            body.as_object_mut().unwrap().remove("company");
            let bytes =
                serde_json::to_vec(&body).map_err(|_| "document-party serialization failed")?;
            api.document_party_original(company, operation(name).unwrap(), bytes, !read(name))
                .await
        }
    };
    Ok(match outcome {
        Ok(core) => wrap(name, core),
        Err(error) => transport_error(error),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn document_party_original_plan_success_keeps_inner_ok_and_failure_does_not() {
        let plan = json!({"documentId":7,"planHash":"a".repeat(64)});
        let wrapped = wrap("documents_party_link_plan", json!({"ok":true,"plan":plan}));
        assert_eq!(
            wrapped["structuredContent"]["data"],
            json!({"ok":true,"plan":plan})
        );
        assert_eq!(wrapped["isError"], false);
        let failed = wrap(
            "documents_party_link_plan",
            json!({"ok":false,"errors":["NO_IDENTIFIER_EVIDENCE"],"appliedRules":["exact_identifier"]}),
        );
        assert_eq!(failed["isError"], true);
        assert!(failed["structuredContent"].get("data").is_none());
        assert_eq!(
            failed["structuredContent"]["appliedRules"],
            json!(["exact_identifier"])
        );
    }
}
