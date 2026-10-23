//! Original file-input VAT purchase commands. Invocation confirms; actor asserts credential only.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{IsTerminal, Read},
};
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "vat")
        && args.get(1).is_some_and(|s| {
            matches!(
                s.as_str(),
                "post-eu-service-purchase" | "post-representation-purchase"
            )
        })
}
pub fn help() -> Value {
    json!({"ok":true,"commands":["vat post-eu-service-purchase --input FILE.json","vat post-representation-purchase --input FILE.json"],"authority":"Invocation confirms write; no --confirm. Optional --actor is only an exact current-credential assertion; no environment actor authority.","connection":"Migrated --company/--server/--local-profile, cookie/service key and --format json|human routing.","representationAccount":"Omitted expenseAccountNo requires a unique active expense account explicitly configured with default_vat_code=REPRESENTATION_SPECIAL; missing or ambiguous configuration requires explicit expenseAccountNo. Never defaults to 3070.","input":"Flat source payload, bounded 16MiB JSON file read once. Optional sourceBankTransactionId; no synthesized bank. netAmount is major DKK.","recovery":"No automatic retry. outcome_unknown requires canonical journal/document/bank readback before another explicit action."})
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
            "--input"
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
                "Unknown VAT purchase flag {key}; see vat --help"
            )));
        }
        let (raw, next) = if matches!(key, "--json" | "--help" | "--allow-loopback-http") {
            if attached.is_some() {
                return Err(usage("Boolean flag takes no value"));
            }
            ("true", cursor + 1)
        } else {
            crate::original_args::value(args, cursor).ok_or_else(|| {
                usage(if key == "--input" {
                    "Missing required --input <file.json>"
                } else {
                    "Missing flag value"
                })
            })?
        };
        if (raw.trim().is_empty() && key != "--input") || flags.insert(key, raw).is_some() {
            return Err(usage("Empty or duplicate VAT purchase flag"));
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
    if flags.get("--input").is_none_or(|s| s.trim().is_empty()) {
        return Err(usage("Missing required --input <file.json>"));
    }
    if !flags.contains_key("--server") && !flags.contains_key("--local-profile") {
        return Err(usage("--server or --local-profile required"));
    }
    if !flags.contains_key("--cookie-file") && !flags.contains_key("--service-key-file") {
        return Err(usage("--cookie-file or --service-key-file required"));
    }
    if let Some(actor) = flags.get("--actor")
        && (actor.trim() != *actor
            || actor.encode_utf16().count() > 160
            || actor.chars().any(char::is_control))
    {
        return Err(usage(
            "--actor is a trimmed current-credential assertion only",
        ));
    }
    Ok(flags)
}

/// Read the bounded input exactly once, before profile/credential/HTTP effects.
pub fn preflight(args: &[String]) -> Result<Option<Value>, Output> {
    let flags = parse(args)?;
    if flags.contains_key("--help") {
        return Ok(None);
    }
    let file = std::fs::File::open(flags["--input"])
        .map_err(|_| usage("Could not read --input JSON file"))?;
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| usage("Could not read --input JSON file"))?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(usage("--input JSON exceeds 16MiB"));
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| usage("Invalid --input JSON"))?;
    rentemester_client::validate_original_vat_purchase(
        &value,
        args[1] == "post-representation-purchase",
    )
    .map_err(|_| usage("Invalid VAT purchase payload"))?;
    if let Some(workspace) = flags.get("--workspace") {
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
    Ok(Some(value))
}
pub fn execute(args: &[String], input: Value) -> Output {
    let flags = match parse(args) {
        Ok(f) => f,
        Err(o) => return o,
    };
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
        if args[1] == "post-eu-service-purchase" {
            api.original_vat_eu_service_purchase(
                flags["--company"],
                &input,
                flags.get("--actor").copied(),
            )
            .await
        } else {
            api.original_vat_representation_purchase(
                flags["--company"],
                &input,
                flags.get("--actor").copied(),
            )
            .await
        }
    });
    match result {
        Ok(v) => {
            if args[1] == "post-eu-service-purchase" && v["ok"] == false {
                if let Some(invoice) = input["invoiceNo"]
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    let message = format!("Could not resolve document for invoiceNo {invoice}");
                    if v["errors"] == json!([message]) {
                        return usage(&message);
                    }
                }
            }
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
            "vat {operation}: {}\n{}\n",
            if exit == 0 { "ok" } else { "failed" },
            serde_json::to_string_pretty(&value).unwrap()
        ),
        stderr: String::new(),
        exit,
    }
}
