//! Seven original commands; local selectors never enter HTTP bodies.
use crate::dated_reports::Output;
use rentemester_client::{
    DocumentPartyOriginalOperation as Op, document_party_original_schema,
    document_party_original_validate,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal};
const COMMANDS: &[&str] = &[
    "party-link-plan",
    "party-link-apply",
    "party-link-inspect",
    "party-link-list",
    "party-link-supersede",
    "internal-no-external-party",
    "internal-no-external-party-supersede",
];
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "documents")
        && args.get(1).is_some_and(|s| COMMANDS.contains(&s.as_str()))
}
fn operation(s: &str) -> Option<Op> {
    match s {
        "party-link-plan" => Some(Op::Plan),
        "party-link-apply" => Some(Op::Apply),
        "party-link-supersede" => Some(Op::Supersede),
        "internal-no-external-party" => Some(Op::Internal),
        "internal-no-external-party-supersede" => Some(Op::InternalSupersede),
        _ => None,
    }
}
pub fn help() -> Value {
    json!({"ok":true,"commands":["documents party-link-plan --document-id ID --role ROLE [--party-id ID] [--jurisdiction DK --identifier-kind dk_cvr|eu_vat|non_eu --identifier TEXT | --legacy-kind customer|vendor --legacy-id ID --reviewed-legacy-reference TEXT]","documents party-link-apply (same evidence flags) --plan-hash HEX64 [--idempotency-key KEY] --confirm yes","documents party-link-inspect --document-id ID","documents party-link-list [--status linked|unlinked|resolved|internal_no_external_party|unresolved]","documents party-link-supersede --document-id ID --role ROLE --plan-hash HEX64 --reason TEXT --confirm yes","documents internal-no-external-party --document-id ID --reason TEXT [--idempotency-key KEY] --confirm yes","documents internal-no-external-party-supersede --document-id ID --decision-hash HEX64 --reason TEXT --confirm yes"],"connection":"--company SLUG or verified company path with --local-profile PATH; --server ORIGIN; --cookie-file PATH or --service-key-file PATH; --allow-loopback-http; --json or --format json|human","authority":"--actor and --principal are optional exact equality assertions on the CURRENT server credential identity, never grants. No environment-inferred actor. Human identity is user:{subject}; service identity is service_account:{subject} from typed server admission. --company-slug asserts the resolved company. Optional --workspace requires --local-profile and only asserts that profile's workspace; no local or remote storage is selected by it. Plan is read-only. Four writes require exact --confirm yes; no automatic retry."})
}
pub fn family_help() -> Value {
    let mut all = crate::documents_original::help();
    let party = help();
    all["commands"]
        .as_array_mut()
        .unwrap()
        .extend(party["commands"].as_array().unwrap().iter().cloned());
    all["partyLinkAuthority"] = party["authority"].clone();
    all
}
fn usage(s: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{s}\n"),
        exit: 2,
    }
}
const FIELDS: &[(&str, &str)] = &[
    ("--document-id", "documentId"),
    ("--role", "role"),
    ("--party-id", "partyId"),
    ("--jurisdiction", "jurisdiction"),
    ("--identifier-kind", "identifierKind"),
    ("--identifier", "identifier"),
    ("--legacy-kind", "legacyKind"),
    ("--legacy-id", "legacyId"),
    ("--reviewed-legacy-reference", "reviewedLegacyReference"),
    ("--plan-hash", "planHash"),
    ("--decision-hash", "decisionHash"),
    ("--reason", "reason"),
    ("--idempotency-key", "idempotencyKey"),
    ("--actor", "expectedActor"),
    ("--principal", "expectedPrincipal"),
];
fn parse(args: &[String]) -> Result<(BTreeMap<&str, &str>, Value), Output> {
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
    let op = operation(&args[1]);
    let schema = op.map(document_party_original_schema);
    while cursor < args.len() {
        let (k, attached) = crate::original_args::option(&args[cursor]);
        let global = matches!(
            k,
            "--company"
                | "--company-slug"
                | "--workspace"
                | "--local-profile"
                | "--server"
                | "--cookie-file"
                | "--service-key-file"
                | "--allow-loopback-http"
                | "--json"
                | "--format"
                | "--help"
        );
        let domain = if let Some(schema) = &schema {
            k == "--confirm" && op != Some(Op::Plan)
                || FIELDS
                    .iter()
                    .any(|(flag, key)| *flag == k && schema["properties"].get(*key).is_some())
        } else if args[1] == "party-link-list" {
            k == "--status"
        } else {
            k == "--document-id"
        };
        if !global && !domain {
            return Err(usage(&format!("Unknown document party flag {k}")));
        }
        let (v, next) = if matches!(k, "--json" | "--help" | "--allow-loopback-http") {
            if attached.is_some() {
                return Err(usage("Boolean flag takes no value"));
            }
            ("true", cursor + 1)
        } else {
            crate::original_args::value(args, cursor).ok_or_else(|| usage("Missing flag value"))?
        };
        if flags.insert(k, v).is_some() {
            return Err(usage("Duplicate document party flag"));
        }
        cursor = next;
    }
    if flags
        .get("--format")
        .is_some_and(|v| !matches!(*v, "json" | "human"))
    {
        return Err(usage("--format must be json or human"));
    }
    if flags.contains_key("--help") {
        return Ok((flags, json!({})));
    }
    let mut body = json!({});
    for (flag, key) in FIELDS {
        if let Some(v) = flags.get(flag) {
            body[*key] = if *key == "documentId" {
                json!(
                    v.parse::<i64>()
                        .ok()
                        .filter(|n| (1..=9_007_199_254_740_991).contains(n))
                        .ok_or_else(|| usage("positive exact document id required"))?
                )
            } else {
                json!(v)
            };
        }
    }
    if let Some(op) = op {
        if op != Op::Plan {
            if flags.get("--confirm") != Some(&"yes") {
                return Err(usage("Document party write requires exact --confirm yes"));
            }
            body["confirm"] = json!(true);
        }
        document_party_original_validate(op, &serde_json::to_vec(&body).unwrap())
            .map_err(|e| usage(&e.to_string()))?;
    } else if args[1] == "party-link-inspect" && body.get("documentId").is_none() {
        return Err(usage("--document-id required"));
    }
    if flags.get("--status").is_some_and(|v| {
        ![
            "linked",
            "unlinked",
            "resolved",
            "internal_no_external_party",
            "unresolved",
        ]
        .contains(v)
    }) {
        return Err(usage("Invalid resolution status"));
    }
    Ok((flags, body))
}
pub fn preflight(args: &[String]) -> Result<(), Output> {
    let (flags, _) = parse(args)?;
    if !flags.contains_key("--help") {
        if let Some(workspace) = flags.get("--workspace") {
            let profile=flags.get("--local-profile").ok_or_else(||usage("--workspace is only a local profile equality assertion; requires --local-profile"))?;
            let local = rentemester_client::LocalRuntime::load(std::path::Path::new(profile))
                .map_err(|_| usage("Could not validate local profile"))?;
            if std::path::Path::new(workspace) != local.workspace_root() {
                return Err(usage(
                    "--workspace differs from verified local profile workspace",
                ));
            }
        }
    }
    Ok(())
}
pub fn execute(args: &[String]) -> Output {
    let (flags, body) = match parse(args) {
        Ok(v) => v,
        Err(o) => return o,
    };
    if flags.contains_key("--help") {
        return Output {
            stdout: format!("{}\n", help()),
            stderr: String::new(),
            exit: 0,
        };
    }
    for k in ["--server", "--company"] {
        if flags.get(k).is_none_or(|s| s.is_empty()) {
            return usage(&format!("Explicit {k} required"));
        }
    }
    if flags
        .get("--company-slug")
        .is_some_and(|s| *s != flags["--company"])
    {
        return usage("--company-slug differs from resolved company");
    }
    let json_mode = flags.contains_key("--json")
        || flags.get("--format") == Some(&"json")
        || (flags.get("--format") != Some(&"human") && !std::io::stdout().is_terminal());
    let api = match crate::remote::credential_client(
        flags["--server"],
        flags.contains_key("--allow-loopback-http"),
        &flags,
        false,
    ) {
        Ok(v) => v,
        Err((exit, e)) => {
            return render(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
                &args[1],
            );
        }
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(v) => v,
        Err(_) => {
            return render(
                json!({"ok":false,"errors":["HTTP runtime unavailable"]}),
                json_mode,
                1,
                &args[1],
            );
        }
    };
    let result = runtime.block_on(async {
        let c = flags["--company"];
        match operation(&args[1]) {
            Some(op) => {
                api.document_party_original(
                    c,
                    op,
                    serde_json::to_vec(&body).unwrap(),
                    op != Op::Plan,
                )
                .await
            }
            None if args[1] == "party-link-list" => {
                api.document_party_original_list(c, flags.get("--status").copied())
                    .await
            }
            None => {
                api.document_party_original_inspect(c, body["documentId"].as_i64().unwrap())
                    .await
            }
        }
    });
    match result {
        Ok(v) => {
            let exit = if v["ok"] == true { 0 } else { 1 };
            render(v, json_mode, exit, &args[1])
        }
        Err(e) => {
            let (exit, e) = crate::remote::client_error(e);
            render(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
                &args[1],
            )
        }
    }
}
fn heading(op: &str) -> &str {
    match op {
        "party-link-plan" => "Planlægger en ikke-mutérende, evidensbundet partskobling.",
        "party-link-apply" => "Anvender en eksakt, bekræftet dokument-part-plan.",
        "party-link-inspect" => "Viser aktuel part og provenance for et bilag.",
        "party-link-list" => "Lister bilag med eller uden aktuel partskobling.",
        "party-link-supersede" => "Superseder append-only en aktuel dokument-part-kobling.",
        "internal-no-external-party" => {
            "Bekræfter at et internt bilag bevidst ikke har ekstern modpart."
        }
        _ => "Supersederer den eksakte aktuelle ingen-ekstern-modpart-beslutning.",
    }
}
fn scalars(v: &Value, lines: &mut Vec<String>, indent: usize) {
    if let Some(o) = v.as_object() {
        for (k, v) in o {
            match v {
                Value::String(s) => lines.push(format!("{}{}: {}", " ".repeat(indent), k, s)),
                Value::Number(n) => lines.push(format!("{}{}: {}", " ".repeat(indent), k, n)),
                Value::Bool(b) => lines.push(format!("{}{}: {}", " ".repeat(indent), k, b)),
                Value::Object(_) => {
                    lines.push(format!("{}{}", " ".repeat(indent), k));
                    scalars(v, lines, indent + 2);
                }
                Value::Array(a) => {
                    lines.push(format!("{}{} ({})", " ".repeat(indent), k, a.len()));
                    for r in a {
                        if let Some(s) = r.as_str() {
                            lines.push(format!("{}→ {}", " ".repeat(indent + 2), s));
                        } else {
                            scalars(r, lines, indent + 2);
                        }
                    }
                }
                Value::Null => {}
            }
        }
    }
}
fn render(v: Value, json_mode: bool, exit: u8, op: &str) -> Output {
    if exit == 2 {
        return usage(
            v["errors"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(Value::as_str)
                .unwrap_or("Invalid invocation"),
        );
    }
    if json_mode {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&v).unwrap()),
            stderr: String::new(),
            exit,
        };
    }
    let mut lines = vec![format!(
        "{} {}",
        if exit == 0 { "✔" } else { "✘" },
        heading(op)
    )];
    scalars(&v, &mut lines, 2);
    let text = format!("{}\n", lines.join("\n"));
    Output {
        stdout: if exit == 0 {
            text.clone()
        } else {
            String::new()
        },
        stderr: if exit != 0 { text } else { String::new() },
        exit,
    }
}
