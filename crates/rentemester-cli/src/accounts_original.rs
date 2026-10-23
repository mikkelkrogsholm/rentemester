//! Explicit source chart commands; no inline JSON or local accounting policy.
use crate::dated_reports::Output;
use rentemester_client::{AccountsOriginalOperation as Op, validate_original_accounts};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "accounts")
}
pub fn help() -> Value {
    json!({"usage":"accounts list | roles-status | add --account-no NO --name NAME --type asset|liability|equity|income|expense|vat [--normal-balance debit|credit] [--default-vat-code CODE] [--allow-direct-posting true|false] --confirm yes | role-confirm --role ROLE --account NO --confirm yes", "transport":"--company SLUG --server ORIGIN (or --local-profile PATH), exactly one --cookie-file PATH/--service-key-file PATH; --json; writes optionally --actor/--principal user:ID|service_account:ID as credential equality assertions only. No automatic retry."})
}
fn fail(message: &str, exit: u8) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{message}\n"),
        exit,
    }
}
struct Parsed<'a> {
    op: Op,
    body: Value,
    flags: BTreeMap<&'a str, &'a str>,
    json: bool,
    http: bool,
}
fn parse(args: &[String]) -> Result<Parsed<'_>, Output> {
    let op = match args.get(1).map(String::as_str) {
        Some("list") => Op::List,
        Some("add") => Op::Add,
        Some("roles-status") => Op::RolesStatus,
        Some("role-confirm") => Op::RoleConfirm,
        _ => {
            return Err(fail(
                "Expected accounts list, add, roles-status or role-confirm",
                2,
            ));
        }
    };
    let write = matches!(op, Op::Add | Op::RoleConfirm);
    let mut p = Parsed {
        op,
        body: json!({}),
        flags: BTreeMap::new(),
        json: false,
        http: false,
    };
    let mut i = 2;
    while i < args.len() {
        let (k, attached) = crate::original_args::option(&args[i]);
        if k == "--json" && attached.is_none() && !p.json {
            p.json = true;
            i += 1;
            continue;
        }
        if k == "--allow-loopback-http" && attached.is_none() && !p.http {
            p.http = true;
            i += 1;
            continue;
        }
        let allowed = [
            "--company",
            "--server",
            "--cookie-file",
            "--service-key-file",
        ]
        .contains(&k)
            || write && ["--actor", "--principal", "--confirm"].contains(&k)
            || op == Op::Add
                && [
                    "--account-no",
                    "--name",
                    "--type",
                    "--normal-balance",
                    "--default-vat-code",
                    "--allow-direct-posting",
                ]
                .contains(&k)
            || op == Op::RoleConfirm && ["--role", "--account"].contains(&k);
        if !allowed {
            return Err(fail(&format!("Unknown/irrelevant accounts flag {k}"), 2));
        }
        let Some((v, next)) = crate::original_args::value(args, i) else {
            return Err(fail(&format!("Missing value for {k}"), 2));
        };
        if p.flags.insert(k, v).is_some() {
            return Err(fail(&format!("Duplicate {k}"), 2));
        }
        i = next;
    }
    if write {
        if p.flags.get("--confirm") != Some(&"yes") {
            return Err(fail("Accounts writes require --confirm yes", 2));
        }
        p.body = json!({"confirm":true});
        if op == Op::Add {
            p.body["input"] = json!({});
        }
        let target = if op == Op::Add {
            &mut p.body["input"]
        } else {
            &mut p.body
        };
        for (flag, key) in [
            ("--account-no", "accountNo"),
            ("--account", "accountNo"),
            ("--name", "name"),
            ("--type", "type"),
            ("--role", "role"),
            ("--normal-balance", "normalBalance"),
            ("--default-vat-code", "defaultVatCode"),
        ] {
            if let Some(v) = p.flags.get(flag) {
                if key == "normalBalance" && v.trim().is_empty() {
                    continue;
                }
                target[key] = json!(v);
            }
        }
        if let Some(v) = p.flags.get("--allow-direct-posting") {
            let normalized = v.to_ascii_lowercase();
            if !matches!(normalized.as_str(), "true" | "false") {
                return Err(fail("--allow-direct-posting must be true or false", 2));
            }
            target["allowDirectPosting"] = json!(normalized == "true");
        }
        for k in ["--actor", "--principal"] {
            if p.flags.get(k).is_some_and(|s| {
                !s.split_once(':').is_some_and(|(k, v)| {
                    matches!(k, "user" | "service_account")
                        && !v.is_empty()
                        && !v.chars().any(|c| c.is_whitespace() || c.is_control())
                })
            }) {
                return Err(fail("Expected canonical typed credential identity", 2));
            }
        }
    }
    validate_original_accounts(op, &p.body).map_err(|e| fail(&e.to_string(), 2))?;
    Ok(p)
}
pub fn preflight(args: &[String]) -> Result<(), Output> {
    if args.len() == 1 || args.iter().any(|s| s == "--help") {
        return Ok(());
    }
    parse(args).map(|_| ())
}
fn output(v: Value, json_mode: bool) -> Output {
    let exit = if v["ok"] == true { 0 } else { 1 };
    if json_mode {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&v).unwrap()),
            stderr: String::new(),
            exit,
        };
    }
    if exit != 0 {
        return fail(
            &v["errors"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_else(|| "Kontohandling afvist".into()),
            exit,
        );
    }
    let mut text = String::new();
    if let Some(rows) = v["rows"].as_array() {
        let width = rows
            .iter()
            .filter_map(|r| r["account_no"].as_str())
            .map(|s| s.chars().count())
            .max()
            .unwrap_or(5)
            .max(5);
        text.push_str(&format!(
            "{:<width$}  {:<32}  {:<10}  Moms\n",
            "Konto", "Navn", "Type"
        ));
        for r in rows {
            text.push_str(&format!(
                "{:<width$}  {:<32}  {:<10}  {}\n",
                r["account_no"].as_str().unwrap(),
                r["name"].as_str().unwrap(),
                r["type"].as_str().unwrap(),
                r["default_vat_code"].as_str().unwrap_or("—")
            ));
        }
        text.push_str(&format!("{} konti\n", rows.len()));
    } else if let Some(no) = v["accountNo"].as_str() {
        text = format!("Konto {no} oprettet\n");
    } else {
        if let Some(r) = v.get("resolution") {
            text.push_str(&format!(
                "Rolle {} bekræftet: konto {}, version {}\n",
                r["role"].as_str().unwrap(),
                r["accountNo"].as_str().unwrap(),
                r["version"]
            ));
        }
        text.push_str(&format!(
            "Kontoroller: {}\n",
            v["status"].as_str().unwrap_or("ukendt")
        ));
        for key in ["missing", "ambiguous"] {
            let roles = v[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            if !roles.is_empty() {
                text.push_str(&format!("{key}: {roles}\n"));
            }
        }
    }
    Output {
        stdout: text,
        stderr: String::new(),
        exit,
    }
}
pub fn execute(args: &[String]) -> Output {
    if args.len() == 1 || args.iter().any(|s| s == "--help") {
        return Output {
            stdout: format!("{}\n", help()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let p = match parse(args) {
        Ok(p) => p,
        Err(e) => return e,
    };
    for k in ["--company", "--server"] {
        if p.flags.get(k).is_none_or(|s| s.is_empty()) {
            return fail(&format!("Explicit {k} required"), 2);
        }
    }
    if p.flags.contains_key("--cookie-file") == p.flags.contains_key("--service-key-file") {
        return fail("Choose exactly one cookie-file/service-key-file", 2);
    }
    let api = match crate::remote::credential_client(p.flags["--server"], p.http, &p.flags, false) {
        Ok(api) => api,
        Err((exit, e)) => return fail(&e.message, exit),
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return fail("HTTP runtime unavailable", 1),
    };
    match rt.block_on(api.original_accounts(
        p.flags["--company"],
        p.op,
        &p.body,
        p.flags.get("--actor").copied(),
        p.flags.get("--principal").copied(),
    )) {
        Ok(v) => output(v, p.json),
        Err(e) => {
            let (exit, e) = crate::remote::client_error(e);
            if p.json && exit != 2 {
                output(json!({"ok":false,"code":e.code,"errors":[e.message]}), true)
            } else {
                fail(&e.message, exit)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_boolean_remains_case_insensitive() {
        for (flag, expected) in [("TRUE", true), ("False", false), ("true", true)] {
            let args: Vec<String> = [
                "accounts",
                "add",
                "--account-no",
                "1790",
                "--name",
                "Synthetic",
                "--type",
                "asset",
                "--confirm",
                "yes",
                "--allow-direct-posting",
                flag,
            ]
            .iter()
            .map(|s| s.to_string())
            .collect();
            let parsed = super::parse(&args).unwrap_or_else(|_| panic!("source boolean refused"));
            assert_eq!(parsed.body["input"]["allowDirectPosting"], expected);
        }
    }
}
