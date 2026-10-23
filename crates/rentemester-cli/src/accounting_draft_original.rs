//! Seven source draft commands. Input is a bounded local file, never a server path.
use crate::dated_reports::Output;
use rentemester_client::{
    AccountingDraftOriginalOperation as Op, original_accounting_draft_file,
    validate_original_accounting_draft,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal};
fn operation(s: &str) -> Option<Op> {
    match s {
        "create" => Some(Op::Create),
        "revise" => Some(Op::Revise),
        "submit" => Some(Op::Submit),
        "reject" => Some(Op::Reject),
        "approve-and-post" => Some(Op::ApproveAndPost),
        "show" => Some(Op::Show),
        "list" => Some(Op::List),
        _ => None,
    }
}
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "accounting-draft")
        && args.get(1).and_then(|s| operation(s)).is_some()
}
pub fn help() -> Value {
    json!({"ok":true,"commands":["accounting-draft create --draft-id ID --input JSON-FILE --confirm yes","accounting-draft revise --draft-id ID --expected-event-hash HEX64 --input JSON-FILE --confirm yes","accounting-draft submit --draft-id ID --expected-event-hash HEX64 --confirm yes","accounting-draft reject --draft-id ID --expected-event-hash HEX64 --reason TEXT --confirm yes","accounting-draft approve-and-post --draft-id ID --expected-event-hash HEX64 --confirm yes","accounting-draft show --draft-id ID","accounting-draft list"],"connection":"--company SLUG or verified company path; --server ORIGIN or --local-profile PATH; --cookie-file PATH or --service-key-file PATH; --allow-loopback-http; --json or --format json|human","authority":"All five writes now require explicit --confirm yes (including the four formerly unguarded source writes). --actor and --principal only assert current typed credential identity, never historical read actors or grants. No automatic retry; reconcile unknown outcomes using show/list. --input is a local bounded unique-key JSON object file, not inline JSON or a server path."})
}
fn usage(s: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{s}\n"),
        exit: 2,
    }
}
fn parse(args: &[String]) -> Result<(BTreeMap<&str, &str>, Value), Output> {
    let op = args
        .get(1)
        .and_then(|s| operation(s))
        .ok_or_else(|| usage("Unknown accounting-draft command"))?;
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
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
        let domain = match k {
            "--draft-id" => op != Op::List,
            "--expected-event-hash" => matches!(
                op,
                Op::Revise | Op::Submit | Op::Reject | Op::ApproveAndPost
            ),
            "--input" => matches!(op, Op::Create | Op::Revise),
            "--reason" => op == Op::Reject,
            "--confirm" | "--actor" | "--principal" => op.write(),
            _ => false,
        };
        if !global && !domain {
            return Err(usage(&format!(
                "Unknown or irrelevant accounting-draft flag {k}"
            )));
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
            return Err(usage("Duplicate accounting-draft flag"));
        }
        cursor = next;
    }
    if flags
        .get("--format")
        .is_some_and(|s| !matches!(*s, "json" | "human"))
    {
        return Err(usage("--format must be json or human"));
    }
    if flags.contains_key("--help") {
        return Ok((flags, json!({})));
    }
    let mut body = json!({});
    for (flag, key) in [
        ("--draft-id", "draftId"),
        ("--expected-event-hash", "expectedEventHash"),
        ("--reason", "reason"),
    ] {
        if let Some(v) = flags.get(flag) {
            body[key] = json!(v);
        }
    }
    if op.write() {
        if flags.get("--confirm") != Some(&"yes") {
            return Err(usage("Accounting draft writes require exact --confirm yes"));
        }
        body["confirm"] = json!(true);
    }
    for key in ["--actor", "--principal"] {
        if let Some(a) = flags.get(key)
            && (a.len() > 256
                || a.chars().any(char::is_control)
                || !["user:", "service_account:"].iter().any(|p| {
                    a.strip_prefix(p)
                        .is_some_and(|s| !s.is_empty() && s.trim() == s)
                }))
        {
            return Err(usage(
                "Current typed credential equality assertion required",
            ));
        }
    }
    // Validate all flags and selectors before opening the payload file, and all local input before credentials.
    if matches!(op, Op::Create | Op::Revise) {
        body["payload"] =
            json!({"transactionDate":"2026-01-01","text":"Preflight placeholder","lines":[]});
    }
    validate_original_accounting_draft(op, &body).map_err(|e| usage(&e.to_string()))?;
    if matches!(op, Op::Create | Op::Revise) {
        let path = flags
            .get("--input")
            .filter(|s| !s.is_empty())
            .ok_or_else(|| usage("--input JSON-FILE required"))?;
        body["payload"] = original_accounting_draft_file(std::path::Path::new(path))
            .map_err(|e| usage(&e.to_string()))?;
    }
    validate_original_accounting_draft(op, &body).map_err(|e| usage(&e.to_string()))?;
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
    let result = runtime.block_on(api.original_accounting_draft(
        flags["--company"],
        operation(&args[1]).unwrap(),
        &body,
        flags.get("--actor").copied(),
        flags.get("--principal").copied(),
    ));
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
fn render(v: Value, json_mode: bool, exit: u8, _op: &str) -> Output {
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
            stdout: format!("{}\n", v),
            stderr: String::new(),
            exit,
        };
    }
    let mut lines = Vec::new();
    if exit != 0 {
        if let Some(a) = v["errors"].as_array() {
            for e in a {
                if let Some(s) = e.as_str() {
                    lines.push(s.to_owned());
                }
            }
        }
    } else {
        let rows = if let Some(a) = v["accountingDrafts"].as_array() {
            a.clone()
        } else if v["accountingDraft"].is_null() {
            vec![]
        } else {
            vec![v["accountingDraft"].clone()]
        };
        if rows.is_empty() {
            lines.push("No accounting drafts found.".into());
        }
        for s in rows {
            lines.push(format!(
                "Draft {} — version {}, {}",
                s["id"].as_str().unwrap_or("?"),
                s["version"],
                s["status"].as_str().unwrap_or("?")
            ));
            lines.push(format!(
                "Event hash: {}",
                s["eventHash"].as_str().unwrap_or("?")
            ));
            lines.push(format!(
                "Payload hash: {}",
                s["payloadHash"].as_str().unwrap_or("?")
            ));
            if let Some(id) = s.get("journalEntryId") {
                lines.push(format!("Posted journal: {id}"));
            }
            if let Some(reason) = s["reason"].as_str() {
                lines.push(format!("Reason: {reason}"));
            }
        }
    }
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
