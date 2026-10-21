//! Original bank-account commands; patches and account identity remain server-owned.
use crate::dated_reports::Output;
use rentemester_client::BankAccountOriginalMutation;
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "bank-account")
        && args
            .get(1)
            .is_some_and(|s| matches!(s.as_str(), "add" | "update"))
}
pub fn help() -> Value {
    json!({"usage":"bank-account add --name NAME [--slug SLUG] | bank-account update --account ID_OR_SLUG [--active true|false]; both require --company SLUG --server ORIGIN and exactly one --cookie-file PATH/--service-key-file PATH. Global --local-profile may replace server. Optional --json, --allow-loopback-http.","fields":"--name --bank-name --registration-no --account-no --iban --bic --account-owner --customer-no --currency --ledger-account. Omitted update fields stay unchanged; empty payment strings clear. Slug is create-only. Command intent confirms the write, no --confirm. No account lookup, actor, retries or implicit selection."})
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
                "Bankkonto\n{}\n",
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
        .unwrap_or_else(|| "Bank account mutation refused".into());
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
    let (mut json_mode, mut http) = (false, false);
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
    while cursor < args.len() {
        let key = args[cursor].as_str();
        match key {
            "--json" if !json_mode => {
                json_mode = true;
                cursor += 1;
            }
            "--allow-loopback-http" if !http => {
                http = true;
                cursor += 1;
            }
            "--company" | "--server" | "--cookie-file" | "--service-key-file" | "--name"
            | "--slug" | "--bank-name" | "--registration-no" | "--account-no" | "--iban"
            | "--bic" | "--account-owner" | "--customer-no" | "--currency" | "--ledger-account"
            | "--account" | "--active" => {
                let Some(value) = args.get(cursor + 1).filter(|s| !s.starts_with("--")) else {
                    return failure(&format!("Missing value for {key}"), false, 2);
                };
                if flags.insert(key, value.as_str()).is_some() {
                    return failure(&format!("Duplicate {key}"), false, 2);
                }
                cursor += 2;
            }
            _ => {
                return failure(
                    &format!("Unknown or duplicate bank account flag {key}"),
                    false,
                    2,
                );
            }
        }
    }
    for key in [
        if args[1] == "add" {
            "--name"
        } else {
            "--account"
        },
        "--company",
        "--server",
    ] {
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
    let create = args[1] == "add";
    if create && (flags.contains_key("--active") || flags.contains_key("--account"))
        || !create && flags.contains_key("--slug")
    {
        return failure(
            "Create forbids --account/--active; update forbids --slug",
            false,
            2,
        );
    }
    if flags
        .get("--active")
        .is_some_and(|s| !matches!(*s, "true" | "false"))
    {
        return failure("--active must be true or false", false, 2);
    }
    let mut body = json!({"confirm":true});
    for (flag, key) in [
        ("--name", "name"),
        ("--slug", "slug"),
        ("--account", "account"),
        ("--bank-name", "bankName"),
        ("--registration-no", "registrationNo"),
        ("--account-no", "accountNo"),
        ("--iban", "iban"),
        ("--bic", "bic"),
        ("--account-owner", "accountOwner"),
        ("--customer-no", "customerNo"),
        ("--currency", "currency"),
        ("--ledger-account", "ledgerAccountNo"),
    ] {
        if let Some(value) = flags.get(flag) {
            body[key] = json!(value);
        }
    }
    if let Some(active) = flags.get("--active") {
        body["active"] = json!(*active == "true");
    }
    let body = match serde_json::to_vec(&body) {
        Ok(body) => body,
        Err(_) => return failure("Bank account body unavailable", false, 2),
    };
    let operation = if create {
        BankAccountOriginalMutation::Create
    } else {
        BankAccountOriginalMutation::Update
    };
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
    match rt.block_on(api.bank_account_original(flags["--company"], operation, body)) {
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
