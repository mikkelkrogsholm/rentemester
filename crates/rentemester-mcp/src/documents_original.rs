//! Original document tools; local paths belong only to this stdio process.
use rentemester_client::{
    ApiClient, DocumentsOriginalMutation, Error, documents_original_file, documents_original_schema,
};
use serde_json::{Value, json};
use std::path::Path;

const NAMES: [&str; 8] = [
    "documents_ingest",
    "documents_list",
    "documents_enrich",
    "documents_set_company_context",
    "documents_parse",
    "documents_parse_pending",
    "documents_parse_status",
    "documents_parsed_text",
];
pub fn accepts(name: &str) -> bool {
    NAMES.contains(&name)
}
fn read(name: &str) -> bool {
    matches!(
        name,
        "documents_list" | "documents_parse_status" | "documents_parsed_text"
    )
}
fn operation(name: &str, id: i64) -> DocumentsOriginalMutation {
    match name {
        "documents_ingest" => DocumentsOriginalMutation::Ingest,
        "documents_enrich" => DocumentsOriginalMutation::Enrich(id),
        _ => DocumentsOriginalMutation::CompanyContext(id),
    }
}
fn schema(name: &str) -> Value {
    let mut schema = if matches!(
        name,
        "documents_ingest" | "documents_enrich" | "documents_set_company_context"
    ) {
        documents_original_schema(operation(name, 1))
    } else {
        json!({"type":"object","properties":{},"required":[]})
    };
    schema["additionalProperties"] = json!(false);
    schema["properties"]["company"] = json!({"type":"string","minLength":1});
    schema["required"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!("company"));
    if !matches!(
        name,
        "documents_ingest" | "documents_list" | "documents_parse_pending"
    ) {
        schema["properties"]["documentId"] =
            json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
        schema["required"]
            .as_array_mut()
            .unwrap()
            .push(json!("documentId"));
    }
    if name == "documents_ingest" {
        schema["properties"]["filePath"] = json!({"type":"string","minLength":1,"description":"Path on the filesystem of THIS LOCAL stdio MCP process, never the HTTP server filesystem. Read only after confirm:true; bounded regular file, no symlinks."});
        schema["required"]
            .as_array_mut()
            .unwrap()
            .push(json!("filePath"));
    }
    if !read(name) {
        schema["properties"]["confirm"] = json!({"type":"boolean"});
    }
    if matches!(
        name,
        "documents_list" | "documents_parse_pending" | "documents_parsed_text"
    ) {
        let maximum = match name {
            "documents_list" => 5000,
            "documents_parse_pending" => 100,
            _ => 10,
        };
        let default = if name == "documents_list" {
            500
        } else {
            maximum
        };
        schema["properties"]["limit"] =
            json!({"type":"integer","minimum":1,"maximum":maximum,"default":default});
        schema["properties"][if name == "documents_parse_pending" {
            "cursor"
        } else {
            "offset"
        }] = json!({"type":"integer","minimum":0,"maximum":9007199254740991u64,"default":0});
    }
    schema
}
pub fn tools() -> Vec<Value> {
    NAMES.into_iter().map(|name| {
        let description = match name {
            "documents_ingest" => "Ingest one physical original with source metadata (amounts in kroner, NOT øre). filePath is explicitly on THIS LOCAL stdio MCP process filesystem, never the HTTP server. Requires confirm:true before any file read or HTTP. force skips only logical supplier/invoice duplicate, never content hash duplicates. No keys or automatic retry; reconcile unknown outcomes through canonical reads.",
            "documents_list" => "List all original document metadata, id DESC, without physical paths or payload_json. Pagination defaults limit500/offset0, max5000; data documents,total,count,limit,offset,hasMore,nextOffset only when hasMore.",
            "documents_enrich" => "Complete missing metadata on an eligible unposted original, retaining immutable original evidence. Requires confirm:true; identical enrichment is a no-op, conflicting repeats refuse. No automatic retry.",
            "documents_set_company_context" => "Record append-only verified company context for a qualifying Danish simplified purchase invoice, not invented recipient fields. Requires confirm:true. Identical context is a no-op; conflicting or stale context refuses.",
            "documents_parse" => "Parse one stored PDF through the existing deterministic parser, with no bookkeeping authority. Requires confirm:true. Inspect canonical status after an unknown outcome; no automatic retry.",
            "documents_parse_pending" => "Parse up to 100 pending stored PDFs with explicit confirm:true; default limit100/cursor0. No ingest, bookkeeping, automatic retries or hidden keys.",
            "documents_parse_status" => "Read verified latest PDF status and metrics, including null when no parse exists. No physical paths, raw child stderr or secrets.",
            _ => "Read verified parsed PDF text, default offset0/limit10, at most10 pages per call. No physical paths, raw child stderr or secrets.",
        };
        let mut tool = crate::tools::tool(name, description, schema(name), read(name));
        tool["annotations"] = json!({"readOnlyHint":read(name),"destructiveHint":false,"idempotentHint":name!="documents_ingest","openWorldHint":false});
        tool["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"data":{"type":"object","additionalProperties":true},"errors":{"type":"array","items":{"type":"string"}},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}},"additionalProperties":false});
        tool
    }).collect()
}
// Validate the SDK's operation schema before confirmation. Explicit null is not
// omission; only fields whose schema explicitly permits null may contain it.
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

// Canonical RFC4648 encoding, without introducing a dependency or touching Cargo.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0] as usize;
        let b = chunk.get(1).copied().unwrap_or(0) as usize;
        let c = chunk.get(2).copied().unwrap_or(0) as usize;
        out.push(ALPHABET[a >> 2] as char);
        out.push(ALPHABET[((a & 3) << 4) | (b >> 4)] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((b & 15) << 2) | (c >> 6)] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[c & 63] as char
        } else {
            '='
        });
    }
    out
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    if !accepts(name) || !valid(&args, &schema(name)) {
        return Err("invalid original document arguments");
    }
    let company = args["company"].as_str().unwrap();
    if !company
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("invalid company slug");
    }
    if !read(name) && args["confirm"] != true {
        return Ok(result(
            json!({"ok":false,"errors":[format!("confirm: true required for write tool {name}")],"code":"CONFIRM_REQUIRED"}),
        ));
    }
    let id = args["documentId"].as_i64().unwrap_or(0);
    let offset = args["offset"].as_u64().unwrap_or(0);
    let outcome = match name {
        "documents_list" => api.documents_original_list(company).await,
        "documents_parse" => api.document_parse(company, id, true).await,
        "documents_parse_status" => api.document_parse_status(company, id).await,
        "documents_parsed_text" => {
            api.document_parsed_text(
                company,
                id,
                offset,
                args["limit"].as_u64().unwrap_or(10) as u8,
            )
            .await
        }
        "documents_parse_pending" => {
            api.documents_parse_pending(
                company,
                args["cursor"].as_u64().unwrap_or(0),
                args["limit"].as_u64().unwrap_or(100) as u8,
                true,
            )
            .await
        }
        _ => {
            let mut body = json!({"confirm":true});
            for field in [
                "metadata",
                "vendorId",
                "force",
                "sourceReference",
                "businessUseReason",
            ] {
                if let Some(value) = args.get(field) {
                    body[field] = value.clone();
                }
            }
            if name == "documents_ingest" {
                let (file_name, bytes) =
                    match documents_original_file(Path::new(args["filePath"].as_str().unwrap())) {
                        Ok(file) => file,
                        Err(error) => return Ok(transport_error(error)),
                    };
                body["fileName"] = json!(file_name);
                body["fileBase64"] = json!(base64(&bytes));
            }
            let bytes = serde_json::to_vec(&body).map_err(|_| "document serialization failed")?;
            api.documents_original_mutation(company, operation(name, id), bytes, true)
                .await
        }
    };
    Ok(match outcome {
        Ok(mut core) => {
            if name == "documents_list" && core["ok"] == true {
                let rows = core["documents"]
                    .as_array()
                    .ok_or("invalid original documents list response")?;
                let total = rows.len();
                let limit = args["limit"].as_u64().unwrap_or(500);
                let page: Vec<Value> = rows
                    .iter()
                    .skip(usize::try_from(offset).unwrap_or(usize::MAX))
                    .take(limit as usize)
                    .cloned()
                    .collect();
                let count = page.len();
                let next = offset.saturating_add(count as u64);
                let has_more = next < total as u64;
                core = json!({"ok":true,"errors":[],"documents":page,"total":total,"count":count,"limit":limit,"offset":offset,"hasMore":has_more});
                if has_more {
                    core["nextOffset"] = json!(next);
                }
            }
            wrap(core)
        }
        Err(error) => transport_error(error),
    })
}
