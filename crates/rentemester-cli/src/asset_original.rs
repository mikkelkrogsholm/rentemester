//! Original asset invocation and flat source results; native aliases stay separate.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal};

pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "asset")
        && args.get(1).is_some_and(|s| {
            matches!(
                s.as_str(),
                "register"
                    | "depreciate"
                    | "write-off"
                    | "register-report"
                    | "disposal-preview"
                    | "dispose"
                    | "disposal-receipt"
            )
        })
}
pub fn help() -> Value {
    json!({"ok":true,"disposalCommands":["asset disposal-preview --asset-id ID --transaction-date YYYY-MM-DD --document-id ID --reason TEXT [--loss-account-no ACCOUNT]","asset dispose (same preview fields) --expected-plan-hash HASH --idempotency-key KEY --confirm yes","asset disposal-receipt --asset-id ID --key KEY"],"commands":["asset register --name TEXT --category TEXT --acquisition-date YYYY-MM-DD --cost DKK --useful-life-months N --document-id ID [--asset-account ACCOUNT] [--depreciation-account ACCOUNT] [--accumulated-account ACCOUNT] [--note TEXT]","asset depreciate --asset-id ID --period N --date YYYY-MM-DD","asset write-off --name TEXT --category TEXT --acquisition-date YYYY-MM-DD --cost DKK --document-id ID --expense-account ACCOUNT --date YYYY-MM-DD --threshold-source TEXT --confirm yes [--payment-account ACCOUNT] [--note TEXT]","asset register-report"],"connection":"--company SLUG or verified workspace path; RENTEMESTER_COMPANY fallback; --server ORIGIN or --local-profile ABS_PRIVATE_PROFILE; --cookie-file PATH or --service-key-file PATH; --allow-loopback-http; --json or --format json|human","authority":"Register/depreciate invocation confirms. Write-off requires exact --confirm yes. Optional --actor asserts current credential identity only; no environment authority, key or automatic retry. Unknown writes require canonical report/journal reconciliation.","defaults":"Server resolves assetAccount5800, accumulatedAccount5810, depreciationAccount5820 and current confirmed bank role. No missing account creation. Registration declares metadata only, creates no journal, and requires physical original plus proven unallocated capitalisation (intentionally stricter evidence compatibility than source registration). Depreciation requires selected period and date, no amount/default-next.","writeOff":"Existing workflow guardrail by acquisition year: through2024 33100 DKK,2025 34400,2026 onward36000. Explicit advisor/user threshold source and confirmation required; not a new legal assessment or inferred tax/VAT eligibility.","native":"remote register-asset/depreciate-asset/assets/next-asset-depreciation remain unchanged."})
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
            "--transaction-date"
                | "--reason"
                | "--loss-account-no"
                | "--expected-plan-hash"
                | "--idempotency-key"
                | "--key"
                | "--name"
                | "--category"
                | "--acquisition-date"
                | "--cost"
                | "--useful-life-months"
                | "--document-id"
                | "--asset-account"
                | "--depreciation-account"
                | "--accumulated-account"
                | "--expense-account"
                | "--payment-account"
                | "--threshold-source"
                | "--confirm"
                | "--note"
                | "--asset-id"
                | "--period"
                | "--date"
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
                "Unknown asset flag {key}; see asset --help"
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
        if raw.trim().is_empty() || flags.insert(key, raw).is_some() {
            return Err(usage("Empty or duplicate asset flag"));
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
    let (input, id) = match payload(args, &flags) {
        Ok(v) => v,
        Err(o) => return o,
    };
    let result = runtime.block_on(async {
        match args[1].as_str() {
            "disposal-preview" => {
                api.asset_disposal_preview(flags["--company"], id.unwrap(), &input)
                    .await
            }
            "dispose" => {
                api.asset_dispose(flags["--company"], id.unwrap(), &input, true)
                    .await
            }
            "disposal-receipt" => {
                api.asset_disposal_receipt(flags["--company"], id.unwrap(), flags["--key"])
                    .await
            }
            "register" => {
                api.original_asset_register(
                    flags["--company"],
                    &input,
                    true,
                    flags.get("--actor").copied(),
                )
                .await
            }
            "depreciate" => {
                api.original_asset_depreciate(
                    flags["--company"],
                    id.unwrap(),
                    &input,
                    true,
                    flags.get("--actor").copied(),
                )
                .await
            }
            "write-off" => {
                api.original_asset_write_off(
                    flags["--company"],
                    &input,
                    true,
                    flags.get("--actor").copied(),
                )
                .await
            }
            _ => api.original_asset_register_report(flags["--company"]).await,
        }
    });
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
        return Output {
            stdout: format!("{value}\n"),
            stderr: String::new(),
            exit,
        };
    }
    let heading = format!("Asset {operation}");
    if exit == 0 {
        return Output {
            stdout: format!(
                "✔ {heading}\n{}\n",
                serde_json::to_string_pretty(&value).unwrap()
            ),
            stderr: String::new(),
            exit,
        };
    }
    let mut lines = vec![format!("✘ {heading} failed")];
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

fn payload(args: &[String], flags: &BTreeMap<&str, &str>) -> Result<(Value, Option<i64>), Output> {
    let op = args[1].as_str();
    if matches!(op, "disposal-preview" | "dispose" | "disposal-receipt") {
        let id = flags
            .get("--asset-id")
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|n| (1..=9007199254740991).contains(n))
            .ok_or_else(|| usage("positive safe --asset-id required"))?;
        let mut input = json!({});
        for (k, v) in flags {
            let field = match *k {
                "--transaction-date" => "transactionDate",
                "--document-id" => "documentId",
                "--reason" => "reason",
                "--loss-account-no" => "lossAccountNo",
                "--expected-plan-hash" => "expectedPlanHash",
                "--idempotency-key" => "idempotencyKey",
                "--confirm" if op == "dispose" && *v == "yes" => {
                    input["confirm"] = json!(true);
                    continue;
                }
                "--key" if op == "disposal-receipt" => continue,
                "--company"
                | "--server"
                | "--cookie-file"
                | "--service-key-file"
                | "--local-profile"
                | "--workspace"
                | "--asset-id"
                | "--actor"
                | "--json"
                | "--format"
                | "--help"
                | "--allow-loopback-http" => continue,
                _ => return Err(usage("Flag not allowed for disposal command")),
            };
            input[field] = if field == "documentId" {
                json!(
                    v.parse::<i64>()
                        .map_err(|_| usage("positive document ID required"))?
                )
            } else {
                json!(v)
            };
        }
        if op == "disposal-receipt" {
            if input != json!({})
                || !flags.get("--key").is_some_and(|s| {
                    (8..=160).contains(&s.encode_utf16().count()) && !s.trim().is_empty()
                })
            {
                return Err(usage("receipt requires only --asset-id and --key"));
            }
        } else {
            rentemester_client::validate_asset_disposal(&input,op=="dispose").map_err(|_|usage("Invalid disposal fields; dispose requires reviewed --expected-plan-hash, --idempotency-key and --confirm yes"))?;
        }
        return Ok((input, Some(id)));
    }
    let common: &[(&str, &str)] = &[
        ("--name", "name"),
        ("--category", "category"),
        ("--acquisition-date", "acquisitionDate"),
        ("--cost", "cost"),
        ("--document-id", "documentId"),
        ("--note", "note"),
    ];
    let mut mapping = if matches!(op, "register" | "write-off") {
        common.to_vec()
    } else {
        vec![]
    };
    match op {
        "register" => mapping.extend([
            ("--useful-life-months", "usefulLifeMonths"),
            ("--asset-account", "assetAccount"),
            ("--depreciation-account", "depreciationAccount"),
            ("--accumulated-account", "accumulatedAccount"),
        ]),
        "depreciate" => mapping.extend([("--period", "period"), ("--date", "date")]),
        "write-off" => mapping.extend([
            ("--expense-account", "expenseAccount"),
            ("--payment-account", "paymentAccount"),
            ("--date", "date"),
            ("--threshold-source", "thresholdRuleSource"),
        ]),
        _ => (),
    }
    let mut input = json!({});
    let mut id = None;
    for (k, v) in flags {
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
        .contains(k)
        {
            continue;
        }
        if *k == "--actor" {
            if op == "register-report"
                || rentemester_client::validate_original_asset_actor(Some(v)).is_err()
            {
                return Err(usage(
                    "--actor only accepts a bounded current-identity assertion on writes",
                ));
            }
            continue;
        }
        if *k == "--asset-id" && op == "depreciate" {
            id = Some(
                v.parse::<i64>()
                    .ok()
                    .filter(|n| (1..=9_007_199_254_740_991).contains(n))
                    .ok_or_else(|| usage("positive safe --asset-id required"))?,
            );
            continue;
        }
        if *k == "--confirm" && op == "write-off" {
            if *v != "yes" {
                return Err(usage("write-off requires exact --confirm yes"));
            }
            input["confirmImmediateWriteOff"] = json!(true);
            continue;
        }
        let (_, field) = mapping
            .iter()
            .find(|(flag, _)| flag == k)
            .ok_or_else(|| usage("Flag not allowed for this asset command"))?;
        input[*field] = if ["cost", "usefulLifeMonths", "documentId", "period"].contains(field) {
            if !v
                .bytes()
                .all(|b| b.is_ascii_digit() || (*field == "cost" && b == b'.'))
            {
                return Err(usage("Expected numeric amount or positive integer"));
            }
            serde_json::from_str(v)
                .map_err(|_| usage("Expected numeric amount or positive integer"))?
        } else {
            json!(v)
        };
    }
    if op == "depreciate" && id.is_none() {
        return Err(usage("--asset-id required"));
    }
    if op != "register-report" {
        rentemester_client::validate_original_asset(&input, op)
            .map_err(|_| usage("Invalid asset fields; see asset --help"))?;
    }
    Ok((input, id))
}

#[cfg(test)]
mod asset_disposal_tests {
    use super::*;
    #[test]
    fn asset_disposal_cli_confirmation_and_frozen_fields() {
        let args: Vec<String> = [
            "asset",
            "dispose",
            "--asset-id",
            "3",
            "--transaction-date",
            "2026-10-20",
            "--document-id",
            "7",
            "--reason",
            "Retired",
            "--expected-plan-hash",
            &"a".repeat(64),
            "--idempotency-key",
            "synthetic-key",
            "--confirm",
            "yes",
            "--server",
            "https://example.test",
            "--cookie-file",
            "/synthetic/cookie",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        assert!(accepts(&args));
        assert!(parse(&args).is_ok());
        let mut asserted = args.clone();
        asserted.extend(["--actor".to_owned(), "user:synthetic".to_owned()]);
        assert!(parse(&asserted).is_ok());
        let mut bad = args.clone();
        bad[1] = "disposal-preview".into();
        assert!(parse(&bad).is_err());
        let mut bad = args;
        let i = bad.iter().position(|v| v == "--confirm").unwrap();
        bad.drain(i..i + 2);
        assert!(parse(&bad).is_err());
    }
}
