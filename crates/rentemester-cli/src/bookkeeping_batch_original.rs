//! Flat source argv; native cookie authority and atomic receipts remain authoritative.
use crate::dated_reports::Output;
use rentemester_client::{OriginalBatchOperation as Op, validate_bookkeeping_batch_original};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal};
fn operation(s: &str) -> Option<Op> {
    Some(match s {
        "plan" => Op::Plan,
        "persist" => Op::Persist,
        "dry-run" => Op::DryRun,
        "approve" => Op::Approve,
        "apply" => Op::Apply,
        "status" => Op::Status,
        _ => return None,
    })
}
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "bookkeeping-batch")
        && args.get(1).and_then(|s| operation(s)).is_some()
}
pub fn help() -> Value {
    json!({"ok":true,"commands":["bookkeeping-batch plan --accounting-from DATE --accounting-to DATE --bank-from DATE --bank-to DATE","bookkeeping-batch persist --accounting-from DATE --accounting-to DATE --bank-from DATE --bank-to DATE --run-key KEY --expected-plan-hash HASH --confirm yes","bookkeeping-batch dry-run (same flags as persist)","bookkeeping-batch approve --run-id ID --plan-hash HASH --confirm yes","bookkeeping-batch apply --run-id ID --plan-hash HASH --confirm yes","bookkeeping-batch status --run-id ID"],"connection":"--company SLUG or verified company path; --server ORIGIN or --local-profile PATH; --cookie-file PATH; --json or --format json|human","authority":"Current human cookie only; service usage unsupported. Optional --actor is equality assertion against current api.me user only. Optional --company-id is a selected company assertion, never a selector or default. All four writes require --confirm yes and reviewed hash, no automatic refresh/retry. Durable dry-run writes immutable reviewed state; only plan/status are read-only.","output":"Native atomic_v1 revisions/approvals/attempts/receipts/finalChecks and inspect-only legacy history retained. Apply returns actual per-item receipts and duplicate metadata, not partial resume events."})
}
fn usage(s: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("invalid_input: {s}\n"),
        exit: 2,
    }
}
fn parse(args: &[String]) -> Result<(BTreeMap<&str, &str>, Value), Output> {
    let op = args
        .get(1)
        .and_then(|s| operation(s))
        .ok_or_else(|| usage("Unknown bookkeeping-batch command"))?;
    let scope = matches!(op, Op::Plan | Op::Persist | Op::DryRun);
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
    while cursor < args.len() {
        if args[cursor] == "--" {
            if cursor + 1 == args.len() {
                break;
            }
            return Err(usage("No positional input accepted"));
        }
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
                | "--example"
        );
        let domain = match k {
            "--accounting-from" | "--accounting-to" | "--bank-from" | "--bank-to"
            | "--company-id" => scope,
            "--run-key" | "--expected-plan-hash" => matches!(op, Op::Persist | Op::DryRun),
            "--run-id" => !scope,
            "--plan-hash" => matches!(op, Op::Approve | Op::Apply),
            "--actor" | "--confirm" => op.write(),
            _ => false,
        };
        if !global && !domain {
            return Err(usage(&format!("Unknown or irrelevant flag {k}")));
        }
        let (v, next) = if matches!(
            k,
            "--json" | "--help" | "--example" | "--allow-loopback-http"
        ) {
            if attached.is_some() {
                return Err(usage("Boolean flag takes no value"));
            }
            ("true", cursor + 1)
        } else {
            crate::original_args::value(args, cursor).ok_or_else(|| usage("Missing flag value"))?
        };
        if flags.insert(k, v).is_some() {
            return Err(usage("Duplicate bookkeeping-batch flag"));
        }
        cursor = next;
    }
    if flags
        .get("--format")
        .is_some_and(|s| !matches!(*s, "json" | "human"))
    {
        return Err(usage("--format must be json or human"));
    }
    if flags.contains_key("--help") || flags.contains_key("--example") {
        return Ok((flags, json!({})));
    }
    let mut input = json!({});
    for (flag, key) in [
        ("--accounting-from", "accountingFrom"),
        ("--accounting-to", "accountingTo"),
        ("--bank-from", "bankFrom"),
        ("--bank-to", "bankTo"),
        ("--run-key", "runKey"),
        ("--expected-plan-hash", "expectedPlanHash"),
        ("--plan-hash", "planHash"),
    ] {
        if let Some(v) = flags.get(flag) {
            input[key] = json!(v);
        }
    }
    for (flag, key) in [("--run-id", "runId"), ("--company-id", "companyId")] {
        if let Some(v) = flags.get(flag) {
            let n = v
                .parse::<u64>()
                .ok()
                .filter(|n| (1..=9007199254740991).contains(n))
                .ok_or_else(|| usage("Positive safe integer required"))?;
            if !v.bytes().all(|b| b.is_ascii_digit()) {
                return Err(usage("Positive safe integer required"));
            }
            input[key] = json!(n);
        }
    }
    if op.write() {
        if flags.get("--confirm") != Some(&"yes") {
            return Err(usage("All four batch writes require exact --confirm yes"));
        }
        input["confirm"] = json!(true);
    }
    if flags.get("--actor").is_some_and(|s| {
        s.len() > 256
            || !s.strip_prefix("user:").is_some_and(|v| {
                !v.is_empty() && !v.chars().any(|c| c.is_whitespace() || c.is_control())
            })
    }) {
        return Err(usage("--actor must assert current user:ID"));
    }
    validate_bookkeeping_batch_original(op, &input).map_err(|e| usage(&e.to_string()))?;
    if flags.contains_key("--service-key-file") {
        return Err(usage(
            "Bookkeeping batch requires current human cookie; service usage unsupported",
        ));
    }
    Ok((flags, input))
}
pub fn preflight(args: &[String]) -> Result<(), Output> {
    let (flags, _) = parse(args)?;
    if !flags.contains_key("--help")
        && !flags.contains_key("--example")
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
    let (flags, input) = match parse(args) {
        Ok(v) => v,
        Err(o) => return o,
    };
    if flags.contains_key("--help") || flags.contains_key("--example") {
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
        return usage("--company-slug differs from selected company");
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
            );
        }
    };
    let result = runtime.block_on(async {
        if let Some(actor) = flags.get("--actor") {
            let me = api.me().await?;
            let subject = me["user"]["id"]
                .as_str()
                .ok_or(rentemester_client::Error::Protocol)?;
            if *actor != format!("user:{subject}") {
                return Err(rentemester_client::Error::InvalidInput(
                    "actor differs from current cookie user",
                ));
            }
        }
        api.bookkeeping_batch_original(flags["--company"], operation(&args[1]).unwrap(), input)
            .await
    });
    match result {
        Ok(v) => {
            let exit = if v["ok"] == true { 0 } else { 1 };
            render(v, json_mode, exit)
        }
        Err(e) => {
            let (exit, e) = crate::remote::client_error(e);
            render(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
            )
        }
    }
}
fn render(v: Value, json_mode: bool, exit: u8) -> Output {
    if exit == 2 {
        return usage(
            v["errors"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(Value::as_str)
                .unwrap_or("Invalid invocation"),
        );
    }
    let text = if json_mode {
        format!("{v}\n")
    } else {
        format!(
            "Bookkeeping batch {}\n{}\n",
            if exit == 0 {
                "succeeded"
            } else {
                "refused or outcome unknown"
            },
            serde_json::to_string_pretty(&v).unwrap_or_default()
        )
    };
    Output {
        stdout: if json_mode || exit == 0 {
            text.clone()
        } else {
            String::new()
        },
        stderr: if !json_mode && exit != 0 {
            text
        } else {
            String::new()
        },
        exit,
    }
}
