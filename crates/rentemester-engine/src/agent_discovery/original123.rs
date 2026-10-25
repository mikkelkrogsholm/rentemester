//! Frozen ORIGINAL definitions with registration-only runtime resolution.
//! No workspace, authorization, provider or filesystem access is performed.
use serde_json::{Value, json};
use std::sync::OnceLock;

fn source() -> &'static Value {
    static SOURCE: OnceLock<Value> = OnceLock::new();
    SOURCE.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../fixtures/original-agent-discovery123.json"
        ))
        .expect("frozen ORIGINAL catalogue")
    })
}
fn catalogue() -> Value {
    let s = source();
    let mut c = super::catalogue();
    for key in ["schemaVersion", "hash", "entryPoint", "sourceCoverage"] {
        c[key] = s[key].clone();
    }
    c["capabilityCount"] = json!(26);
    c["workflowCount"] = json!(31);
    c["coverage"] = json!({"fullCoverage":false,"reason":"Original conceptual definitions, not whole product parity. Current registration facts are separate from frozen sourceCoverage; missing tools, 42 source CLI keys and nonmatching native HTTP remain unavailable."});
    c
}
pub fn about() -> Value {
    let mut v = super::about();
    v["catalogue"] = catalogue();
    v["serverName"] = json!("rentemester-mcp");
    v
}
fn id(op: &Value) -> String {
    match op["surface"].as_str().unwrap_or("") {
        "mcp" => format!("mcp:{}", op["name"].as_str().unwrap_or("")),
        "cli" => format!("cli:{}", op["key"].as_str().unwrap_or("")),
        _ => format!(
            "http:{} {}",
            op["method"].as_str().unwrap_or(""),
            op["pattern"].as_str().unwrap_or("")
        ),
    }
}
fn normalized(path: &str) -> String {
    path.split('/')
        .map(|part| {
            part.strip_prefix('{')
                .and_then(|p| p.strip_suffix('}'))
                .map(|p| format!(":{p}"))
                .unwrap_or_else(|| part.to_owned())
        })
        .collect::<Vec<_>>()
        .join("/")
}
fn retry(id: &str, safety: &str, external: bool) -> &'static str {
    if safety == "read" {
        return "safe-read";
    }
    let s = &source()["retrySets"];
    let has = |key: &str, prefix: &str| {
        id.strip_prefix(prefix)
            .is_some_and(|name| s[key].as_array().unwrap().iter().any(|n| n == name))
    };
    if external || has("externalProviderReconciled", "mcp:") {
        "external-provider-reconciled"
    } else if has("keyIdempotent", "mcp:") {
        "key-idempotent"
    } else if has("naturalIdempotent", "mcp:") || has("naturalIdempotentCli", "cli:") {
        "natural-idempotent"
    } else {
        "unsafe-read-back"
    }
}
fn fact(op: &Value, routes: &[Value], tools: Option<&[Value]>, connected: bool) -> Value {
    let key = id(op);
    let mut v = op.clone();
    v["id"] = json!(key);
    v["resolved"] = json!(false);
    v["reason"] = json!("Operation is not registered on the selected runtime surface.");
    let (safety, idem, actor, confirm, external) = match op["surface"].as_str() {
        Some("cli") => {
            let Some(c) = source()["currentCommands"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["key"] == op["key"])
            else {
                return v;
            };
            let write = c["mutating"] == true || c["sideEffecting"] == true;
            v["registrationFacts"] = c.clone();
            (
                if write { "write" } else { "read" },
                if write { Value::Null } else { json!(true) },
                c["mutating"] == true,
                c["allowedFlags"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("--confirm")),
                false,
            )
        }
        Some("mcp") => {
            let Some(tools) = tools else {
                v["resolved"] = Value::Null;
                v["reason"] = json!(
                    "Resolve against actual MCP tools/list; this HTTP transport cannot attest MCP registration."
                );
                return v;
            };
            let Some(t) = tools.iter().find(|t| t["name"] == op["name"]) else {
                return v;
            };
            let a = &t["annotations"];
            let Some(read) = a["readOnlyHint"].as_bool() else {
                v["reason"] = json!("Registered MCP tool lacks explicit boolean readOnlyHint.");
                return v;
            };
            let safety = if read {
                "read"
            } else if a["destructiveHint"] == true {
                "destructive"
            } else {
                "write"
            };
            v["annotations"] = a.clone();
            (
                safety,
                json!(a["idempotentHint"] == true),
                a["requiresActor"].as_bool().unwrap_or(!read),
                a["requiresConfirmation"].as_bool().unwrap_or(!read),
                false,
            )
        }
        Some("http") => {
            let Some(r) = routes.iter().find(|r| {
                r["method"] == op["method"]
                    && r["path"]
                        .as_str()
                        .or(r["pattern"].as_str())
                        .is_some_and(|p| normalized(p) == op["pattern"].as_str().unwrap_or(""))
            }) else {
                return v;
            };
            if !connected {
                v["reason"] =
                    json!("Workspace disconnected; product HTTP availability is not attested.");
                return v;
            }
            let legacy = source()["sourceRoutes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["method"] == op["method"] && r["pattern"] == op["pattern"]);
            let effect = r["effect"]
                .as_str()
                .or_else(|| legacy.and_then(|r| r["effect"].as_str()))
                .unwrap_or("");
            let safety = if effect == "destructive" || op["method"] == "DELETE" {
                "destructive"
            } else if effect == "read" || op["method"] == "GET" || op["method"] == "HEAD" {
                "read"
            } else {
                "write"
            };
            (
                safety,
                if safety == "read" {
                    json!(true)
                } else {
                    Value::Null
                },
                false,
                false,
                effect == "external",
            )
        }
        _ => return v,
    };
    v["resolved"] = json!(true);
    v["reason"] = Value::Null;
    v["safety"] = json!(safety);
    v["idempotent"] = idem;
    v["requiresActor"] = json!(actor);
    v["requiresConfirmation"] = json!(confirm);
    v["retryClass"] = json!(retry(&key, safety, external));
    v["resolutionScope"] = json!(
        "Registration presence only; not authorization, provider configuration or per-argument acceptance."
    );
    v
}
fn bindings(routes: &[Value], tools: Option<&[Value]>, connected: bool) -> Vec<Value> {
    let mut refs = source()["currentCommands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| json!({"surface":"cli","key":c["key"]}))
        .collect::<Vec<_>>();
    refs.extend(routes.iter().filter_map(|r| Some(json!({"surface":"http","method":r["method"],"pattern":normalized(r["path"].as_str().or(r["pattern"].as_str())?)}))));
    if let Some(tools) = tools {
        refs.extend(
            tools
                .iter()
                .map(|t| json!({"surface":"mcp","name":t["name"]})),
        );
    }
    let mut out = Vec::new();
    for op in refs {
        let key = id(&op);
        let Some(caps) = source()["classification"]
            .get(&key)
            .and_then(Value::as_array)
        else {
            continue;
        };
        if caps.is_empty() {
            continue;
        }
        let mut f = fact(&op, routes, tools, connected);
        if f["resolved"] != true {
            continue;
        }
        f["capabilityIds"] = json!(caps);
        out.push(f);
    }
    sort(&mut out);
    out
}
fn sort(bindings: &mut Vec<Value>) {
    let order = source()["bindingOrder"].as_array().unwrap();
    bindings.sort_by_key(|b| {
        (
            order
                .iter()
                .position(|i| i == &b["id"])
                .unwrap_or(usize::MAX),
            b["id"].as_str().unwrap_or("").to_owned(),
        )
    });
    bindings.dedup_by(|a, b| a["id"] == b["id"]);
}
pub fn search(
    query: &str,
    cursor: u64,
    limit: usize,
    routes: &[Value],
    connected: bool,
) -> Result<Value, &'static str> {
    if query.encode_utf16().count() > 200 {
        return Err("query exceeds 200 UTF16 code units");
    }
    if !(1..=50).contains(&limit) {
        return Err("limit must be between 1 and 50");
    }
    let tokens = query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    let matches = source()["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| {
            let mut parts = Vec::new();
            for k in ["id", "title", "purpose", "domain"] {
                parts.push(c[k].as_str().unwrap_or("").to_owned());
            }
            for k in ["outcomes", "keywords"] {
                parts.extend(
                    c[k].as_array()
                        .unwrap()
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned),
                );
            }
            let haystack = parts.join(" ").to_lowercase();
            tokens.iter().all(|t| haystack.contains(t))
        })
        .collect::<Vec<_>>();
    let ops = bindings(routes, None, connected);
    let items = matches
        .iter()
        .skip(usize::try_from(cursor).unwrap_or(usize::MAX))
        .take(limit)
        .map(|c| {
            let mut c = (*c).clone();
            c.as_object_mut().unwrap().remove("canonicalState");
            c.as_object_mut().unwrap().remove("keywords");
            c["operations"] = json!(
                ops.iter()
                    .filter(|o| o["capabilityIds"].as_array().unwrap().contains(&c["id"]))
                    .collect::<Vec<_>>()
            );
            c
        })
        .collect::<Vec<_>>();
    let next = cursor.saturating_add(items.len() as u64);
    let more = next < matches.len() as u64;
    Ok(
        json!({"catalogue":catalogue(),"total":matches.len(),"count":items.len(),"cursor":cursor,"limit":limit,"hasMore":more,"nextCursor":if more {json!(next)} else {Value::Null},"items":items}),
    )
}
fn resolve_workflow(v: &mut Value, routes: &[Value], tools: Option<&[Value]>, connected: bool) {
    let mut problems = Vec::new();
    for step in v["workflow"]["steps"].as_array_mut().unwrap() {
        let mut mismatches = Vec::new();
        for field in ["operation", "uncertainOutcomeReadBack"] {
            let Some(reference) = step.get(field) else {
                if field == "uncertainOutcomeReadBack" && step["expectedSafety"] != "read" {
                    problems.push(json!({"step":step["id"],"reason":"Missing uncertain-outcome readback; write recovery cannot be attested."}));
                }
                continue;
            };
            let resolved = if tools.is_some()
                && reference["surface"] != "mcp"
                && reference.get("resolved").is_some()
            {
                reference.clone()
            } else {
                fact(reference, routes, tools, connected)
            };
            if resolved["resolved"] != true {
                problems.push(json!({"step":step["id"],"field":field,"id":resolved["id"],"reason":resolved["reason"]}));
            }
            if field == "uncertainOutcomeReadBack"
                && resolved["resolved"] == true
                && resolved["safety"] != "read"
            {
                mismatches.push(json!({"field":"uncertainOutcomeReadBack.safety","expected":"read","actual":resolved["safety"]}));
            }
            if field == "operation" && resolved["resolved"] == true {
                if resolved["safety"] != step["expectedSafety"] {
                    mismatches.push(json!({"field":"safety","expected":step["expectedSafety"],"actual":resolved["safety"]}));
                }
                if resolved["idempotent"] != step["expectedIdempotent"] {
                    mismatches.push(json!({"field":"idempotent","expected":step["expectedIdempotent"],"actual":resolved["idempotent"]}));
                }
            }
            step[field] = resolved;
        }
        if !mismatches.is_empty() {
            problems.push(json!({"step":step["id"],"reason":"Source expectation differs from actual runtime safety/idempotency metadata.","mismatches":mismatches}));
        }
        step["runtimeExpectationMismatches"] = json!(mismatches);
    }
    v["workflow"]["live"] = json!(problems.is_empty());
    v["workflow"]["unresolvedOperations"] = json!(problems);
    v["workflow"]["resolutionReason"] = json!(
        "Every required operation and recovery must resolve positively with matching safety/idempotency; discovery grants no authority or provider configuration."
    );
}
pub fn describe(id: &str, routes: &[Value], connected: bool) -> Option<Value> {
    if id.encode_utf16().count() > 100 {
        return None;
    }
    let w = source()["workflows"]
        .as_array()?
        .iter()
        .find(|w| w["id"] == id)?;
    let c = source()["capabilities"]
        .as_array()?
        .iter()
        .find(|c| c["id"] == w["capabilityId"])?;
    let mut v =
        json!({"catalogue":catalogue(),"capability":c,"workflow":w,"workspaceConnected":connected});
    resolve_workflow(&mut v, routes, None, connected);
    Some(v)
}
pub fn decorate_mcp(mut value: Value, tools: &[Value]) -> Value {
    if value.get("serverName").is_some() {
        value["toolCount"] = json!(tools.len());
    }
    let ops = bindings(&[], Some(tools), false);
    if let Some(items) = value["items"].as_array_mut() {
        for item in items {
            let cap = item["id"].clone();
            let mut merged = item["operations"].as_array().cloned().unwrap_or_default();
            merged.retain(|o| !o["id"].as_str().unwrap_or("").starts_with("mcp:"));
            merged.extend(
                ops.iter()
                    .filter(|o| o["capabilityIds"].as_array().unwrap().contains(&cap))
                    .cloned(),
            );
            sort(&mut merged);
            item["operations"] = json!(merged);
        }
    }
    // Preserve server CLI/HTTP facts while replacing only MCP references.
    if value.get("workflow").is_some() {
        let mut routes = Vec::new();
        for step in value["workflow"]["steps"].as_array().unwrap() {
            for key in ["operation", "uncertainOutcomeReadBack"] {
                if let Some(op) = step.get(key) {
                    if op["surface"] == "http" && op["resolved"] == true {
                        routes.push(json!({"method":op["method"],"path":op["pattern"],"effect":op["safety"]}));
                    }
                }
            }
        }
        let connected = value["workspaceConnected"] == true;
        resolve_workflow(&mut value, &routes, Some(tools), connected);
    }
    value
}
#[cfg(test)]
#[path = "original123_tests.rs"]
mod tests;
