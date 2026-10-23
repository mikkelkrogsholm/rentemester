//! Original dimensions invocation and flat source results; native aliases stay separate.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal};

use rentemester_client::{DimensionsOriginalOperation as Op, validate_original_dimensions};
fn operation(name: &str) -> Option<Op> {
    Some(match name {
        "define" => Op::Define,
        "member" => Op::Member,
        "definition-lifecycle" => Op::DefinitionLifecycle,
        "member-lifecycle" => Op::MemberLifecycle,
        "definitions" => Op::Definitions,
        "members" => Op::Members,
        "plan" => Op::Plan,
        "apply" => Op::Apply,
        "replace" => Op::Replace,
        "supersede" => Op::Supersede,
        "list" => Op::List,
        "budget-plan" => Op::BudgetPlan,
        "budget-apply" => Op::BudgetApply,
        "budgets" => Op::Budgets,
        _ => return None,
    })
}
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "dimensions")
        && args.get(1).is_some_and(|s| operation(s).is_some())
}
pub fn help() -> Value {
    json!({"ok":true,"commands":[
"dimensions define --dimension-id ID --kind KIND --name NAME --confirm yes",
"dimensions member --dimension-id ID --member-id ID --name NAME [--status active|inactive --effective-from DATE --effective-to DATE] --confirm yes",
"dimensions definition-lifecycle --dimension-id ID --action activate|deactivate|rename|supersede [--name NAME --supersedes-id ID] --confirm yes",
"dimensions member-lifecycle --dimension-id ID --member-id ID --action activate|deactivate|rename|supersede [--name NAME --supersedes-id ID] --confirm yes",
"dimensions definitions", "dimensions members [--dimension-id ID]",
"dimensions plan --journal-line-id ID --allocations INLINE_JSON [--source reviewed|imported --reviewed-import yes|no --source-ref REF]",
"dimensions apply --journal-line-id ID --allocations INLINE_JSON --plan-hash HASH [--source reviewed|imported --reviewed-import yes|no --source-ref REF --idempotency-key KEY] --confirm yes",
"dimensions replace --journal-line-id ID --allocations INLINE_JSON --plan-hash HASH --expected-assignment-id ID --reason TEXT [--source reviewed|imported --reviewed-import yes|no --source-ref REF --idempotency-key KEY] --confirm yes",
"dimensions supersede --assignment-id ID --reason TEXT --confirm yes",
"dimensions list --journal-line-id ID",
"dimensions budget-plan --account ACCOUNT --period YYYY-MM --allocations INLINE_JSON --source-ref REF",
"dimensions budget-apply --account ACCOUNT --period YYYY-MM --allocations INLINE_JSON --source-ref REF --plan-hash HASH [--idempotency-key KEY] --confirm yes",
"dimensions budgets"],"connection":"--company SLUG --server ORIGIN or --local-profile PATH; --cookie-file PATH or --service-key-file PATH; --json or --format human","identity":"Optional write --actor and --principal assert equality with the current credential only; neither selects identity.","safety":"Immutable journal classification and reviewed dimension budgets. No automatic retry; reconcile uncertain writes with canonical reads."})
}
fn usage(message: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{message}\n"),
        exit: 2,
    }
}
fn parse(args: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
    while cursor < args.len() {
        let (key, attached) = crate::original_args::option(&args[cursor]);
        if !matches!(
            key,
            "--dimension-id"
                | "--member-id"
                | "--kind"
                | "--name"
                | "--status"
                | "--effective-from"
                | "--effective-to"
                | "--action"
                | "--supersedes-id"
                | "--journal-line-id"
                | "--allocations"
                | "--source"
                | "--reviewed-import"
                | "--source-ref"
                | "--plan-hash"
                | "--idempotency-key"
                | "--expected-assignment-id"
                | "--reason"
                | "--assignment-id"
                | "--account"
                | "--period"
                | "--confirm"
                | "--principal"
                | "--company"
                | "--server"
                | "--cookie-file"
                | "--service-key-file"
                | "--local-profile"
                | "--workspace"
                | "--actor"
                | "--json"
                | "--format"
                | "--help"
                | "--allow-loopback-http"
        ) {
            return Err(usage(&format!(
                "Unknown dimensions flag {key}; see dimensions --help"
            )));
        }
        let (raw, next) = if matches!(key, "--json" | "--help" | "--allow-loopback-http") {
            if attached.is_some() {
                return Err(usage("Boolean flag takes no value"));
            }
            ("true", cursor + 1)
        } else {
            crate::original_args::value(args, cursor).ok_or_else(|| usage("Missing flag value"))?
        };
        if flags.insert(key, raw).is_some() {
            return Err(usage("Empty or duplicate dimensions flag"));
        }
        cursor = next;
    }
    if flags
        .get("--format")
        .is_some_and(|s| !matches!(*s, "json" | "human"))
    {
        return Err(usage("--format must be json or human"));
    }
    if flags.contains_key("--cookie-file") && flags.contains_key("--service-key-file") {
        return Err(usage("Choose exactly one --cookie-file/--service-key-file"));
    }
    if flags.contains_key("--local-profile")
        && (flags.contains_key("--server") || flags.contains_key("--allow-loopback-http"))
    {
        return Err(usage(
            "--local-profile conflicts with --server/--allow-loopback-http",
        ));
    }
    if flags
        .get("--local-profile")
        .is_some_and(|s| !std::path::Path::new(s).is_absolute())
    {
        return Err(usage(
            "--local-profile requires an absolute private profile path",
        ));
    }
    if flags.contains_key("--help") {
        return Ok(flags);
    }
    if !flags.contains_key("--server") && !flags.contains_key("--local-profile") {
        return Err(usage("--server or --local-profile required"));
    }
    if !flags.contains_key("--cookie-file") && !flags.contains_key("--service-key-file") {
        return Err(usage("--cookie-file or --service-key-file required"));
    }
    payload(args, &flags)?;
    Ok(flags)
}
pub fn preflight(args: &[String]) -> Result<(), Output> {
    let flags = parse(args)?;
    if !flags.contains_key("--help")
        && let Some(workspace) = flags.get("--workspace")
    {
        let profile = flags
            .get("--local-profile")
            .ok_or_else(|| usage("--workspace requires --local-profile"))?;
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
    let flags = match parse(args) {
        Ok(f) => f,
        Err(o) => return o,
    };
    if flags.contains_key("--help") {
        return Output {
            stdout: format!("{}\n", help()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let json_mode = flags.contains_key("--json")
        || flags.get("--format") == Some(&"json")
        || (flags.get("--format") != Some(&"human") && !std::io::stdout().is_terminal());
    for key in ["--server", "--company"] {
        if !flags.contains_key(key) {
            return usage(&format!("Explicit {key} required"));
        }
    }
    let api = match crate::remote::credential_client(
        flags["--server"],
        flags.contains_key("--allow-loopback-http"),
        &flags,
        false,
    ) {
        Ok(api) => api,
        Err((exit, e)) => return client_failure(exit, e, json_mode, &args[1]),
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => {
            return render(
                json!({"ok":false,"errors":["HTTP runtime unavailable"]}),
                json_mode,
                1,
                &args[1],
            );
        }
    };
    let input = match payload(args, &flags) {
        Ok(v) => v,
        Err(o) => return o,
    };
    let result = runtime.block_on(api.original_dimensions(
        flags["--company"],
        operation(&args[1]).unwrap(),
        &input,
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
            client_failure(exit, e, json_mode, &args[1])
        }
    }
}
fn client_failure(
    exit: u8,
    e: rentemester_engine::Error,
    json_mode: bool,
    operation: &str,
) -> Output {
    if exit == 2 {
        return usage(&e.message);
    }
    render(
        json!({"ok":false,"errors":[format!("{}: {}", e.code, e.message)]}),
        json_mode,
        exit,
        operation,
    )
}
fn render(value: Value, json_mode: bool, exit: u8, operation: &str) -> Output {
    if json_mode {
        Output {
            stdout: format!("{value}\n"),
            stderr: String::new(),
            exit,
        }
    } else if exit == 0 {
        let detail = value.get("plan").unwrap_or(&value);
        let mut lines = vec![format!("✔ Dimensions {operation}")];
        for (key, v) in detail.as_object().into_iter().flatten() {
            if key == "ok" {
                continue;
            }
            if v.is_string() || v.is_number() || v.is_boolean() {
                lines.push(format!(
                    "  {key}: {}",
                    v.as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| v.to_string())
                ));
            } else if let Some(rows) = v.as_array() {
                lines.push(format!("  {key}: {} records", rows.len()));
                for row in rows {
                    let fields = row
                        .as_object()
                        .map(|o| {
                            o.iter()
                                .filter(|(_, v)| v.is_string() || v.is_number() || v.is_boolean())
                                .map(|(k, v)| {
                                    format!(
                                        "{k}={}",
                                        v.as_str()
                                            .map(str::to_owned)
                                            .unwrap_or_else(|| v.to_string())
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("; ")
                        })
                        .unwrap_or_else(|| row.to_string());
                    lines.push(format!("    {fields}"));
                }
            }
        }
        Output {
            stdout: format!("{}\n", lines.join("\n")),
            stderr: String::new(),
            exit,
        }
    } else {
        let mut lines = vec![format!("✘ Dimensions {operation} failed")];
        if let Some(errors) = value["errors"].as_array() {
            for error in errors.iter().filter_map(Value::as_str) {
                lines.push(format!("  → {error}"));
            }
        }
        Output {
            stdout: String::new(),
            stderr: format!("{}\n", lines.join("\n")),
            exit,
        }
    }
}

fn payload(args: &[String], flags: &BTreeMap<&str, &str>) -> Result<Value, Output> {
    let op = operation(&args[1]).ok_or_else(|| usage("Unknown dimensions operation"))?;
    let schema = rentemester_client::original_dimensions_schema(op);
    let properties = schema["properties"].as_object().unwrap();
    let mappings = [
        ("--dimension-id", "dimensionId"),
        ("--member-id", "memberId"),
        ("--kind", "kind"),
        ("--name", "name"),
        ("--status", "status"),
        ("--effective-from", "effectiveFrom"),
        ("--effective-to", "effectiveTo"),
        ("--action", "action"),
        ("--supersedes-id", "supersedesId"),
        ("--journal-line-id", "journalLineId"),
        ("--allocations", "allocations"),
        ("--source", "source"),
        ("--reviewed-import", "reviewedImport"),
        ("--source-ref", "sourceRef"),
        ("--plan-hash", "planHash"),
        ("--idempotency-key", "idempotencyKey"),
        ("--expected-assignment-id", "expectedAssignmentId"),
        ("--reason", "reason"),
        ("--assignment-id", "assignmentId"),
        ("--account", "accountNo"),
        ("--period", "period"),
        ("--confirm", "confirm"),
    ];
    let mut input = json!({});
    for (flag, raw) in flags {
        if [
            "--company",
            "--server",
            "--cookie-file",
            "--service-key-file",
            "--local-profile",
            "--workspace",
            "--json",
            "--format",
            "--help",
            "--allow-loopback-http",
        ]
        .contains(flag)
        {
            continue;
        }
        if ["--actor", "--principal"].contains(flag) {
            if !properties.contains_key("confirm")
                || raw.trim().is_empty()
                || raw.len() > 256
                || raw.chars().any(char::is_control)
            {
                return Err(usage("Identity assertion allowed only on writes"));
            }
            continue;
        }
        let key = mappings
            .iter()
            .find(|(f, _)| f == flag)
            .map(|(_, k)| *k)
            .ok_or_else(|| usage("Unknown dimensions flag"))?;
        if !properties.contains_key(key) {
            return Err(usage("Flag not allowed for this dimensions operation"));
        }
        input[key] = match key {
            "confirm" => {
                if *raw != "yes" {
                    return Err(usage("--confirm yes required"));
                }
                json!(true)
            }
            "reviewedImport" => match *raw {
                "yes" => json!(true),
                "no" => json!(false),
                _ => return Err(usage("--reviewed-import must be yes or no")),
            },
            "journalLineId" | "assignmentId" | "expectedAssignmentId" => json!(
                raw.parse::<u64>()
                    .map_err(|_| usage("Positive safe integer required"))?
            ),
            "allocations" => {
                if raw.len() > 66560 {
                    return Err(usage("Inline allocations exceed byte limit"));
                }
                serde_json::from_str(raw)
                    .map_err(|_| usage("--allocations requires inline JSON, never a file"))?
            }
            _ => json!(raw),
        };
    }
    validate_original_dimensions(op, &input).map_err(|e| usage(&e.to_string()))?;
    Ok(input)
}
