use crate::tools::{native_body, tool};
use rentemester_client::{
    ApiClient, Error, MAX_COMPANY_CREATION_BODY, company_creation_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let mut body = native_body(company_creation_body_schema());
    body["maxLength"] = json!(MAX_COMPANY_CREATION_BODY);
    let mut tools = vec![tool(
        "create_company",
        "WorkspaceManage: create one fresh separate company ledger and explicit creator owner grant. Exact native16KiB body, outer confirm:true only; no company selector or inner confirm/key. Server validates namespace/current rights and owns slug/calendar/VAT defaults. No copied chart/balances, inferred mappings or automatic retry. Unknown publication outcome requires companies/current profile reads and operator reconciliation before another attempt; duplicate refusal is no replay receipt.",
        json!({"type":"object","required":["body","confirm"],"properties":{"body":body,"confirm":{"const":true}},"additionalProperties":false}),
        false,
    )];
    let mut original: Value = serde_json::from_str(ORIGINAL_CATALOGUE).expect("frozen company_add catalogue");
    original["description"] = json!(format!("{} Rust binding: supplied workspace must be an absolute root matching a freshly verified --local-profile. Omitted/blank workspace uses the configured authenticated server, not RENTEMESTER_WORKSPACE discovery; this unqualified variant is not filesystem compatibility. Current WorkspaceManage, fresh ledger and explicit creator grant remain server-owned. No retry; reconcile unknown outcomes through companies/profile reads.", original["description"].as_str().unwrap()));
    tools.push(original);
    tools
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    body: String,
    confirm: bool,
}
pub async fn call(api: &ApiClient, args: Value) -> Result<Result<Value, Error>, &'static str> {
    let input: Input = serde_json::from_value(args)
        .map_err(|_| "create_company requires only body and confirm:true")?;
    if input.body.len() > MAX_COMPANY_CREATION_BODY {
        return Err("company creation exceeds native16KiB");
    }
    Ok(api
        .create_company(input.body.into_bytes(), input.confirm)
        .await)
}

const ORIGINAL_CATALOGUE: &str = r###"{"name":"company_add","title":"Add a company to the workspace","description":"Opretter en ny virksomhed i workspace'et: udleder/validerer et slug, bygger virksomheds-volumen under <workspace>/<slug>/, initialiserer ledgeren og registrerer virksomheden i workspace-manifestet. Workspace-niveau write-tool — kalder kerne-funktionen createCompany. Mutationer er altid enkelt-virksomhed; dette tool tilføjer netop én. write-irreversible.","annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"workspace":{"description":"Absolute path to the workspace root. The new company is created at <workspace>/<slug>/. When omitted, the workspace falls back to the RENTEMESTER_WORKSPACE environment variable on the MCP server's host; if that is also unset the call is rejected with \"no workspace given: pass 'workspace' or set RENTEMESTER_WORKSPACE\".","type":"string"},"name":{"type":"string","minLength":1,"description":"Registered name of the company, e.g. 'Acme ApS'."},"slug":{"description":"Optional URL-safe slug used as the company's directory name under the workspace. When omitted, a slug is derived from `name`.","type":"string"},"cvr":{"description":"Optional Danish CVR number for the company, e.g. '12345678'.","type":"string"},"fiscalYearStartMonth":{"description":"Month the fiscal year starts, 1 = January … 12 = December (default 1).","type":"integer","minimum":1,"maximum":12},"fiscalYearLabelStrategy":{"description":"How a fiscal year spanning two calendar years is labelled: 'end-year' = the year it ends in; 'start-year' = the year it starts in; 'span' = both years (e.g. '2025/2026').","type":"string","enum":["end-year","start-year","span"]},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["name"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for bogførings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik — agenten kan branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false},"exactSuccessDataSchema":{"status":"UNKNOWN","reason":"Not exposed as a complete typed per-operation wire schema by this registry; handler/fixture verification is required."},"refusalSchema":{"status":"UNKNOWN","reason":"Not exposed as a complete typed per-operation wire schema by this registry; handler/fixture verification is required."},"protocolFailureSchema":{"status":"UNKNOWN","reason":"Not exposed as a complete typed per-operation wire schema by this registry; handler/fixture verification is required."},"retry":{"classification":"UNKNOWN","basis":"No exact operation entry in exposed retry registry. Do not infer safe retries from annotation alone."},"migrationOwner":"R45","evidence":[{"path":"src/mcp/registry.ts","sha256":"98e898529c433a6f688719e13c981675d5267ebc88c6389b911e0012b8e51e11"},{"path":"src/mcp/tools/portfolio.ts","sha256":"8665e307b7aa1eed4a2549b7d8b0d07cf25aa82b8bfd4574a7704e8f15616f0c"}]}"###;

fn original_result(envelope: Value) -> Value {
    json!({"isError":envelope["ok"] != true,"content":[{"type":"text","text":envelope.to_string()}],"structuredContent":envelope})
}
fn original_failure(error: Error) -> Value {
    let code = match error {
        Error::InvalidInput(_) => "invalid_input",
        Error::Refused { .. } => "server_refused",
        Error::OutcomeUnknown => "outcome_unknown",
        Error::Protocol => "protocol",
        Error::Transport => "transport",
    };
    original_result(json!({"ok":false,"errors":[error.to_string()],"code":code}))
}
/// Already wrapped original envelope; never pass through the generic native wrapper.
pub async fn call_original(api: &ApiClient, args: Value) -> Result<Value, &'static str> {
    let schema: Value = serde_json::from_str(ORIGINAL_CATALOGUE).expect("source catalogue");
    if !valid_original_args(&args, &schema["inputSchema"]) {
        // Original SDK validation failures have text only, not a business envelope.
        return Ok(json!({"isError":true,"content":[{"type":"text","text":"MCP error -32602: Input validation error: Invalid arguments for tool company_add"}]}));
    }
    if args.get("confirm") != Some(&json!(true)) {
        return Ok(original_result(json!({"ok":false,"errors":["confirm: true required for write tool company_add"],"code":"CONFIRM_REQUIRED"})));
    }
    match api.create_company_original116(args).await {
        Ok(v) if v["ok"] == false => Ok(original_result(v)),
        Ok(v) => Ok(original_result(json!({"ok":true,"data":{"slug":v["company"]["slug"],"name":v["company"]["name"]},"errors":[]}))),
        Err(e) => Ok(original_failure(e)),
    }
}

fn valid_original_args(args: &Value, schema: &Value) -> bool {
    let Some(o) = args.as_object() else { return false; };
    o.get("name").is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()))
        && o.iter().all(|(k,v)| {
            if schema["properties"].get(k).is_none() { return false; }
            match k.as_str() {
                "confirm" => v.is_boolean(),
                "fiscalYearStartMonth" => v.as_f64().is_some_and(|n| n.fract() == 0.0 && (1.0..=12.0).contains(&n)),
                "fiscalYearLabelStrategy" => matches!(v.as_str(),Some("end-year"|"start-year"|"span")),
                _ => v.is_string(),
            }
        })
}
