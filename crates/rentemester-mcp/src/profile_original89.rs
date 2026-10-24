//! Original read ABI over the current authorized canonical company producer.
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
const CATALOGUE: &str = r###"{"name": "company_profile_get", "title": "Get the stored company profile", "description": "Returnerer virksomhedens gemte profil-stamdata: navn, CVR, valuta, land, adresse, regnskabsår-start, regnskabsklasse-strategi, betalingsfrist (dage), momsperiode-kadence, revisor-fravalg og CVR-status. Hver fakturering, momsrapport og årsrapport bygger implicit på disse felter, så en agent bør læse dem før den udsteder noget på virksomhedens vegne. Read-only.\n\nenvelope.data.profile har shape { id, name, country, currency, cvr, fiscalYearStartMonth, fiscalYearLabelStrategy, address, postalCode, city, companyForm, industryCode, industryText, cvrStatus, auditWaived, cvrSyncedAt, paymentTermsDays, vatPeriodType, vatRegistered }. `vatPeriodType` er 'month' / 'quarter' / 'half-year', ELLER `null` når virksomheden IKKE er momsregistreret (holdingselskab m.fl.); `vatRegistered` er den afledte boolean (false ⇔ vatPeriodType er null) — branch på den frem for at antage en kadence. For et ikke-momsregistreret selskab afviser moms-rapport-værktøjerne, og købsmoms absorberes i udgiften (non_deductible). Defaults indsættes for felter ledgeren ikke har (ny virksomhed), så shape'en er stabil.", "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}, "inputSchema": {"$schema": "https://json-schema.org/draft/2020-12/schema", "type": "object", "properties": {"company": {"type": "string", "minLength": 1, "description": "Absolute path to the company directory, or a workspace slug."}}, "required": ["company"], "additionalProperties": false}, "outputSchema": {"$schema": "https://json-schema.org/draft/2020-12/schema", "type": "object", "properties": {"ok": {"type": "boolean", "description": "true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."}, "data": {"description": "Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.", "type": "object", "properties": {}, "additionalProperties": {}}, "errors": {"type": "array", "items": {"type": "string"}, "description": "Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."}, "appliedRules": {"description": "Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for bogførings-tools; udeladt ellers.", "type": "array", "items": {"type": "string"}}, "code": {"description": "Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik — agenten kan branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.", "type": "string"}}, "required": ["ok", "errors"], "additionalProperties": false}, "exactSuccessDataSchema": {"status": "UNKNOWN", "reason": "Not exposed as a complete typed per-operation wire schema by this registry; handler/fixture verification is required."}, "refusalSchema": {"status": "UNKNOWN", "reason": "Not exposed as a complete typed per-operation wire schema by this registry; handler/fixture verification is required."}, "protocolFailureSchema": {"status": "UNKNOWN", "reason": "Not exposed as a complete typed per-operation wire schema by this registry; handler/fixture verification is required."}, "retry": {"classification": "safe-read", "basis": "INFERRED from read-only registry classification; not independently behavior-verified"}, "migrationOwner": "R45", "evidence": [{"path": "src/mcp/registry.ts", "sha256": "98e898529c433a6f688719e13c981675d5267ebc88c6389b911e0012b8e51e11"}, {"path": "scripts/rust/capture-baseline.ts", "sha256": "109b98a3a0811e7b735cc0bf9ab7e39047cd967932c06293227b017c5a69b96e"}, {"path": "src/mcp/tools/company.ts", "sha256": "27021e8ea35227338125f585bae804672fe51894e3b8f0c7099fe52d12123faa"}]}"###;
pub fn accepts(name: &str) -> bool {
    name == "company_profile_get"
}
pub fn tools() -> Vec<Value> {
    let mut t: Value = serde_json::from_str(CATALOGUE).expect("source catalogue");
    t["description"] = json!(format!(
        "{} Native binding: explicit company slug on the already configured server/private Unix runtime; filesystem selectors are unavailable here. Current server credentials and CompanyRead authority apply. Projects the stored 18 common fields and derives vatRegistered only from validated nullable vatPeriodType; native payment/createdAt are omitted. No local data fallback, client defaults, retries, writes, or cross-request snapshot guarantee.",
        t["description"].as_str().unwrap()
    ));
    vec![t]
}
fn result(e: Value) -> Value {
    json!({"isError":e["ok"]!=true,"content":[{"type":"text","text":e.to_string()}],"structuredContent":e})
}
fn fail(message: &str, code: &str) -> Value {
    result(json!({"ok":false,"errors":[message],"code":code}))
}
const TEXT: &[&str] = &["name", "country", "currency"];
const NULLABLE: &[&str] = &[
    "cvr",
    "address",
    "postalCode",
    "city",
    "companyForm",
    "industryCode",
    "industryText",
    "cvrStatus",
    "cvrSyncedAt",
];
// Required producer fields, not defaults. A source-shaped or incomplete response
// cannot be substituted for the current native authority.
fn project(p: &Value) -> Option<Value> {
    let o = p.as_object()?;
    if o.len() != 20
        || !TEXT.iter().all(|k| o.get(*k).is_some_and(Value::is_string))
        || !NULLABLE
            .iter()
            .all(|k| o.get(*k).is_some_and(|v| v.is_null() || v.is_string()))
        || !p["id"].as_i64().is_some_and(|n| n > 0)
        || !p["fiscalYearStartMonth"]
            .as_i64()
            .is_some_and(|n| (1..=12).contains(&n))
        || !matches!(
            p["fiscalYearLabelStrategy"].as_str(),
            Some("start-year" | "end-year" | "span")
        )
        || !p["paymentTermsDays"]
            .as_i64()
            .is_some_and(|n| i32::try_from(n).is_ok())
        || !o
            .get("auditWaived")
            .is_some_and(|v| v.is_null() || v.is_boolean())
        || !o.get("vatPeriodType").is_some_and(|v| {
            v.is_null() || matches!(v.as_str(), Some("month" | "quarter" | "half-year"))
        })
        || !p["createdAt"].is_string()
        || !o.get("payment").is_some_and(|v| {
            v.is_null()
                || v.as_object().is_some_and(|o| {
                    o.len() == 4
                        && ["bankName", "registrationNo", "accountNo", "iban"]
                            .iter()
                            .all(|k| o.get(*k).is_some_and(|v| v.is_null() || v.is_string()))
                })
        })
    {
        return None;
    }
    let mut out = o.clone();
    out.remove("createdAt");
    out.remove("payment");
    out.insert("vatRegistered".into(), json!(!p["vatPeriodType"].is_null()));
    Some(Value::Object(out))
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let o = args
        .as_object()
        .ok_or("expected original profile arguments")?;
    let company = o
        .get("company")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("company must be a nonempty string")?;
    if !accepts(name) || o.len() != 1 {
        return Err("company_profile_get accepts only company");
    }
    // Source grammar accepts paths; this adapter does not resolve local files.
    // A valid source selector outside the native binding is a tool refusal.
    if !company
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    {
        return Ok(fail(
            "This binding requires an explicit company slug; filesystem company selectors are unavailable",
            "invalid_input",
        ));
    }
    let v = match api.company_profile(company).await {
        Ok(v) => v,
        Err(e) => {
            return Ok(fail(
                &e.to_string(),
                match e {
                    Error::InvalidInput(_) => "invalid_input",
                    Error::Refused { .. } => "server_refused",
                    Error::Protocol => "protocol",
                    _ => "transport",
                },
            ));
        }
    };
    if v["ok"] == false {
        if let Some(o) = v.as_object().filter(|o| {
            o.keys()
                .all(|k| matches!(k.as_str(), "ok" | "errors" | "code"))
        }) {
            if v["errors"]
                .as_array()
                .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
                && o.get("code").is_none_or(Value::is_string)
            {
                return Ok(result(v));
            }
        }
    }
    if v["ok"] == true && v.as_object().is_some_and(|o| o.len() == 2) {
        if let Some(p) = project(&v["company"]) {
            return Ok(result(json!({"ok":true,"data":{"profile":p},"errors":[]})));
        }
    }
    Ok(fail(
        "Invalid canonical native company profile response",
        "protocol",
    ))
}
