//! Original explicit merge commands; current server cookie authority, no local writer.
use crate::dated_reports::Output;
use rentemester_client::PartyMutation;
use serde_json::{Value, json};
use std::collections::BTreeMap;
const COMMANDS: &[&str] = &["propose-merge", "approve-merge"];
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|v| v == "party")
}
pub fn help() -> Value {
    json!({"commands":["party propose-merge --from-party-id ID --into-party-id ID --review-assertion TEXT --confirm yes", "party approve-merge --from-party-id ID --proposal-hash SHA --confirm yes"],"connection":"Explicit --company SLUG or verified local company PATH; --server ORIGIN or --local-profile ABS_PRIVATE_PROFILE; --cookie-file PRIVATE_FILE; --allow-loopback-http; --json or --format human|json", "authority":"Human cookie only. Optional --actor user:ID asserts current cookie identity; --workspace is an equality assertion only with --local-profile. Both full writable party scopes required. No stdin, inferred merge, identity rewrite, retries or generated keys/hashes. Same human may propose and approve."})
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
        ) || matches!(k, "--actor" | "--confirm" | "--from-party-id")
            || if op == "propose-merge" {
                matches!(k, "--into-party-id" | "--review-assertion")
            } else {
                k == "--proposal-hash"
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
    for k in ["--from-party-id", "--into-party-id", "--actor"] {
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
    if f.get("--review-assertion")
        .is_some_and(|v| v.trim().is_empty() || v.encode_utf16().count() > 500)
    {
        return Err(usage("Review assertion required within500 UTF16 units"));
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
    let required: &[&str] = if op == "propose-merge" {
        &["--from-party-id", "--into-party-id", "--review-assertion"]
    } else {
        &["--from-party-id", "--proposal-hash"]
    };
    if required.iter().any(|k| !f.contains_key(k)) || f.get("--confirm") != Some(&"yes") {
        return Err(usage("Missing command value or exact --confirm yes"));
    }
    Ok(f)
}
pub fn preflight(args: &[String]) -> Result<bool, Output> {
    parse(args).map(|f| f.contains_key("--help"))
}
fn label(op: &str) -> &'static str {
    if op == "propose-merge" {
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
            stdout: format!("✔ {}\n", {
                let l = label(op);
                if l.chars().count() > 60 {
                    format!("{}…", l.chars().take(57).collect::<String>().trim_end())
                } else {
                    l.into()
                }
            }),
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
