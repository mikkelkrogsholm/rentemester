//! Original six-command workspace evidence intake over the existing authenticated SDK.
use crate::dated_reports::Output;
use rentemester_client::{WorkspaceInboxMutation as Mutation, workspace_inbox_body_schema};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
const COMMANDS: &[&str] = &["list", "inspect", "status", "ingest", "assign", "complete"];
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "workspace-inbox")
}
pub fn help() -> Value {
    json!({"commands":["workspace-inbox list [--cursor 0 --limit 25]","workspace-inbox inspect --source-id ID","workspace-inbox status --source-id ID","workspace-inbox ingest --input JSONFILE --file BYTES --idempotency-key TEXT --confirm yes","workspace-inbox assign --source-id ID --target-company SLUG --confirm yes","workspace-inbox complete --source-id ID --target-company SLUG --confirm yes"],
    "connection":"Explicit --company SLUG or verified local company PATH; --server ORIGIN or --local-profile ABS_PRIVATE_PROFILE; --cookie-file PRIVATE_FILE; --allow-loopback-http; --json or --format human|json. Optional --workspace ABS_PATH asserts local-profile workspace equality only.",
    "authority":"Human cookie only; service-key profiles and --service-key-file unsupported. Optional write --actor user:ID asserts current api.me user equality; never forwarded authority. No stdin, environment authority, generated keys or retries.",
    "boundaries":"Native upload only: original mail/connector are unsupported, never relabelled. Native filename maximum255 UTF16 plus Prepared rules (original512), no rename/truncation. Strict canonical cursor>=0, limit1..100, not legacy clamping. Replay requires full intent/current principal equality, not merely legacy byte SHA. Empty staged metadata is legal; completion still requires document facts and server policy. No journal posting or provider effects. Unknown writes require canonical source/history readback before retry.",
    "input":"Exact native ingest JSON fields excluding confirm/bytesBase64/idempotencyKey and authority; filename,mimeType,transport=upload,receivedAt,metadata required; optional sourceId,transportIdentity,candidates. Exact original bytes1..25MiB; metadata64KiB/depth32; body35MiB. Server owns all visibility, assignment, sensitivity, backup, transfer and handoff policy."})
}
fn usage(message: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{message}; see workspace-inbox --help\n"),
        exit: 2,
    }
}
pub fn binding_refusal(_: &[String]) -> Output {
    usage("Explicit verified company and transport binding required")
}
fn write(op: &str) -> bool {
    matches!(op, "ingest" | "assign" | "complete")
}
fn action(op: &str, id: &str) -> Mutation {
    match op {
        "assign" => Mutation::Assign(id.into()),
        "complete" => Mutation::Complete(id.into()),
        _ => Mutation::Ingest,
    }
}
fn text(v: &str, max: usize) -> bool {
    !v.trim().is_empty() && v.encode_utf16().count() <= max && !v.chars().any(char::is_control)
}
fn parse(args: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    if args == ["workspace-inbox"] || args == ["workspace-inbox", "--help"] {
        return Ok(BTreeMap::from([("--help", "true")]));
    }
    let op = args
        .get(1)
        .map(String::as_str)
        .filter(|s| COMMANDS.contains(s))
        .ok_or_else(|| usage("Unknown workspace-inbox command"))?;
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
                "ingest" => matches!(k, "--input" | "--file" | "--idempotency-key"),
                "list" => matches!(k, "--cursor" | "--limit"),
                "assign" | "complete" => matches!(k, "--source-id" | "--target-company"),
                _ => k == "--source-id",
            };
        if !allowed {
            return Err(usage("Unknown or unsupported workspace-inbox flag"));
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
        ("--source-id", 100),
        ("--target-company", 128),
        ("--idempotency-key", 200),
        ("--actor", 160),
    ] {
        if let Some(v) = f.get(k) {
            if !text(v, max)
                || k == "--source-id" && matches!(*v, "." | "..")
                || k == "--target-company"
                    && !v
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
            {
                return Err(usage("Invalid bounded command value"));
            }
        }
    }
    for k in ["--cursor", "--limit"] {
        if let Some(v) = f.get(k) {
            let n = v
                .parse::<usize>()
                .map_err(|_| usage("Invalid pagination"))?;
            if n.to_string() != *v || k == "--limit" && !(1..=100).contains(&n) {
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
        "ingest" => &["--input", "--file", "--idempotency-key"],
        "list" => &[],
        "assign" | "complete" => &["--source-id", "--target-company"],
        _ => &["--source-id"],
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
fn hash(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn nonempty(v: &Value) -> bool {
    v.as_str().is_some_and(|s| !s.trim().is_empty())
}
fn nullable(v: Option<&Value>) -> bool {
    v.is_some_and(|v| v.is_null() || nonempty(v))
}
fn source(v: &Value) -> bool {
    [
        "sourceId",
        "visibilityAnchorSlug",
        "filename",
        "mimeType",
        "receivedAt",
        "createdBy",
        "createdAt",
    ]
    .iter()
    .all(|k| nonempty(&v[k]))
        && hash(&v["sha256"])
        && matches!(
            v["transport"].as_str(),
            Some("upload" | "mail" | "connector")
        )
        && nullable(v.get("transportIdentity"))
        && v["metadata"].is_object()
        && v.get("exception").is_some_and(|e| {
            e.is_null()
                || ["code", "requiredAction", "openedAt"]
                    .iter()
                    .all(|k| nonempty(&e[k]))
                    && nullable(e.get("resolvedAt"))
        })
        && v["assignments"].as_array().is_some_and(|a| {
            a.len() <= 1
                && a.iter().all(|a| {
                    nonempty(&a["companySlug"])
                        && nonempty(&a["assignedBy"])
                        && nonempty(&a["assignedAt"])
                        && match a["state"].as_str() {
                            Some("approved") => {
                                a.get("documentId") == Some(&Value::Null)
                                    && a.get("documentNo") == Some(&Value::Null)
                                    && a.get("completedAt") == Some(&Value::Null)
                            }
                            Some("completed") => {
                                a["documentId"].as_u64().is_some_and(|n| n > 0)
                                    && nonempty(&a["documentNo"])
                                    && nonempty(&a["completedAt"])
                            }
                            _ => false,
                        }
                })
        })
        && v["history"].as_array().is_some_and(|h| {
            !h.is_empty()
                && h.iter().all(|e| {
                    ["event_type", "actor", "created_at"]
                        .iter()
                        .all(|k| nonempty(&e[k]))
                        && hash(&e["payload_hash"])
                        && e["payload"].is_object()
                        && (e["event_type"] != "ingested"
                            || e["payload"]["sourceHash"] == v["sha256"])
                        && (e["event_type"] != "handoff_completed"
                            || e["payload"]["sourceHash"] == v["sha256"])
                })
        })
}
fn depth(v: &Value, n: usize) -> bool {
    n <= 32
        && match v {
            Value::Array(a) => a.iter().all(|v| depth(v, n + 1)),
            Value::Object(o) => o.values().all(|v| depth(v, n + 1)),
            _ => true,
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
    let label = format!("workspace-inbox {op}");
    if exit == 0 {
        Output {
            stdout: format!(
                "✔ {label}\n{}",
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
                "✘ {label} failed\n{}",
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
    let id = f.get("--source-id").copied().unwrap_or("");
    let mutation = action(op, id);
    let mut body = json!({"confirm":true});
    let mut expected_sha = None;
    if op == "ingest" {
        let bytes = match file(f["--input"], rentemester_client::MAX_WORKSPACE_INBOX_BODY) {
            Ok(b) => b,
            Err(o) => return o,
        };
        body = match serde_json::from_slice::<Value>(&bytes) {
            Ok(v) if v.is_object() => v,
            _ => return usage("Input must be a JSON metadata object"),
        };
        // The closed native schema refuses all unknown authority and business fields rather than silently discarding them.
        let mut schema = workspace_inbox_body_schema(&mutation);
        schema["required"]
            .as_array_mut()
            .unwrap()
            .retain(|v| v != "bytesBase64" && v != "confirm" && v != "idempotencyKey");
        for k in ["confirm", "bytesBase64", "idempotencyKey"] {
            schema["properties"].as_object_mut().unwrap().remove(k);
        }
        if !shape(&body, &schema) {
            return usage(
                "Input violates native upload schema; no authority overrides or mail/connector projection",
            );
        }
        if !body["metadata"].is_object()
            || !depth(&body["metadata"], 0)
            || serde_json::to_vec(&body["metadata"]).unwrap().len() > 64 * 1024
        {
            return usage("Metadata must be an object within64KiB/depth32");
        }
        let bytes = match file(f["--file"], 25 * 1024 * 1024) {
            Ok(b) if !b.is_empty() => b,
            Ok(_) => return usage("Original bytes required"),
            Err(o) => return o,
        };
        expected_sha = Some(rentemester_storage::invoice_email::sha(&bytes));
        body["bytesBase64"] = json!(encode(&bytes));
        body["idempotencyKey"] = json!(f["--idempotency-key"]);
        body["confirm"] = json!(true);
    } else if write(op) {
        body["companySlug"] = json!(f["--target-company"]);
    }
    let wire = serde_json::to_vec(&body).unwrap();
    if write(op)
        && (!shape(&body, &workspace_inbox_body_schema(&mutation))
            || wire.len() > rentemester_client::MAX_WORKSPACE_INBOX_BODY)
    {
        return usage("Native inbox body schema or35MiB bound exceeded");
    }
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
    let mut prior_sha = None;
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
        if matches!(op, "assign" | "complete") {
            let prior = api.workspace_inbox_source(f["--company"], id).await?;
            if prior["ok"] != true
                || !source(&prior["source"])
                || prior["source"]["sourceId"] != id
                || prior["source"]["visibilityAnchorSlug"] != f["--company"]
            {
                return Err(rentemester_client::Error::Protocol);
            }
            prior_sha = prior["source"]["sha256"].as_str().map(str::to_owned);
        }
        match op {
            "list" => {
                api.workspace_inbox(
                    f["--company"],
                    f.get("--cursor").and_then(|v| v.parse().ok()).unwrap_or(0),
                    f.get("--limit").and_then(|v| v.parse().ok()).unwrap_or(25),
                )
                .await
            }
            "inspect" | "status" => api.workspace_inbox_source(f["--company"], id).await,
            _ => {
                api.mutate_workspace_inbox(f["--company"], mutation, wire, true)
                    .await
            }
        }
    });
    match result {
        Err(rentemester_client::Error::InvalidInput(_)) => {
            usage("Actor assertion or native input refused")
        }
        Err(rentemester_client::Error::Refused { status: 404 }) => {
            fail("workspace inbox source not found")
        }
        Err(rentemester_client::Error::OutcomeUnknown) => fail(
            "operation outcome unknown; read back canonical source/history and assignments before retrying",
        ),
        Err(_) => fail(if write(op) {
            "operation outcome unknown or refused; read back canonical source/history before retrying; transport, authorization or protocol refused"
        } else {
            "Workspace inbox transport, authorization or protocol refused"
        }),
        Ok(v) => {
            if v["ok"] == false
                && v["errors"]
                    .as_array()
                    .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
            {
                return emit(json!({"ok":false,"errors":v["errors"]}), json_mode, op);
            }
            let valid = v["ok"] == true
                && if op == "list" {
                    v["rows"].as_array().is_some_and(|a| {
                        a.iter()
                            .all(|s| source(s) && s["visibilityAnchorSlug"] == f["--company"])
                    }) && v["count"].as_u64().is_some_and(|n| {
                        n >= v["rows"].as_array().map_or(usize::MAX, Vec::len) as u64
                    }) && v
                        .get("nextCursor")
                        .is_some_and(|v| v.is_null() || v.is_u64())
                } else {
                    let s = &v["source"];
                    source(s)
                        && s["visibilityAnchorSlug"] == f["--company"]
                        && (id.is_empty() || s["sourceId"] == id)
                        && expected_sha.as_ref().is_none_or(|sha| s["sha256"] == *sha)
                        && prior_sha.as_ref().is_none_or(|sha| s["sha256"] == *sha)
                        && (op != "ingest"
                            || (body
                                .get("sourceId")
                                .is_none_or(|id| id.is_null() || *id == s["sourceId"])
                                && ["filename", "mimeType", "transport", "metadata"]
                                    .iter()
                                    .all(|k| body[k] == s[k])))
                        && (!matches!(op, "assign" | "complete")
                            || s["assignments"].as_array().is_some_and(|a| {
                                a.iter().any(|a| {
                                    a["companySlug"] == f["--target-company"]
                                        && (a["state"] == "completed"
                                            || op == "assign" && a["state"] == "approved")
                                })
                            }))
                };
            if !valid {
                return fail(if write(op) {
                    "operation outcome unknown; malformed or mismatched reply; read back canonical source/history and assignments before retrying"
                } else {
                    "Invalid workspace inbox response"
                });
            }
            emit(
                if op == "list" {
                    json!({"ok":true,"rows":v["rows"],"count":v["count"],"nextCursor":v["nextCursor"]})
                } else {
                    json!({"ok":true,"source":v["source"]})
                },
                json_mode,
                op,
            )
        }
    }
}
