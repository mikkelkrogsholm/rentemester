//! Original read-only archive ABI over current guarded physical-source history.
use rentemester_client::ApiClient;
use rentemester_engine::archive_import::ArchiveDecimal;
use serde_json::{Value, json};
const CATALOGUE: &str = r###"[{"name":"import_archive_list","title":"List archived fiscal years","description":"Lister de pre-cut-over regnskabsår der er arkiveret fra et flerårigt eksport. Read-only referencedata uden for hovedbogen.","annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1,"description":"Absolute path to the company directory, or a workspace slug."},"sourceSystem":{"type":"string","minLength":1}},"required":["company"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for bogførings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik — agenten kan branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false},"exactSuccessDataSchema":{"status":"UNKNOWN","reason":"Not exposed as a complete typed per-operation wire schema by this registry; handler/fixture verification is required."},"refusalSchema":{"status":"UNKNOWN","reason":"Not exposed as a complete typed per-operation wire schema by this registry; handler/fixture verification is required."},"protocolFailureSchema":{"status":"UNKNOWN","reason":"Not exposed as a complete typed per-operation wire schema by this registry; handler/fixture verification is required."},"retry":{"classification":"safe-read","basis":"INFERRED from read-only registry classification; not independently behavior-verified"},"migrationOwner":"R45","evidence":[{"path":"src/mcp/registry.ts","sha256":"98e898529c433a6f688719e13c981675d5267ebc88c6389b911e0012b8e51e11"},{"path":"scripts/rust/capture-baseline.ts","sha256":"109b98a3a0811e7b735cc0bf9ab7e39047cd967932c06293227b017c5a69b96e"},{"path":"src/mcp/tools/import.ts","sha256":"7eccc9b2ea3e07848245bbe4c604593b5a2683db52c3461c4f1ca7c5fa49f829"}]},{"name":"import_archive_year","title":"Read an archived fiscal year","description":"Henter ét arkiveret regnskabsårs fulde Posteringer og SaldoBalance som audit- og matchnings-kontekst. Read-only.","annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1,"description":"Absolute path to the company directory, or a workspace slug."},"fiscalYear":{"type":"integer","minimum":-9007199254740991,"maximum":9007199254740991},"sourceSystem":{"type":"string","minLength":1}},"required":["company","fiscalYear"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for bogførings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik — agenten kan branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false},"exactSuccessDataSchema":{"status":"UNKNOWN","reason":"Not exposed as a complete typed per-operation wire schema by this registry; handler/fixture verification is required."},"refusalSchema":{"status":"UNKNOWN","reason":"Not exposed as a complete typed per-operation wire schema by this registry; handler/fixture verification is required."},"protocolFailureSchema":{"status":"UNKNOWN","reason":"Not exposed as a complete typed per-operation wire schema by this registry; handler/fixture verification is required."},"retry":{"classification":"safe-read","basis":"INFERRED from read-only registry classification; not independently behavior-verified"},"migrationOwner":"R45","evidence":[{"path":"src/mcp/registry.ts","sha256":"98e898529c433a6f688719e13c981675d5267ebc88c6389b911e0012b8e51e11"},{"path":"scripts/rust/capture-baseline.ts","sha256":"109b98a3a0811e7b735cc0bf9ab7e39047cd967932c06293227b017c5a69b96e"},{"path":"src/mcp/tools/import.ts","sha256":"7eccc9b2ea3e07848245bbe4c604593b5a2683db52c3461c4f1ca7c5fa49f829"}]}]"###;
pub fn accepts(name: &str) -> bool {
    matches!(name, "import_archive_list" | "import_archive_year")
}
pub fn tools() -> Vec<Value> {
    let mut tools: Vec<Value> =
        serde_json::from_str(CATALOGUE).expect("immutable original catalogue");
    for t in &mut tools {
        t["description"] = json!(format!(
            "{} Native binding: explicit company slug; current guarded Dinero archive history, physical originals and current rights revalidated on every request. Arbitrary sourceSystem and safe-integer fiscalYear selectors are filtered only after an authoritative list. No fallback, retries, cross-request snapshot or ledger effects. Raw six-decimal witnesses must be representable as source JSON numbers.",
            t["description"].as_str().unwrap()
        ));
    }
    tools
}
fn result(e: Value) -> Value {
    json!({"isError":e["ok"]!=true,"content":[{"type":"text","text":e.to_string()}],"structuredContent":e})
}
fn success(years: Vec<Value>) -> Value {
    result(json!({"ok":true,"data":{"years":years},"errors":[]}))
}
fn safe_integer(v: &Value) -> Option<i64> {
    let n = v.as_f64()?;
    if n.is_finite() && n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0 {
        Some(n as i64)
    } else {
        None
    }
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let detail = name == "import_archive_year";
    if !accepts(name) {
        return Err("unknown archive tool");
    }
    let obj = args
        .as_object()
        .ok_or("archive arguments must be an object")?;
    if obj.keys().any(|k| {
        !matches!(k.as_str(), "company" | "sourceSystem") && !(detail && k == "fiscalYear")
    }) {
        return Err("unknown archive argument");
    }
    let company = args["company"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("company must be a nonempty string")?;
    let source = match args.get("sourceSystem") {
        None => "dinero",
        Some(v) => v
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("sourceSystem must be a nonempty string")?,
    };
    let year = if detail {
        Some(safe_integer(&args["fiscalYear"]).ok_or("fiscalYear must be a safe integer")?)
    } else {
        None
    };
    let v = rentemester_client::query_original_archive(
        api,
        company,
        source,
        year.map(|y| y as f64),
        |raw, numeric| {
            ArchiveDecimal::parse_canonical(raw).is_ok_and(|p| p.matches_numeric_decimal(numeric))
        },
    )
    .await;
    if v["ok"] == true {
        Ok(success(v["years"].as_array().unwrap().clone()))
    } else {
        let mut e = json!({"ok":false,"errors":v["errors"]});
        if let Some(code) = v.get("code") {
            e["code"] = code.clone();
        }
        Ok(result(e))
    }
}
