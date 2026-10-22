//! Original read commands; no local filtering, account lookup or accounting calculations.
use crate::dated_reports::Output;
use rentemester_client::BankQuery;
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn output(value: Value, json_mode: bool, exit: u8) -> Output {
    if json_mode && exit != 2 {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
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
        .unwrap_or_else(|| "Bank query refused".into());
    Output {
        stdout: String::new(),
        stderr: format!("{message}\n"),
        exit,
    }
}
fn usage(message: &str) -> Output {
    output(json!({"ok":false,"errors":[message]}), false, 2)
}
pub fn accepts(args: &[String]) -> bool {
    args.len() >= 2
        && matches!(
            (args[0].as_str(), args[1].as_str()),
            ("bank", "list") | ("reconcile", "bank") | ("bank-account", "list")
        )
}
pub fn help() -> Value {
    json!({"usage":"bank list | reconcile bank --from DATE --to DATE | bank-account list; each requires --server ORIGIN --company SLUG and exactly one --cookie-file PATH/--service-key-file PATH (or global --local-profile instead of server). Optional --json, --allow-loopback-http. Bank list/reconcile filters: --status all|matched|unmatched --from DATE --to DATE --text-match TEXT --amount ORIGINAL_UNITS --account ID_OR_SLUG.","contract":"One all-year canonical server read; bank list is complete/unpaged, not the MCP500-row page. No CLI pagination/year filter, local account lookup/math, actor or implicit filesystem/default company selection. Reconciliation totals retain signed original units and may mix currencies; never DKK-coalesced. Bank-account list includes inactive accounts. Parse errors2, business/source/auth refusal1, success0."})
}
struct Input<'a> {
    kind: &'a str,
    flags: BTreeMap<&'a str, &'a str>,
    json: bool,
    http: bool,
}
fn parse(args: &[String]) -> Result<Input<'_>, String> {
    if !accepts(args) {
        return Err("Expected bank list, reconcile bank or bank-account list".into());
    }
    let kind = args[0].as_str();
    let mut flags = BTreeMap::new();
    let (mut json, mut http) = (false, false);
    let mut cursor = 2;
    while cursor < args.len() {
        let (key, attached) = crate::original_args::option(&args[cursor]);
        match key {
            "--json" if attached.is_none() && !json => {
                json = true;
                cursor += 1;
            }
            "--allow-loopback-http" if attached.is_none() && !http => {
                http = true;
                cursor += 1;
            }
            "--server" | "--company" | "--cookie-file" | "--service-key-file" | "--status"
            | "--from" | "--to" | "--text-match" | "--amount" | "--account" => {
                if kind == "bank-account"
                    && matches!(
                        key,
                        "--status" | "--from" | "--to" | "--text-match" | "--amount" | "--account"
                    )
                {
                    return Err("bank-account list accepts no transaction filters".into());
                }
                let (value, next) = crate::original_args::value(args, cursor)
                    .ok_or_else(|| format!("Missing value for {key}"))?;
                if flags.insert(key, value).is_some() {
                    return Err(format!("Duplicate {key}"));
                }
                cursor = next;
            }
            _ => return Err(format!("Unknown or duplicate bank read flag {key}")),
        }
    }
    for key in ["--server", "--company"] {
        if flags.get(key).is_none_or(|v| v.is_empty()) {
            return Err(format!("Explicit {key} required"));
        }
    }
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") {
        return Err("Choose exactly one --cookie-file or --service-key-file".into());
    }
    if kind == "reconcile"
        && ["--from", "--to"]
            .iter()
            .any(|key| flags.get(key).is_none_or(|v| v.is_empty()))
    {
        return Err("Reconcile bank requires --from and --to".into());
    }
    if flags
        .get("--amount")
        .is_some_and(|v| v.trim().parse::<f64>().ok().is_none_or(|n| !n.is_finite()))
    {
        return Err("--amount must be finite numeric original-currency units".into());
    }
    Ok(Input {
        kind,
        flags,
        json,
        http,
    })
}
fn flat(value: Value) -> Value {
    if value["ok"] != true {
        return value;
    }
    let mut data = value["data"].clone();
    for key in ["ok", "errors", "appliedRules"] {
        if let Some(v) = value.get(key) {
            data[key] = v.clone();
        }
    }
    data
}
fn display(value: &Value) -> String {
    if value.is_null() {
        "—".into()
    } else {
        value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string())
    }
}
fn human(kind: &str, value: &Value) -> String {
    let mut lines = vec![];
    match kind {
        "bank" => {
            lines.push(format!("Banktransaktioner ({})", display(&value["count"])));
            for row in value["rows"].as_array().unwrap() {
                lines.push(format!(
                    "#{} — {} | {} {}\n  Tekst: {}\n  Status: {} | {}\n  Postering: {}",
                    display(&row["id"]),
                    display(&row["transactionDate"]),
                    display(&row["amount"]),
                    display(&row["currency"]),
                    display(&row["text"]),
                    display(&row["ledgerStatus"]),
                    display(&row["reconciliationStatus"]),
                    display(&row["journalEntryNo"])
                ));
            }
        }
        "reconcile" => {
            lines.push(format!(
                "Bankafstemning {} til {}",
                display(&value["periodStart"]),
                display(&value["periodEnd"])
            ));
            lines.push(format!("Afstemt: {} | Sum: {} originale valutaenheder\nUafstemt: {} | Sum: {} originale valutaenheder",display(&value["matchedCount"]),display(&value["matchedAmountTotal"]),display(&value["unmatchedCount"]),display(&value["unmatchedAmountTotal"])));
            for key in ["matched", "unmatched"] {
                for row in value[key].as_array().unwrap() {
                    lines.push(format!(
                        "{} #{} — {} | {} | {}",
                        if key == "matched" {
                            "Afstemt"
                        } else {
                            "Uafstemt"
                        },
                        display(&row["bankTransactionId"]),
                        display(&row["transactionDate"]),
                        display(&row["amount"]),
                        display(&row["text"])
                    ));
                }
            }
        }
        _ => {
            lines.push(format!("Bankkonti ({})", display(&value["count"])));
            for row in value["accounts"].as_array().unwrap() {
                lines.push(format!(
                    "#{} {} — {} | {} | Aktiv: {} | Finanskonto: {} | IBAN: {}",
                    display(&row["id"]),
                    display(&row["slug"]),
                    display(&row["name"]),
                    display(&row["currency"]),
                    display(&row["active"]),
                    display(&row["ledgerAccountNo"]),
                    display(&row["iban"])
                ));
            }
        }
    }
    lines.join("\n") + "\n"
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
    let input = match parse(args) {
        Ok(v) => v,
        Err(e) => return usage(&e),
    };
    let company = input.flags["--company"];
    if !company
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return output(
            json!({"ok":false,"code":"local_company_unavailable","errors":["Use a connected-server company slug; legacy filesystem/default selectors remain unsupported."]}),
            input.json,
            1,
        );
    }
    let api = match crate::remote::credential_client(
        input.flags["--server"],
        input.http,
        &input.flags,
        false,
    ) {
        Ok(api) => api,
        Err((code, e)) => {
            return output(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                input.json,
                code,
            );
        }
    };
    let mut query = BankQuery::default();
    for (flag, slot) in [
        ("--status", &mut query.status),
        ("--from", &mut query.from),
        ("--to", &mut query.to),
        ("--text-match", &mut query.text_match),
        ("--amount", &mut query.amount),
        ("--account", &mut query.account),
    ] {
        *slot = input.flags.get(flag).map(|s| (*s).to_owned());
    }
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => {
            return output(
                json!({"ok":false,"errors":["HTTP runtime unavailable"]}),
                input.json,
                1,
            );
        }
    };
    let response = rt.block_on(async {
        match input.kind {
            "bank" => api.bank_transactions(company, &query).await,
            "reconcile" => api.bank_reconciliation(company, &query).await,
            _ => api.bank_account_list(company, None).await,
        }
    });
    let value = match response {
        Ok(v) => flat(v),
        Err(e) => {
            let (code, error) = crate::remote::client_error(e);
            return output(
                json!({"ok":false,"code":error.code,"errors":[error.message]}),
                input.json,
                code,
            );
        }
    };
    if value["ok"] != true {
        return output(value, input.json, 1);
    }
    if input.json {
        output(value, true, 0)
    } else {
        Output {
            stdout: human(input.kind, &value),
            stderr: String::new(),
            exit: 0,
        }
    }
}
