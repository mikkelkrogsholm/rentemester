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
fn fail(message: impl Into<String>, code: Option<&str>) -> Value {
    let mut e = json!({"ok":false,"errors":[message.into()]});
    if let Some(code) = code {
        e["code"] = json!(code);
    }
    result(e)
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
fn header(v: &Value) -> Option<Value> {
    let year = safe_integer(&v["fiscalYear"])?;
    if !(1000..=9999).contains(&year) || v["sourceSystem"] != "dinero" {
        return None;
    }
    for key in ["postingCount", "balanceCount"] {
        if safe_integer(&v[key])? < 0 {
            return None;
        }
    }
    if v["importedAt"].as_str()?.is_empty() {
        return None;
    }
    let mut out = json!({});
    for key in [
        "fiscalYear",
        "sourceSystem",
        "postingCount",
        "balanceCount",
        "importedAt",
    ] {
        out[key] = v[key].clone();
    }
    Some(out)
}
// Native Money is deliberately NOT read here. Reuse the native decimal witness
// validator and numeric round-trip guard instead of rounding or scaled casts.
fn number(v: &Value) -> Option<Value> {
    let raw = v.as_str()?;
    let proof = ArchiveDecimal::parse_canonical(raw).ok()?;
    let n: serde_json::Number = raw.parse().ok()?;
    if !proof.matches_numeric_decimal(&n.to_string()) {
        return None;
    }
    Some(Value::Number(n))
}
fn rows(v: &Value, posting: bool) -> Option<Vec<Value>> {
    let mut out = vec![];
    let mut last = None;
    for row in v.as_array()? {
        let line = safe_integer(&row["lineNo"])?;
        if line < 0 || last.is_some_and(|n| line <= n) {
            return None;
        }
        last = Some(line);
        let mut r = json!({"lineNo":line,"amount":number(&row["sourceAmount"])?});
        let fields: &[&str] = if posting {
            &[
                "accountNo",
                "accountName",
                "transactionDate",
                "voucher",
                "voucherType",
                "text",
                "vatType",
            ]
        } else {
            &["accountNo", "accountName"]
        };
        for key in fields {
            r[*key] = json!(row[*key].as_str()?);
        }
        if posting {
            let raw = row.get("sourceRunningBalance")?;
            r["runningBalance"] = if raw.is_null() {
                Value::Null
            } else {
                number(raw)?
            };
        }
        out.push(r);
    }
    Some(out)
}
async fn read(api: &ApiClient, company: &str, year: Option<u16>) -> Result<Value, Value> {
    match api.archives(company, year).await {
        Err(e) => Err(fail(
            e.to_string(),
            Some(match e {
                rentemester_client::Error::Protocol => "protocol",
                rentemester_client::Error::InvalidInput(_) => "native_binding_unavailable",
                rentemester_client::Error::Refused { .. } => "server_refused",
                _ => "transport",
            }),
        )),
        Ok(v) if v["ok"] == false => {
            if !v["errors"]
                .as_array()
                .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
                || !v["code"].is_string()
            {
                return Err(fail("Invalid native archive refusal", Some("protocol")));
            }
            Err(result(
                json!({"ok":false,"errors":v["errors"],"code":v["code"]}),
            ))
        }
        Ok(v)
            if v["ok"] == true
                && v["errors"].as_array().is_some_and(Vec::is_empty)
                && v["years"].is_array() =>
        {
            Ok(v)
        }
        Ok(_) => Err(fail("Invalid guarded archive response", Some("protocol"))),
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
    let list = match read(api, company, None).await {
        Ok(v) => v,
        Err(e) => return Ok(e),
    };
    let mut headers = vec![];
    let mut previous = 0;
    for v in list["years"].as_array().unwrap() {
        let Some(h) = header(v) else {
            return Ok(fail("Invalid native archive header", Some("protocol")));
        };
        let y = safe_integer(&h["fiscalYear"]).unwrap();
        if y <= previous {
            return Ok(fail("Invalid native archive ordering", Some("protocol")));
        }
        previous = y;
        if h["sourceSystem"] == source && year.is_none_or(|n| n == y) {
            headers.push(h);
        }
    }
    if !detail {
        return Ok(success(headers));
    }
    if headers.is_empty() {
        return Ok(fail(
            format!(
                "no archived {source} data for fiscal year {}",
                year.unwrap()
            ),
            None,
        ));
    }
    // Only a present guarded four-digit year can reach the native DETAIL route.
    let y = u16::try_from(year.unwrap()).expect("validated native header year");
    let data = match read(api, company, Some(y)).await {
        Ok(v) => v,
        Err(e) => return Ok(e),
    };
    let years = data["years"].as_array().unwrap();
    if years.len() != 1 {
        return Ok(fail("Invalid native archive detail", Some("protocol")));
    }
    let Some(mut h) = header(&years[0]) else {
        return Ok(fail(
            "Invalid native archive detail header",
            Some("protocol"),
        ));
    };
    if h["fiscalYear"] != y || h["sourceSystem"] != source {
        return Ok(fail(
            "Native archive detail selector mismatch",
            Some("protocol"),
        ));
    }
    let Some(postings) = rows(&years[0]["postings"], true) else {
        return Ok(fail(
            "Archive postings lack representable raw source witnesses",
            Some("native_source_unavailable"),
        ));
    };
    let Some(balances) = rows(&years[0]["balances"], false) else {
        return Ok(fail(
            "Archive balances lack representable raw source witnesses",
            Some("native_source_unavailable"),
        ));
    };
    if h["postingCount"].as_u64() != Some(postings.len() as u64)
        || h["balanceCount"].as_u64() != Some(balances.len() as u64)
    {
        return Ok(fail(
            "Native archive detail counts differ",
            Some("protocol"),
        ));
    }
    h["postings"] = json!(postings);
    h["balances"] = json!(balances);
    Ok(success(vec![h]))
}
