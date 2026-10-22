//! Original mileage invocation and flat source results; native aliases stay separate.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal};

pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "mileage")
        && args
            .get(1)
            .is_some_and(|s| matches!(s.as_str(), "log" | "list" | "report" | "export"))
}
pub fn help() -> Value {
    json!({"ok":true,"commands":["mileage log --date YYYY-MM-DD --purpose TEXT --from TEXT --to TEXT --km N --vehicle TEXT --driver TEXT --rate-per-km N --rate-basis TEXT [--rate-source TEXT] [--notes TEXT]","mileage list","mileage report --from YYYY-MM-DD --to YYYY-MM-DD","mileage export --from YYYY-MM-DD --to YYYY-MM-DD --out DIR"],"connection":"--company SLUG --server ORIGIN or --local-profile PATH; --cookie-file PATH or --service-key-file PATH; --json","authority":"Log/export invocation confirms. No retry. Export files are written on this CLI host; prepared server audit may exist on local failure.","native":"remote register-mileage and mileage unchanged"})
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
            "--date"
                | "--purpose"
                | "--from"
                | "--to"
                | "--km"
                | "--vehicle"
                | "--driver"
                | "--rate-per-km"
                | "--rate-basis"
                | "--rate-source"
                | "--notes"
                | "--out"
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
                "Unknown mileage flag {key}; see mileage --help"
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
            return Err(usage("Empty or duplicate mileage flag"));
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
    let result = runtime.block_on(async {
        let bytes = serde_json::to_vec(&input).unwrap();
        match args[1].as_str() {
            "log" => {
                api.original_mileage_log(flags["--company"], &bytes, true)
                    .await
            }
            "export" => {
                api.original_mileage_export(flags["--company"], &bytes, true)
                    .await
            }
            "report" => {
                api.original_mileage_report(
                    flags["--company"],
                    flags.get("--from").copied().unwrap_or(""),
                    flags.get("--to").copied().unwrap_or(""),
                )
                .await
            }
            _ => api.original_mileage_list(flags["--company"]).await,
        }
    });
    match result {
        Ok(v) => {
            let v = if args[1] == "export" && v["ok"] == true {
                match rentemester_client::publish_original_mileage(
                    &v,
                    std::path::Path::new(flags.get("--out").copied().unwrap_or("")),
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        json!({"ok":false,"code":"local_export_failed","errors":[e.to_string()]})
                    }
                }
            } else {
                v
            };
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
            stdout: format!("Mileage {operation}\n{value}\n"),
            stderr: String::new(),
            exit,
        }
    } else {
        Output {
            stdout: String::new(),
            stderr: format!("Mileage {operation} failed\n{value}\n"),
            exit,
        }
    }
}
fn payload(args: &[String], flags: &BTreeMap<&str, &str>) -> Result<Value, Output> {
    let log = args[1] == "log";
    let mapping: &[(&str, &str)] = if log {
        &[
            ("--date", "tripDate"),
            ("--purpose", "purpose"),
            ("--from", "fromLocation"),
            ("--to", "toLocation"),
            ("--km", "kilometers"),
            ("--vehicle", "vehicle"),
            ("--driver", "driver"),
            ("--rate-per-km", "ratePerKm"),
            ("--rate-basis", "rateBasis"),
            ("--rate-source", "rateSource"),
            ("--notes", "notes"),
        ]
    } else if args[1] == "report" || args[1] == "export" {
        &[("--from", "from"), ("--to", "to")]
    } else {
        &[]
    };
    let mut input = json!({});
    for (flag, field) in mapping {
        if log && ["kilometers", "ratePerKm"].contains(field) {
            input[*field] = flags
                .get(flag)
                .and_then(|s| s.trim().parse::<f64>().ok())
                .filter(|n| n.is_finite())
                .map_or(Value::Null, |n| json!(n));
        } else if let Some(v) = flags.get(flag) {
            input[*field] = json!(v);
        } else if !["rateSource", "notes"].contains(field) {
            input[*field] = json!("");
        }
    }
    for k in flags.keys() {
        if mapping.iter().any(|(f, _)| f == k)
            || [
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
            || (*k == "--out" && args[1] == "export")
        {
            continue;
        }
        return Err(usage("Flag not allowed for this mileage command"));
    }
    Ok(input)
}
