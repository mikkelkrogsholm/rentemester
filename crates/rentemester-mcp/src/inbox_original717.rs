//! Original flat inbox tools. Native upload and current-principal replay controls remain binding.
use rentemester_client::{
    ApiClient, Error, WorkspaceInboxMutation as Mutation, workspace_inbox_body_schema,
};
use serde_json::{Value, json};
const NAMES: &[&str] = &[
    "workspace_inbox_list",
    "workspace_inbox_inspect",
    "workspace_inbox_status",
    "workspace_inbox_ingest",
    "workspace_inbox_assign",
    "workspace_inbox_complete",
];
pub fn accepts(name: &str) -> bool {
    NAMES.contains(&name)
}
fn write(op: &str) -> bool {
    matches!(op, "ingest" | "assign" | "complete")
}
fn action(op: &str, id: &str) -> Mutation {
    match op {
        "assign" => Mutation::Assign(id.into()),
        "complete" => Mutation::Complete(id.into()),
        _ => Mutation::Ingest,
    }
}
fn shape(v: &Value, s: &Value) -> bool {
    if let Some(c) = s.get("const") {
        if v != c {
            return false;
        }
    }
    if let Some(e) = s["enum"].as_array() {
        if !e.contains(v) {
            return false;
        }
    }
    let kind = |t: &str| match t {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "null" => v.is_null(),
        "boolean" => v.is_boolean(),
        "integer" => v.is_u64(),
        _ => false,
    };
    if let Some(t) = s.get("type") {
        if !t.as_str().is_some_and(kind)
            && !t
                .as_array()
                .is_some_and(|a| a.iter().any(|t| t.as_str().is_some_and(kind)))
        {
            return false;
        }
    }
    if let Some(n) = v.as_u64() {
        if s["minimum"].as_u64().is_some_and(|m| n < m)
            || s["maximum"].as_u64().is_some_and(|m| n > m)
        {
            return false;
        }
    }
    if let Some(t) = v.as_str() {
        let n = t.encode_utf16().count() as u64;
        if s["minLength"].as_u64().is_some_and(|m| n < m)
            || s["maxLength"].as_u64().is_some_and(|m| n > m)
        {
            return false;
        }
    }
    if let Some(o) = v.as_object() {
        if s["required"]
            .as_array()
            .is_some_and(|r| r.iter().any(|k| !o.contains_key(k.as_str().unwrap())))
        {
            return false;
        }
        for (k, v) in o {
            match s["properties"].get(k) {
                Some(p) => {
                    if !shape(v, p) {
                        return false;
                    }
                }
                None => {
                    if s["additionalProperties"] == false {
                        return false;
                    }
                }
            }
        }
    }
    if let Some(a) = v.as_array() {
        if s["maxItems"].as_u64().is_some_and(|m| a.len() as u64 > m)
            || a.iter().any(|v| !shape(v, &s["items"]))
        {
            return false;
        }
    }
    true
}
fn hash(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn nonempty(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| !rentemester_engine::company_knowledge::trim(s).is_empty())
}
fn nullable(v: Option<&Value>) -> bool {
    v.is_some_and(|v| v.is_null() || nonempty(v))
}
fn canonical_text(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(rentemester_engine::company_knowledge::trim)
}
fn ingest_intent(body: &Value, receipt: &Value) -> bool {
    canonical_text(&body["filename"]) == receipt["filename"].as_str()
        && ["mimeType", "transport", "metadata"]
            .iter()
            .all(|k| body[k] == receipt[k])
        && body["receivedAt"]
            .as_str()
            .and_then(|s| rentemester_engine::company_knowledge::iso(s).ok())
            .is_some_and(|s| receipt["receivedAt"] == s)
        && match body.get("transportIdentity") {
            None | Some(Value::Null) => receipt.get("transportIdentity") == Some(&Value::Null),
            Some(value) => canonical_text(value) == receipt["transportIdentity"].as_str(),
        }
}
fn source(v: &Value) -> bool {
    [
        "sourceId",
        "visibilityAnchorSlug",
        "filename",
        "mimeType",
        "receivedAt",
        "createdBy",
        "createdAt",
    ]
    .iter()
    .all(|k| nonempty(&v[k]))
        && hash(&v["sha256"])
        && matches!(
            v["transport"].as_str(),
            Some("upload" | "mail" | "connector")
        )
        && nullable(v.get("transportIdentity"))
        && v["metadata"].is_object()
        && v.get("exception").is_some_and(|e| {
            e.is_null()
                || ["code", "requiredAction", "openedAt"]
                    .iter()
                    .all(|k| nonempty(&e[k]))
                    && nullable(e.get("resolvedAt"))
        })
        && v["assignments"].as_array().is_some_and(|a| {
            a.len() <= 1
                && a.iter().all(|a| {
                    nonempty(&a["companySlug"])
                        && nonempty(&a["assignedBy"])
                        && nonempty(&a["assignedAt"])
                        && match a["state"].as_str() {
                            Some("approved") => {
                                a.get("documentId") == Some(&Value::Null)
                                    && a.get("documentNo") == Some(&Value::Null)
                                    && a.get("completedAt") == Some(&Value::Null)
                            }
                            Some("completed") => {
                                a["documentId"].as_u64().is_some_and(|n| n > 0)
                                    && nonempty(&a["documentNo"])
                                    && nonempty(&a["completedAt"])
                            }
                            _ => false,
                        }
                })
        })
        && v["history"].as_array().is_some_and(|h| {
            h.iter()
                .any(|e| e["event_type"] == "ingested" && e["payload"]["sourceHash"] == v["sha256"])
                && h.iter().any(|e| {
                    e["event_type"] == "candidate_resolved" && e["payload"]["candidates"].is_array()
                })
                && h.iter().all(|e| {
                    ["event_type", "actor", "created_at"]
                        .iter()
                        .all(|k| nonempty(&e[k]))
                        && hash(&e["payload_hash"])
                        && e["payload"].is_object()
                        && (e["event_type"] != "ingested"
                            || e["payload"]["sourceHash"] == v["sha256"])
                        && (e["event_type"] != "handoff_completed"
                            || e["payload"]["sourceHash"] == v["sha256"])
                })
        })
}
fn depth(v: &Value, n: usize) -> bool {
    n <= 32
        && match v {
            Value::Array(a) => a.iter().all(|v| depth(v, n + 1)),
            Value::Object(o) => o.values().all(|v| depth(v, n + 1)),
            _ => true,
        }
}
fn schema(name: &str) -> Value {
    let op = name.strip_prefix("workspace_inbox_").unwrap_or("");
    let mut s = if op == "ingest" {
        workspace_inbox_body_schema(&Mutation::Ingest)
    } else {
        json!({"type":"object","properties":{},"required":[],"additionalProperties":false})
    };
    s["properties"]["company"] = json!({"type":"string","minLength":1,"maxLength":128});
    s["required"].as_array_mut().unwrap().push(json!("company"));
    if op == "list" {
        s["properties"]["cursor"] = json!({"type":"integer","minimum":0});
        s["properties"]["limit"] = json!({"type":"integer","minimum":1,"maximum":100});
    } else if op != "ingest" {
        s["properties"]["sourceId"] = json!({"type":"string","minLength":1,"maxLength":100});
        s["required"]
            .as_array_mut()
            .unwrap()
            .push(json!("sourceId"));
        if write(op) {
            s["properties"]["companySlug"] = json!({"type":"string","minLength":1,"maxLength":128});
            s["required"]
                .as_array_mut()
                .unwrap()
                .push(json!("companySlug"));
        }
    }
    if write(op) {
        s["properties"]["confirm"] = json!({"type":"boolean"});
        s["required"]
            .as_array_mut()
            .unwrap()
            .retain(|v| v != "confirm");
    }
    if op == "ingest" {
        s["properties"]["sourceId"] = json!({"type":"string","minLength":1,"maxLength":100});
        s["properties"]["idempotencyKey"]["maxLength"] = json!(200);
        s["properties"]["mimeType"]["maxLength"] = json!(160);
        s["properties"]["transportIdentity"] =
            json!({"type":"string","minLength":1,"maxLength":512});
        s["properties"]["candidates"]["items"]["properties"]["evidenceKind"] =
            json!({"enum":["recipient_alias","buyer_vat","buyer_name","account","portal"]});
        s["properties"]["candidates"]["items"]["properties"]["evidence"]["maxLength"] = json!(1000);
    }
    s
}
pub fn tools() -> Vec<Value> {
    NAMES.iter().map(|name| {
        let read = !write(name.strip_prefix("workspace_inbox_").unwrap());
        let mut t = crate::tools::tool(name,"Original flat workspace inbox operation, explicit company visibility anchor; companySlug is an independent required assignment/completion destination. Human credentials only; server owns visibility, backup, sensitivity, fences and handoff. Native upload ONLY (mail/connector unsupported); filename255UTF16 plus Prepared255UTF8/path/filetype limits, not legacy512; no truncation. Native pagination25/100 and whole-intent/current-principal replay retained. Original bytes1..25MiB, body35MiB, metadata64KiB/depth32 preserved, arbitrary facts stage only and do not imply completion eligibility. Writes require confirm:true. No filesystem, inferred facts, generated keys, accounting or automatic retry. Unknown outcomes require canonical exact-source/history/assignment and target document readback before further action.",schema(name),read);
        t["annotations"] = json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read,"openWorldHint":false});
        t["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"data":{"type":"object"},"errors":{"type":"array","items":{"type":"string"}},"code":{"type":"string"},"appliedRules":{"type":"array","items":{"type":"string"}}},"additionalProperties":false});
        t
    }).collect()
}
fn result(e: Value) -> Value {
    json!({"isError":e["ok"]!=true,"content":[{"type":"text","text":e.to_string()}],"structuredContent":e})
}
fn fail(message: &str, code: &str) -> Value {
    result(json!({"ok":false,"errors":[message],"code":code}))
}
fn transport(e: Error, mutation: bool) -> Value {
    let code = match e {
        Error::Refused { status: 404 } => {
            return fail(
                "workspace inbox source not found",
                "WORKSPACE_INBOX_NOT_FOUND",
            );
        }
        Error::Refused { .. } => "server_refused",
        Error::InvalidInput(_) => "invalid_input",
        Error::OutcomeUnknown => "outcome_unknown",
        Error::Protocol => {
            return fail(
                "Server protocol admission refused before the inbox request; no mutation sent",
                "protocol",
            );
        }
        _ if mutation => "outcome_unknown",
        _ => "transport",
    };
    fail(
        if mutation {
            "Workspace inbox operation refused or outcome unknown; read back canonical exact-source/history and assignments and target documents before further action; no automatic retry"
        } else {
            "Workspace inbox transport, authorization or protocol refused"
        },
        code,
    )
}
fn slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
fn bounded(s: &str, max: usize) -> bool {
    rentemester_engine::company_knowledge::text(s, max).is_ok() && !s.chars().any(char::is_control)
}
// Strict canonical standard base64; transmitted JSON/base64 is never rewritten.
fn decode(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() || s.len() % 4 != 0 || s.len() > 34_952_536 {
        return None;
    }
    let digit = |b: u8| -> Option<u32> {
        Some(match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    for (i, c) in s.as_bytes().chunks_exact(4).enumerate() {
        let a = digit(c[0])?;
        let b = digit(c[1])?;
        let pad = if c[2] == b'=' {
            if c[3] != b'=' || b & 15 != 0 {
                return None;
            }
            2
        } else if c[3] == b'=' {
            1
        } else {
            0
        };
        if pad > 0 && i + 1 != s.len() / 4 {
            return None;
        }
        let d = if pad == 2 { 0 } else { digit(c[2])? };
        let e = if pad > 0 { 0 } else { digit(c[3])? };
        if pad == 1 && d & 3 != 0 {
            return None;
        }
        let n = (a << 18) | (b << 12) | (d << 6) | e;
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad == 0 {
            out.push(n as u8);
        }
    }
    (out.len() <= 25 * 1024 * 1024).then_some(out)
}
// SHA-256 binds upload receipt to the supplied original bytes, never to a client-rebuilt history.
fn sha256(bytes: &[u8]) -> String {
    rentemester_engine::sha256_bytes(bytes)
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    if !accepts(name) || !shape(&args, &schema(name)) {
        return Err("invalid original inbox arguments");
    }
    let op = name.strip_prefix("workspace_inbox_").unwrap();
    let company = args["company"].as_str().unwrap();
    if !slug(company) {
        return Err("invalid company visibility anchor");
    }
    let id = args["sourceId"].as_str().unwrap_or("");
    let canonical_id = rentemester_engine::company_knowledge::trim(id);
    if args.get("sourceId").is_some() && (!bounded(id, 100) || matches!(canonical_id, "." | "..")) {
        return Err("invalid sourceId");
    }
    if matches!(op, "assign" | "complete") && !slug(args["companySlug"].as_str().unwrap()) {
        return Err("invalid explicit target companySlug");
    }
    let mut body = args.clone();
    body.as_object_mut().unwrap().remove("company");
    if op != "ingest" {
        body.as_object_mut().unwrap().remove("sourceId");
    }
    let mut expected_sha = None;
    if op == "ingest" {
        for (k, max) in [
            ("filename", 255),
            ("mimeType", 160),
            ("idempotencyKey", 200),
            ("transportIdentity", 512),
        ] {
            if args
                .get(k)
                .is_some_and(|v| !bounded(v.as_str().unwrap(), max))
            {
                return Err("invalid bounded inbox text");
            }
        }
        if rentemester_engine::company_knowledge::iso(args["receivedAt"].as_str().unwrap()).is_err()
        {
            return Err("native ISO receivedAt required");
        }
        if !depth(&args["metadata"], 0)
            || serde_json::to_vec(&args["metadata"]).unwrap().len() > 64 * 1024
        {
            return Err("metadata exceeds64KiB/depth32");
        }
        let bytes = decode(args["bytesBase64"].as_str().unwrap())
            .ok_or("canonical base64 original1..25MiB required")?;
        let filename = args["filename"].as_str().unwrap();
        if filename.len() > 255
            || filename.contains(['/', '\\'])
            || !["pdf", "png", "jpg", "jpeg", "txt", "json", "xml"].contains(
                &filename
                    .rsplit('.')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .as_str(),
            )
        {
            return Err("native Prepared filename255UTF8/path/filetype boundary");
        }
        let ext = filename
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let sniff = if bytes.starts_with(b"%PDF-") {
            Some("pdf")
        } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some("png")
        } else if bytes.starts_with(b"\xff\xd8\xff") {
            Some("jpg")
        } else {
            None
        };
        let normalized_ext = if ext == "jpeg" { "jpg" } else { &ext };
        if sniff.is_some_and(|v| v != normalized_ext)
            || (["pdf", "png", "jpg"].contains(&normalized_ext) && sniff != Some(normalized_ext))
        {
            return Err("native document content/filetype mismatch");
        }
        let mut seen = std::collections::BTreeSet::new();
        if let Some(candidates) = args["candidates"].as_array() {
            for c in candidates {
                if !slug(c["companySlug"].as_str().unwrap())
                    || !bounded(c["evidence"].as_str().unwrap(), 1000)
                {
                    return Err("invalid candidate");
                }
                let key = (
                    rentemester_engine::company_knowledge::trim(c["companySlug"].as_str().unwrap()),
                    c["evidenceKind"].as_str().unwrap(),
                    rentemester_engine::company_knowledge::trim(c["evidence"].as_str().unwrap()),
                );
                if !seen.insert(key) {
                    return Err("duplicate-equivalent candidate");
                }
            }
        }
        expected_sha = Some(sha256(&bytes));
    }
    let wire = serde_json::to_vec(&body).map_err(|_| "inbox serialization failed")?;
    if write(op) && wire.len() > rentemester_client::MAX_WORKSPACE_INBOX_BODY {
        return Err("native35MiB body bound exceeded");
    }
    if write(op) && args["confirm"] != true {
        return Ok(fail(
            &format!("confirm: true required for write tool {name}"),
            "CONFIRM_REQUIRED",
        ));
    }
    let mut prior_sha = None;
    if matches!(op, "assign" | "complete") {
        match api.workspace_inbox_source(company, id).await {
            Ok(v)
                if v["ok"] == true
                    && source(&v["source"])
                    && v["source"]["sourceId"] == canonical_id
                    && v["source"]["visibilityAnchorSlug"] == company =>
            {
                prior_sha = v["source"]["sha256"].as_str().map(str::to_owned);
            }
            Ok(_) => {
                return Ok(fail(
                    "Invalid canonical source readback; no mutation sent",
                    "protocol",
                ));
            }
            Err(e) => return Ok(transport(e, false)),
        }
    }
    let response = match op {
        "list" => {
            api.workspace_inbox(
                company,
                args["cursor"].as_u64().unwrap_or(0) as usize,
                args["limit"].as_u64().unwrap_or(25) as usize,
            )
            .await
        }
        "inspect" | "status" => api.workspace_inbox_source(company, id).await,
        _ => {
            api.mutate_workspace_inbox(company, action(op, id), wire, true)
                .await
        }
    };
    Ok(match response {
        Err(e) => transport(e, write(op)),
        Ok(v) => {
            if v["ok"] == false
                && v["errors"]
                    .as_array()
                    .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
            {
                let mut e = json!({"ok":false,"errors":v["errors"]});
                if let Some(code) = v.get("code").filter(|v| v.is_string()) {
                    e["code"] = if code == "workspace_inbox_not_found" {
                        json!("WORKSPACE_INBOX_NOT_FOUND")
                    } else {
                        code.clone()
                    };
                }
                return Ok(result(e));
            }
            let valid = v["ok"] == true
                && if op == "list" {
                    v["rows"].as_array().is_some_and(|a| {
                        a.len() <= args["limit"].as_u64().unwrap_or(25) as usize
                            && a.iter()
                                .all(|s| source(s) && s["visibilityAnchorSlug"] == company)
                    }) && v["count"].as_u64().is_some_and(|n| {
                        n >= v["rows"].as_array().map_or(usize::MAX, Vec::len) as u64
                    }) && v
                        .get("nextCursor")
                        .is_some_and(|c| c.is_null() || c.is_u64())
                } else {
                    let s = &v["source"];
                    source(s)
                        && s["visibilityAnchorSlug"] == company
                        && (id.is_empty() || s["sourceId"] == canonical_id)
                        && expected_sha.as_ref().is_none_or(|sha| s["sha256"] == *sha)
                        && prior_sha.as_ref().is_none_or(|sha| s["sha256"] == *sha)
                        && (op != "ingest" || ingest_intent(&body, s))
                        && (!matches!(op, "assign" | "complete")
                            || s["history"].as_array().is_some_and(|h| {
                                h.iter().any(|e| {
                                    e["event_type"] == "assigned"
                                        && e["payload"]["companySlug"] == args["companySlug"]
                                })
                            }))
                        && (op != "complete"
                            || s["history"].as_array().is_some_and(|h| {
                                h.iter().any(|e| {
                                    e["event_type"] == "handoff_completed"
                                        && e["payload"]["companySlug"] == args["companySlug"]
                                        && e["payload"]["sourceHash"] == s["sha256"]
                                        && s["assignments"].as_array().is_some_and(|a| {
                                            a.iter().any(|a| {
                                                a["documentId"] == e["payload"]["documentId"]
                                                    && a["documentNo"] == e["payload"]["documentNo"]
                                            })
                                        })
                                })
                            }))
                        && (!matches!(op, "assign" | "complete")
                            || s["assignments"].as_array().is_some_and(|a| {
                                a.iter().any(|a| {
                                    a["companySlug"] == args["companySlug"]
                                        && (a["state"] == "completed"
                                            || op == "assign" && a["state"] == "approved")
                                })
                            }))
                };
            if !valid {
                fail(
                    if write(op) {
                        "Operation outcome unknown; malformed or mismatched receipt; read back canonical exact-source/history and assignments and target documents before further action; no retry"
                    } else {
                        "Invalid workspace inbox response"
                    },
                    if write(op) {
                        "outcome_unknown"
                    } else {
                        "protocol"
                    },
                )
            } else {
                result(
                    json!({"ok":true,"errors":[],"data":if op=="list" {json!({"rows":v["rows"],"count":v["count"],"nextCursor":v["nextCursor"]})}else{json!({"source":v["source"]})}}),
                )
            }
        }
    })
}
