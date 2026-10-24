//! Original corporate-record grammar and flat projections over the existing native authority.
use crate::dated_reports::Output;
use rentemester_client::{CorporateRecordMutation as Mutation, corporate_record_body_schema};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
const COMMANDS: &[&str] = &[
    "ingest",
    "list",
    "inspect",
    "download",
    "link",
    "enrich",
    "supersede",
];
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|v| v == "corporate-record")
}
pub fn help() -> Value {
    json!({"commands":["corporate-record ingest --input JSONFILE --file BYTES --confirm yes","corporate-record list [--cursor 0 --limit 25]","corporate-record inspect --record-id ID","corporate-record download --record-id ID","corporate-record link --record-id ID --link-type TYPE --link-id ID --confirm yes","corporate-record enrich --record-id ID --assertion TEXT --confirm yes","corporate-record supersede --record-id ID --replacement-record-id ID --reason TEXT --confirm yes"],"connection":"Explicit --company SLUG or verified local company PATH; --server ORIGIN or --local-profile ABSOLUTE_PRIVATE_PROFILE; --cookie-file PRIVATE_FILE; --allow-loopback-http; --json or --format human|json","authority":"Human cookie only; --service-key-file unsupported. Optional write --actor user:ID asserts current user equality, never authority. --workspace PATH is only an equality assertion with --local-profile. No stdin, retry, generated identity or local fallback.","input":"Ingest metadata uses native schema excluding bytesBase64/confirm/actor/principal/authority; exact file bytes up to16MiB. Server owns scope, sensitivity, immutable originals and history."})
}
fn usage(message: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{message}; see corporate-record --help\n"),
        exit: 2,
    }
}
pub fn binding_refusal(_: &[String]) -> Output {
    usage("Explicit verified company and transport binding required")
}
fn write(op: &str) -> bool {
    matches!(op, "ingest" | "link" | "enrich" | "supersede")
}
fn action(op: &str, id: &str) -> Mutation {
    match op {
        "link" => Mutation::Link(id.into()),
        "enrich" => Mutation::Enrich(id.into()),
        "supersede" => Mutation::Supersede(id.into()),
        _ => Mutation::Ingest,
    }
}
fn text(v: &str, max: usize) -> bool {
    !v.trim().is_empty() && v.encode_utf16().count() <= max && !v.chars().any(char::is_control)
}
fn parse(args: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    if args == ["corporate-record"] || args == ["corporate-record", "--help"] {
        return Ok(BTreeMap::from([("--help", "true")]));
    }
    let op = args
        .get(1)
        .map(String::as_str)
        .filter(|s| COMMANDS.contains(s))
        .ok_or_else(|| usage("Unknown corporate-record command"))?;
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
        ) || write(op) && matches!(k, "--actor" | "--confirm")
            || match op {
                "ingest" => matches!(k, "--input" | "--file"),
                "list" => matches!(k, "--cursor" | "--limit"),
                "link" => matches!(k, "--record-id" | "--link-type" | "--link-id"),
                "enrich" => matches!(k, "--record-id" | "--assertion"),
                "supersede" => matches!(k, "--record-id" | "--replacement-record-id" | "--reason"),
                _ => k == "--record-id",
            };
        if !allowed {
            return Err(usage("Unknown or unsupported corporate-record flag"));
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
    for (k, max) in [
        ("--record-id", 64),
        ("--replacement-record-id", 64),
        ("--link-id", 256),
        ("--assertion", 2000),
        ("--reason", 1000),
        ("--actor", 160),
    ] {
        if let Some(v) = f.get(k) {
            let valid = if matches!(k, "--assertion" | "--reason" | "--link-id") {
                rentemester_engine::company_knowledge::text(v, max).is_ok()
            } else {
                text(v, max)
            };
            if !valid
                || matches!(k, "--record-id" | "--replacement-record-id")
                    && matches!(*v, "." | "..")
            {
                return Err(usage("Invalid bounded command value"));
            }
        }
    }
    if f.get("--link-type").is_some_and(|v| {
        !matches!(
            *v,
            "company" | "party" | "ownership" | "disposition" | "legacy_evidence_ref"
        )
    }) {
        return Err(usage("Invalid link type"));
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
        "ingest" => &["--input", "--file"],
        "list" => &[],
        "link" => &["--record-id", "--link-type", "--link-id"],
        "enrich" => &["--record-id", "--assertion"],
        "supersede" => &["--record-id", "--replacement-record-id", "--reason"],
        _ => &["--record-id"],
    };
    if required.iter().any(|k| !f.contains_key(k))
        || write(op) && f.get("--confirm") != Some(&"yes")
    {
        return Err(usage("Missing command value or exact --confirm yes"));
    }
    Ok(f)
}
pub fn preflight(args: &[String]) -> Result<bool, Output> {
    parse(args).map(|f| f.contains_key("--help"))
}
// Validate the existing SDK schema before credentials or business dispatch, without normalizing metadata.
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
fn encode(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if c.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
fn original(v: &Value) -> bool {
    let Some(s) = v["bytesBase64"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 22_369_624 && s.len() % 4 == 0)
    else {
        return false;
    };
    let mut bytes = Vec::with_capacity(s.len() / 4 * 3);
    for c in s.as_bytes().chunks_exact(4) {
        let mut n = 0u32;
        for b in c {
            let p = if *b == b'=' {
                0
            } else if let Some(p) =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
                    .iter()
                    .position(|x| x == b)
            {
                p as u32
            } else {
                return false;
            };
            n = (n << 6) | p;
        }
        bytes.push((n >> 16) as u8);
        if c[2] != b'=' {
            bytes.push((n >> 8) as u8);
        }
        if c[3] != b'=' {
            bytes.push(n as u8);
        }
    }
    bytes.len() <= 16 * 1024 * 1024
        && encode(&bytes) == s
        && v["sha256"].as_str() == Some(rentemester_storage::invoice_email::sha(&bytes).as_str())
}
fn hash(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn record(v: &Value) -> bool {
    [
        "recordId",
        "type",
        "filename",
        "source",
        "receivedAt",
        "uploader",
        "sensitivity",
    ]
    .iter()
    .all(|k| v[k].as_str().is_some_and(|s| !s.is_empty()))
        && hash(&v["sha256"])
        && ["recordDate", "retentionUntil"]
            .iter()
            .all(|k| v.get(k).is_some_and(|v| v.is_null() || v.is_string()))
        && v["links"].as_array().is_some_and(|a| {
            a.iter()
                .all(|l| l["type"].is_string() && l["id"].is_string())
        })
        && v["history"].as_array().is_some_and(|a| {
            a.iter().all(|e| {
                ["event_type", "actor", "created_at"]
                    .iter()
                    .all(|k| e[k].is_string())
                    && hash(&e["payload_hash"])
            })
        })
}
fn label(op: &str) -> &str {
    match op {
        "ingest" => "Gemmer uforanderlige governance-bytes og deres SHA-256 uden ledger-effekt.",
        "list" => "Lister governance-records der er fuldt synlige i virksomheds-scope.",
        "inspect" => "Viser record-metadata, scope-links og append-only historik.",
        "download" => "Læs original bytes efter SHA-256-verifikation (base64 i JSON-output).",
        "link" => "Knytter en record til eksisterende scope append-only.",
        "enrich" => "Tilføjer auditeret metadata uden at ændre original bytes.",
        _ => "Kæder en rettet governance-record som append-only supersession.",
    }
}
fn emit(v: Value, json_mode: bool, op: &str) -> Output {
    let exit = if v["ok"] == true { 0 } else { 1 };
    if json_mode {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&v).unwrap()),
            stderr: String::new(),
            exit,
        };
    }
    if exit == 0 {
        let l = label(op);
        let heading = if l.chars().count() > 60 {
            format!("{}…", l.chars().take(57).collect::<String>().trim_end())
        } else {
            l.into()
        };
        Output {
            stdout: format!(
                "✔ {heading}\n{}",
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
    let json_mode = f.contains_key("--json") || f.get("--format") == Some(&"json");
    let fail = |s: &str| emit(json!({"ok":false,"errors":[s]}), json_mode, op);
    let id = f.get("--record-id").copied().unwrap_or("");
    let mutation = action(op, id);
    let mut body = json!({"confirm":true});
    if op == "ingest" {
        let bytes = match file(f["--input"], rentemester_client::MAX_CORPORATE_INGEST_BODY) {
            Ok(b) => b,
            Err(o) => return o,
        };
        body = match serde_json::from_slice::<Value>(&bytes) {
            Ok(v) if v.is_object() => v,
            _ => return usage("Input must be a JSON metadata object"),
        };
        if body.as_object().unwrap().keys().any(|k| {
            matches!(
                k.as_str(),
                "actor" | "principal" | "confirm" | "bytesBase64" | "authority"
            )
        }) {
            return usage("Input authority or byte override unsupported");
        }
        let mut schema = corporate_record_body_schema(&mutation);
        schema["required"]
            .as_array_mut()
            .unwrap()
            .retain(|v| v != "bytesBase64" && v != "confirm");
        if !shape(&body, &schema) {
            return usage("Input metadata violates native schema");
        }
        let bytes = match file(f["--file"], 16 * 1024 * 1024) {
            Ok(b) if !b.is_empty() => b,
            Ok(_) => return usage("Original bytes required"),
            Err(o) => return o,
        };
        body["bytesBase64"] = json!(encode(&bytes));
        body["confirm"] = json!(true);
    } else {
        for (flag, key) in [
            ("--link-type", "type"),
            ("--link-id", "id"),
            ("--assertion", "assertion"),
            ("--replacement-record-id", "replacementRecordId"),
            ("--reason", "reason"),
        ] {
            if let Some(v) = f.get(flag) {
                body[key] = json!(v);
            }
        }
    }
    let wire = serde_json::to_vec(&body).unwrap();
    if write(op)
        && (!shape(&body, &corporate_record_body_schema(&mutation))
            || wire.len()
                > if op == "ingest" {
                    rentemester_client::MAX_CORPORATE_INGEST_BODY
                } else {
                    rentemester_client::MAX_CORPORATE_BODY
                })
    {
        return usage("Input violates native body schema or bound");
    }
    let api = match crate::remote::credential_client(
        f["--server"],
        f.contains_key("--allow-loopback-http"),
        &f,
        false,
    ) {
        Ok(a) => a,
        Err(_) => return fail("Private credential or transport refused"),
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
        match op {
            "list" => {
                api.corporate_records(
                    f["--company"],
                    f.get("--cursor").and_then(|v| v.parse().ok()).unwrap_or(0),
                    f.get("--limit").and_then(|v| v.parse().ok()).unwrap_or(25),
                )
                .await
            }
            "inspect" => api.corporate_record(f["--company"], id).await,
            "download" => api.corporate_record_original(f["--company"], id).await,
            _ => {
                api.mutate_corporate_record(f["--company"], mutation, wire, true)
                    .await
            }
        }
    });
    match result {
        Err(rentemester_client::Error::InvalidInput(_)) => {
            usage("Actor assertion or native input refused")
        }
        Err(rentemester_client::Error::Refused { status: 404 }) => {
            fail("corporate record not found")
        }
        Err(rentemester_client::Error::OutcomeUnknown) => {
            fail("operation outcome unknown; reconcile corporate record/history before retrying")
        }
        Err(_) => fail("Corporate record transport, authorization or protocol refused"),
        Ok(v) => {
            if v["ok"] == false
                && v["errors"]
                    .as_array()
                    .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
            {
                return emit(v, json_mode, op);
            }
            let valid = v["ok"] == true
                && match op {
                    "list" => {
                        v["rows"].as_array().is_some_and(|a| a.iter().all(record))
                            && v["count"].is_u64()
                            && v.get("nextCursor")
                                .is_some_and(|v| v.is_null() || v.is_u64())
                    }
                    "download" => original(&v),
                    "enrich" | "supersede" => hash(&v["payloadHash"]),
                    _ => record(&v["record"]) && (id.is_empty() || v["record"]["recordId"] == id),
                };
            if !valid {
                return fail(if write(op) {
                    "operation outcome unknown; malformed reply; reconcile corporate record/history before retrying"
                } else {
                    "Invalid corporate record response"
                });
            }
            let projected = match op {
                "list" => {
                    json!({"ok":true,"rows":v["rows"],"count":v["count"],"nextCursor":v["nextCursor"]})
                }
                "download" => {
                    json!({"ok":true,"bytesBase64":v["bytesBase64"],"sha256":v["sha256"]})
                }
                "enrich" | "supersede" => json!({"ok":true,"payloadHash":v["payloadHash"]}),
                _ => json!({"ok":true,"record":v["record"]}),
            };
            emit(projected, json_mode, op)
        }
    }
}
