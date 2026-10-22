//! Original accrual invocation and flat source results; native aliases stay separate.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal};

pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "accrual")
        && args
            .get(1)
            .is_some_and(|s| matches!(s.as_str(), "register" | "recognize" | "register-report"))
}
pub fn help() -> Value {
    json!({"ok":true,"commands":["accrual register --type prepaid_expense|accrued_expense|deferred_revenue --description TEXT --amount DKK --periods N --first-date YYYY-MM-DD --result-account ACCOUNT [--registration-date YYYY-MM-DD] [--period-step-months N] [--balance-account ACCOUNT] [--settlement-account ACCOUNT] [--document-id ID] [--note TEXT]","accrual recognize --accrual-id ID --period N [--date YYYY-MM-DD] [--settlement-account ACCOUNT]","accrual register-report"],"connection":"--company SLUG or verified workspace path; RENTEMESTER_COMPANY fallback; --server ORIGIN or --local-profile ABS_PRIVATE_PROFILE; --cookie-file PATH or --service-key-file PATH; --allow-loopback-http; --json or --format json|human","authority":"Write invocation confirms. Optional --actor only asserts exact current credential identity; never environment authority. No key or automatic retry. After outcome_unknown inspect register-report and canonical journal before further action.","defaults":"Server resolves registrationDate=firstRecognitionDate, step=1, balance accounts1300/7300/7310, confirmed bank role; never creates missing accounts. Final period carries remainder. Period is1-based, recognition has no caller amount; explicit date may precede schedule subject to shared current date/period controls.","evidence":"Prepaid and deferred balance-only registration may omit document. All expense/income postings require shared evidence. Documentless recognition refuses. Supplied originals always checked.","native":"remote register-accrual/recognize-accrual/accruals/accrual-schedule remain unchanged."})
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
            "--type"
                | "--description"
                | "--amount"
                | "--periods"
                | "--first-date"
                | "--result-account"
                | "--registration-date"
                | "--period-step-months"
                | "--balance-account"
                | "--settlement-account"
                | "--document-id"
                | "--note"
                | "--accrual-id"
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
                "Unknown accrual flag {key}; see accrual --help"
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
            return Err(usage("Empty or duplicate accrual flag"));
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
            "register" => {
                api.original_accrual_register(
                    flags["--company"],
                    &input,
                    true,
                    flags.get("--actor").copied(),
                )
                .await
            }
            "recognize" => {
                api.original_accrual_recognize(
                    flags["--company"],
                    id.unwrap(),
                    &input,
                    true,
                    flags.get("--actor").copied(),
                )
                .await
            }
            _ => {
                api.original_accrual_register_report(flags["--company"])
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
    let heading = format!("Accrual {operation}");
    if exit == 0 {
        let fields = if let Some(rows) = value["accruals"].as_array() {
            let mut lines = rows
                .iter()
                .map(|r| {
                    format!(
                        "#{} {}: recognized {} / {}, remaining {} DKK",
                        r["accrualId"],
                        r["description"].as_str().unwrap_or(""),
                        r["recognizedPeriods"],
                        r["recognitionPeriods"],
                        r["remainingAmount"]
                    )
                })
                .collect::<Vec<_>>();
            lines.push(format!(
                "Total {} DKK; recognized {}; remaining {}",
                value["totals"]["totalAmount"],
                value["totals"]["recognizedAmount"],
                value["totals"]["remainingAmount"]
            ));
            lines.join("\n")
        } else {
            format!(
                "Accrual #{}; journal {} ({}); period amount {} DKK; periods {}",
                value["accrualId"],
                value["entryNo"].as_str().unwrap_or(""),
                value["entryId"],
                value["periodAmount"],
                value["totalPeriods"]
            )
        };
        return Output {
            stdout: format!("✔ {heading}\n{fields}\n"),
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
    let mapping: &[(&str, &str)] = match op {
        "register" => &[
            ("--type", "accrualType"),
            ("--description", "description"),
            ("--amount", "totalAmount"),
            ("--periods", "recognitionPeriods"),
            ("--first-date", "firstRecognitionDate"),
            ("--result-account", "resultAccountNo"),
            ("--registration-date", "registrationDate"),
            ("--period-step-months", "periodStepMonths"),
            ("--balance-account", "balanceAccountNo"),
            ("--settlement-account", "settlementAccountNo"),
            ("--document-id", "documentId"),
            ("--note", "note"),
        ],
        "recognize" => &[
            ("--period", "period"),
            ("--date", "date"),
            ("--settlement-account", "settlementAccountNo"),
        ],
        _ => &[],
    };
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
                || rentemester_client::validate_original_accrual_actor(Some(v)).is_err()
            {
                return Err(usage(
                    "--actor only accepts a bounded current-identity assertion on writes",
                ));
            }
            continue;
        }
        if *k == "--accrual-id" && op == "recognize" {
            id = Some(
                v.parse::<i64>()
                    .ok()
                    .filter(|n| (1..=9_007_199_254_740_991).contains(n))
                    .ok_or_else(|| usage("positive safe --accrual-id required"))?,
            );
            continue;
        }
        let (_, field) = mapping
            .iter()
            .find(|(flag, _)| flag == k)
            .ok_or_else(|| usage("Flag not allowed for this accrual command"))?;
        input[*field] = if [
            "totalAmount",
            "recognitionPeriods",
            "periodStepMonths",
            "documentId",
            "period",
        ]
        .contains(field)
        {
            if !v
                .bytes()
                .all(|b| b.is_ascii_digit() || (*field == "totalAmount" && b == b'.'))
            {
                return Err(usage("Expected numeric amount or positive integer"));
            }
            serde_json::from_str(v)
                .map_err(|_| usage("Expected numeric amount or positive integer"))?
        } else {
            json!(v)
        };
    }
    if op == "recognize" && id.is_none() {
        return Err(usage("--accrual-id required"));
    }
    if op != "register-report" {
        rentemester_client::validate_original_accrual(&input, op == "register")
            .map_err(|_| usage("Invalid accrual fields; see accrual --help"))?;
    }
    Ok((input, id))
}
