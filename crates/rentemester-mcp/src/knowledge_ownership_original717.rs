//! Original registry grammars over the existing authenticated SDK. No filesystem or actor inputs.
//! Native limits: knowledge 256KiB (value 32768 UTF-16/depth bounded), ownership
//! proposal 2MiB/2048 facts, other writes 32KiB. Current authority and retained
//! whole-intent replay checks remain server-owned; no automatic retry.
use rentemester_client::{ApiClient, KnowledgeMutation as K, OwnershipMutation as O};
use serde_json::{Value, json};
use rentemester_engine::{company_knowledge as kp, ownership as op};
const METADATA: &str = r###"[{"name":"company_knowledge_context","title":"Get source-backed company operating context","inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"asOf":{"type":"string","minLength":1},"includeProposed":{"type":"boolean"}},"required":["company"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}},{"name":"company_knowledge_propose","title":"Propose dated company operating knowledge","inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"predicate":{"type":"string","enum":["business_description","operating_status","products_services","revenue_models","markets","customer_types","industry_tags","operational_systems","document_sources","reporting_dimensions","responsible_party","canonical_reference"]},"value":{},"source":{"type":"object","properties":{"kind":{"type":"string","enum":["user","registry","party","corporate_record","external_snapshot","canonical_setting"]},"ref":{"type":"string","minLength":1}},"required":["kind","ref"],"additionalProperties":false},"validFrom":{"type":"string","minLength":1},"validToExclusive":{"type":"string"},"certainty":{"type":"string","enum":["confirmed","probable","uncertain"]},"assertionId":{"type":"string"},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company","predicate","value","source","validFrom"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}},{"name":"company_knowledge_review","title":"Review company knowledge assertion","inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"assertionId":{"type":"string","minLength":3},"decision":{"type":"string","enum":["approved","rejected"]},"reason":{"type":"string","maxLength":1000},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company","assertionId","decision"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}},{"name":"company_knowledge_supersede","title":"Supersede approved company knowledge","inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"assertionId":{"type":"string","minLength":3},"replacement":{"type":"object","properties":{"predicate":{"type":"string","enum":["business_description","operating_status","products_services","revenue_models","markets","customer_types","industry_tags","operational_systems","document_sources","reporting_dimensions","responsible_party","canonical_reference"]},"value":{},"source":{"type":"object","properties":{"kind":{"type":"string","enum":["user","registry","party","corporate_record","external_snapshot","canonical_setting"]},"ref":{"type":"string","minLength":1}},"required":["kind","ref"],"additionalProperties":false},"validFrom":{"type":"string","minLength":1},"validToExclusive":{"type":"string"},"certainty":{"type":"string","enum":["confirmed","probable","uncertain"]},"assertionId":{"type":"string"}},"required":["predicate","value","source","validFrom"],"additionalProperties":false},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company","assertionId","replacement"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}},{"name":"ownership_graph_query","title":"Query approved ownership and control facts","inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"asOf":{"type":"string","minLength":10}},"required":["company","asOf"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}},{"name":"ownership_snapshot_history","title":"Read redacted ownership snapshot history","inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"snapshotId":{"type":"string","minLength":3}},"required":["company"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}},{"name":"ownership_snapshot_propose","title":"Propose source-backed ownership snapshot","inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"snapshotId":{"type":"string","minLength":3},"source":{"type":"string","minLength":1},"observedAt":{"type":"string","minLength":10},"facts":{"minItems":1,"maxItems":2048,"type":"array","items":{"type":"object","properties":{"owner":{"oneOf":[{"type":"object","properties":{"kind":{"type":"string","const":"company"},"companySlug":{"type":"string","minLength":1}},"required":["kind","companySlug"],"additionalProperties":false},{"type":"object","properties":{"kind":{"type":"string","const":"party"},"partyId":{"type":"string","minLength":3}},"required":["kind","partyId"],"additionalProperties":false}]},"ownedCompanySlug":{"type":"string","minLength":1},"validFrom":{"type":"string","minLength":10},"validToExclusive":{"type":"string","minLength":10},"economicBasisPoints":{"type":"integer","minimum":0,"maximum":10000},"economicIntervalBasisPoints":{"type":"object","properties":{"min":{"type":"integer","minimum":0,"maximum":10000},"max":{"type":"integer","minimum":0,"maximum":10000}},"required":["min","max"],"additionalProperties":false},"votingBasisPoints":{"type":"integer","minimum":0,"maximum":10000},"controlType":{"type":"string","enum":["equity","voting","board","agreement","other"]},"shareClass":{"type":"string","minLength":1},"jurisdiction":{"type":"string","minLength":2,"maxLength":2},"evidenceRefs":{"minItems":1,"maxItems":32,"type":"array","items":{"type":"string","minLength":1}}},"required":["owner","ownedCompanySlug","validFrom","controlType","jurisdiction","evidenceRefs"],"additionalProperties":false}},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company","source","observedAt","facts"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}},{"name":"ownership_snapshot_review","title":"Review ownership snapshot","inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"snapshotId":{"type":"string","minLength":3},"decision":{"type":"string","enum":["approved","rejected"]},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company","snapshotId","decision"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}},{"name":"ownership_snapshot_apply","title":"Apply exact approved ownership diff","inputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"company":{"type":"string","minLength":1},"snapshotId":{"type":"string","minLength":3},"snapshotHash":{"type":"string","pattern":"^[a-f0-9]{64}$"},"diffHash":{"type":"string","pattern":"^[a-f0-9]{64}$"},"confirm":{"description":"Must be set to true to acknowledge the write side effects of this tool. Omitting it (or sending false) returns an { ok:false, errors:[...] } envelope rather than performing the write.","type":"boolean"}},"required":["company","snapshotId","snapshotHash","diffHash"],"additionalProperties":false},"outputSchema":{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true \u21d2 kaldet lykkedes og `data` er sat; false \u21d2 se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den pr\u00e6cise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskel\u00e6sbare fejl-/foruds\u00e6tnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). S\u00e6ttes for bogf\u00f8rings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinl\u00e6sbar fejl-mark\u00f8r for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). S\u00e6ttes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik \u2014 agenten kan branche p\u00e5 `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}}]"###;
fn metadata() -> Vec<Value> { serde_json::from_str(METADATA).expect("frozen original schemas") }
pub fn accepts(name: &str) -> bool { metadata().iter().any(|t| t["name"] == name) }
fn read(name: &str) -> bool { matches!(name,"company_knowledge_context"|"ownership_graph_query"|"ownership_snapshot_history") }
pub fn tools() -> Vec<Value> {
    metadata().into_iter().map(|mut t| {
        t["description"] = json!("Original flat registry inputs. Current authenticated authority only; no ledger/legal/group-manifest effects. Writes require outer confirm:true. Native bounds: knowledge256KiB/value32768 UTF-16/depth64; ownership proposal2MiB/2048 facts, review/apply32KiB. Native whole-intent/current-principal replay is stronger than legacy dedup. Native ISO dates only (no locale-dependent dates). No retries; unknown writes require canonical exact-source/context/history readback before further explicit action.");
        t
    }).collect()
}
fn valid(v: &Value, s: &Value) -> bool {
    if s.get("pattern").is_some() && !hash(v) { return false; }
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
    if let Some(t) = s.get("type") {
        let test = |t: &str| match t {
            "object" => v.is_object(),
            "array" => v.is_array(),
            "string" => v.is_string(),
            "integer" => v.is_i64() || v.is_u64(),
            "boolean" => v.is_boolean(),
            "null" => v.is_null(),
            _ => false,
        };
        if !t.as_str().map(test).unwrap_or_else(|| {
            t.as_array()
                .is_some_and(|a| a.iter().any(|t| t.as_str().is_some_and(test)))
        }) {
            return false;
        }
    }
    if let Some(choices) = s["oneOf"].as_array() {
        if choices.iter().filter(|s| valid(v, s)).count() != 1 {
            return false;
        }
    }
    if let Some(o) = v.as_object() {
        if s["required"]
            .as_array()
            .is_some_and(|a| a.iter().any(|k| !o.contains_key(k.as_str().unwrap())))
        {
            return false;
        }
        for (k, v) in o {
            if let Some(p) = s["properties"].get(k) {
                if !valid(v, p) {
                    return false;
                }
            } else if s["additionalProperties"] == false {
                return false;
            }
        }
    }
    if let Some(a) = v.as_array() {
        if a.len() < s["minItems"].as_u64().unwrap_or(0) as usize
            || a.len() > s["maxItems"].as_u64().unwrap_or(u64::MAX) as usize
        {
            return false;
        }
        if let Some(items) = s.get("items") {
            if !a.iter().all(|v| valid(v, items)) {
                return false;
            }
        }
    }
    if let Some(t) = v.as_str() {
        let n = t.encode_utf16().count() as u64;
        if n < s["minLength"].as_u64().unwrap_or(0)
            || n > s["maxLength"].as_u64().unwrap_or(u64::MAX)
        {
            return false;
        }
    }
    if let Some(n) = v.as_f64() {
        if s["minimum"].as_f64().is_some_and(|b| n < b)
            || s["maximum"].as_f64().is_some_and(|b| n > b)
        {
            return false;
        }
    }
    true
}
fn hash(v: &Value) -> bool { v.as_str().is_some_and(|s| s.len()==64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))) }
fn text(v: &Value) -> bool { v.as_str().is_some_and(|s| kp::text(s,512).is_ok()) }
fn iso(v: &Value) -> bool { v.as_str().is_some_and(|s| kp::iso(s).ok().as_deref()==Some(s)) }
fn principal(v: &Value) -> bool { matches!(v["kind"].as_str(),Some("user"|"service_account"|"local_operator")) && text(&v["id"]) }
fn events(v: &Value, state: &Value, knowledge: bool) -> bool {
    v.as_array().is_some_and(|a| !a.is_empty() && a.iter().all(|e| {
        matches!(e["event_type"].as_str(),Some("proposed"|"approved"|"rejected"|"applied"|"superseded"))
        && text(&e["actor"]) && iso(&e["created_at"])
        && principal(&json!({"kind":e["principal_kind"],"id":e["principal_id"]}))
        && (!knowledge || (e.get("reason").is_some_and(|v| v.is_null() || v.is_string()) && e.get("supersedes_assertion_id").is_some_and(|v| v.is_null() || v.is_string())))
    }) && a.last().is_some_and(|e| e["event_type"] == *state || knowledge && *state=="conflict" && e["event_type"]=="approved"))
}
fn assertion(v: &Value, company: &str) -> bool {
    v["companySlug"]==company && text(&v["assertionId"]) && v.get("value").is_some()
    && kp::PREDICATES.contains(&v["predicate"].as_str().unwrap_or("")) && hash(&v["valueHash"])
    && kp::SOURCES.contains(&v["source"]["kind"].as_str().unwrap_or("")) && text(&v["source"]["ref"])
    && iso(&v["validFrom"]) && v.get("validToExclusive").is_some_and(|v| v.is_null() || iso(v))
    && matches!(v["certainty"].as_str(),Some("confirmed"|"probable"|"uncertain")) && text(&v["actor"])
    && principal(&v["principal"]) && matches!(v["reviewState"].as_str(),Some("proposed"|"approved"|"rejected"|"superseded"|"conflict"))
    && events(&v["history"],&v["reviewState"],true)
}
fn canonical_fact(v: &Value) -> bool {
    serde_json::from_value::<op::Fact>(v.clone()).ok().is_some_and(|f|f.clone().normalize().ok()==Some(f))
}
fn snapshot(v: &Value) -> bool {
    ["snapshotId","source","actor"].iter().all(|k| text(&v[*k])) && iso(&v["observedAt"])
    && hash(&v["snapshotHash"]) && hash(&v["diffHash"]) && principal(&v["principal"])
    && matches!(v["state"].as_str(),Some("proposed"|"approved"|"rejected"|"applied"))
    && v["diff"]["version"]==2 && v["diff"]["unchanged"].as_array().is_some_and(|a|a.iter().all(hash))
    && ["add","conflicts"].iter().all(|k| v["diff"][*k].as_array().is_some_and(|a|a.iter().all(canonical_fact)))
    && v["diff"].is_object() && ["add","endings","conflicts"].iter().all(|k| v["diff"][*k].is_array())
    && events(&v["history"],&v["state"],false)
}
fn result(envelope: Value) -> Value { json!({"isError":envelope["ok"]!=true,"content":[{"type":"text","text":envelope.to_string()}],"structuredContent":envelope}) }
fn fail(message: &str) -> Value { result(json!({"ok":false,"errors":[message]})) }
fn canonical(s: &Value, max: usize) -> Option<String> { kp::text(s.as_str()?,max).ok() }
fn same_id(receipt: &Value, supplied: &Value, key: &str) -> bool {
    supplied.get(key).is_none_or(|id| canonical(id,80).as_deref()==receipt[key].as_str())
}
fn proposal_matches(receipt: &Value, input: &Value) -> bool {
    same_id(receipt,input,"assertionId") && receipt["predicate"]==input["predicate"] && receipt["value"]==input["value"]
    && receipt["source"]["kind"]==input["source"]["kind"] && receipt["source"]["ref"].as_str()==canonical(&input["source"]["ref"],512).as_deref()
    && receipt["validFrom"].as_str()==input["validFrom"].as_str().and_then(|s| kp::iso(s).ok()).as_deref()
    && receipt["validToExclusive"] == input.get("validToExclusive").and_then(Value::as_str).filter(|s|!s.is_empty()).and_then(|s|kp::iso(s).ok()).map(Value::String).unwrap_or(Value::Null)
    && receipt["certainty"]==input.get("certainty").cloned().unwrap_or(json!("confirmed"))
}
fn supported_proposal(v: &Value) -> bool {
    v["validFrom"].as_str().is_some_and(|s| kp::iso(s).is_ok())
    && v.get("validToExclusive").is_none_or(|v|v.as_str().is_some_and(|s|s.is_empty() || kp::iso(s).is_ok()))
    && v.get("assertionId").is_none_or(|v|canonical(v,80).is_some()) && canonical(&v["source"]["ref"],512).is_some()
}
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value,&'static str> {
    let meta=metadata().into_iter().find(|t|t["name"]==name).ok_or("unknown registry tool")?;
    if !valid(&args,&meta["inputSchema"]) { return Err("invalid original registry arguments"); }
    let company=args["company"].as_str().ok_or("explicit company required")?;
    // Original grammar is retained. Deliberately narrower native admission is a business refusal.
    if kp::text(company,120).is_err() { return Ok(fail("Native bounded company selector required")); }
    if !read(name) && args["confirm"]!=true {
        return Ok(result(json!({"ok":false,"errors":[format!("confirm: true required for write tool {name}")],"code":"CONFIRM_REQUIRED"})));
    }
    let action=name.rsplit('_').next().unwrap_or("");
    let knowledge=name.starts_with("company_knowledge_");
    if name=="ownership_graph_query" && args["asOf"].as_str().is_none_or(|s|op::date(s).is_err())
        || name=="company_knowledge_context" && args.get("asOf").is_some_and(|v|v.as_str().is_none_or(|s|kp::iso(s).is_err()))
        || knowledge && action=="propose" && !supported_proposal(&args)
        || knowledge && action=="supersede" && !supported_proposal(&args["replacement"])
    { return Ok(fail("Native ISO/canonical text admission refused; locale-dependent source dates cannot be projected")); }
    if name=="ownership_snapshot_propose" {
        if args["observedAt"].as_str().is_none_or(|s|kp::iso(s).is_err()) || args["facts"].as_array().is_none_or(|a|a.iter().any(|v|serde_json::from_value::<op::Fact>(v.clone()).ok().and_then(|f|f.normalize().ok()).is_none())) {
            return Ok(fail("Native ownership fact/date admission refused; exactly one economic value or interval and canonical bounded facts required"));
        }
    }
    let mut body=args.clone(); body.as_object_mut().unwrap().remove("company");
    let bytes=serde_json::to_vec(&body).map_err(|_|"invalid registry JSON")?;
    let response=match name {
        "company_knowledge_context"=>api.company_knowledge(company,args["asOf"].as_str(),args["includeProposed"].as_bool().unwrap_or(false)).await,
        "ownership_graph_query"=>api.ownership(company,args["asOf"].as_str().unwrap()).await,
        "ownership_snapshot_history"=>api.ownership_history(company,args["snapshotId"].as_str(),None).await,
        _ if knowledge=>api.mutate_company_knowledge(company,match action {"propose"=>K::Propose,"review"=>K::Review,_=>K::Supersede},bytes,true).await,
        _=>api.mutate_ownership(company,match action {"propose"=>O::Propose,"review"=>O::Review,_=>O::Apply},bytes,true).await,
    };
    let mut v=match response {
        Ok(v)=>v,
        Err(rentemester_client::Error::InvalidInput(_))=>return Ok(fail("Native registry input/body bound refused; knowledge256KiB, ownership proposal2MiB, other ownership writes32KiB; no retry performed")),
        Err(_)=>return Ok(fail(if read(name) {"Registry transport, authorization or protocol refused"} else {"Registry write not established; outcome may be unknown. Reconcile canonical exact-source context/snapshot/history before further explicit action; no retry performed"})),
    };
    let valid_receipt=v["ok"]==true && match name {
        "company_knowledge_context"=>{
            let c=&v["context"];
            c["companySlug"]==company && iso(&c["asOf"]) && args.get("asOf").is_none_or(|a|a.as_str().and_then(|s|kp::iso(s).ok()).as_deref()==c["asOf"].as_str())
            && c["assertions"].as_array().is_some_and(|a|a.iter().all(|v|assertion(v,company)))
            && c["conflicts"].as_array().is_some_and(|a|a.iter().all(Value::is_string)) && c["safeForProductBehavior"].is_boolean()
            && v["history"].as_array().is_some_and(|a|a.iter().all(|v|assertion(v,company)))
        },
        "company_knowledge_propose"=>assertion(&v["assertion"],company) && proposal_matches(&v["assertion"],&args),
        "company_knowledge_review"=>assertion(&v["assertion"],company) && same_id(&v["assertion"],&args,"assertionId") && v["assertion"]["reviewState"]==args["decision"],
        "company_knowledge_supersede"=>assertion(&v["superseded"],company) && assertion(&v["replacement"],company) && same_id(&v["superseded"],&args,"assertionId") && v["superseded"]["reviewState"]=="superseded" && proposal_matches(&v["replacement"],&args["replacement"]),
        "ownership_graph_query"=>v["asOf"]==args["asOf"] && v["partial"].is_boolean() && v["consolidation"]["eligible"]==false && v["consolidation"]["reason"].is_string() && v["facts"].as_array().is_some_and(|a| a.iter().all(|v|serde_json::from_value::<op::Fact>(v.clone()).ok().is_some_and(|f|f.clone().normalize().ok()==Some(f)))),
        "ownership_snapshot_history"=>v["history"].as_array().is_some_and(|a|a.iter().all(|v|snapshot(v) && same_id(v,&args,"snapshotId"))),
        _=> snapshot(&v["snapshot"]) && same_id(&v["snapshot"],&args,"snapshotId") && match action {
            "propose"=>v["snapshot"]["source"].as_str()==canonical(&args["source"],160).as_deref() && v["snapshot"]["observedAt"].as_str()==args["observedAt"].as_str().and_then(|s|kp::iso(s).ok()).as_deref(),
            "review"=>v["snapshot"]["state"]==args["decision"],
            _=>v["snapshot"]["snapshotHash"]==args["snapshotHash"] && v["snapshot"]["diffHash"]==args["diffHash"] && v["snapshot"]["state"]=="applied" && matches!(v["status"].as_str(),Some("applied"|"unchanged")),
        },
    };
    if !valid_receipt { return Ok(fail(if read(name) {"Registry observation unavailable: malformed or mismatched native receipt"} else {"Registry write outcome unknown: malformed or mismatched native receipt; reconcile canonical exact-source context/snapshot/history; no retry performed"})); }
    if name=="ownership_graph_query" {
        let facts=v["facts"].as_array_mut().unwrap(); let n=facts.len();
        facts.retain(|f| f["ownedCompanySlug"]==company && (f["owner"]["kind"]!="company" || f["owner"]["companySlug"]==company));
        if facts.len()!=n {v["partial"]=json!(true);v["consolidation"]=json!({"eligible":false,"reason":"hidden endpoints"});}
    }
    // Select actual source producer fields, never blindly wrap a native ok/data envelope.
    let keys: &[&str]=match name {
        "company_knowledge_context"=>&["context","history"],
        "company_knowledge_propose"|"company_knowledge_review"=>&["assertion"],
        "company_knowledge_supersede"=>&["superseded","replacement"],
        "ownership_graph_query"=>&["asOf","facts","partial","consolidation"],
        "ownership_snapshot_history"=>&["history"],
        "ownership_snapshot_apply"=>&["status","snapshot"], _=>&["snapshot"],
    };
    let mut data=serde_json::Map::new(); for key in keys {data.insert((*key).into(),v[*key].clone());}
    Ok(result(json!({"ok":true,"errors":[],"data":data})))
}
