//! Source customer/vendor tools over the common authenticated Rust server.
use rentemester_client::{
    ApiClient, Error, MasterDataOriginalKind, master_data_original_create_schema,
};
use serde_json::{Value, json};

const NAMES: [&str; 5] = [
    "customer_list",
    "vendor_list",
    "customer_create",
    "vendor_create",
    "customer_validate_vat",
];

pub fn accepts(name: &str) -> bool {
    NAMES.contains(&name)
}
fn kind(name: &str) -> MasterDataOriginalKind {
    if name.starts_with("vendor_") {
        MasterDataOriginalKind::Vendor
    } else {
        MasterDataOriginalKind::Customer
    }
}
fn is_list(name: &str) -> bool {
    name.ends_with("_list")
}
fn schema(name: &str) -> Value {
    let mut properties = json!({"company":{"type":"string","minLength":1}});
    let mut required = vec!["company"];
    if is_list(name) {
        properties["archived"] = json!({"type":"boolean"});
        properties["limit"] = json!({"type":"integer","minimum":1,"maximum":5000,"default":500});
        properties["offset"] = json!({"type":"integer","minimum":0,"default":0});
    } else {
        properties["confirm"] = json!({"type":"boolean"});
        if name == "customer_validate_vat" {
            properties["cvr"] = json!({"type":"string","minLength":1});
            required.push("cvr");
        } else {
            properties["input"] = master_data_original_create_schema(kind(name));
            properties["fromCvr"] = json!({"type":"string"});
            required.push("input");
        }
    }
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
pub fn tools() -> Vec<Value> {
    NAMES.into_iter().map(|name| {
        let read = is_list(name);
        let vat = name == "customer_validate_vat";
        let description = if read {
            "List source master data in lower(name) ASC, id ASC order. archived:true includes all records. Pagination defaults to limit 500/offset 0, max limit 5000; data contains rows,total,count,limit,offset,hasMore and nextOffset only when hasMore."
        } else if vat {
            "Validate a general VAT number (including DK) against the trusted VIES provider and store the current observation. Requires confirm:true; every executed call contacts the provider, even within cache TTL. Authoritative valid:false is a successful observation. No automatic retries."
        } else {
            "Create source master data through the existing atomic writer and audit. Requires confirm:true. Optional fromCvr fills blank name/address/vatOrCvr/email/phone/website; explicit nonblank values win. Provider cache may commit independently before creation; no cross-provider rollback or automatic retry. Reconcile unknown outcomes before further action. Current cookie/service authority only."
        };
        let mut tool = crate::tools::tool(name, description, schema(name), read);
        tool["annotations"] = json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read || vat,"openWorldHint":vat});
        tool["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"data":{"type":"object"},"errors":{"type":"array","items":{"type":"string"}},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}},"additionalProperties":false});
        tool
    }).collect()
}
// Validate before confirmation and transport. In particular, null is never omission,
// and unknown contact/authority fields cannot pass through to HTTP.
fn valid(value: &Value, schema: &Value) -> bool {
    if schema
        .get("enum")
        .and_then(Value::as_array)
        .is_some_and(|options| !options.contains(value))
    {
        return false;
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => value.as_object().is_some_and(|fields| {
            schema
                .get("required")
                .and_then(Value::as_array)
                .is_none_or(|keys| {
                    keys.iter()
                        .all(|key| key.as_str().is_some_and(|key| fields.contains_key(key)))
                })
                && fields.iter().all(|(key, value)| {
                    schema["properties"]
                        .get(key)
                        .is_some_and(|schema| valid(value, schema))
                })
        }),
        Some("string") => value.as_str().is_some_and(|s| {
            let length = s.chars().count() as u64;
            schema
                .get("minLength")
                .is_none_or(|min| length >= min.as_u64().unwrap())
                && schema
                    .get("maxLength")
                    .is_none_or(|max| length <= max.as_u64().unwrap())
        }),
        Some("integer") => value.as_u64().is_some_and(|n| {
            schema
                .get("minimum")
                .is_none_or(|min| n >= min.as_u64().unwrap())
                && schema
                    .get("maximum")
                    .is_none_or(|max| n <= max.as_u64().unwrap())
                && schema
                    .get("exclusiveMinimum")
                    .is_none_or(|min| n > min.as_u64().unwrap())
        }),
        Some("boolean") => value.is_boolean(),
        None => schema.get("enum").is_some(),
        _ => false,
    }
}
fn result(envelope: Value) -> Value {
    json!({"isError":envelope["ok"]!=true,"content":[{"type":"text","text":envelope.to_string()}],"structuredContent":envelope})
}
fn wrap(mut core: Value) -> Value {
    let Some(fields) = core.as_object_mut() else {
        return transport_error(Error::Protocol);
    };
    let ok = fields.remove("ok").unwrap_or(json!(false));
    let errors = fields.remove("errors").unwrap_or(json!([]));
    let rules = fields.remove("appliedRules");
    let mut envelope = json!({"ok":ok,"errors":errors});
    if let Some(rules) = rules {
        envelope["appliedRules"] = rules;
    }
    if ok == true {
        envelope["data"] = core;
    } else if let Some(code) = core.get("code") {
        envelope["code"] = code.clone();
    }
    result(envelope)
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
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    if !accepts(name) || !valid(&args, &schema(name)) {
        return Err("invalid original master data arguments");
    }
    let company = args["company"].as_str().unwrap();
    if !company
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("invalid company slug");
    }
    if !is_list(name) && args["confirm"] != true {
        return Ok(result(
            json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":[format!("confirm: true required for write tool {name}")]}),
        ));
    }
    let outcome = if is_list(name) {
        api.master_data_original_list(company, kind(name), args["archived"] == true)
            .await
    } else if name == "customer_validate_vat" {
        api.customer_original_validate_vat(company, args["cvr"].as_str().unwrap(), true)
            .await
    } else {
        let mut wire = json!({"input":args["input"],"confirm":true});
        if let Some(cvr) = args.get("fromCvr") {
            wire["fromCvr"] = cvr.clone();
        }
        let bytes = serde_json::to_vec(&wire).map_err(|_| "master data serialization failed")?;
        if bytes.len() > 262144 {
            return Err("master data request exceeds 256KiB");
        }
        api.master_data_original_create(company, kind(name), bytes, true)
            .await
    };
    Ok(match outcome {
        Ok(mut core) => {
            if is_list(name) && core["ok"] == true {
                let rows = core["rows"]
                    .as_array()
                    .ok_or("invalid source list response")?;
                let total = rows.len();
                let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(500);
                let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0);
                let page: Vec<Value> = rows
                    .iter()
                    .skip(usize::try_from(offset).unwrap_or(usize::MAX))
                    .take(limit as usize)
                    .cloned()
                    .collect();
                let count = page.len();
                let next = offset.saturating_add(count as u64);
                let has_more = next < total as u64;
                core = json!({"ok":true,"errors":[],"rows":page,"total":total,"count":count,"limit":limit,"offset":offset,"hasMore":has_more});
                if has_more {
                    core["nextOffset"] = json!(next);
                }
            }
            wrap(core)
        }
        Err(error) => transport_error(error),
    })
}
