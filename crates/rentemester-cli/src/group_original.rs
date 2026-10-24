//! Original group CLI over the existing authenticated common engine. Never writes legal ledgers.
use crate::dated_reports::Output;
use rentemester_client::{
    ApiClient, ConsolidationMutation as CM, DispositionAction as Action, EliminationMutation as EM,
    IntercompanyMutation as IM,
};
use rentemester_engine::consolidation::Profile;
use rentemester_engine::intercompany::{Manifest, Mapping};
use rentemester_engine::intercompany_dispositions::{Disposition, text};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
};
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
    let rows: Vec<Event> = v
        .as_array()?
        .iter()
        .map(|v| {
            Some(Event {
                event_type: v["event_type"].as_str()?.into(),
                actor: v["actor"].as_str()?.into(),
                principal_kind: v["principal_kind"].as_str()?.into(),
                principal_id: v["principal_id"].as_str()?.into(),
                payload_hash: v["payload_hash"].as_str()?.into(),
                canonical_payload: v["canonical_payload"].as_str()?.into(),
                created_at: v["created_at"].as_str()?.into(),
            })
        })
        .collect::<Option<_>>()?;
    rows.into_iter().map(|e| {
        if text(&e.actor,160).ok().as_deref()!=Some(&e.actor) || text(&e.principal_id,160).ok().as_deref()!=Some(&e.principal_id) || !matches!(e.principal_kind.as_str(),"user"|"service") || !hash(&json!(e.payload_hash)) || rentemester_engine::company_knowledge::iso(&e.created_at).ok().as_deref()!=Some(&e.created_at) { return None; }
        let p:Value=serde_json::from_str(&e.canonical_payload).ok()?;
        if !p.is_object() || digest(&p).as_deref()!=Some(&e.payload_hash) { return None; }
        Some(json!({"event_type":e.event_type,"actor":e.actor,"principal_kind":e.principal_kind,"principal_id":e.principal_id,"payload_hash":e.payload_hash,"canonical_payload":e.canonical_payload,"created_at":e.created_at}))
    }).collect()
}
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
    if d != v["disposition"]
        || (require_endpoint && !endpoint(&d, company))
        || !hash(&v["payloadHash"])
    {
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
        let payload: Value = serde_json::from_str(e["canonical_payload"].as_str()?).ok()?;
        if e["event_type"] == "approved" {
            if approved
                || payload != json!({"payloadHash":v["payloadHash"]})
                || (e["principal_kind"] == first["principal_kind"]
                    && e["principal_id"] == first["principal_id"])
            {
                return None;
            }
            approved = true;
        } else if !matches!(
            e["event_type"].as_str(),
            Some("rejected" | "cancelled" | "settled" | "superseded" | "exception")
        ) || payload
            .get("payloadHash")
            .is_some_and(|h| h != &v["payloadHash"])
        {
            return None;
        }
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
    for e in life.iter().filter(|e| e["event_type"] == "settled") {
        let payload: Value = serde_json::from_str(e["canonical_payload"].as_str()?).ok()?;
        let ids: Vec<String> =
            serde_json::from_value(payload["settlementEvidenceRecordIds"].clone()).ok()?;
        if ids.is_empty()
            || ids.len() > 64
            || ids.iter().any(|id| text(id, 160).ok().as_ref() != Some(id))
        {
            return None;
        }
        let mut canonical = ids.clone();
        canonical.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
        canonical.dedup();
        if canonical != ids {
            return None;
        }
    }
    let links: Vec<Link> = v["links"]
        .as_array()?
        .iter()
        .map(|v| {
            Some(Link {
                side: v["side"].as_str()?.into(),
                company_slug: v["company_slug"].as_str()?.into(),
                journal_entry_id: v["journal_entry_id"].as_u64()?,
                journal_entry_no: v["journal_entry_no"].as_str()?.into(),
                journal_entry_hash: v["journal_entry_hash"].as_str()?.into(),
                ledger_head_hash: if v["ledger_head_hash"].is_null() {
                    None
                } else {
                    Some(v["ledger_head_hash"].as_str()?.into())
                },
                linked_at: v["linked_at"].as_str()?.into(),
                actor: v["actor"].as_str()?.into(),
                principal_kind: v["principal_kind"].as_str()?.into(),
                principal_id: v["principal_id"].as_str()?.into(),
            })
        })
        .collect::<Option<_>>()?;
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
            e["event_type"] == "exception"
                && serde_json::from_str::<Value>(e["canonical_payload"].as_str().unwrap())
                    .ok()
                    .is_some_and(|p| p["kind"] == "one_sided_posting" && p["side"] == l.side)
        }) {
            return None;
        }
        out.push(json!({"side":l.side,"company_slug":l.company_slug,"journal_entry_id":l.journal_entry_id,"journal_entry_no":l.journal_entry_no,"journal_entry_hash":l.journal_entry_hash,"ledger_head_hash":l.ledger_head_hash,"linked_at":l.linked_at,"actor":l.actor,"principal_kind":l.principal_kind,"principal_id":l.principal_id}));
    }
    let mut status = if approved { "approved" } else { "proposed" };
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

const CATALOGUE: &str = r###"[{"key": "group plan-disposition", "usage": "group plan-disposition --workspace <dir> --disposition <file>", "description": "Validerer den tosidige disposition som dry-run; hverken workspacet eller en ledger ændres.", "allowedFlags": ["--workspace", "--disposition"]}, {"key": "group propose-disposition", "usage": "group propose-disposition --workspace <dir> --disposition <file> --confirm yes --actor <id>", "description": "Foreslår en fælles mellemregningsdisposition med kildebevis; den bogfører aldrig i nogen selskabsledger.", "allowedFlags": ["--workspace", "--disposition", "--confirm"]}, {"key": "group approve-disposition", "usage": "group approve-disposition --workspace <dir> --disposition-id <id> --payload-hash <sha256> --confirm yes --actor <id>", "description": "Godkender den eksakte disposition med en særskilt stabil principal; ingen ledger ændres.", "allowedFlags": ["--workspace", "--disposition-id", "--payload-hash", "--confirm"]}, {"key": "group link-disposition", "usage": "group link-disposition --workspace <dir> --disposition-id <id> --payload-hash <sha256> --side <left|right> --journal-entry-id <id> --ledger-head-hash <sha256> --confirm yes --actor <id>", "description": "Linker én allerede bogført journal efter ny kildevalidering; kommandoen bogfører aldrig.", "allowedFlags": ["--workspace", "--disposition-id", "--payload-hash", "--side", "--journal-entry-id", "--ledger-head-hash", "--confirm"]}, {"key": "group disposition-status", "usage": "group disposition-status --workspace <dir> --disposition-id <id> [--as-of YYYY-MM-DD]", "description": "Viser append-only status og sikre ensidige, forældede eller forfaldne undtagelser uden mutation.", "allowedFlags": ["--workspace", "--disposition-id", "--as-of"]}, {"key": "group settle-disposition", "usage": "group settle-disposition --workspace <dir> --disposition-id <id> --payload-hash <sha256> --settlement-evidence-json <json> --confirm yes --actor <id>", "description": "Afslutter en allerede tosidigt bogført disposition med kildebevis; den ændrer aldrig en ledger.", "allowedFlags": ["--workspace", "--disposition-id", "--payload-hash", "--settlement-evidence-json", "--confirm"]}, {"key": "group reopen-disposition", "usage": "group reopen-disposition --workspace <dir> --disposition-id <id> --payload-hash <sha256> --reason <text> --confirm yes --actor <id>", "description": "Genåbner append-only efter aktuel journal- og ledger-head-kontrol; den ændrer aldrig en ledger.", "allowedFlags": ["--workspace", "--disposition-id", "--payload-hash", "--reason", "--confirm"]}, {"key": "group supersede-disposition", "usage": "group supersede-disposition --workspace <dir> --disposition-id <id> --payload-hash <sha256> --replacement-disposition-id <id> --reason <text> --confirm yes --actor <id>", "description": "Supersederer append-only en aktiv disposition til fordel for en eksisterende erstatning; den ændrer aldrig en ledger.", "allowedFlags": ["--workspace", "--disposition-id", "--payload-hash", "--replacement-disposition-id", "--reason", "--confirm"]}, {"key": "group validate-manifest", "usage": "group validate-manifest --workspace <dir> --manifest <file>", "description": "Validerer koncernstruktur uden at ændre noget.", "allowedFlags": ["--workspace", "--manifest"]}, {"key": "group apply-manifest", "usage": "group apply-manifest --workspace <dir> --manifest <file> --policy-company <slug> --confirm yes --actor <id>", "description": "Gemmer en append-only, effektivt dateret koncernstruktur uden at røre selskabernes hovedbøger.", "allowedFlags": ["--workspace", "--manifest", "--policy-company", "--confirm"]}, {"key": "group overview", "usage": "group overview --workspace <dir> --as-of <YYYY-MM-DD>", "description": "Viser kun datoafgrænset koncernstruktur og parathed; ingen konsoliderede tal.", "allowedFlags": ["--workspace", "--as-of"]}, {"key": "group validate-mapping", "usage": "group validate-mapping --workspace <dir> --mapping <file>", "description": "Validerer en eksplicit, effektivt dateret mellemregningsmapping uden skrivning.", "allowedFlags": ["--workspace", "--mapping"]}, {"key": "group propose-mapping", "usage": "group propose-mapping --workspace <dir> --mapping <file> --confirm yes --actor <id>", "description": "Foreslår en append-only mellemregningsmapping; forslaget er inert indtil særskilt review.", "allowedFlags": ["--workspace", "--mapping", "--confirm"]}, {"key": "group approve-mapping", "usage": "group approve-mapping --workspace <dir> --mapping-id <id> --mapping-hash <sha256> --confirm yes --actor <id>", "description": "Godkender den præcise foreslåede mapping med en anden reviewer-actor.", "allowedFlags": ["--workspace", "--mapping-id", "--mapping-hash", "--confirm"]}, {"key": "group revoke-mapping", "usage": "group revoke-mapping --workspace <dir> --mapping-id <id> --confirm yes --actor <id>", "description": "Tilbagekalder en godkendt mapping append-only; historikken bevares.", "allowedFlags": ["--workspace", "--mapping-id", "--confirm"]}, {"key": "group reconcile", "usage": "group reconcile --workspace <dir> --as-of <YYYY-MM-DD>", "description": "Afstemmer godkendte mellemregninger read-only, eksakt og kun i samme funktionsvaluta.", "allowedFlags": ["--workspace", "--as-of"]}, {"key": "group propose-elimination", "usage": "group propose-elimination --workspace <dir> --elimination <file> --confirm yes --actor <id>", "description": "Foreslår en balanceeliminering afledt af en eksakt afstemt mellemregning; ingen fri beløbsindtastning.", "allowedFlags": ["--workspace", "--elimination", "--confirm"]}, {"key": "group approve-elimination", "usage": "group approve-elimination --workspace <dir> --elimination-id <id> --payload-hash <sha256> --confirm yes --actor <id>", "description": "Godkender den præcise elimination med en anden reviewer og genverificerer kildeevidens.", "allowedFlags": ["--workspace", "--elimination-id", "--payload-hash", "--confirm"]}, {"key": "group reject-elimination", "usage": "group reject-elimination --workspace <dir> --elimination-id <id> --payload-hash <sha256> --confirm yes --actor <id>", "description": "Afviser et foreslået eliminationsevent append-only.", "allowedFlags": ["--workspace", "--elimination-id", "--payload-hash", "--confirm"]}, {"key": "group apply-elimination", "usage": "group apply-elimination --workspace <dir> --elimination-id <id> --payload-hash <sha256> --confirm yes --actor <id>", "description": "Anvender en godkendt elimination kun i workspace-konsolideringslaget efter ny kildekontrol.", "allowedFlags": ["--workspace", "--elimination-id", "--payload-hash", "--confirm"]}, {"key": "group reverse-elimination", "usage": "group reverse-elimination --workspace <dir> --elimination-id <id> --payload-hash <sha256> --confirm yes --actor <id>", "description": "Tilbagefører en anvendt elimination append-only uden at ændre selskabsledgers.", "allowedFlags": ["--workspace", "--elimination-id", "--payload-hash", "--confirm"]}, {"key": "group eliminations", "usage": "group eliminations --workspace <dir> --as-of <YYYY-MM-DD>", "description": "Viser anvendte balanceelimineringer for den præcise rapportdato.", "allowedFlags": ["--workspace", "--as-of"]}, {"key": "group validate-profile", "usage": "group validate-profile --workspace <dir> --profile <file>", "description": "Validerer et generisk rapporteringskontoskema uden skrivning eller selskabsspecifikke defaults.", "allowedFlags": ["--workspace", "--profile"]}, {"key": "group propose-profile", "usage": "group propose-profile --workspace <dir> --profile <file> --confirm yes --actor <id>", "description": "Foreslår en effektivt dateret, append-only konsolideringsprofil.", "allowedFlags": ["--workspace", "--profile", "--confirm"]}, {"key": "group approve-profile", "usage": "group approve-profile --workspace <dir> --profile-id <id> --profile-hash <sha256> --confirm yes --actor <id>", "description": "Godkender den præcise rapporteringsprofil med en anden reviewer.", "allowedFlags": ["--workspace", "--profile-id", "--profile-hash", "--confirm"]}, {"key": "group revoke-profile", "usage": "group revoke-profile --workspace <dir> --profile-id <id> --confirm yes --actor <id>", "description": "Tilbagekalder en godkendt rapporteringsprofil append-only.", "allowedFlags": ["--workspace", "--profile-id", "--confirm"]}, {"key": "group consolidated-report", "usage": "group consolidated-report --workspace <dir> --profile-id <id> --from <YYYY-MM-DD> --as-of <YYYY-MM-DD>", "description": "Bygger et traceable read-only resultat og balance; returnerer ingen konsoliderede tal ved blockers.", "allowedFlags": ["--workspace", "--profile-id", "--from", "--as-of"]}]"###;
fn catalogue() -> Vec<Value> {
    serde_json::from_str(CATALOGUE).expect("source catalogue")
}
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "group")
}
pub fn help() -> Value {
    json!({"commands":catalogue(),"connection":"Explicit selected --company SLUG (or verified local PATH), --server ORIGIN OR --local-profile ABS_PRIVATE_PROFILE, --cookie-file PRIVATE_HUMAN_COOKIE. --workspace only exact verified local workspace equality. --json or --format human|json.","nativeProfile":"Human current credentials, all-reference company rights, backup fencing, independent review and source freshness remain server-owned. Optional --actor user:ID asserts current cookie equality only. Endpoint anchor required for disposition mutations/plan. No ledger, payment or provider mutations, generated IDs/hashes/heads, retry or local DB fallback. Native bounds apply: dispositions32KiB, manifest/mapping/elimination256KiB, profile1MiB (lifecycle32KiB).","eliminationProposal":"Native expectedPayloadHash is required explicitly in --elimination file, alongside id,mappingId,asOf,evidenceRefs. No automatic plan/hash manufacture. revoke-mapping reads exact current approved mappingHash before one POST.","missingCapability":"overview: SDK/current HTTP has no source structure overview with ledger availability facts. Current intercompany manifest alone cannot prove readiness; this operation refuses rather than fabricating readiness.","recovery":"Known passive protocol refusal before dispatch has no effect. Lost/malformed/mismatched successful mutation is outcome_unknown, never retry; read exact current-authorized disposition status/history, intercompany mapping/manifest, elimination state or consolidation profiles and original source evidence before further explicit action. Conditional link/settle replay retains stored heads/evidence, never claims new refs adopted."})
}
fn usage(s: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{s}; see group --help\n"),
        exit: 2,
    }
}
pub fn binding_refusal(_args: &[String]) -> Output {
    usage("Explicit verified company and transport binding required")
}
fn write(op: &str) -> bool {
    op.starts_with("propose-")
        || op.starts_with("approve-")
        || op.starts_with("revoke-")
        || op.starts_with("reject-")
        || op.starts_with("apply-")
        || op.starts_with("reverse-")
        || op.starts_with("link-")
        || op.starts_with("settle-")
        || op.starts_with("reopen-")
        || op.starts_with("supersede-")
}
fn parse(args: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    if args == ["group"] || args == ["group", "--help"] {
        return Ok(BTreeMap::from([("--help", "true")]));
    }
    let op = args.get(1).ok_or_else(|| usage("Missing group command"))?;
    let row = catalogue()
        .into_iter()
        .find(|r| r["key"] == format!("group {op}"))
        .ok_or_else(|| usage("Unknown group command"))?;
    let mut f = BTreeMap::new();
    let mut i = 2;
    while i < args.len() {
        let (k, a) = crate::original_args::option(&args[i]);
        let allowed = matches!(
            k,
            "--company"
                | "--server"
                | "--cookie-file"
                | "--local-profile"
                | "--allow-loopback-http"
                | "--json"
                | "--format"
                | "--help"
        ) || write(op) && k == "--actor"
            || row["allowedFlags"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == k);
        if !allowed {
            return Err(usage("Unknown or unsupported group flag"));
        }
        let (v, next) = if matches!(k, "--json" | "--help" | "--allow-loopback-http") {
            if a.is_some() {
                return Err(usage("Boolean flag takes no value"));
            }
            ("true", i + 1)
        } else {
            crate::original_args::value(args, i)
                .filter(|(v, _)| !v.is_empty())
                .ok_or_else(|| usage("Missing flag value"))?
        };
        if f.insert(k, v).is_some() {
            return Err(usage("Duplicate flag"));
        }
        i = next;
    }
    if f.get("--format")
        .is_some_and(|v| !matches!(*v, "json" | "human"))
    {
        return Err(usage("Invalid output format"));
    }
    let local = f.contains_key("--local-profile");
    if local && (f.contains_key("--server") || f.contains_key("--allow-loopback-http"))
        || !local && f.contains_key("--workspace")
    {
        return Err(usage("Conflicting transport or workspace assertion"));
    }
    for k in ["--workspace", "--local-profile"] {
        if f.get(k).is_some_and(|v| {
            !std::path::Path::new(v).is_absolute() || v.chars().any(char::is_control)
        }) {
            return Err(usage("Absolute local assertion required"));
        }
    }
    if let Some(s) = f.get("--server") {
        ApiClient::new(s, f.contains_key("--allow-loopback-http"), None)
            .map_err(|_| usage("Invalid server origin"))?;
    }
    for k in [
        "--payload-hash",
        "--mapping-hash",
        "--profile-hash",
        "--ledger-head-hash",
    ] {
        if f.get(k).is_some_and(|v| !hash(&json!(v))) {
            return Err(usage("Exact lowercase SHA256 required"));
        }
    }
    for k in ["--as-of", "--from"] {
        if f.get(k)
            .is_some_and(|v| rentemester_engine::fiscal_year::Date::parse(v).is_err())
        {
            return Err(usage("Native calendar date required"));
        }
    }
    if f.get("--confirm").is_some_and(|v| *v != "yes") {
        return Err(usage("--confirm must be exactly yes"));
    }
    if f.get("--side")
        .is_some_and(|v| !matches!(*v, "left" | "right"))
    {
        return Err(usage("--side must be left or right"));
    }
    if f.get("--journal-entry-id").is_some_and(|v| {
        v.parse::<u64>().is_err()
            || v.parse::<u64>()
                .is_ok_and(|n| n == 0 || n > 9_007_199_254_740_991 || n.to_string() != *v)
    }) {
        return Err(usage("Exact positive journal entry ID required"));
    }
    if let Some(v) = f.get("--settlement-evidence-json") {
        let a: Value =
            serde_json::from_str(v).map_err(|_| usage("Evidence JSON array required"))?;
        if !a.as_array().is_some_and(|a| {
            !a.is_empty()
                && a.len() <= 64
                && a.iter()
                    .all(|v| v.as_str().is_some_and(|s| text(s, 160).is_ok()))
        }) {
            return Err(usage("Bounded evidence ID array required"));
        }
    }
    if f.contains_key("--help") {
        return Ok(f);
    }
    if !f.contains_key("--company")
        || !f.contains_key("--cookie-file")
        || !local && !f.contains_key("--server")
    {
        return Err(binding_refusal(args));
    }
    if !local && !slug(f["--company"]) {
        return Err(usage("Explicit company slug required"));
    }
    if write(op) && f.get("--confirm") != Some(&"yes") {
        return Err(usage("Exact --confirm yes required"));
    }
    let required: &[&str] = match op.as_str() {
        "plan-disposition" | "propose-disposition" => &["--disposition"],
        "disposition-status" => &["--disposition-id"],
        "approve-disposition" => &["--disposition-id", "--payload-hash"],
        "link-disposition" => &[
            "--disposition-id",
            "--payload-hash",
            "--side",
            "--journal-entry-id",
        ],
        "settle-disposition" => &[
            "--disposition-id",
            "--payload-hash",
            "--settlement-evidence-json",
        ],
        "reopen-disposition" => &["--disposition-id", "--payload-hash", "--reason"],
        "supersede-disposition" => &[
            "--disposition-id",
            "--payload-hash",
            "--reason",
            "--replacement-disposition-id",
        ],
        "validate-manifest" => &["--manifest"],
        "apply-manifest" => &["--manifest", "--policy-company"],
        "validate-mapping" | "propose-mapping" => &["--mapping"],
        "approve-mapping" => &["--mapping-id", "--mapping-hash"],
        "revoke-mapping" => &["--mapping-id"],
        "propose-elimination" => &["--elimination"],
        "approve-elimination"
        | "reject-elimination"
        | "apply-elimination"
        | "reverse-elimination" => &["--elimination-id", "--payload-hash"],
        "validate-profile" | "propose-profile" => &["--profile"],
        "approve-profile" => &["--profile-id", "--profile-hash"],
        "revoke-profile" => &["--profile-id"],
        "consolidated-report" => &["--profile-id", "--from", "--as-of"],
        _ => &["--as-of"],
    };
    if required.iter().any(|k| !f.contains_key(k)) {
        return Err(usage("Missing required source selector"));
    }
    Ok(f)
}
pub fn preflight(args: &[String]) -> Result<bool, Output> {
    parse(args).map(|f| f.contains_key("--help"))
}
fn file(path: &str, max: usize) -> Result<Value, String> {
    let f =
        std::fs::File::open(path).map_err(|_| "group manifest file must be readable valid JSON")?;
    if !f
        .metadata()
        .is_ok_and(|m| m.is_file() && m.len() <= max as u64)
    {
        return Err("bounded regular JSON file required".into());
    }
    let mut b = Vec::new();
    f.take(max as u64 + 1)
        .read_to_end(&mut b)
        .map_err(|_| "input read refused")?;
    if b.len() > max {
        return Err("input exceeds native bound".into());
    }
    serde_json::from_slice(&b).map_err(|_| "group manifest file must be readable valid JSON".into())
}
fn error(message: &str, code: &str) -> Value {
    json!({"ok":false,"errors":[message],"code":code})
}
fn unknown() -> Value {
    error(
        "Write outcome unknown; read exact current-authorized state/history and original source evidence before any explicit action; never automatically retry",
        "outcome_unknown",
    )
}
fn client_error(e: rentemester_client::Error, write: bool) -> Value {
    use rentemester_client::Error;
    match e {
        Error::Protocol => error(
            "Protocol refused before dispatch; no mutation sent",
            "protocol",
        ),
        Error::OutcomeUnknown | Error::Transport if write => unknown(),
        e => error(&e.to_string(), "server_refused"),
    }
}
fn blockers() -> Value {
    json!([
        "structure overview deliberately contains no financial consolidation",
        "use an approved reporting profile and the consolidated-report contract for figures"
    ])
}
fn structure() -> Value {
    json!({"ok":true,"scope":"structure-status-only","consolidationStatus":"not-available","consolidatedFigures":null,"rawCompanySums":null,"blockers":blockers()})
}
fn bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).expect("JSON value")
}
async fn disposition(api: &ApiClient, op: &str, f: &BTreeMap<&str, &str>) -> Result<Value, Value> {
    let write = write(op);
    let company = f["--company"];
    let action = match op {
        "plan-disposition" => Action::Plan,
        "propose-disposition" => Action::Propose,
        "approve-disposition" => Action::Approve,
        "link-disposition" => Action::Link,
        "settle-disposition" => Action::Settle,
        "supersede-disposition" => Action::Supersede,
        _ => Action::Reopen,
    };
    let mut args = json!({});
    if matches!(op, "plan-disposition" | "propose-disposition") {
        args["disposition"] = file(f["--disposition"], rentemester_client::MAX_DISPOSITION_BODY)
            .map_err(|s| error(&s, "invalid_input"))?;
        let mut check = args["disposition"].clone();
        if check.get("dispositionId").is_none_or(|v| v == "") {
            check["dispositionId"] = json!("validation-only");
        }
        if normalized(&check).is_none() {
            return Err(error(
                "Native typed disposition calendar/money/UTF16/source grammar required; null ID/date is not safely representable",
                "invalid_input",
            ));
        }
    } else {
        args["dispositionId"] = json!(f["--disposition-id"]);
        if write {
            args["payloadHash"] = json!(f["--payload-hash"]);
        }
    }
    for (flag, key) in [
        ("--side", "side"),
        ("--ledger-head-hash", "ledgerHeadHash"),
        ("--reason", "reason"),
        ("--replacement-disposition-id", "replacementDispositionId"),
    ] {
        if let Some(v) = f.get(flag) {
            args[key] = json!(v);
        }
    }
    if op == "link-disposition" {
        args["journalEntryId"] = json!(f["--journal-entry-id"].parse::<u64>().unwrap());
        if args.get("ledgerHeadHash").is_none() {
            args["ledgerHeadHash"] = Value::Null;
        }
    }
    if let Some(v) = f.get("--settlement-evidence-json") {
        args["settlementEvidenceRecordIds"] = serde_json::from_str(v).unwrap();
    }
    if write {
        args["confirm"] = json!(true);
    }
    let status = op == "disposition-status";
    let v = if status {
        let companies = api.companies().await.map_err(|e| client_error(e, false))?;
        if companies["ok"] != true
            || !companies["companies"].as_array().is_some_and(|a| {
                a.iter()
                    .any(|c| c["slug"] == company && c["archived"] == false)
            })
        {
            return Err(error(
                "Current company anchor not admitted",
                "access_denied",
            ));
        }
        api.group_disposition_status(f["--disposition-id"], f.get("--as-of").copied())
            .await
    } else {
        api.intercompany_disposition(company, action, bytes(&args), write)
            .await
    }
    .map_err(|e| client_error(e, write))?;
    if v["ok"] == false {
        return Err(v);
    }
    let invalid = || {
        if write {
            unknown()
        } else {
            error(
                "Malformed or mismatched canonical disposition receipt",
                "protocol",
            )
        }
    };
    if v["ok"] != true {
        return Err(invalid());
    }
    if op == "plan-disposition" {
        let d = normalized(&v["disposition"]).ok_or_else(invalid)?;
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
            return Err(invalid());
        }
        return Ok(
            json!({"ok":true,"mode":"dry-run","disposition":d,"payloadHash":v["payloadHash"],"ledgerEffects":[],"proposedSides":["left","right"],"warnings":v["warnings"]}),
        );
    }
    let mut d = state(&v, company, !status).ok_or_else(invalid)?;
    if op == "propose-disposition" {
        if d["status"] != "proposed" || !intent(&args["disposition"], &d["disposition"]) {
            return Err(invalid());
        }
    } else if d["disposition"]["dispositionId"].as_str()
        != text(f["--disposition-id"], 80).ok().as_deref()
        || write && d["payloadHash"] != args["payloadHash"]
    {
        return Err(invalid());
    }
    let bound = match op {
        "approve-disposition" => matches!(
            d["status"].as_str(),
            Some("approved" | "partly_posted" | "posted")
        ),
        "link-disposition" => {
            matches!(d["status"].as_str(), Some("partly_posted" | "posted"))
                && d["links"].as_array().unwrap().iter().any(|l| {
                    l["side"] == args["side"] && l["journal_entry_id"] == args["journalEntryId"]
                })
        }
        "settle-disposition" => d["status"] == "settled",
        "supersede-disposition" => {
            d["status"] == "superseded"
                && lifecycle_matches(
                    &d,
                    "superseded",
                    &args,
                    &["payloadHash", "replacementDispositionId", "reason"],
                )
        }
        "reopen-disposition" => {
            matches!(
                d["status"].as_str(),
                Some("approved" | "partly_posted" | "posted")
            ) && lifecycle_matches(&d, "reopened", &args, &["payloadHash", "reason"])
        }
        _ => true,
    };
    if !bound {
        return Err(invalid());
    }
    if status {
        let ex = v["exceptions"].as_array().ok_or_else(invalid)?;
        if v["scope"] != "intercompany-disposition-status"
            || v["balanced"] != json!(d["status"] == "posted" && ex.is_empty())
            || ex.iter().any(|e| {
                e["waivable"] != false
                    || match e["kind"].as_str() {
                        Some("stale_or_reversed_journal" | "one_sided_posting") => {
                            !matches!(e["side"].as_str(), Some("left" | "right"))
                        }
                        Some("overdue_settlement") => {
                            e["dueDate"] != d["disposition"]["settlementDueDate"]
                        }
                        _ => true,
                    }
            })
        {
            return Err(invalid());
        }
        for k in ["exceptions", "balanced", "scope"] {
            d[k] = v[k].clone();
        }
    }
    d["ok"] = json!(true);
    Ok(d)
}
async fn current_manifest(api: &ApiClient, company: &str) -> Result<(Manifest, Value), Value> {
    let v = api
        .intercompany(company)
        .await
        .map_err(|e| client_error(e, false))?;
    if v["ok"] != true {
        return Err(error(
            "Current authorized group structure unavailable",
            "access_denied",
        ));
    }
    let m: Manifest = serde_json::from_value(v["manifest"].clone())
        .map_err(|_| error("group structure must be configured", "invalid_input"))?;
    let registered = m.companies();
    let m = m
        .normalize(&registered)
        .map_err(|_| error("Invalid current manifest", "protocol"))?;
    if serde_json::to_value(&m).ok().as_ref() != Some(&v["manifest"])
        || m.canonical()
            .ok()
            .map(|s| rentemester_engine::sha256_bytes(s.as_bytes()))
            .as_deref()
            != v["manifestHash"].as_str()
    {
        return Err(error(
            "Current manifest canonical hash mismatch",
            "protocol",
        ));
    }
    Ok((m, v))
}
async fn run(api: &ApiClient, op: &str, f: &BTreeMap<&str, &str>) -> Result<Value, Value> {
    if let Some(actor) = f.get("--actor") {
        let me = api.me().await.map_err(|e| client_error(e, false))?;
        let user = me["user"]["id"]
            .as_str()
            .ok_or_else(|| error("Current human credential required", "access_denied"))?;
        if *actor != format!("user:{user}") {
            return Err(error(
                "actor differs from current cookie user",
                "access_denied",
            ));
        }
    }
    if op.contains("disposition") {
        return disposition(api, op, f).await;
    }
    let company = f["--company"];
    if op == "overview" {
        return Err(error(
            "Existing SDK/current HTTP does not expose source structure overview with current ledger-availability facts; manifest alone cannot prove source readiness",
            "unsupported_native_profile",
        ));
    }
    if matches!(op, "validate-manifest" | "apply-manifest") {
        if f.get("--policy-company").is_some_and(|s| *s != company) {
            return Err(error(
                "policy-company must equal verified selected company; never an authority override",
                "access_denied",
            ));
        }
        let input = file(f["--manifest"], rentemester_client::MAX_INTERCOMPANY_BODY)
            .map_err(|s| error(&s, "invalid_input"))?;
        let companies = api.companies().await.map_err(|e| client_error(e, false))?;
        let rows = companies["companies"]
            .as_array()
            .filter(|_| companies["ok"] == true)
            .ok_or_else(|| error("Current company catalogue unavailable", "protocol"))?;
        let registered: BTreeSet<String> = rows
            .iter()
            .map(|c| c["slug"].as_str().map(str::to_owned))
            .collect::<Option<_>>()
            .ok_or_else(|| error("Malformed company catalogue", "protocol"))?;
        let m: Manifest = serde_json::from_value(input.clone())
            .map_err(|_| error("Native V1 manifest grammar required", "invalid_input"))?;
        let m = m
            .normalize(&registered)
            .map_err(|e| error(&e.to_string(), "invalid_input"))?;
        let mut out = structure();
        if op == "validate-manifest" {
            out["groupCount"] = json!(m.groups.len());
            return Ok(out);
        }
        let mut body = input;
        body["confirm"] = json!(true);
        let v = api
            .mutate_intercompany(company, IM::Manifest, bytes(&body), true)
            .await
            .map_err(|e| client_error(e, true))?;
        let h = m
            .canonical()
            .map(|s| rentemester_engine::sha256_bytes(s.as_bytes()))
            .map_err(|_| unknown())?;
        if v["ok"] != true
            || v["manifestHash"] != h
            || !matches!(v["status"].as_str(), Some("applied" | "unchanged"))
        {
            return Err(unknown());
        }
        out["status"] = v["status"].clone();
        out["manifestHash"] = v["manifestHash"].clone();
        return Ok(out);
    }
    if matches!(
        op,
        "validate-mapping" | "propose-mapping" | "approve-mapping" | "revoke-mapping"
    ) {
        let (manifest, current) = current_manifest(api, company).await?;
        let expected;
        let mut body = if matches!(op, "validate-mapping" | "propose-mapping") {
            let input = file(f["--mapping"], rentemester_client::MAX_INTERCOMPANY_BODY)
                .map_err(|s| error(&s, "invalid_input"))?;
            let m: Mapping = serde_json::from_value(input.clone())
                .map_err(|_| error("Native mapping grammar required", "invalid_input"))?;
            let m = m
                .normalize(&manifest)
                .map_err(|e| error(&e.to_string(), "invalid_input"))?;
            if op == "validate-mapping" {
                return Ok(json!({"ok":true,"mapping":m}));
            }
            expected = Some((
                m.id.clone(),
                rentemester_engine::sha256_bytes(
                    m.canonical()
                        .map_err(|_| error("Invalid mapping", "invalid_input"))?
                        .as_bytes(),
                ),
            ));
            input
        } else {
            let id = f["--mapping-id"];
            let h = if op == "revoke-mapping" {
                let s = current["mappings"]
                    .as_array()
                    .and_then(|a| {
                        a.iter()
                            .find(|s| s["mapping"]["id"] == id && s["status"] == "approved")
                    })
                    .ok_or_else(|| error("approved mapping not found", "invalid_input"))?;
                let m: Mapping = serde_json::from_value(s["mapping"].clone())
                    .map_err(|_| error("Malformed current mapping", "protocol"))?;
                let m = m
                    .normalize(&manifest)
                    .map_err(|_| error("Invalid current mapping", "protocol"))?;
                let h = rentemester_engine::sha256_bytes(
                    m.canonical()
                        .map_err(|_| error("Invalid current mapping", "protocol"))?
                        .as_bytes(),
                );
                if s["mappingHash"] != h {
                    return Err(error("Current mapping hash mismatch", "protocol"));
                }
                h
            } else {
                f["--mapping-hash"].to_string()
            };
            expected = Some((id.to_string(), h.clone()));
            json!({"mappingId":id,"mappingHash":h})
        };
        body["confirm"] = json!(true);
        let action = match op {
            "propose-mapping" => IM::Propose,
            "approve-mapping" => IM::Approve,
            _ => IM::Revoke,
        };
        let v = api
            .mutate_intercompany(company, action, bytes(&body), true)
            .await
            .map_err(|e| client_error(e, true))?;
        let (id, h) = expected.unwrap();
        let status = match op {
            "propose-mapping" => "proposed",
            "approve-mapping" => "approved",
            _ => "revoked",
        };
        if v["ok"] != true || v["mappingId"] != id || v["mappingHash"] != h || v["status"] != status
        {
            return Err(unknown());
        }
        return Ok(json!({"ok":true,"mappingId":id,"mappingHash":h,"status":status}));
    }
    if op == "reconcile" {
        let v = api
            .group_reconciliation(f["--as-of"])
            .await
            .map_err(|e| client_error(e, false))?;
        if v["ok"] != true
            || v["scope"] != "intercompany-reconciliation"
            || v["asOf"] != f["--as-of"]
            || !v["rows"].is_array()
        {
            return Err(error("Malformed reconciliation receipt", "protocol"));
        }
        return Ok(v);
    }
    if matches!(
        op,
        "validate-profile" | "propose-profile" | "approve-profile" | "revoke-profile"
    ) {
        let mut expected = None;
        let body = if matches!(op, "validate-profile" | "propose-profile") {
            let (manifest, _) = current_manifest(api, company).await?;
            let input = file(
                f["--profile"],
                rentemester_client::MAX_CONSOLIDATION_PROPOSE_BODY,
            )
            .map_err(|s| error(&s, "invalid_input"))?;
            let p: Profile = serde_json::from_value(input.clone())
                .map_err(|_| error("Native profile grammar required", "invalid_input"))?;
            let p = p
                .normalize(&manifest)
                .map_err(|e| error(&e.to_string(), "invalid_input"))?;
            if op == "validate-profile" {
                return Ok(json!({"ok":true,"profile":p}));
            }
            expected = Some((
                p.id.clone(),
                rentemester_engine::sha256_bytes(
                    p.canonical()
                        .map_err(|_| error("Invalid profile", "invalid_input"))?
                        .as_bytes(),
                ),
            ));
            json!({"profile":input,"confirm":true})
        } else if op == "approve-profile" {
            json!({"profileId":f["--profile-id"],"profileHash":f["--profile-hash"],"confirm":true})
        } else {
            json!({"profileId":f["--profile-id"],"confirm":true})
        };
        let action = match op {
            "propose-profile" => CM::Propose,
            "approve-profile" => CM::Approve,
            _ => CM::Revoke,
        };
        let v = api
            .mutate_consolidation(company, action, bytes(&body), true)
            .await
            .map_err(|e| client_error(e, true))?;
        let id = expected
            .as_ref()
            .map(|p| p.0.as_str())
            .unwrap_or_else(|| f["--profile-id"]);
        let status = match op {
            "propose-profile" => "proposed",
            "approve-profile" => "approved",
            _ => "revoked",
        };
        if v["ok"] != true
            || v["profileId"] != id
            || v["status"] != status
            || op != "revoke-profile"
                && v["profileHash"].as_str()
                    != expected
                        .as_ref()
                        .map(|p| p.1.as_str())
                        .or_else(|| f.get("--profile-hash").copied())
        {
            return Err(unknown());
        }
        let mut out = json!({"ok":true,"profileId":id,"status":status});
        if op != "revoke-profile" {
            out["profileHash"] = v["profileHash"].clone();
        }
        return Ok(out);
    }
    if op == "eliminations" {
        let v = api
            .group_eliminations(f["--as-of"])
            .await
            .map_err(|e| client_error(e, false))?;
        if v["ok"] != true
            || v["scope"] != "consolidation-eliminations"
            || v["asOf"] != f["--as-of"]
            || !v["rows"].is_array()
        {
            return Err(error("Malformed eliminations receipt", "protocol"));
        }
        let rows=v["rows"].as_array().unwrap().iter().map(|r|{
 let payload:rentemester_engine::eliminations::BalancePayload=serde_json::from_value(r["payload"].clone()).ok()?;
 let h=rentemester_engine::sha256_bytes(payload.canonical().ok()?.as_bytes());
 if r["payloadHash"]!=h||r["eliminationId"]!=payload.id||payload.as_of!=f["--as-of"]||!hash(&r["eventHash"]){return None;}
 Some(json!({"eliminationId":r["eliminationId"],"payloadHash":h,"eventHash":r["eventHash"],"payload":payload}))
 }).collect::<Option<Vec<_>>>().ok_or_else(||error("Elimination payload/hash mismatch","protocol"))?;
        return Ok(
            json!({"ok":true,"scope":"consolidation-eliminations","asOf":f["--as-of"],"eliminations":rows}),
        );
    }
    if op.ends_with("-elimination") {
        let body = if op == "propose-elimination" {
            let mut input = file(f["--elimination"], rentemester_client::MAX_ELIMINATION_BODY)
                .map_err(|s| error(&s, "invalid_input"))?;
            if !hash(&input["expectedPayloadHash"]) {
                return Err(error(
                    "Explicit native expectedPayloadHash required in elimination file; never generated from a fresh plan",
                    "unsupported_native_profile",
                ));
            }
            input["confirm"] = json!(true);
            input
        } else {
            json!({"eliminationId":f["--elimination-id"],"payloadHash":f["--payload-hash"],"confirm":true})
        };
        let action = match op {
            "propose-elimination" => EM::Propose,
            "approve-elimination" => EM::Approve,
            "reject-elimination" => EM::Reject,
            "apply-elimination" => EM::Apply,
            _ => EM::Reverse,
        };
        let v = api
            .mutate_elimination(company, action, bytes(&body), true)
            .await
            .map_err(|e| client_error(e, true))?;
        let id = if op == "propose-elimination" {
            &body["id"]
        } else {
            &body["eliminationId"]
        };
        let h = if op == "propose-elimination" {
            &body["expectedPayloadHash"]
        } else {
            &body["payloadHash"]
        };
        let status = match op {
            "propose-elimination" => "proposed",
            "approve-elimination" => "approved",
            "reject-elimination" => "rejected",
            "apply-elimination" => "applied",
            _ => "reversed",
        };
        if v["ok"] != true
            || &v["eliminationId"] != id
            || &v["payloadHash"] != h
            || !hash(&v["eventHash"])
            || v["status"] != status
        {
            return Err(unknown());
        }
        return Ok(
            json!({"ok":true,"eliminationId":id,"payloadHash":h,"eventHash":v["eventHash"],"status":status}),
        );
    }
    if op == "consolidated-report" {
        let v = api
            .group_consolidated_report(f["--profile-id"], f["--from"], f["--as-of"])
            .await
            .map_err(|e| client_error(e, false))?;
        if v["ok"] != true
            || v["scope"] != "consolidated-report"
            || v["period"] != json!({"from":f["--from"],"to":f["--as-of"]})
            || !v["blockers"].is_array()
        {
            return Err(error("Malformed consolidated report", "protocol"));
        }
        if v["status"] == "blocked" || !v["blockers"].as_array().unwrap().is_empty() {
            return Err(error(
                "Consolidated report blocked; no partial figures admitted",
                "consolidation_blocked",
            ));
        }
        if v["status"] != "ready"
            || v["profileId"] != f["--profile-id"]
            || !hash(&v["profileHash"])
            || !v["rawCompanySums"].is_array()
            || !v["appliedEliminations"].is_array()
            || !v["sourceSnapshots"].is_array()
            || !v["consolidatedFigures"].is_object()
        {
            return Err(error("Missing consolidated figures", "protocol"));
        }
        return Ok(v);
    }
    Err(error("Unsupported group command", "invalid_input"))
}
fn emit(v: Value, f: &BTreeMap<&str, &str>, op: &str) -> Output {
    let ok = v["ok"] == true;
    let machine = f.contains_key("--json") || f.get("--format") == Some(&"json");
    if machine {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&v).unwrap()),
            stderr: String::new(),
            exit: if ok { 0 } else { 1 },
        };
    }
    let description = catalogue()
        .into_iter()
        .find(|r| r["key"] == format!("group {op}"))
        .and_then(|r| r["description"].as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("group {op}"));
    if !ok {
        let errors = v["errors"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(|s| format!("  → {s}\n"))
                    .collect::<String>()
            })
            .unwrap_or_default();
        return Output {
            stdout: String::new(),
            stderr: format!("✘ {description} failed\n{errors}"),
            exit: 1,
        };
    }
    let heading = description.split(". ").next().unwrap_or(&description);
    let heading = if heading.chars().count() > 60 {
        format!(
            "{}…",
            heading.chars().take(57).collect::<String>().trim_end()
        )
    } else {
        heading.to_string()
    };
    let mut out = format!("✔ {heading}\n");
    // Source generic renderer has a fixed preferred scalar list, not arbitrary receipt flattening.
    let fields = [
        ("message", "Besked"),
        ("journalEntryId", "Finanspostering-id"),
        ("count", "Antal"),
        ("total", "I alt"),
        ("name", "Navn"),
        ("currency", "Valuta"),
        ("slug", "Slug"),
    ];
    for (k, label) in fields {
        if let Some(s) = v[k].as_str() {
            if !s.is_empty() {
                out.push_str(&format!("  {label}: {s}\n"));
            }
        }
    }
    if let Some(p) = v["profile"].as_object() {
        for (k, v) in p {
            if let Some(s) = v.as_str() {
                let label = if k == "currency" {
                    "Valuta".into()
                } else {
                    human_key(k)
                };
                out.push_str(&format!("  {label}: {s}\n"));
            } else if v.is_number() || v.is_boolean() {
                out.push_str(&format!("  {}: {v}\n", human_key(k)));
            }
        }
    }
    if let Some(w) = v["warnings"].as_array().filter(|a| !a.is_empty()) {
        out.push_str("  Advarsler:\n");
        for s in w.iter().filter_map(Value::as_str) {
            out.push_str(&format!("    - {s}\n"));
        }
    }
    Output {
        stdout: out,
        stderr: String::new(),
        exit: 0,
    }
}
fn human_key(k: &str) -> String {
    let mut s = String::new();
    for (i, c) in k.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            s.push(' ');
        }
        s.push(if i == 0 { c.to_ascii_uppercase() } else { c });
    }
    s
}
pub fn execute(args: &[String]) -> Output {
    let f = match parse(args) {
        Ok(f) => f,
        Err(o) => return o,
    };
    if f.contains_key("--help") {
        return Output {
            stdout: format!("{}\n", help()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let op = &args[1];
    let fail = |v| emit(v, &f, op);
    let api = match crate::remote::credential_client(
        f["--server"],
        f.contains_key("--allow-loopback-http"),
        &f,
        false,
    ) {
        Ok(a) => a,
        Err(_) => {
            return fail(error(
                "Private human credential or transport refused",
                "access_denied",
            ));
        }
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => return fail(error("HTTP runtime unavailable", "transport")),
    };
    fail(match rt.block_on(run(&api, op, &f)) {
        Ok(v) | Err(v) => v,
    })
}
