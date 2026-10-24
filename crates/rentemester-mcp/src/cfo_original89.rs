//! Original CFO read ABI over the canonical producer; no local reads or calculations.
use rentemester_client::{ApiClient, CfoAnalyticsQuery, Error};
use serde_json::{Value, json};
const CATALOGUE: &str = r###"{"name":"cfo_analytics_query","title":"Query source-linked CFO analytics","description":"Read-only, versioned historical journal/archive analytics. Portfolio is explicitly non-consolidated; group delegates only to an approved consolidation profile.","annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"workspace":{"type":"string","minLength":1},"scope":{"type":"string","enum":["company","portfolio","group"]},"companySlug":{"type":"string"},"companySlugs":{"maxItems":100,"type":"array","items":{"type":"string"}},"groupProfileId":{"type":"string"},"from":{"type":"string"},"to":{"type":"string"},"account":{"type":"string"},"party":{"description":"Exact canonical workspace party ID; names and document text are not identity filters.","type":"string"},"dimension":{"type":"string"},"currency":{"type":"string"},"aggregate":{"type":"string","enum":["none","sum"]},"cursor":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":200}},"required":["workspace","scope","from","to"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for bogførings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik — agenten kan branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false},"exactSuccessDataSchema":{"status":"UNKNOWN","reason":"Not exposed as a complete typed per-operation wire schema by this registry; handler/fixture verification is required."},"refusalSchema":{"status":"UNKNOWN","reason":"Not exposed as a complete typed per-operation wire schema by this registry; handler/fixture verification is required."},"protocolFailureSchema":{"status":"UNKNOWN","reason":"Not exposed as a complete typed per-operation wire schema by this registry; handler/fixture verification is required."},"retry":{"classification":"safe-read","basis":"INFERRED from read-only registry classification; not independently behavior-verified"},"migrationOwner":"R45","evidence":[{"path":"src/mcp/registry.ts","sha256":"98e898529c433a6f688719e13c981675d5267ebc88c6389b911e0012b8e51e11"},{"path":"scripts/rust/capture-baseline.ts","sha256":"109b98a3a0811e7b735cc0bf9ab7e39047cd967932c06293227b017c5a69b96e"},{"path":"src/mcp/tools/cfo-analytics.ts","sha256":"1c2852c4b1702f551c5f6b446bd9af9d813d822440bd8636e27881f7d459d186"}]}"###;
pub fn accepts(name: &str) -> bool {
    name == "cfo_analytics_query"
}
pub fn tools() -> Vec<Value> {
    let mut t: Value = serde_json::from_str(CATALOGUE).expect("static source definition");
    t["description"] = json!(format!(
        "{} Native binding: workspace must equal the already verified --local-profile workspace root or ID, freshly checked before this read. Remote original-workspace assertions are unavailable: no authenticated remote workspace identity producer. Native cfo_analytics remains remote-capable. Current server visibility, physical archive proof, calendar/text/cursor bounds and approved group profile apply; no cross-request snapshot, fallback, aggregation or FX on the client.",
        t["description"].as_str().unwrap()
    ));
    vec![t]
}
fn grammar(v: &Value, s: &Value) -> bool {
    if s["enum"].as_array().is_some_and(|a| !a.contains(v)) {
        return false;
    }
    match s["type"].as_str() {
        Some("object") => v.as_object().is_some_and(|o| {
            s["required"]
                .as_array()
                .is_none_or(|a| a.iter().all(|k| o.contains_key(k.as_str().unwrap())))
                && o.iter()
                    .all(|(k, v)| s["properties"].get(k).is_some_and(|s| grammar(v, s)))
        }),
        Some("string") => v.as_str().is_some_and(|v| {
            s["minLength"]
                .as_u64()
                .is_none_or(|n| v.encode_utf16().count() as u64 >= n)
        }),
        Some("integer") => v.as_u64().is_some_and(|n| n >= 1 && n <= 200),
        Some("array") => v
            .as_array()
            .is_some_and(|a| a.len() <= 100 && a.iter().all(Value::is_string)),
        _ => false,
    }
}
fn result(e: Value) -> Value {
    json!({"isError":e["ok"]!=true,"content":[{"type":"text","text":e.to_string()}],"structuredContent":e})
}
fn fail(message: &str, code: &str) -> Value {
    result(json!({"ok":false,"errors":[message],"code":code}))
}
fn optional(args: &Value, key: &str) -> Option<String> {
    args[key].as_str().map(str::to_owned)
}
fn normal(args: &Value, key: &str) -> Option<String> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    if !accepts(name) {
        return Err("unknown original CFO tool");
    }
    let t: Value = serde_json::from_str(CATALOGUE).expect("static source definition");
    if !grammar(&args, &t["inputSchema"]) {
        return Err("invalid original CFO arguments");
    }
    if let Err(e) = api.assert_original_workspace89(args["workspace"].as_str().unwrap()) {
        return Ok(fail(&e.to_string(), "ANALYTICS_QUERY_REJECTED"));
    }
    let scope = args["scope"].as_str().unwrap();
    // Source company scope ignores plural, portfolio/group ignore singular.
    // Empty plural leaves selection to the server, never a client catalogue.
    let company_slugs = if scope == "company" {
        vec![normal(&args, "companySlug").unwrap_or_default()]
    } else {
        let mut slugs: Vec<String> = args["companySlugs"]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
            .unwrap_or_default();
        slugs.sort();
        slugs.dedup();
        slugs
    };
    let query = CfoAnalyticsQuery {
        scope: scope.to_owned(),
        from: args["from"].as_str().unwrap().to_owned(),
        to: args["to"].as_str().unwrap().to_owned(),
        company_slugs,
        group_profile_id: if scope == "group" {
            optional(&args, "groupProfileId")
        } else {
            None
        },
        account: normal(&args, "account"),
        party: normal(&args, "party"),
        dimension: normal(&args, "dimension"),
        currency: normal(&args, "currency").map(|s| s.to_uppercase()),
        aggregate: optional(&args, "aggregate"),
        cursor: optional(&args, "cursor").filter(|s| !s.is_empty()),
        limit: args["limit"].as_u64().map(|n| n as u16),
    };
    let mut body = match api.cfo_analytics(&query).await {
        Ok(v) => v,
        Err(e) => {
            return Ok(fail(
                &e.to_string(),
                match e {
                    Error::Refused { status: 401 } => "UNAUTHORIZED",
                    Error::Protocol => "protocol",
                    Error::Transport => "transport",
                    _ => "ANALYTICS_QUERY_REJECTED",
                },
            ));
        }
    };
    if body["ok"] == false {
        let Some(code) = body["code"].as_str() else {
            return Ok(fail("Malformed CFO refusal", "protocol"));
        };
        let errors = body["errors"]
            .as_array()
            .filter(|a| !a.is_empty() && a.iter().all(Value::is_string))
            .cloned()
            .unwrap_or_else(|| vec![json!(code)]);
        return Ok(result(json!({"ok":false,"errors":errors,"code":code})));
    }
    if body["ok"] != true
        || body["schemaVersion"] != "rentemester-cfo-analytics-v1"
        || body["scope"] != scope
        || (scope == "group" && !body["group"].is_object())
        || (scope != "group"
            && (!body["rows"].is_array()
                || !body["page"].is_object()
                || !body["reconciliation"].is_object()))
    {
        return Ok(fail("Malformed canonical CFO result", "protocol"));
    }
    // Only the native transport marker is removed. Preserve all producer body fields,
    // including source links, partial/blocked state and exact camelCase names.
    body.as_object_mut().unwrap().remove("ok");
    Ok(result(json!({"ok":true,"data":body,"errors":[]})))
}
