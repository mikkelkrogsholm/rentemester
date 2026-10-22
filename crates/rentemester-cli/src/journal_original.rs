//! Original manual journal commands; all domain decisions belong to the server.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};

pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "journal")
        && args
            .get(1)
            .is_some_and(|s| matches!(s.as_str(), "post" | "dry-run" | "reverse"))
}
pub fn help() -> Value {
    json!({"commands":["journal post --input FILE","journal dry-run --input FILE","journal reverse (--entry-id N | --entry-no NO | --match-text TEXT [--match-date DATE] [--match-document-id N]) --date DATE --reason TEXT","journal list"],"connection":"--company SLUG (or RENTEMESTER_COMPANY), --server ORIGIN or --local-profile PATH; exactly one --cookie-file/--service-key-file; --json/--format json|human","authority":"Explicit post/reverse invocation confirms one unkeyed operation; no retries. Dry-run rolls back. Current server principal owns attribution; caller createdBy/createdByProgram cannot impersonate or grant historical-import exemptions."})
}
fn usage(message: &str) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{message}; see journal --help\n"),
        exit: 2,
    }
}
fn output(value: Value, exit: u8) -> Output {
    if exit == 2 {
        return usage(&value["errors"].to_string());
    }
    Output {
        stdout: format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
        stderr: String::new(),
        exit,
    }
}
type Flags<'a> = BTreeMap<&'a str, &'a str>;
fn parse(args: &[String]) -> Result<Flags<'_>, Output> {
    let reverse = args.get(1).is_some_and(|s| s == "reverse");
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
    while cursor < args.len() {
        let (key, attached) = crate::original_args::option(&args[cursor]);
        let global = matches!(
            key,
            "--company"
                | "--server"
                | "--local-profile"
                | "--cookie-file"
                | "--service-key-file"
                | "--allow-loopback-http"
                | "--help"
                | "--json"
        );
        let allowed = if reverse {
            matches!(
                key,
                "--entry-id"
                    | "--entry-no"
                    | "--match-text"
                    | "--match-date"
                    | "--match-document-id"
                    | "--date"
                    | "--reason"
            )
        } else {
            key == "--input"
        };
        if !global && !allowed {
            return Err(usage("Unknown journal flag"));
        }
        let boolean = matches!(key, "--json" | "--help" | "--allow-loopback-http");
        let (value, next) = if boolean {
            if attached.is_some() {
                return Err(usage("Boolean flag does not take a value"));
            }
            ("true", cursor + 1)
        } else {
            crate::original_args::value(args, cursor).ok_or_else(|| usage("Missing flag value"))?
        };
        if flags.insert(key, value).is_some() {
            return Err(usage("Duplicate journal flag"));
        }
        cursor = next;
    }
    if flags.contains_key("--help") {
        return Ok(flags);
    }
    if reverse {
        for key in ["--date", "--reason"] {
            if flags.get(key).is_none_or(|v| v.is_empty()) {
                return Err(usage(&format!("Missing {key}")));
            }
        }
        let id = flags
            .get("--entry-id")
            .and_then(|s| s.parse::<f64>().ok())
            .is_some_and(|n| n > 0.0 && n.fract() == 0.0);
        let number = flags
            .get("--entry-no")
            .is_some_and(|s| !s.trim().is_empty());
        let text = flags
            .get("--match-text")
            .is_some_and(|s| !s.trim().is_empty());
        if !id && !number && !text {
            return Err(usage("Missing journal selector"));
        }
        if !id
            && !number
            && let Some(raw) = flags.get("--match-document-id")
        {
            let valid = raw
                .parse::<f64>()
                .ok()
                .is_some_and(|n| n > 0.0 && n.fract() == 0.0);
            if !valid {
                return Err(usage("--match-document-id must be a positive integer"));
            }
        }
    } else if flags.get("--input").is_none_or(|s| s.is_empty()) {
        return Err(usage("Missing --input FILE"));
    }
    Ok(flags)
}
fn body(args: &[String], flags: &Flags<'_>) -> Result<Vec<u8>, Output> {
    let value = if args[1] == "reverse" {
        let mut value =
            json!({"transactionDate":flags["--date"],"reason":flags["--reason"],"confirm":true});
        for (flag, key) in [
            ("--entry-id", "entryId"),
            ("--entry-no", "entryNo"),
            ("--match-text", "matchText"),
            ("--match-date", "matchDate"),
            ("--match-document-id", "matchDocumentId"),
        ] {
            if let Some(raw) = flags.get(flag) {
                value[key] = if matches!(key, "entryId" | "matchDocumentId") {
                    // Preserve invalid domain values for authoritative server refusal.
                    raw.parse::<serde_json::Number>()
                        .map(Value::Number)
                        .unwrap_or_else(|_| json!(raw))
                } else {
                    json!(raw)
                };
            }
        }
        value
    } else {
        // '-' is a literal filename, never stdin. Bound the actual wire envelope below.
        let mut file = std::fs::File::open(flags["--input"])
            .map_err(|_| usage("Could not read --input file"))?;
        let mut bytes = Vec::new();
        file.by_ref()
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(|_| usage("Could not read --input file"))?;
        if bytes.len() > 65536 {
            return Err(usage("Journal request exceeds65536 bytes"));
        }
        let input: Value = serde_json::from_slice(&bytes)
            .map_err(|_| usage("--input must contain one JSON object"))?;
        if !input.is_object() {
            return Err(usage("--input must contain one JSON object"));
        }
        if args[1] == "post" {
            json!({"input":input,"confirm":true})
        } else {
            json!({"input":input})
        }
    };
    let bytes = serde_json::to_vec(&value).unwrap();
    if bytes.len() > 65536 {
        return Err(usage("Journal request exceeds65536 bytes"));
    }
    Ok(bytes)
}
/// Validate local files and syntax before any credential/profile I/O.
pub fn preflight(args: &[String]) -> Result<(), Output> {
    let flags = parse(args)?;
    if !flags.contains_key("--help") {
        body(args, &flags)?;
    }
    Ok(())
}
pub fn execute(args: &[String]) -> Output {
    let flags = match parse(args) {
        Ok(f) => f,
        Err(o) => return o,
    };
    if flags.contains_key("--help") {
        return output(help(), 0);
    }
    let bytes = match body(args, &flags) {
        Ok(b) => b,
        Err(o) => return o,
    };
    for key in ["--company", "--server"] {
        if flags.get(key).is_none_or(|s| s.is_empty()) {
            return usage(&format!("Explicit {key} required"));
        }
    }
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") {
        return usage("Choose exactly one --cookie-file/--service-key-file");
    }
    let api = match crate::remote::credential_client(
        flags["--server"],
        flags.contains_key("--allow-loopback-http"),
        &flags,
        false,
    ) {
        Ok(api) => api,
        Err((exit, e)) => {
            return output(json!({"ok":false,"code":e.code,"errors":[e.message]}), exit);
        }
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return output(json!({"ok":false,"errors":["HTTP runtime unavailable"]}), 1),
    };
    let result = rt.block_on(async {
        match args[1].as_str() {
            "post" => {
                api.journal_original_post(flags["--company"], bytes, true)
                    .await
            }
            "dry-run" => {
                api.journal_original_dry_run(flags["--company"], bytes)
                    .await
            }
            "reverse" => {
                api.journal_original_reverse(flags["--company"], bytes, true)
                    .await
            }
            _ => unreachable!(),
        }
    });
    match result {
        Ok(value) => {
            let exit = if value["code"] == "journal_selection_usage" {
                2
            } else if value["ok"] == false {
                1
            } else {
                0
            };
            output(value, exit)
        }
        Err(e) => {
            let (exit, e) = crate::remote::client_error(e);
            output(json!({"ok":false,"code":e.code,"errors":[e.message]}), exit)
        }
    }
}
