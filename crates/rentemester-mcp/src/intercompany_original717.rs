//! Original flat evidence lifecycle over the shared SDK. No ledger effects or retries.
use rentemester_client::{ApiClient, DispositionAction as Action, Error};
use rentemester_engine::intercompany_dispositions::{Disposition, text};
use serde::Deserialize;
use serde_json::{Value, json};
const CATALOGUE: &str = r###"[{"name":"intercompany_disposition_plan","title":"Dry-run intercompany disposition","description":"Validates both legal sides and source evidence. It has no workspace or ledger effects.","annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"disposition":{}},"required":["company","disposition"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}},{"name":"intercompany_disposition_propose","title":"Propose two-sided intercompany disposition","description":"Records immutable source-linked evidence only. Each legal company must still post through its normal flow. Requires actor attribution and confirm:true; retry reads the existing source-hash result. write-reversible.","annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"disposition":{},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company","disposition"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}},{"name":"intercompany_disposition_approve","title":"Approve exact intercompany disposition","description":"Approves an exact proposal with a distinct stable principal; it never posts either ledger. Requires actor attribution and confirm:true; retry reads the exact approval. write-reversible.","annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"dispositionId":{"type":"string","minLength":3},"payloadHash":{"type":"string","pattern":"^[a-f0-9]{64}$"},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company","dispositionId","payloadHash"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}},{"name":"intercompany_disposition_link","title":"Link independently posted journal","description":"Validates the current posted journal, source currency/date/side and ledger head, then records one immutable link. It never posts a ledger entry. Requires actor attribution and confirm:true; retry reads the immutable link. write-reversible.","annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"dispositionId":{"type":"string","minLength":3},"payloadHash":{"type":"string","pattern":"^[a-f0-9]{64}$"},"side":{"type":"string","enum":["left","right"]},"journalEntryId":{"type":"integer","exclusiveMinimum":0,"maximum":9007199254740991},"ledgerHeadHash":{"anyOf":[{"type":"string","pattern":"^[a-f0-9]{64}$"},{"type":"null"}]},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company","dispositionId","payloadHash","side","journalEntryId","ledgerHeadHash"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}},{"name":"intercompany_disposition_status","title":"Inspect intercompany disposition status","description":"Read-only current status with one-sided, stale/reversed and overdue exceptions. Hidden endpoints are denied before this tool runs.","annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"dispositionId":{"type":"string","minLength":3},"asOf":{"type":"string"}},"required":["company","dispositionId"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}},{"name":"intercompany_disposition_settle","title":"Settle fully posted intercompany disposition","description":"Appends settlement evidence only after both current legal-ledger links validate; it never posts either ledger. Requires actor attribution and confirm:true; retry reads the settlement state. write-reversible.","annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"dispositionId":{"type":"string","minLength":3},"payloadHash":{"type":"string","pattern":"^[a-f0-9]{64}$"},"settlementEvidenceRecordIds":{"minItems":1,"type":"array","items":{"type":"string","minLength":3}},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company","dispositionId","payloadHash","settlementEvidenceRecordIds"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}},{"name":"intercompany_disposition_reopen","title":"Reopen settled or superseded intercompany disposition","description":"Appends a reopen decision only after current journal and ledger-head validation; it never changes a ledger. Requires actor attribution and confirm:true; retry reads the reopen state. write-reversible.","annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"dispositionId":{"type":"string","minLength":3},"payloadHash":{"type":"string","pattern":"^[a-f0-9]{64}$"},"reason":{"type":"string","minLength":1,"maxLength":1000},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company","dispositionId","payloadHash","reason"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}},{"name":"intercompany_disposition_supersede","title":"Supersede active intercompany disposition","description":"Appends a replacement reference to an existing disposition; it never deletes evidence or writes a ledger. Requires actor attribution and confirm:true; retry reads the supersession chain. write-reversible.","annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false},"inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"dispositionId":{"type":"string","minLength":3},"payloadHash":{"type":"string","pattern":"^[a-f0-9]{64}$"},"replacementDispositionId":{"type":"string","minLength":3},"reason":{"type":"string","minLength":1,"maxLength":1000},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company","dispositionId","payloadHash","replacementDispositionId","reason"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false}}]"###;
fn catalogue() -> Vec<Value> {
    serde_json::from_str(CATALOGUE).expect("static original catalogue")
}
pub fn accepts(name: &str) -> bool {
    catalogue().iter().any(|t| t["name"] == name)
}
pub fn tools() -> Vec<Value> {
    catalogue().into_iter().map(|mut t| {
        t["description"] = json!(format!("{} Shared native32KiB evidence-only lifecycle; current two-company authority and independent approval remain server-owned. Native calendar, exact integer money and UTF16/NFC bounds apply. No posting, payment, generated IDs/hashes, head refresh or retry. Unknown writes require current-authorized exact-ID status/history reconciliation. Native human credentials only; service credentials are refused by the server. Global status accepts any current admitted company anchor; native plan/mutation routes require an endpoint anchor. Nonendpoint mutation contexts remain an explicit native-profile boundary. Reuse normalized plan disposition and ID for proposal.", t["description"].as_str().unwrap())); t
    }).collect()
}
fn op(name: &str) -> &str {
    name.strip_prefix("intercompany_disposition_").unwrap_or("")
}
fn action(op: &str) -> Action {
    match op {
        "plan" => Action::Plan,
        "propose" => Action::Propose,
        "approve" => Action::Approve,
        "link" => Action::Link,
        "settle" => Action::Settle,
        "supersede" => Action::Supersede,
        _ => Action::Reopen,
    }
}
fn hash(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
// Only the original outer grammar. disposition is deliberately unknown in that grammar;
// the native typed normalizer below admits its bounded safe subset without rewriting it.
fn grammar(v: &Value, s: &Value) -> bool {
    if s.as_object().is_some_and(|o| o.is_empty()) {
        return true;
    }
    if s["enum"].as_array().is_some_and(|a| !a.contains(v)) {
        return false;
    }
    if s["anyOf"]
        .as_array()
        .is_some_and(|a| a.iter().any(|s| grammar(v, s)))
    {
        return true;
    }
    match s["type"].as_str() {
        Some("object") => v.as_object().is_some_and(|o| {
            s["required"]
                .as_array()
                .is_none_or(|r| r.iter().all(|k| o.contains_key(k.as_str().unwrap())))
                && o.iter()
                    .all(|(k, v)| s["properties"].get(k).is_some_and(|s| grammar(v, s)))
        }),
        Some("string") => v.as_str().is_some_and(|t| {
            s["minLength"]
                .as_u64()
                .is_none_or(|m| t.encode_utf16().count() as u64 >= m)
                && s["maxLength"]
                    .as_u64()
                    .is_none_or(|m| t.encode_utf16().count() as u64 <= m)
                && (s.get("pattern").is_none() || hash(v))
        }),
        Some("integer") => v
            .as_u64()
            .is_some_and(|n| n > 0 && n <= 9_007_199_254_740_991),
        Some("boolean") => v.is_boolean(),
        Some("null") => v.is_null(),
        Some("array") => v.as_array().is_some_and(|a| {
            s["minItems"].as_u64().is_none_or(|m| a.len() as u64 >= m)
                && a.iter().all(|v| grammar(v, &s["items"]))
        }),
        _ => false,
    }
}
fn result(e: Value) -> Value {
    json!({"isError":e["ok"]!=true,"content":[{"type":"text","text":e.to_string()}],"structuredContent":e})
}
fn fail(message: &str, code: &str) -> Value {
    result(json!({"ok":false,"errors":[message],"code":code}))
}
fn unknown() -> Value {
    fail(
        "Evidence write outcome unknown; read current-authorized exact disposition status and full history, original evidence and linked entry/head proofs before further action; never automatically retry",
        "outcome_unknown",
    )
}
fn transport(e: Error, write: bool) -> Value {
    match e {
        Error::Protocol => fail(
            "Protocol admission refused before dispatch; no evidence mutation sent",
            "protocol",
        ),
        Error::OutcomeUnknown if write => unknown(),
        Error::Transport if write => unknown(),
        Error::Refused { status: 404 } => fail(
            "disposition not found",
            "INTERCOMPANY_DISPOSITION_NOT_FOUND",
        ),
        other => fail(
            &other.to_string(),
            match other {
                Error::InvalidInput(_) => "invalid_input",
                Error::Refused { .. } => "server_refused",
                _ => "transport",
            },
        ),
    }
}
fn business(v: &Value) -> Option<Value> {
    if v["ok"] != false {
        return None;
    }
    let code = v["code"].as_str()?;
    let errors = v["errors"]
        .as_array()
        .filter(|a| !a.is_empty() && a.iter().all(Value::is_string))
        .cloned()
        .unwrap_or_else(|| vec![json!(code)]);
    Some(result(
        json!({"ok":false,"errors":errors,"code":if code=="disposition_not_found" {"INTERCOMPANY_DISPOSITION_NOT_FOUND"}else{code}}),
    ))
}
// Reuse the engine's ECMAScript canonical spelling and existing SHA primitive.
// These hashes only check source receipts; no outgoing keys/hashes are generated.
fn digest(v: &Value) -> Option<String> {
    let canonical = rentemester_engine::imported_receivables::canonical(v).ok()?;
    Some(rentemester_engine::sha256_bytes(canonical.as_bytes()))
}
fn normalized(v: &Value) -> Option<Value> {
    let d: Disposition = serde_json::from_value(v.clone()).ok()?;
    serde_json::to_value(d.normalize().ok()?).ok()
}
fn intent(input: &Value, actual: &Value) -> bool {
    let mut input = input.clone();
    // Server alone may allocate an omitted/empty ID; adopting a receipt is not
    // permission to synthesize an ID in the outgoing request.
    if input.get("dispositionId").is_none_or(|v| v == "") {
        input["dispositionId"] = actual["dispositionId"].clone();
    }
    normalized(&input).is_some_and(|d| d == *actual)
}
fn endpoint(d: &Value, company: &str) -> bool {
    d["left"]["companySlug"] == company || d["right"]["companySlug"] == company
}
#[derive(Deserialize)]
struct Event {
    event_type: String,
    actor: String,
    principal_kind: String,
    principal_id: String,
    payload_hash: String,
    canonical_payload: String,
    created_at: String,
}
fn events(v: &Value) -> Option<Vec<Value>> {
    let rows: Vec<Event> = serde_json::from_value(v.clone()).ok()?;
    rows.into_iter().map(|e| {
        if text(&e.actor,160).ok().as_deref()!=Some(&e.actor) || text(&e.principal_id,160).ok().as_deref()!=Some(&e.principal_id) || !matches!(e.principal_kind.as_str(),"user"|"service") || !hash(&json!(e.payload_hash)) || rentemester_engine::company_knowledge::iso(&e.created_at).ok().as_deref()!=Some(&e.created_at) { return None; }
        let p:Value=serde_json::from_str(&e.canonical_payload).ok()?;
        if !p.is_object() || digest(&p).as_deref()!=Some(&e.payload_hash) { return None; }
        Some(json!({"event_type":e.event_type,"actor":e.actor,"principal_kind":e.principal_kind,"principal_id":e.principal_id,"payload_hash":e.payload_hash,"canonical_payload":e.canonical_payload,"created_at":e.created_at}))
    }).collect()
}
#[derive(Deserialize)]
struct Link {
    side: String,
    company_slug: String,
    journal_entry_id: u64,
    journal_entry_no: String,
    journal_entry_hash: String,
    ledger_head_hash: Option<String>,
    linked_at: String,
    actor: String,
    principal_kind: String,
    principal_id: String,
}
fn state(v: &Value, company: &str, require_endpoint: bool) -> Option<Value> {
    let d = normalized(&v["disposition"])?;
    if d != v["disposition"] || (require_endpoint && !endpoint(&d, company)) || !hash(&v["payloadHash"]) {
        return None;
    }
    let ev = events(&v["events"])?;
    let life = events(&v["lifecycleEvents"])?;
    let first = ev.first()?;
    if first["event_type"] != "proposed"
        || first["payload_hash"] != v["payloadHash"]
        || serde_json::from_str::<Value>(first["canonical_payload"].as_str()?).ok()? != d
    {
        return None;
    }
    let mut approved = false;
    for e in ev.iter().skip(1) {
        let payload:Value=serde_json::from_str(e["canonical_payload"].as_str()?).ok()?;
        if e["event_type"]=="approved" {
            if approved || payload!=json!({"payloadHash":v["payloadHash"]}) || (e["principal_kind"]==first["principal_kind"] && e["principal_id"]==first["principal_id"]) {return None;}
            approved=true;
        } else if !matches!(e["event_type"].as_str(),Some("rejected"|"cancelled"|"settled"|"superseded"|"exception")) || payload.get("payloadHash").is_some_and(|h|h!=&v["payloadHash"]) {return None;}
    }
    for e in &life {
        if !matches!(
            e["event_type"].as_str(),
            Some("settled" | "superseded" | "reopened" | "exception" | "exception_resolved")
        ) || serde_json::from_str::<Value>(e["canonical_payload"].as_str()?).ok()?["payloadHash"]
            != v["payloadHash"]
        {
            return None;
        }
    }
    // Retain typed/hash-bound native receipt facts, including admitted historical events.
    // The server remains the authority for records, current signed ledgers and write admission.
    for e in life.iter().filter(|e| e["event_type"]=="settled") {
        let payload:Value=serde_json::from_str(e["canonical_payload"].as_str()?).ok()?;
        let ids:Vec<String>=serde_json::from_value(payload["settlementEvidenceRecordIds"].clone()).ok()?;
        if ids.is_empty() || ids.len()>64 || ids.iter().any(|id|text(id,160).ok().as_ref()!=Some(id)) {return None;}
        let mut canonical=ids.clone();canonical.sort_by(|a,b|a.encode_utf16().cmp(b.encode_utf16()));canonical.dedup();if canonical!=ids{return None;}
    }
    let links: Vec<Link> = serde_json::from_value(v["links"].clone()).ok()?;
    let mut out = vec![];
    let mut seen = std::collections::BTreeSet::new();
    for l in links {
        if !matches!(l.side.as_str(), "left" | "right")
            || !seen.insert(l.side.clone())
            || d[&l.side]["companySlug"] != l.company_slug
            || l.journal_entry_id == 0
            || l.journal_entry_id > 9_007_199_254_740_991
            || !hash(&json!(l.journal_entry_hash))
            || l.ledger_head_hash
                .as_ref()
                .is_some_and(|h| !hash(&json!(h)))
            || l.journal_entry_no.is_empty()
            || text(&l.actor, 160).is_err()
            || text(&l.principal_id, 160).is_err()
            || !matches!(l.principal_kind.as_str(), "user" | "service")
            || rentemester_engine::company_knowledge::iso(&l.linked_at).is_err()
        {
            return None;
        }
        if !ev.iter().chain(life.iter()).any(|e| {
            e["event_type"]=="exception" && serde_json::from_str::<Value>(e["canonical_payload"].as_str().unwrap()).ok().is_some_and(|p|p["kind"]=="one_sided_posting" && p["side"]==l.side)
        }) {return None;}
        out.push(json!({"side":l.side,"company_slug":l.company_slug,"journal_entry_id":l.journal_entry_id,"journal_entry_no":l.journal_entry_no,"journal_entry_hash":l.journal_entry_hash,"ledger_head_hash":l.ledger_head_hash,"linked_at":l.linked_at,"actor":l.actor,"principal_kind":l.principal_kind,"principal_id":l.principal_id}));
    }
    let mut status = if approved {
        "approved"
    } else {
        "proposed"
    };
    for e in &life {
        match e["event_type"].as_str()? {
            "settled" => status = "settled",
            "superseded" => status = "superseded",
            "reopened" => status = "approved",
            _ => {}
        }
    }
    if status == "approved" {
        status = match out.len() {
            1 => "partly_posted",
            2 => "posted",
            _ => "approved",
        };
    }
    if v["status"] != status {
        return None;
    }
    Some(
        json!({"disposition":d,"payloadHash":v["payloadHash"],"status":status,"events":ev,"lifecycleEvents":life,"links":out}),
    )
}
fn lifecycle_matches(data: &Value, kind: &str, args: &Value, keys: &[&str]) -> bool {
    data["lifecycleEvents"].as_array().is_some_and(|a| {
        a.iter()
            .rev()
            .find(|e| e["event_type"] == kind)
            .is_some_and(|e| {
                let Ok(p) = serde_json::from_str::<Value>(e["canonical_payload"].as_str().unwrap())
                else {
                    return false;
                };
                keys.iter().all(|k| {
                    if *k == "settlementEvidenceRecordIds" {
                        let mut ids: Vec<String> = args[*k]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| text(v.as_str().unwrap(), 160).unwrap())
                            .collect();
                        ids.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
                        ids.dedup();
                        p[*k] == json!(ids)
                    } else if *k == "reason" || *k == "replacementDispositionId" {
                        p[*k].as_str()
                            == text(
                                args[*k].as_str().unwrap(),
                                if *k == "reason" { 1000 } else { 80 },
                            )
                            .ok()
                            .as_deref()
                    } else {
                        p[*k] == args[*k]
                    }
                })
            })
    })
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let t = catalogue()
        .into_iter()
        .find(|t| t["name"] == name)
        .ok_or("unknown original disposition tool")?;
    if !grammar(&args, &t["inputSchema"]) {
        return Err("invalid original disposition arguments");
    }
    let op = op(name);
    let write = !matches!(op, "plan" | "status");
    let company = args["company"].as_str().unwrap();
    if !slug(company) {
        return Err("explicit bounded company slug required");
    }
    if write && args["confirm"] != true {
        return Ok(fail(
            &format!("confirm: true required for write tool {name}"),
            "CONFIRM_REQUIRED",
        ));
    }
    for k in ["dispositionId", "replacementDispositionId"] {
        if let Some(v) = args.get(k) {
            let s = match text(v.as_str().unwrap(),80){Ok(s)=>s,Err(_)=>return Ok(fail("bounded disposition ID required","invalid_input"))};
            if matches!(s.as_str(), "." | "..") {
                return Ok(fail("invalid disposition selector","invalid_input"));
            }
        }
    }
    if let Some(v) = args.get("reason") {
        if text(v.as_str().unwrap(),1000).is_err(){return Ok(fail("bounded reason required","invalid_input"));}
    }
    if args
        .get("asOf")
        .is_some_and(|v| rentemester_engine::fiscal_year::Date::parse(v.as_str().unwrap()).is_err())
    {
        return Ok(fail("native calendar asOf required","invalid_input"));
    }
    if args
        .get("ledgerHeadHash")
        .is_some_and(|v| !v.is_null() && !hash(v))
    {
        return Ok(fail("exact nullable ledger head required","invalid_input"));
    }
    if let Some(a) = args["settlementEvidenceRecordIds"].as_array() {
        if a.len() > 64 || a.iter().any(|v| text(v.as_str().unwrap(), 160).is_err()) {
            return Ok(fail("native settlement reference bounds exceeded","invalid_input"));
        }
    }
    if matches!(op, "plan" | "propose") {
        let mut check = args["disposition"].clone();
        if !check.is_object() {
            return Ok(fail("native disposition object required","invalid_input"));
        }
        if check.get("dispositionId").is_none_or(|v| v == "") {
            check["dispositionId"] = json!("validation-only");
        }
        if normalized(&check).is_none() {
            return Ok(fail("unsupported disposition: native typed calendar/money/text/source grammar required; null ID/date is not safely representable by native typed parser","invalid_input"));
        }
    }
    let mut body = args.clone();
    body.as_object_mut().unwrap().remove("company");
    let bytes = serde_json::to_vec(&body).map_err(|_| "serialization failed")?;
    if bytes.len() > rentemester_client::MAX_DISPOSITION_BODY {
        return Ok(fail("native32KiB disposition bound exceeded","invalid_input"));
    }
    // The global native status route has no company argument. Current company
    // catalogue admission preserves the original anchor, which need not be an endpoint.
    if op == "status" {
        match api.companies().await {
            Ok(v)
                if v["ok"] == true
                    && v["companies"].as_array().is_some_and(|a| {
                        a.iter()
                            .any(|c| c["slug"] == company && c["archived"] == false)
                    }) => {}
            Err(e) => return Ok(transport(e, false)),
            _ => {
                return Ok(fail(
                    "Company anchor is not currently admitted",
                    "access_denied",
                ));
            }
        }
    }
    let response = if op == "status" {
        api.group_disposition_status(
            args["dispositionId"].as_str().unwrap(),
            args["asOf"].as_str(),
        )
        .await
    } else {
        api.intercompany_disposition(company, action(op), bytes, write)
            .await
    };
    let v = match response {
        Ok(v) => v,
        Err(e) => return Ok(transport(e, write)),
    };
    if let Some(e) = business(&v) {
        return Ok(e);
    }
    let invalid = || {
        if write {
            unknown()
        } else {
            fail(
                "Malformed or mismatched canonical disposition receipt",
                "protocol",
            )
        }
    };
    if v["ok"] != true {
        return Ok(invalid());
    }
    let mut data = if op == "plan" {
        let Some(d) = normalized(&v["disposition"]) else {
            return Ok(invalid());
        };
        if d != v["disposition"]
            || !endpoint(&d, company)
            || !intent(&args["disposition"], &d)
            || digest(&d).as_deref() != v["payloadHash"].as_str()
            || v["mode"] != "dry-run"
            || v["ledgerEffects"] != json!([])
            || v["proposedSides"] != json!(["left", "right"])
            || v["warnings"]
                != json!([
                    "Each legal company must post separately through its normal policy, document, VAT and period controls."
                ])
        {
            return Ok(invalid());
        }
        json!({"ok":true,"mode":"dry-run","disposition":d,"payloadHash":v["payloadHash"],"ledgerEffects":[],"proposedSides":["left","right"],"warnings":v["warnings"]})
    } else {
        let Some(d) = state(&v, company, op != "status") else {
            return Ok(invalid());
        };
        d
    };
    if op == "propose"
        && (data["status"] != "proposed" || !intent(&args["disposition"], &data["disposition"]))
    {
        return Ok(invalid());
    }
    if !matches!(op, "plan" | "propose")
        && (data["disposition"]["dispositionId"].as_str()
            != text(args["dispositionId"].as_str().unwrap(), 80)
                .ok()
                .as_deref()
            || (write && data["payloadHash"] != args["payloadHash"]))
    {
        return Ok(invalid());
    }
    let bound = match op {
        "approve" => matches!(
            data["status"].as_str(),
            Some("approved" | "partly_posted" | "posted")
        ),
        "link" => {
            matches!(data["status"].as_str(), Some("partly_posted" | "posted"))
                && data["links"].as_array().unwrap().iter().any(|l| {
                    l["side"] == args["side"]
                        && l["journal_entry_id"] == args["journalEntryId"]
                        // Existing same-side/same-entry replay returns the immutable head;
                        // the server checks current links and the head precondition for NEW links.
                        // Do not claim a replay adopted this invocation's different/null head.
                })
        }
        "settle" => {
            // Already-settled replay returns existing canonical evidence without consuming
            // newly supplied IDs. A new settlement remains server-authorized and hash-bound.
            data["status"] == "settled"
        }
        "supersede" => {
            data["status"] == "superseded"
                && lifecycle_matches(
                    &data,
                    "superseded",
                    &args,
                    &["payloadHash", "replacementDispositionId", "reason"],
                )
        }
        "reopen" => {
            matches!(
                data["status"].as_str(),
                Some("approved" | "partly_posted" | "posted")
            ) && lifecycle_matches(&data, "reopened", &args, &["payloadHash", "reason"])
        }
        _ => true,
    };
    if !bound {
        return Ok(invalid());
    }
    if op == "status" {
        let Some(ex) = v["exceptions"].as_array() else {
            return Ok(invalid());
        };
        if v["scope"] != "intercompany-disposition-status"
            || v["balanced"] != json!(data["status"] == "posted" && ex.is_empty())
            || ex.iter().any(|e| {
                e["waivable"] != false
                    || match e["kind"].as_str() {
                        Some("stale_or_reversed_journal" | "one_sided_posting") => {
                            !matches!(e["side"].as_str(), Some("left" | "right"))
                        }
                        Some("overdue_settlement") => {
                            e["dueDate"] != data["disposition"]["settlementDueDate"]
                        }
                        _ => true,
                    }
            })
        {
            return Ok(invalid());
        }
        data["exceptions"] = v["exceptions"].clone();
        data["balanced"] = v["balanced"].clone();
        data["scope"] = v["scope"].clone();
    }
    Ok(result(json!({"ok":true,"errors":[],"data":data})))
}
