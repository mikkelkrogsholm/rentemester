//! Original financial adapters. Validation is pure; all ledger effects stay server-owned.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "invoice")
        && args.get(1).is_some_and(|s| {
            matches!(
                s.as_str(),
                "validate" | "write-off-bad-debt" | "imported-receivables"
            )
        })
}
fn output(value: Value, json_mode: bool, exit: u8) -> Output {
    let text = format!("{}\n", serde_json::to_string_pretty(&value).unwrap());
    if exit == 2 {
        Output {
            stdout: String::new(),
            stderr: text,
            exit,
        }
    } else {
        Output {
            stdout: if json_mode {
                text
            } else {
                format!("Faktura\n{text}")
            },
            stderr: String::new(),
            exit,
        }
    }
}
fn failure(message: &str) -> Output {
    output(json!({"ok":false,"errors":[message]}), false, 2)
}
fn read_input(path: &str) -> Result<Vec<u8>, &'static str> {
    let mut bytes = vec![];
    let reader: Box<dyn Read> = if path == "-" {
        Box::new(std::io::stdin())
    } else {
        Box::new(std::fs::File::open(path).map_err(|_| "Could not open input JSON")?)
    };
    reader
        .take(262145)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read input JSON")?;
    if bytes.len() > 262144 {
        return Err("Input exceeds256KiB");
    }
    Ok(bytes)
}
pub fn execute(args: &[String]) -> Output {
    let command = args[1].as_str();
    if args.iter().skip(2).any(|s| s == "--help") {
        return output(
            json!({"usage":"invoice validate --input FILE|- [--json]; invoice write-off-bad-debt --input FILE|-; invoice imported-receivables [--as-of DATE]","transport":"Validation is offline without company/server. Other commands require --company SLUG --server ORIGIN and exactly one --cookie-file/--service-key-file; optional --json/--allow-loopback-http. Write invocation confirms payload; explicit expenseAccountNo required. No retry. Omitted as-of resolved by server UTC today."}),
            true,
            0,
        );
    }
    let mut flags = BTreeMap::new();
    let (mut json_mode, mut http) = (false, false);
    let mut cursor = 2;
    while cursor < args.len() {
        let key = args[cursor].as_str();
        if key == "--json" && !json_mode {
            json_mode = true;
            cursor += 1;
            continue;
        }
        if key == "--allow-loopback-http" && command != "validate" && !http {
            http = true;
            cursor += 1;
            continue;
        }
        let allowed = if command == "validate" {
            vec!["--input"]
        } else {
            let mut a = vec![
                "--company",
                "--server",
                "--cookie-file",
                "--service-key-file",
            ];
            a.push(if command == "imported-receivables" {
                "--as-of"
            } else {
                "--input"
            });
            a
        };
        if !allowed.contains(&key) {
            return failure("Unknown financial invoice flag");
        }
        let Some(value) = args.get(cursor + 1).filter(|s| !s.starts_with("--")) else {
            return failure("Missing flag value");
        };
        if flags.insert(key, value.as_str()).is_some() {
            return failure("Duplicate flag");
        }
        cursor += 2;
    }
    let mut bytes = vec![];
    if command != "imported-receivables" {
        let Some(path) = flags.get("--input") else {
            return failure("Required --input FILE|-");
        };
        bytes = match read_input(path) {
            Ok(b) => b,
            Err(e) => return failure(e),
        };
        if command == "validate" {
            let payload: Value = match serde_json::from_slice(&bytes) {
                Ok(v) => v,
                Err(_) => return failure("Input must be valid JSON"),
            };
            let value = rentemester_engine::invoice_validation::validate(&payload);
            let exit = if value["ok"] == true { 0 } else { 1 };
            return output(value, json_mode, exit);
        }
        let mut envelope = b"{\"confirm\":true,\"payload\":".to_vec();
        envelope.extend(bytes);
        envelope.push(b'}');
        bytes = envelope;
        if let Err(e) = rentemester_client::validate_invoice_bad_debt_original(&bytes) {
            return failure(&e.to_string());
        }
    }
    for key in ["--company", "--server"] {
        if flags.get(key).is_none_or(|s| s.is_empty()) {
            return failure("Required --company and --server");
        }
    }
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") {
        return failure("Choose exactly one cookie-file/service-key-file");
    }
    let api = match crate::remote::credential_client(flags["--server"], http, &flags, false) {
        Ok(a) => a,
        Err((exit, e)) => {
            return output(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
            );
        }
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => {
            return output(
                json!({"ok":false,"errors":["HTTP runtime unavailable"]}),
                json_mode,
                1,
            );
        }
    };
    let result = rt.block_on(async {
        if command == "write-off-bad-debt" {
            api.invoice_bad_debt_original(flags["--company"], bytes)
                .await
        } else {
            api.imported_receivables_optional(flags["--company"], flags.get("--as-of").copied())
                .await
        }
    });
    match result {
        Ok(mut value) => {
            if command == "imported-receivables" && value["ok"] == true {
                value = value["importedReceivables"].take();
                value["ok"] = json!(true);
            }
            let exit = if value["ok"] == true { 0 } else { 1 };
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_verdict_is_direct_and_invalid_verdict_exits_one() {
        let path = std::env::temp_dir().join(format!(
            "rentemester-152-verdict-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, b"{}").unwrap();
        let args = vec![
            "invoice".into(),
            "validate".into(),
            "--input".into(),
            path.to_str().unwrap().into(),
            "--json".into(),
        ];
        let out = execute(&args);
        assert_eq!(out.exit, 1);
        assert_eq!(
            serde_json::from_str::<Value>(&out.stdout).unwrap(),
            rentemester_engine::invoice_validation::validate(&json!({}))
        );
        std::fs::write(&path, b"invalid-json").unwrap();
        assert_eq!(execute(&args).exit, 2);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn validation_needs_no_transport_and_refuses_transport_flags() {
        let a = vec![
            "invoice".into(),
            "validate".into(),
            "--company".into(),
            "synthetic".into(),
        ];
        assert!(accepts(&a));
        assert_eq!(execute(&a).exit, 2);
    }
}
