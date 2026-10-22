//! Original expense invocation and flat source results; native aliases stay separate.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal};

pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "expense")
        && args
            .get(1)
            .is_some_and(|s| matches!(s.as_str(), "vat-preflight" | "book"))
}
pub fn help() -> Value {
    json!({"ok":true,"commands":["expense vat-preflight --document-id ID [--apply yes]","expense book --document-id ID --bank-transaction-id ID --expense-account ACCOUNT [--vat-treatment standard|reverse_charge|eu_goods_acquisition|representation|exempt|non_deductible] [--payment-account ACCOUNT] [--date YYYY-MM-DD] [--text TEXT]"],"connection":"--company SLUG or verified local company path; RENTEMESTER_COMPANY fallback; --server ORIGIN or --local-profile ABS_PRIVATE_PROFILE; --cookie-file PATH or --service-key-file PATH; --allow-loopback-http; --json or --format json|human","authority":"Only exact --apply yes mutates preflight. expense book invocation confirms booking; no --confirm or CLI idempotency key. Optional --actor on writes is only an exact assertion of current credential identity, never authority. No environment actor.","output":"Flat source result; piped defaults to JSON, terminal to human. Safe evidence and resumable exceptions retained, including definite ok:false provider results.","native":"remote expense-booking-options and remote match-bank-expense retain native APIs.","recovery":"No automatic retry. After outcome_unknown inspect canonical documents/journal/bank and VAT evidence before another explicit action."})
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
            "--document-id"
                | "--bank-transaction-id"
                | "--expense-account"
                | "--vat-treatment"
                | "--payment-account"
                | "--date"
                | "--text"
                | "--apply"
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
                "Unknown expense flag {key}; see expense --help"
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
            return Err(usage("Empty or duplicate expense flag"));
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
    let write = args[1] == "book" || flags.contains_key("--apply");
    if let Some(actor) = flags.get("--actor")
        && (!write
            || actor.trim() != *actor
            || actor.encode_utf16().count() > 160
            || actor.chars().any(char::is_control))
    {
        return Err(usage(
            "--actor is a trimmed current-identity assertion on writes only",
        ));
    }
    if args[1] == "vat-preflight" {
        if flags.get("--apply").is_some_and(|v| *v != "yes") {
            return Err(usage("--apply must be exactly yes"));
        }
        for key in [
            "--bank-transaction-id",
            "--expense-account",
            "--vat-treatment",
            "--payment-account",
            "--date",
            "--text",
        ] {
            if flags.contains_key(key) {
                return Err(usage("Booking flags are not preflight flags"));
            }
        }
        positive_id(flags.get("--document-id").copied())?;
    } else {
        if flags.contains_key("--apply") {
            return Err(usage("--apply is only for vat-preflight"));
        }
        input(&flags)?;
    }
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
    let result = runtime.block_on(async {
        if args[1] == "book" {
            api.original_expense_book(
                flags["--company"],
                &input(&flags).unwrap(),
                true,
                flags.get("--actor").copied(),
            )
            .await
        } else {
            let id = positive_id(flags.get("--document-id").copied()).unwrap();
            if flags.contains_key("--apply") {
                api.original_expense_vat_preflight_apply(
                    flags["--company"],
                    id,
                    true,
                    flags.get("--actor").copied(),
                )
                .await
            } else {
                api.original_expense_vat_preflight(flags["--company"], id)
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
    // Preserve every semantic field, particularly safe evidence and failed resumable exceptions.
    Output {
        stdout: format!(
            "expense {operation}: {}\n{}\n",
            if exit == 0 { "ok" } else { "failed" },
            serde_json::to_string_pretty(&value).unwrap()
        ),
        stderr: String::new(),
        exit,
    }
}
fn positive_id(raw: Option<&str>) -> Result<i64, Output> {
    let raw = raw.ok_or_else(|| usage("Required positive safe integer ID missing"))?;
    if !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(usage("Positive safe integer ID required"));
    }
    raw.parse::<i64>()
        .ok()
        .filter(|id| (1..=9_007_199_254_740_991).contains(id))
        .ok_or_else(|| usage("Positive safe integer ID required"))
}
fn input(flags: &BTreeMap<&str, &str>) -> Result<Value, Output> {
    let mut value = json!({"documentId":positive_id(flags.get("--document-id").copied())?,"bankTransactionId":positive_id(flags.get("--bank-transaction-id").copied())?,"expenseAccount":flags.get("--expense-account").ok_or_else(|| usage("--expense-account required"))?});
    for (flag, key) in [
        ("--vat-treatment", "vatTreatment"),
        ("--payment-account", "paymentAccount"),
        ("--date", "date"),
        ("--text", "text"),
    ] {
        if let Some(raw) = flags.get(flag) {
            value[key] = json!(raw);
        }
    }
    rentemester_client::validate_original_expense_book(&value, false)
        .map_err(|_| usage("Invalid expense booking input"))?;
    Ok(value)
}
