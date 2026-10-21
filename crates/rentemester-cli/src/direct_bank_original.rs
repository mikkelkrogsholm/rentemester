//! Original direct-payable flags with explicit intent and authenticated principal assertion only.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "bank")
        && args
            .get(1)
            .is_some_and(|s| matches!(s.as_str(), "direct-payable-plan" | "direct-payable-apply"))
}
pub fn help() -> Value {
    json!({"usage":"bank direct-payable-plan|direct-payable-apply --document-id ID --bank-transaction-id ID --bill-date DATE --due-date DATE --expense-account ACCOUNT [--vat-treatment standard|exempt|non_deductible] [--vendor-id ID] [--note TEXT]. Apply additionally --plan-hash HASH --reason TEXT --idempotency-key KEY --principal user:ID|service-account:ID --confirm yes.","transport":"Explicit --company SLUG, --server ORIGIN or global --local-profile, exactly one --cookie-file PATH/--service-key-file PATH. Optional --json/--allow-loopback-http. Principal is an assertion checked against current authentication, never actor authority. Full native plan/hash and flattened server receipt preserved; raw key/reason unchanged. No local math/hashes or automatic retries."})
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
            stdout: format!("Bank\n{}\n", serde_json::to_string_pretty(&value).unwrap()),
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
        .unwrap_or_else(|| "Bank action refused".into());
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
    if args.iter().skip(2).any(|s| s == "--help") {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&help()).unwrap()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let apply = args[1] == "direct-payable-apply";
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
        if key == "--allow-loopback-http" && !http {
            http = true;
            cursor += 1;
            continue;
        }
        if ![
            "--server",
            "--company",
            "--cookie-file",
            "--service-key-file",
            "--document-id",
            "--bank-transaction-id",
            "--bill-date",
            "--due-date",
            "--expense-account",
            "--vat-treatment",
            "--vendor-id",
            "--note",
        ]
        .contains(&key)
            && !(apply
                && [
                    "--plan-hash",
                    "--reason",
                    "--idempotency-key",
                    "--principal",
                    "--confirm",
                ]
                .contains(&key))
        {
            return failure(
                &format!("Unknown/duplicate direct-payable flag {key}"),
                false,
                2,
            );
        }
        let Some(value) = args.get(cursor + 1).filter(|s| !s.starts_with("--")) else {
            return failure(&format!("Missing value for {key}"), false, 2);
        };
        if flags.insert(key, value.as_str()).is_some() {
            return failure(&format!("Duplicate {key}"), false, 2);
        }
        cursor += 2;
    }
    if apply && flags.get("--confirm") != Some(&"yes") {
        return failure(
            "bank direct-payable-apply requires the exact confirmation --confirm yes",
            false,
            2,
        );
    }
    let mut required = vec![
        "--company",
        "--server",
        "--document-id",
        "--bank-transaction-id",
        "--bill-date",
        "--due-date",
        "--expense-account",
    ];
    if apply {
        required.extend([
            "--plan-hash",
            "--reason",
            "--idempotency-key",
            "--principal",
        ]);
    }
    for key in required {
        if flags.get(key).is_none_or(|v| v.is_empty()) {
            return failure(&format!("Required {key}"), false, 2);
        }
    }
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") {
        return failure("Choose exactly one cookie-file/service-key-file", false, 2);
    }
    let mut body = json!({});
    for (flag, key) in [
        ("--document-id", "documentId"),
        ("--bank-transaction-id", "bankTransactionId"),
        ("--vendor-id", "vendorId"),
    ] {
        if let Some(text) = flags.get(flag) {
            let Some(id) = text
                .parse::<i64>()
                .ok()
                .filter(|n| (1..=9007199254740991).contains(n))
            else {
                return failure(&format!("Positive safe integer {flag} required"), false, 2);
            };
            body[key] = json!(id);
        }
    }
    for (flag, key) in [
        ("--bill-date", "billDate"),
        ("--due-date", "dueDate"),
        ("--expense-account", "expenseAccountNo"),
        ("--vat-treatment", "vatTreatment"),
        ("--note", "note"),
        ("--plan-hash", "planHash"),
        ("--reason", "reason"),
        ("--idempotency-key", "idempotencyKey"),
    ] {
        if let Some(text) = flags.get(flag) {
            body[key] = json!(text);
        }
    }
    if apply {
        body["confirm"] = json!(true);
        let Some((kind, subject)) =
            flags["--principal"]
                .split_once(':')
                .filter(|(kind, subject)| {
                    matches!(*kind, "user" | "service-account") && !subject.trim().is_empty()
                })
        else {
            return failure(
                "--principal requires user:ID or service-account:ID",
                false,
                2,
            );
        };
        body["expectedPrincipal"] = json!({"kind":kind,"subjectId":subject});
    }
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
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return failure("HTTP runtime unavailable", json_mode, 1),
    };
    match rt.block_on(api.direct_bank_original(
        flags["--company"],
        serde_json::to_vec(&body).unwrap(),
        apply,
    )) {
        Ok(mut value) => {
            let exit = if value["ok"] == true { 0 } else { 1 };
            if exit == 0 {
                let mut data = value["data"].take();
                for key in ["ok", "errors", "appliedRules"] {
                    if let Some(v) = value.get(key) {
                        data[key] = v.clone();
                    }
                }
                value = data;
            }
            output(value, json_mode, exit)
        }
        Err(error) => {
            let (exit, e) = crate::remote::client_error(error);
            output(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
            )
        }
    }
}
