//! Original bank import command over one confirmed shared-server mutation.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "bank") && args.get(1).is_some_and(|s| s == "import")
}
pub fn help() -> Value {
    json!({"usage":"bank import --file PATH --company SLUG --server ORIGIN (--cookie-file PATH | --service-key-file PATH) [--account ID_OR_SLUG] [--profile danske-bank] [--statement-order ascending|descending] [--json] [--allow-loopback-http]", "contract":"Global --local-profile may replace server. This explicit import command confirms the write; no additional --confirm. Original file bytes are bounded25MiB and never decoded. One request, no automatic retry, account lookup or implicit default company/account."})
}
fn output(value: Value, json_mode: bool, exit: u8) -> Output {
    if json_mode && exit != 2 {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
            stderr: String::new(),
            exit,
        };
    }
    if exit == 0 {
        return Output {
            stdout: format!(
                "Bankimport\n{}\n",
                serde_json::to_string_pretty(&value).unwrap()
            ),
            stderr: String::new(),
            exit,
        };
    }
    let message = value["errors"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| "Bank import refused".into());
    Output {
        stdout: String::new(),
        stderr: format!("{message}\n"),
        exit,
    }
}
fn failure(message: &str, json_mode: bool, exit: u8) -> Output {
    output(json!({"ok":false,"errors":[message]}), json_mode, exit)
}
pub fn execute(args: &[String]) -> Output {
    if args
        .iter()
        .skip(2)
        .take_while(|s| s.as_str() != "--")
        .any(|s| s == "--help")
    {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&help()).unwrap()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let (mut json_mode, mut http) = (false, false);
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
    while cursor < args.len() {
        let (key, attached) = crate::original_args::option(&args[cursor]);
        match key {
            "--json" if attached.is_none() && !json_mode => {
                json_mode = true;
                cursor += 1;
            }
            "--allow-loopback-http" if attached.is_none() && !http => {
                http = true;
                cursor += 1;
            }
            "--file" | "--company" | "--server" | "--cookie-file" | "--service-key-file"
            | "--account" | "--profile" | "--statement-order" => {
                let Some((value, next)) = crate::original_args::value(args, cursor) else {
                    return failure(&format!("Missing value for {key}"), false, 2);
                };
                if flags.insert(key, value).is_some() {
                    return failure(&format!("Duplicate {key}"), false, 2);
                }
                cursor = next;
            }
            _ => {
                return failure(
                    &format!("Unknown or duplicate bank import flag {key}"),
                    false,
                    2,
                );
            }
        }
    }
    for key in ["--file", "--company", "--server"] {
        if flags.get(key).is_none_or(|s| s.is_empty()) {
            return failure(&format!("Explicit {key} required"), false, 2);
        }
    }
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") {
        return failure(
            "Choose exactly one --cookie-file or --service-key-file",
            false,
            2,
        );
    }
    if flags.get("--statement-order").is_some_and(|v|!matches!(*v,"ascending"|"descending")) {return failure("statement order must be ascending or descending",false,2);}
    let api = match crate::remote::credential_client(flags["--server"], http, &flags, false) {
        Ok(api) => api,
        Err((exit, e)) => {
            return output(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
            );
        }
    };
    let body = match rentemester_client::bank_import_ordered_file_body(
        Path::new(flags["--file"]),
        flags.get("--account").copied(),
        flags.get("--profile").copied(),
        flags.get("--statement-order").copied(),
    ) {
        Ok(body) => body,
        Err(e) => return failure(&e.to_string(), json_mode, 1),
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return failure("HTTP runtime unavailable", json_mode, 1),
    };
    match rt.block_on(api.bank_import_original(flags["--company"], body)) {
        Ok(mut value) => {
            let exit = if value["ok"] == true { 0 } else { 1 };
            if exit == 0 {
                let mut data = value["data"].take();
                data["ok"] = json!(true);
                data["errors"] = value["errors"].take();
                value = data;
            }
            output(value, json_mode, exit)
        }
        Err(e) => {
            let (exit, e) = crate::remote::client_error(e);
            output(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
            )
        }
    }
}
