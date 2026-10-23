//! Original vat invocation and flat source results; native aliases stay separate.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal};

pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "vat")
        && args.get(1).is_some_and(|s| {
            matches!(
                s.as_str(),
                "report" | "eu-sales-list" | "oss-report" | "momsangivelse" | "filing"
            )
        })
}
pub fn help() -> Value {
    json!({"ok":true,"commands":["vat report --from YYYY-MM-DD --to YYYY-MM-DD","vat eu-sales-list --from YYYY-MM-DD --to YYYY-MM-DD","vat oss-report --from YYYY-MM-DD --to YYYY-MM-DD","vat momsangivelse --from YYYY-MM-DD --to YYYY-MM-DD","vat filing --from YYYY-MM-DD --to YYYY-MM-DD"],"scope":"Read-only reports and exact filing aliases. No submission. Native remote VAT endpoints unchanged."})
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
            "--from"
                | "--to"
                | "--company"
                | "--server"
                | "--cookie-file"
                | "--service-key-file"
                | "--local-profile"
                | "--workspace"
                | "--json"
                | "--format"
                | "--help"
                | "--allow-loopback-http"
        ) {
            return Err(usage(&format!("Unknown vat flag {key}; see vat --help")));
        }
        let (raw, next) = if matches!(key, "--json" | "--help" | "--allow-loopback-http") {
            if attached.is_some() {
                return Err(usage("Boolean flag takes no value"));
            }
            ("true", cursor + 1)
        } else {
            crate::original_args::value(args, cursor).ok_or_else(|| {
                usage(if matches!(key, "--from" | "--to") {
                    "Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>"
                } else {
                    "Missing flag value"
                })
            })?
        };
        if flags.insert(key, raw).is_some() {
            return Err(usage("Empty or duplicate vat flag"));
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
    payload(args, &flags)?;
    if !flags.contains_key("--server") && !flags.contains_key("--local-profile") {
        return Err(usage("--server or --local-profile required"));
    }
    if !flags.contains_key("--cookie-file") && !flags.contains_key("--service-key-file") {
        return Err(usage("--cookie-file or --service-key-file required"));
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
    let _input = match payload(args, &flags) {
        Ok(v) => v,
        Err(o) => return o,
    };
    let result = runtime.block_on(async {
        let c = flags["--company"];
        let from = flags["--from"];
        let to = flags["--to"];
        match args[1].as_str() {
            "report" => api.original_vat_report(c, from, to).await,
            "eu-sales-list" => api.original_vat_eu_sales_list(c, from, to).await,
            "oss-report" => api.original_vat_oss_report(c, from, to).await,
            _ => api.original_vat_filing(c, from, to).await,
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
        Output {
            stdout: format!("{value}\n"),
            stderr: String::new(),
            exit,
        }
    } else if exit == 0 {
        Output {
            stdout: format!("VAT {operation}\n{value}\n"),
            stderr: String::new(),
            exit,
        }
    } else {
        Output {
            stdout: String::new(),
            stderr: format!("VAT {operation} failed\n{value}\n"),
            exit,
        }
    }
}
fn payload(_args: &[String], flags: &BTreeMap<&str, &str>) -> Result<Value, Output> {
    if ["--from", "--to"]
        .iter()
        .any(|k| flags.get(k).is_none_or(|s| s.is_empty()))
    {
        return Err(usage(
            "Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>",
        ));
    }
    Ok(json!({}))
}
