//! Original recurring CLI commands. All periods, source validation and issuance stay server-owned.
use crate::dated_reports::Output;
use rentemester_client::{OriginalRecurringKind as Kind, validate_recurring_original};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "recurring-invoice")
        && args
            .get(1)
            .is_some_and(|s| matches!(s.as_str(), "create" | "generate" | "list"))
}
fn output(value: Value, exit: u8) -> Output {
    let text = format!("{}\n", serde_json::to_string_pretty(&value).unwrap());
    Output {
        stdout: if exit == 2 {
            String::new()
        } else {
            text.clone()
        },
        stderr: if exit == 2 { text } else { String::new() },
        exit,
    }
}
fn invalid(message: &str) -> Output {
    output(json!({"ok":false,"errors":[message]}), 2)
}
fn read(path: &str) -> Result<Value, &'static str> {
    let reader: Box<dyn Read> = if path == "-" {
        Box::new(std::io::stdin())
    } else {
        Box::new(std::fs::File::open(path).map_err(|_| "Could not open input JSON")?)
    };
    let mut bytes = vec![];
    reader
        .take(262145)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read input JSON")?;
    if bytes.len() > 262144 {
        return Err("Input exceeds256KiB");
    }
    serde_json::from_slice(&bytes).map_err(|_| "Input must be valid JSON")
}
pub fn execute(args: &[String]) -> Output {
    let command = args[1].as_str();
    if args.iter().skip(2).any(|a| a == "--help") {
        return output(
            json!({"usage":"recurring-invoice create --input FILE|-; recurring-invoice generate --template-id N --as-of YYYY-MM-DD; recurring-invoice list [--include-inactive]","transport":"Explicit --company SLUG --server ORIGIN and exactly one private --cookie-file/--service-key-file. Optional --json/--allow-loopback-http or global --local-profile. Create/generate invocation confirms this single mutation. No timer, posting, delivery or retry; unknown outcome requires canonical template/invoice/document reads. Workspace runner is a separate command."}),
            0,
        );
    }
    let mut flags = BTreeMap::new();
    let (mut json_mode, mut http, mut include) = (false, false, false);
    let mut cursor = 2;
    while cursor < args.len() {
        let key = args[cursor].as_str();
        if key == "--json" && !json_mode {
            json_mode = true;
            cursor += 1;
            continue;
        }
        if key == "--allow-loopback-http" && !http {
            http = true;
            cursor += 1;
            continue;
        }
        if key == "--include-inactive" && command == "list" && !include {
            include = true;
            cursor += 1;
            continue;
        }
        let allowed = [
            "--company",
            "--server",
            "--cookie-file",
            "--service-key-file",
        ]
        .contains(&key)
            || (command == "create" && key == "--input")
            || (command == "generate" && ["--template-id", "--as-of"].contains(&key));
        if !allowed {
            return invalid("Unknown recurring-invoice flag");
        }
        let Some(value) = args.get(cursor + 1).filter(|s| !s.starts_with("--")) else {
            return invalid("Missing flag value");
        };
        if flags.insert(key, value.as_str()).is_some() {
            return invalid("Duplicate flag");
        }
        cursor += 2;
    }
    let kind = if command == "create" {
        Some(Kind::Create)
    } else if command == "generate" {
        Some(Kind::Generate)
    } else {
        None
    };
    let bytes = if let Some(kind) = kind {
        let mut value = if kind == Kind::Create {
            let Some(path) = flags.get("--input") else {
                return invalid("Missing required --input <file.json>");
            };
            match read(path) {
                Ok(v) => v,
                Err(e) => return invalid(e),
            }
        } else {
            let Some(id) = flags
                .get("--template-id")
                .and_then(|s| s.parse::<i64>().ok())
                .filter(|n| (1..=9007199254740991i64).contains(n))
            else {
                return invalid("Missing required --template-id <n> or --as-of <YYYY-MM-DD>");
            };
            let Some(as_of) = flags.get("--as-of").filter(|s| !s.is_empty()) else {
                return invalid("Missing required --template-id <n> or --as-of <YYYY-MM-DD>");
            };
            json!({"templateId":id,"asOfDate":as_of})
        };
        let Some(object) = value.as_object_mut() else {
            return invalid("Input must be an object");
        };
        // Confirmation belongs to this invocation; identity remains credential-derived server-side.
        if object.contains_key("confirm") {
            return invalid(
                "Original create input does not contain confirm; invocation confirms the write",
            );
        }
        object.insert("confirm".into(), json!(true));
        let bytes = serde_json::to_vec(&value).unwrap();
        if let Err(e) = validate_recurring_original(kind, &bytes) {
            return invalid(&e.to_string());
        }
        bytes
    } else {
        vec![]
    };
    for key in ["--company", "--server"] {
        if flags.get(key).is_none_or(|s| s.is_empty()) {
            return invalid("Required --company and --server");
        }
    }
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") {
        return invalid("Choose exactly one cookie-file/service-key-file");
    }
    let api = match crate::remote::credential_client(flags["--server"], http, &flags, false) {
        Ok(a) => a,
        Err((code, e)) => {
            return output(json!({"ok":false,"code":e.code,"errors":[e.message]}), code);
        }
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => return output(json!({"ok":false,"errors":["HTTP runtime unavailable"]}), 1),
    };
    let result = rt.block_on(async {
        if let Some(kind) = kind {
            api.recurring_original(flags["--company"], kind, bytes)
                .await
        } else {
            api.recurring_original_list(flags["--company"], include)
                .await
        }
    });
    match result {
        Ok(value) => {
            let exit = if value["ok"] == true { 0 } else { 1 };
            if command == "list" && !json_mode && exit == 0 {
                let mut text = format!("Recurring invoice templates ({})\n", value["count"]);
                let rows = value["rows"].as_array().unwrap();
                if rows.is_empty() {
                    text.push_str("No recurring invoice templates.\n");
                } else {
                    text.push_str("id\tname\tinterval\tnextIssueDate\tpaymentTermsDays\tactive\n");
                    for row in rows {
                        for (i, key) in [
                            "id",
                            "name",
                            "interval",
                            "nextIssueDate",
                            "paymentTermsDays",
                            "active",
                        ]
                        .iter()
                        .enumerate()
                        {
                            if i > 0 {
                                text.push('\t');
                            }
                            text.push_str(
                                &row[*key]
                                    .as_str()
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| row[*key].to_string()),
                            );
                        }
                        text.push('\n');
                    }
                }
                Output {
                    stdout: text,
                    stderr: String::new(),
                    exit,
                }
            } else {
                output(value, exit)
            }
        }
        Err(e) => {
            let (code, e) = crate::remote::client_error(e);
            output(json!({"ok":false,"code":e.code,"errors":[e.message]}), code)
        }
    }
}
