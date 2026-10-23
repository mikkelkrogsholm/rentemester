//! Six original commands; local selectors never enter HTTP bodies.
use crate::dated_reports::Output;
use rentemester_client::{
    PostingRulesOriginalOperation as Op, original_posting_rules_schema,
    validate_original_posting_rules,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal};

pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "posting-rules")
}
fn operation(s: &str) -> Option<Op> {
    match s {
        "propose" => Some(Op::Propose),
        "approve" => Some(Op::Approve),
        "disable" => Some(Op::Disable),
        "supersede" => Some(Op::Supersede),
        "explain" => Some(Op::Explain),
        "test" => Some(Op::Test),
        _ => None,
    }
}
pub fn help() -> Value {
    json!({"ok":true,"commands":["posting-rules propose --input INLINE_PROPOSAL_JSON --confirm yes","posting-rules approve|disable|supersede --rule-id ID --version N --expected-payload-hash HEX64 --rationale TEXT --provenance TEXT [--effective-at ISO] --confirm yes","posting-rules explain|test --context INLINE_JSON [--at ISO]"],"authority":"Explicit --confirm yes is required for all four writes (deliberate correction of unguarded source CLI). --actor and --principal only assert current server credential identity, never grant authority; writes only. No automatic retry. Shared --company, --server, --local-profile, --cookie-file or --service-key-file, --allow-loopback-http and --json/--format json|human."})
}
fn usage(s: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{s}\n"),
        exit: 2,
    }
}
const FIELDS: &[(&str, &str)] = &[
    ("--input", "proposal"),
    ("--context", "context"),
    ("--rule-id", "ruleId"),
    ("--version", "version"),
    ("--expected-payload-hash", "expectedPayloadHash"),
    ("--rationale", "rationale"),
    ("--provenance", "provenance"),
    ("--effective-at", "effectiveAt"),
    ("--at", "at"),
];
fn parse(args: &[String]) -> Result<(BTreeMap<&str, &str>, Value), Output> {
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
    if args.len() < 2 {
        return Ok((BTreeMap::from([("--help", "true")]), json!({})));
    }
    if args[1] == "--help" {
        return Ok((BTreeMap::from([("--help", "true")]), json!({})));
    }
    let op = operation(&args[1]).ok_or_else(|| usage("Unknown posting-rules operation"))?;
    let schema = original_posting_rules_schema(op);
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
        let domain = k == "--confirm" && op.write()
            || matches!(k, "--actor" | "--principal") && op.write()
            || FIELDS
                .iter()
                .any(|(flag, key)| *flag == k && schema["properties"].get(*key).is_some());
        if !global && !domain {
            return Err(usage(&format!("Unknown posting-rule flag {k}")));
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
            return Err(usage("Duplicate posting-rule flag"));
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
            body[*key] = if *key == "version" {
                json!(
                    v.parse::<i64>()
                        .ok()
                        .filter(|n| (1..=9_007_199_254_740_991).contains(n))
                        .ok_or_else(|| usage("positive exact version required"))?
                )
            } else if matches!(*key, "proposal" | "context") {
                rentemester_client::parse_original_posting_rules_json(v).map_err(|_| {
                    usage("Expected strict inline JSON object without duplicate keys")
                })?
            } else {
                json!(v)
            };
        }
    }
    if op.write() {
        if flags.get("--confirm") != Some(&"yes") {
            return Err(usage("Posting-rule write requires exact --confirm yes"));
        }
        body["confirm"] = json!(true);
    }
    validate_original_posting_rules(op, &body).map_err(|e| usage(&e.to_string()))?;
    let mut wire = body.clone();
    for (flag, key) in [("--actor", "actor"), ("--principal", "principal")] {
        if let Some(value) = flags.get(flag) {
            if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
                return Err(usage("Invalid current credential equality assertion"));
            }
            wire[key] = json!(value);
        }
    }
    if serde_json::to_vec(&wire).unwrap().len() > 65_536 {
        return Err(usage("Posting-rule final body exceeds 65 KiB"));
    }
    Ok((flags, body))
}
pub fn preflight(args: &[String]) -> Result<(), Output> {
    let (flags, _) = parse(args)?;
    if !flags.contains_key("--help")
        && let Some(workspace) = flags.get("--workspace")
    {
        let profile = flags.get("--local-profile").ok_or_else(|| {
            usage(
                "--workspace is only a local profile equality assertion; requires --local-profile",
            )
        })?;
        let local = rentemester_client::LocalRuntime::load(std::path::Path::new(profile))
            .map_err(|_| usage("Could not validate local profile"))?;
        if std::path::Path::new(workspace) != local.workspace_root() {
            return Err(usage(
                "--workspace differs from verified local profile workspace",
            ));
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
    let result = runtime.block_on(api.original_posting_rules(
        flags["--company"],
        operation(&args[1]).unwrap(),
        &body,
        flags.get("--actor").copied(),
        flags.get("--principal").copied(),
    ));
    match result {
        Ok(v) => {
            let exit = if v["ok"] == false { 1 } else { 0 };
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
        "propose" => "Posting rule proposal/version",
        "approve" => "Posting rule approved",
        "disable" => "Posting rule disabled",
        "supersede" => "Posting rule superseded",
        _ => "Posting rule dry-run decision (no posting authority)",
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
