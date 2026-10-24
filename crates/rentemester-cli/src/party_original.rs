//! Original explicit merge commands; current server cookie authority, no local writer.
use crate::dated_reports::Output;
use rentemester_client::PartyMutation;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
const COMMANDS: &[&str] = &[
    "propose-merge",
    "approve-merge",
    "create",
    "search",
    "inspect",
    "link-role",
];
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|v| v == "party")
}
pub fn help() -> Value {
    json!({"commands":["party create --input JSONFILE --confirm yes", "party search [--query TEXT --cursor 0 --limit 25]", "party inspect --party-id ID", "party link-role --party-id ID --role ROLE [--defaults JSONFILE] --confirm yes", "party propose-merge --from-party-id ID --into-party-id ID --review-assertion TEXT --confirm yes", "party approve-merge --from-party-id ID --proposal-hash SHA --confirm yes"],"connection":"Explicit --company SLUG or verified local company PATH; --server ORIGIN or --local-profile ABS_PRIVATE_PROFILE; --cookie-file PRIVATE_FILE; --allow-loopback-http; --json or --format human|json", "authority":"Human cookie only. Optional --actor user:ID asserts current cookie identity; --workspace is an equality assertion only with --local-profile. Both full writable party scopes required. No stdin, inferred merge, identity rewrite, retries or generated keys/hashes. Same human may propose and approve."})
}
fn usage(message: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{message}; see party --help\n"),
        exit: 2,
    }
}
pub fn binding_refusal(_: &[String]) -> Output {
    usage("Explicit verified company and transport binding required")
}
fn text(v: &str, max: usize) -> bool {
    !v.trim().is_empty() && v.encode_utf16().count() <= max && !v.chars().any(char::is_control)
}
fn parse(args: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    if args == ["party"] || args == ["party", "--help"] {
        return Ok(BTreeMap::from([("--help", "true")]));
    }
    let op = args
        .get(1)
        .map(String::as_str)
        .filter(|s| COMMANDS.contains(s))
        .ok_or_else(|| usage("Unknown party command"))?;
    let mut f = BTreeMap::new();
    let mut i = 2;
    while i < args.len() {
        let (k, a) = crate::original_args::option(&args[i]);
        let allowed = matches!(
            k,
            "--company"
                | "--server"
                | "--local-profile"
                | "--workspace"
                | "--cookie-file"
                | "--allow-loopback-http"
                | "--json"
                | "--format"
                | "--help"
        ) || matches!(
            op,
            "create" | "link-role" | "propose-merge" | "approve-merge"
        ) && matches!(k, "--actor" | "--confirm")
            || match op {
                "create" => k == "--input",
                "search" => matches!(k, "--query" | "--cursor" | "--limit"),
                "inspect" => k == "--party-id",
                "link-role" => matches!(k, "--party-id" | "--role" | "--defaults"),
                "propose-merge" => matches!(
                    k,
                    "--from-party-id" | "--into-party-id" | "--review-assertion"
                ),
                _ => matches!(k, "--from-party-id" | "--proposal-hash"),
            };
        if !allowed {
            return Err(usage("Unknown or unsupported party flag"));
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
        .is_some_and(|v| !matches!(*v, "human" | "json"))
    {
        return Err(usage("Invalid output format"));
    }
    let local = f.contains_key("--local-profile");
    if local && (f.contains_key("--server") || f.contains_key("--allow-loopback-http"))
        || !local && f.contains_key("--workspace")
    {
        return Err(usage(
            "Conflicting explicit transport or workspace assertion",
        ));
    }
    for k in ["--local-profile", "--workspace"] {
        if let Some(v) = f.get(k) {
            if !text(v, 16384) || !std::path::Path::new(v).is_absolute() {
                return Err(usage("Bounded absolute local assertion required"));
            }
        }
    }
    if let Some(s) = f.get("--server") {
        rentemester_client::ApiClient::new(s, f.contains_key("--allow-loopback-http"), None)
            .map_err(|_| usage("Invalid explicit server origin"))?;
    }
    for k in ["--from-party-id", "--party-id", "--actor"] {
        if let Some(v) = f.get(k) {
            if !text(v, if k == "--actor" { 160 } else { 64 }) || matches!(*v, "." | "..") {
                return Err(usage("Invalid bounded identifier"));
            }
        }
    }
    if f.get("--actor")
        .is_some_and(|v| !v.starts_with("user:") || v.len() <= 5)
    {
        return Err(usage("Actor must assert current user:ID"));
    }
    for (key, max) in [("--into-party-id", 64), ("--review-assertion", 500)] {
        if let Some(v) = f.get(key) {
            let value = rentemester_engine::parties::bounded(v, max)
                .map_err(|_| usage("Bounded nonblank merge text required"))?;
            if key == "--into-party-id"
                && (value.chars().any(char::is_control) || matches!(value.as_str(), "." | ".."))
            {
                return Err(usage("Invalid target party identifier"));
            }
        }
    }
    if f.get("--proposal-hash").is_some_and(|v| {
        v.len() != 64
            || !v
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }) {
        return Err(usage("Exact lowercase proposal hash required"));
    }
    if f.get("--from-party-id")
        .is_some_and(|v| Some(v) == f.get("--into-party-id"))
    {
        return Err(usage("Cannot merge party into itself"));
    }
    for k in ["--cursor", "--limit"] {
        if let Some(v) = f.get(k) {
            let n = v
                .parse::<usize>()
                .map_err(|_| usage("Invalid pagination"))?;
            if !v.bytes().all(|b| b.is_ascii_digit()) || k == "--limit" && !(1..=100).contains(&n) {
                return Err(usage("Invalid pagination"));
            }
        }
    }
    if f.get("--query")
        .is_some_and(|v| v.encode_utf16().count() > 512)
        || f.get("--role")
            .is_some_and(|v| !rentemester_engine::parties::ROLES.contains(v))
    {
        return Err(usage("Invalid bounded query or role"));
    }
    if f.get("--confirm").is_some_and(|v| *v != "yes") {
        return Err(usage("--confirm must be exactly yes"));
    }
    if f.contains_key("--help") {
        return Ok(f);
    }
    let company = f
        .get("--company")
        .ok_or_else(|| usage("Explicit --company required"))?;
    if !text(company, if local { 16384 } else { 128 })
        || !local
            && !company
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    {
        return Err(usage("Invalid company selector"));
    }
    if !local && !f.contains_key("--server") {
        return Err(usage("Explicit transport required"));
    }
    if !f.contains_key("--cookie-file") {
        return Err(usage("Explicit private human cookie required"));
    }
    let required: &[&str] = match op {
        "create" => &["--input"],
        "search" => &[],
        "inspect" => &["--party-id"],
        "link-role" => &["--party-id", "--role"],
        "propose-merge" => &["--from-party-id", "--into-party-id", "--review-assertion"],
        _ => &["--from-party-id", "--proposal-hash"],
    };
    if required.iter().any(|k| !f.contains_key(k))
        || !matches!(op, "inspect" | "search") && f.get("--confirm") != Some(&"yes")
    {
        return Err(usage("Missing command value or exact --confirm yes"));
    }
    Ok(f)
}
pub fn preflight(args: &[String]) -> Result<bool, Output> {
    parse(args).map(|f| f.contains_key("--help"))
}
fn label(op: &str) -> &'static str {
    if op == "create" {
        "Opretter en kanonisk workspace-part med kilde- og review-evidens."
    } else if op == "search" {
        "Søger deterministisk blandt parter synlige i den angivne virksomhed."
    } else if op == "inspect" {
        "Viser en kanonisk parts append-only historik."
    } else if op == "link-role" {
        "Knytter en parts virksomhedslokale rolle og defaults uden at ændre historiske poster."
    } else if op == "propose-merge" {
        "Foreslår, men udfører aldrig automatisk, en kanonisk partssammenslutning."
    } else {
        "Godkender og supersederer den præcise foreslåede partsrelation append-only."
    }
}
fn emit(v: Value, machine: bool, op: &str) -> Output {
    let exit = if v["ok"] == true { 0 } else { 1 };
    if machine {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&v).unwrap()),
            stderr: String::new(),
            exit,
        };
    }
    if exit == 0 {
        Output {
            stdout: format!(
                "✔ {}\n{}",
                {
                    let l = label(op);
                    if l.chars().count() > 60 {
                        format!("{}…", l.chars().take(57).collect::<String>().trim_end())
                    } else {
                        l.into()
                    }
                },
                v["count"]
                    .as_u64()
                    .map(|n| format!("  Antal: {n}\n"))
                    .unwrap_or_default()
            ),
            stderr: String::new(),
            exit,
        }
    } else {
        Output {
            stdout: String::new(),
            stderr: format!(
                "✘ {} failed\n{}",
                label(op),
                v["errors"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|s| format!("  → {s}\n"))
                    .collect::<String>()
            ),
            exit,
        }
    }
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
    let op = args[1].as_str();
    let machine = f.contains_key("--json") || f.get("--format") == Some(&"json");
    let fail = |s: &str| emit(json!({"ok":false,"errors":[s]}), machine, op);
    if matches!(op, "create" | "search" | "inspect" | "link-role") {
        return lifecycle(&f, machine, op);
    }
    let id = f["--from-party-id"];
    let (action, body) = if op == "propose-merge" {
        (
            PartyMutation::ProposeMerge(id.into()),
            json!({"intoPartyId":f["--into-party-id"],"reviewAssertion":f["--review-assertion"],"confirm":true}),
        )
    } else {
        (
            PartyMutation::ApproveMerge(id.into()),
            json!({"proposalHash":f["--proposal-hash"],"confirm":true}),
        )
    };
    let api = match crate::remote::credential_client(
        f["--server"],
        f.contains_key("--allow-loopback-http"),
        &f,
        false,
    ) {
        Ok(a) => a,
        Err(_) => return fail("Private human credential or transport refused"),
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => return fail("Runtime unavailable"),
    };
    let result = rt.block_on(async {
        if let Some(actor) = f.get("--actor") {
            let me = api.me().await?;
            let user = me["user"]["id"]
                .as_str()
                .ok_or(rentemester_client::Error::Protocol)?;
            if *actor != format!("user:{user}") {
                return Err(rentemester_client::Error::InvalidInput(
                    "actor differs from current cookie user",
                ));
            }
        }
        api.mutate_workspace_party(
            f["--company"],
            action,
            serde_json::to_vec(&body).unwrap(),
            true,
        )
        .await
    });
    match result {
        Err(rentemester_client::Error::InvalidInput(_)) => {
            usage("Actor assertion or native input refused")
        }
        Err(rentemester_client::Error::OutcomeUnknown) => {
            fail("operation outcome unknown; reconcile canonical party/history before retrying")
        }
        Err(_) => fail("Party transport, authorization or protocol refused"),
        Ok(v) => {
            if v["ok"] == false {
                return fail("Party merge refused");
            }
            if v["ok"] != true {
                return fail("operation outcome unknown; malformed merge reply");
            }
            let projected = if op == "propose-merge" {
                json!({"ok":true,"proposalHash":v["proposalHash"]})
            } else {
                json!({"ok":true,"party":v["party"]})
            };
            emit(projected, machine, op)
        }
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
fn file(path: &str, max: usize) -> Result<Vec<u8>, Output> {
    if !std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() <= max as u64) {
        return Err(usage("Bounded regular input file required"));
    }
    let f = std::fs::File::open(path).map_err(|_| usage("Input file unavailable"))?;
    if !f
        .metadata()
        .is_ok_and(|m| m.is_file() && m.len() <= max as u64)
    {
        return Err(usage("Bounded regular input file required"));
    }
    let mut bytes = Vec::new();
    f.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| usage("Input read refused"))?;
    if bytes.len() > max {
        return Err(usage("Input exceeds bound"));
    }
    Ok(bytes)
}

fn hash(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn value_text(v: &Value, max: usize) -> bool {
    v.as_str()
        .is_some_and(|s| rentemester_engine::company_knowledge::text(s, max).is_ok())
}
fn id(v: &Value) -> bool {
    value_text(v, 64)
        && v.as_str()
            .is_some_and(|s| !matches!(s, "." | "..") && !s.chars().any(char::is_control))
}
fn iso(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| rentemester_engine::company_knowledge::iso(s).is_ok())
}
fn history(v: &Value) -> bool {
    v.as_array().is_some_and(|a| {
        !a.is_empty()
            && a.iter().all(|e| {
                hash(&e["payload_hash"])
                    && ["event_type", "actor", "created_at"]
                        .iter()
                        .all(|k| e[k].as_str().is_some_and(|s| !s.is_empty()))
                    && iso(&e["created_at"])
            })
    })
}

fn party(v: &Value) -> bool {
    id(&v["partyId"])
        && matches!(
            v["kind"].as_str(),
            Some("person" | "organization" | "public_authority" | "financial_institution")
        )
        && ["name", "source", "reviewAssertion"]
            .iter()
            .all(|k| value_text(&v[k], 500))
        && iso(&v["observedAt"])
        && history(&v["history"])
        && v["identifiers"].as_array().is_some_and(|a| {
            a.iter().all(|i| {
                ["country", "identifier", "identifierKind"]
                    .iter()
                    .all(|k| i[k].is_string())
            })
        })
        && v["roles"].as_array().is_some_and(|a| {
            a.iter().all(|r| {
                r["companySlug"].is_string() && r["role"].is_string() && r["defaults"].is_object()
            })
        })
        && v["aliases"].as_array().is_some_and(|a| {
            a.iter().all(|r| {
                r["alias"].is_string()
                    && r["source"].is_string()
                    && r["review_state"].is_string()
                    && r.get("observed_at").is_some_and(|v| v.is_null() || iso(v))
                    && r.get("payload_hash")
                        .is_some_and(|v| v.is_null() || hash(v))
            })
        })
        && v["assertions"].as_array().is_some_and(|a| {
            a.iter().all(|r| {
                ["field", "value", "source", "review_state"]
                    .iter()
                    .all(|k| r[k].is_string())
                    && iso(&r["observed_at"])
                    && hash(&r["payload_hash"])
            })
        })
}
// Bind the new canonical receipt to exactly the supplied identity, never an inferred role.
// The producer projects aliases/assertions as typed rows (not original input strings).
fn created_identity(p: &Value, args: &Value) -> bool {
    use rentemester_engine::company_knowledge::{iso, trim};
    if !p["roles"].as_array().is_some_and(Vec::is_empty)
        || p["kind"] != args["kind"]
        || ["name", "source", "reviewAssertion"]
            .iter()
            .any(|k| p[k].as_str() != args[k].as_str().map(trim))
        || args
            .get("partyId")
            .is_some_and(|id| p["partyId"].as_str() != id.as_str().map(trim))
        || p["observedAt"].as_str() != iso(args["observedAt"].as_str().unwrap()).ok().as_deref()
        || !p["history"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["event_type"] == "created")
    {
        return false;
    }
    let source = p["source"].as_str().unwrap();
    let observed = &p["observedAt"];
    let assertion = |field: &str, value: &str| {
        p["assertions"].as_array().unwrap().iter().any(|r| {
            r["field"] == field
                && r["value"] == value
                && r["source"] == source
                && r["observed_at"] == *observed
                && r["review_state"] == "approved"
        })
    };
    if !assertion("name", p["name"].as_str().unwrap()) {
        return false;
    }
    let identifiers: Option<Vec<Value>> = args["identifiers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|i| {
            let i = serde_json::from_value::<rentemester_engine::parties::Identifier>(i.clone())
                .ok()?;
            serde_json::to_value(rentemester_engine::parties::identity(&i).ok()?).ok()
        })
        .collect();
    let Some(identifiers) = identifiers else {
        return false;
    };
    let actual = p["identifiers"].as_array().unwrap();
    if actual.len() != identifiers.len()
        || identifiers.iter().any(|i| {
            !actual.contains(i)
                || !assertion(
                    "identifier",
                    &format!(
                        "{}:{}:{}",
                        i["country"].as_str().unwrap(),
                        i["identifierKind"].as_str().unwrap(),
                        i["identifier"].as_str().unwrap()
                    ),
                )
        })
    {
        return false;
    }
    let aliases: std::collections::BTreeSet<String> = args["aliases"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| trim(a.as_str().unwrap()).to_lowercase())
        .collect();
    let actual = p["aliases"].as_array().unwrap();
    actual.len() == aliases.len()
        && aliases.iter().all(|a| {
            actual.iter().any(|r| {
                r["alias"] == *a
                    && r["source"] == source
                    && r["observed_at"] == *observed
                    && r["review_state"] == "approved"
                    && hash(&r["payload_hash"])
            })
        })
}

fn lifecycle(f: &BTreeMap<&str, &str>, machine: bool, op: &str) -> Output {
    use rentemester_client::{Error, MAX_PARTY_BODY, party_body_schema};
    use rentemester_engine::company_knowledge::{iso, text, trim};
    let write = matches!(op, "create" | "link-role");
    let selected = f.get("--party-id").copied().unwrap_or("");
    let selected = trim(selected);
    let action = if op == "create" {
        PartyMutation::CreateIdentity
    } else {
        PartyMutation::Role(selected.into())
    };
    let body = (|| -> Result<Value, Output> {
        if !write {
            return Ok(json!({}));
        }
        let mut body = if op == "create" {
            serde_json::from_slice::<Value>(&file(f["--input"], MAX_PARTY_BODY)?)
                .map_err(|_| usage("--input must be a readable JSON object"))?
        } else {
            let mut b = json!({"role":f["--role"]});
            if let Some(path) = f.get("--defaults") {
                b["defaults"] = serde_json::from_slice(&file(path, MAX_PARTY_BODY)?)
                    .map_err(|_| usage("--defaults must be a readable JSON object"))?;
            }
            b
        };
        if !body.is_object() || body.get("confirm").is_some() {
            return Err(usage(
                "Input object must not override confirmation or authority",
            ));
        }
        body["confirm"] = json!(true);
        if !shape(&body, &party_body_schema(&action)) {
            return Err(usage("Invalid party input fields or types"));
        }
        if op == "create" {
            for (key, max) in [("name", 320), ("source", 160), ("reviewAssertion", 500)] {
                if !body[key].as_str().is_some_and(|s| text(s, max).is_ok()) {
                    return Err(usage("Invalid bounded identity text"));
                }
            }
            if !body["observedAt"].as_str().is_some_and(|s| iso(s).is_ok())
                || body.get("partyId").is_some_and(|v| !id(v))
            {
                return Err(usage("Invalid identity selector or ISO instant"));
            }
            let mut seen = std::collections::BTreeSet::new();
            for i in body["identifiers"].as_array().into_iter().flatten() {
                let i =
                    serde_json::from_value::<rentemester_engine::parties::Identifier>(i.clone())
                        .map_err(|_| usage("Invalid typed identifier"))?;
                let r = rentemester_engine::parties::identity(&i)
                    .map_err(|_| usage("Invalid typed identity"))?;
                if !seen.insert((r.country, r.identifier_kind, r.identifier)) {
                    return Err(usage("Duplicate-equivalent typed identity"));
                }
            }
            if body["aliases"].as_array().is_some_and(|a| {
                a.iter()
                    .any(|v| !v.as_str().is_some_and(|s| text(s, 160).is_ok()))
            }) {
                return Err(usage("Invalid bounded alias"));
            }
        }
        if serde_json::to_vec(&body).unwrap().len() > MAX_PARTY_BODY {
            return Err(usage("Native party body bound exceeded"));
        }
        Ok(body)
    })();
    let body = match body {
        Ok(b) => b,
        Err(o) => return o,
    };
    let failure = |message: &str, code: &str| {
        emit(
            json!({"ok":false,"errors":[message],"code":code}),
            machine,
            op,
        )
    };
    let unknown = || {
        failure(
            "operation outcome unknown; read exact canonical party and full history before further action; do not assume rollback or resend",
            "outcome_unknown",
        )
    };
    let api = match crate::remote::credential_client(
        f["--server"],
        f.contains_key("--allow-loopback-http"),
        f,
        false,
    ) {
        Ok(a) => a,
        Err(_) => return failure("Private human credential or transport refused", "transport"),
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => return failure("Runtime unavailable", "transport"),
    };
    let cursor = f
        .get("--cursor")
        .map(|v| v.parse::<usize>().unwrap())
        .unwrap_or(0);
    let limit = f
        .get("--limit")
        .map(|v| v.parse::<usize>().unwrap())
        .unwrap_or(25);
    let result = rt.block_on(async {
        if let Some(actor) = f.get("--actor") {
            let me = api.me().await?;
            let user = me["user"]["id"].as_str().ok_or(Error::Protocol)?;
            if *actor != format!("user:{user}") {
                return Err(Error::InvalidInput(
                    "actor differs from current cookie user",
                ));
            }
        }
        match op {
            "search" => {
                api.workspace_parties(f["--company"], f.get("--query").copied(), cursor, limit)
                    .await
            }
            "inspect" => api.workspace_party(f["--company"], &selected).await,
            _ => {
                api.mutate_workspace_party(
                    f["--company"],
                    action,
                    serde_json::to_vec(&body).unwrap(),
                    true,
                )
                .await
            }
        }
    });
    let v = match result {
        Ok(v) => v,
        Err(Error::InvalidInput(_)) => return usage("Actor assertion or native input refused"),
        Err(Error::OutcomeUnknown) => return unknown(),
        Err(Error::Transport) if write => return unknown(),
        Err(Error::Protocol) => {
            return failure(
                "Server protocol admission refused before the party request; no mutation sent",
                "protocol",
            );
        }
        Err(Error::Refused { status: 404 }) => {
            return failure("party not found", "PARTY_NOT_FOUND");
        }
        Err(e) => {
            return failure(
                &e.to_string(),
                if matches!(e, Error::Refused { .. }) {
                    "server_refused"
                } else {
                    "transport"
                },
            );
        }
    };
    if v["ok"] == false {
        if v["errors"]
            .as_array()
            .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
        {
            let mut e = json!({"ok":false,"errors":v["errors"]});
            if v["code"].is_string() {
                e["code"] = v["code"].clone();
            }
            return emit(e, machine, op);
        }
        return failure("Party operation refused", "server_refused");
    }
    let receipt = v["ok"] == true
        && if op == "search" {
            v["rows"].as_array().is_some_and(|a| {
                a.len() <= limit
                    && a.iter().all(|p| {
                        party(p)
                            && p["roles"].as_array().is_some_and(|a| {
                                !a.is_empty()
                                    && a.iter().all(|r| r["companySlug"] == f["--company"])
                            })
                    })
            }) && v["count"].is_u64()
                && v.get("nextCursor")
                    .is_some_and(|v| v.is_null() || v.as_u64().is_some_and(|n| n > cursor as u64))
        } else {
            party(&v["party"])
                && if op == "create" {
                    created_identity(&v["party"], &body)
                } else {
                    v["party"]["partyId"] == selected
                        && (op != "link-role"
                            || v["party"]["roles"].as_array().unwrap().iter().any(|r| {
                                r["companySlug"] == f["--company"]
                                    && r["role"] == body["role"]
                                    && r["defaults"]
                                        == body.get("defaults").cloned().unwrap_or(json!({}))
                            }))
                }
        };
    if !receipt {
        return if write {
            unknown()
        } else {
            failure("Invalid canonical party response", "protocol")
        };
    }
    emit(
        if op == "search" {
            json!({"ok":true,"rows":v["rows"],"count":v["count"],"nextCursor":v["nextCursor"]})
        } else {
            json!({"ok":true,"party":v["party"]})
        },
        machine,
        op,
    )
}
