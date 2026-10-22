//! Original payable flags and human display; no client balances, aging or filtering.
use crate::dated_reports::Output;
use rentemester_client::{PayableOriginalMutation, PayableOriginalQuery};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "payable")
        && args
            .get(1)
            .is_some_and(|s| matches!(s.as_str(), "register" | "list" | "pay"))
}
pub fn help() -> Value {
    json!({"usage":"payable register --document-id ID --bill-date DATE --due-date DATE --expense-account ACCOUNT [--vat-treatment standard|exempt|non_deductible] [--vendor-id ID] [--note TEXT]; payable pay --payable-id ID --bank-transaction-id ID [--amount NUMBER] [--date DATE] [--payment-account ACCOUNT] [--note TEXT]; payable list [--status all|open|paid|overdue] [--as-of DATE] [--supplier TEXT] [--vendor-id ID] [--from DATE] [--to DATE] [--min-days N]", "transport":"Explicit --company SLUG --server ORIGIN or global --local-profile; exactly one --cookie-file PATH/--service-key-file PATH. Optional --json/--allow-loopback-http. Register/pay invocation confirms the write; no --confirm, --idempotency-key, actor or principal flags. No automatic retries or local financial/filter/aging calculations."})
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
                "Kreditor\n{}\n",
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
        .unwrap_or_else(|| "Payable action refused".into());
    Output {
        stdout: String::new(),
        stderr: format!("{message}\n"),
        exit,
    }
}
fn failure(message: &str, json_mode: bool, exit: u8) -> Output {
    output(json!({"ok":false,"errors":[message]}), json_mode, exit)
}

fn show(value: &Value) -> String {
    value.as_str().map(str::to_owned).unwrap_or_else(|| {
        if value.is_null() {
            "—".into()
        } else {
            value.to_string()
        }
    })
}
fn kroner(value: &Value) -> String {
    let Some(number) = value.as_f64() else {
        return "—".into();
    };
    let rendered = format!("{number:.2}");
    let Some((whole, fraction)) = rendered.split_once('.') else {
        return "—".into();
    };
    let digits = whole.strip_prefix('-').unwrap_or(whole);
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(c);
    }
    format!(
        "{}{},{} kr.",
        if whole.starts_with('-') { "-" } else { "" },
        grouped,
        fraction
    )
}
fn human_list(value: &Value) -> String {
    let mut lines = vec![
        format!(
            "Kreditorliste ({}) — pr. {}",
            show(&value["count"]),
            show(&value["asOfDate"])
        ),
        format!(
            "  Åben saldo i alt: {} (forfaldne: {}, ikke-forfaldne: {})",
            kroner(&value["totalOpenBalance"]),
            kroner(&value["overdueOpenBalance"]),
            kroner(&value["notYetDueOpenBalance"])
        ),
    ];
    let rows = value["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if rows.is_empty() {
        lines.push("Ingen kreditorposter for det valgte filter.".into());
    }
    for row in rows {
        let aging = if row["isOverdue"] == true {
            format!(
                "Forfalden {} dage (interval {})",
                show(&row["overdueDays"]),
                show(&row["agingBucket"])
            )
        } else if row["status"] == "paid" {
            "Betalt".into()
        } else {
            "Ikke forfalden".into()
        };
        lines.push(format!("\n#{} — {} | bilag {}\n  Bilagsdato: {} | Forfald: {}\n  Brutto: {} | Betalt: {} | Åben: {}\n  Status: {aging}",show(&row["payableId"]),row["supplierName"].as_str().unwrap_or("Ukendt leverandør"),show(if row["billNo"].is_null(){&row["documentId"]}else{&row["billNo"]}),show(&row["billDate"]),show(&row["dueDate"]),kroner(&row["grossAmount"]),kroner(&row["paidAmount"]),kroner(&row["openBalance"])));
    }
    format!("{}\n", lines.join("\n"))
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
    let kind = args[1].as_str();
    let (mut json_mode, mut http) = (false, false);
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
    let allowed = match kind {
        "register" => vec![
            "--document-id",
            "--bill-date",
            "--due-date",
            "--expense-account",
            "--vat-treatment",
            "--vendor-id",
            "--note",
        ],
        "pay" => vec![
            "--payable-id",
            "--bank-transaction-id",
            "--amount",
            "--date",
            "--payment-account",
            "--note",
        ],
        _ => vec![
            "--status",
            "--as-of",
            "--supplier",
            "--vendor-id",
            "--from",
            "--to",
            "--min-days",
        ],
    };
    while cursor < args.len() {
        let (key, attached) = crate::original_args::option(&args[cursor]);
        if key == "--json" && attached.is_none() && !json_mode {
            json_mode = true;
            cursor += 1;
            continue;
        }
        if key == "--allow-loopback-http" && attached.is_none() && !http {
            http = true;
            cursor += 1;
            continue;
        }
        if ![
            "--server",
            "--company",
            "--cookie-file",
            "--service-key-file",
        ]
        .contains(&key)
            && !allowed.contains(&key)
        {
            return failure(&format!("Unknown/duplicate payable flag {key}"), false, 2);
        }
        let Some((value, next)) = crate::original_args::value(args, cursor) else {
            return failure(&format!("Missing value for {key}"), false, 2);
        };
        if flags.insert(key, value).is_some() {
            return failure(&format!("Duplicate {key}"), false, 2);
        }
        cursor = next;
    }
    let mut required = vec!["--server", "--company"];
    match kind {
        "register" => required.extend([
            "--document-id",
            "--bill-date",
            "--due-date",
            "--expense-account",
        ]),
        "pay" => required.extend(["--payable-id", "--bank-transaction-id"]),
        _ => {}
    }
    for key in required {
        if flags.get(key).is_none_or(|s| s.is_empty()) {
            return failure(&format!("Required {key}"), false, 2);
        }
    }
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") {
        return failure("Choose exactly one cookie-file/service-key-file", false, 2);
    }
    let mut body = json!({"confirm":true});
    let mut query = PayableOriginalQuery::default();
    for (flag, key) in [
        ("--document-id", "documentId"),
        ("--payable-id", "payableId"),
        ("--bank-transaction-id", "bankTransactionId"),
        ("--vendor-id", "vendorId"),
        ("--min-days", "minDays"),
    ] {
        if let Some(text) = flags.get(flag) {
            let Some(id) = text
                .parse::<u64>()
                .ok()
                .filter(|n| *n <= 9007199254740991 && (*n > 0 || flag == "--min-days"))
            else {
                return failure(
                    &format!("Safe nonnegative minDays/positive ID required for {flag}"),
                    false,
                    2,
                );
            };
            body[key] = json!(id);
            if flag == "--vendor-id" {
                query.vendor_id = Some(id);
            }
            if flag == "--min-days" {
                query.min_days = Some(id);
            }
        }
    }
    for (flag, key) in [
        ("--bill-date", "billDate"),
        ("--due-date", "dueDate"),
        ("--expense-account", "expenseAccount"),
        ("--vat-treatment", "vatTreatment"),
        ("--note", "note"),
        ("--date", "date"),
        ("--payment-account", "paymentAccount"),
    ] {
        if let Some(text) = flags.get(flag) {
            body[key] = json!(text);
        }
    }
    if let Some(text) = flags.get("--amount") {
        let Some(amount) = text
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n > 0.0)
        else {
            return failure("--amount must be a positive finite number", false, 2);
        };
        body["amount"] = json!(amount);
    }
    for (flag, slot) in [
        ("--status", &mut query.status),
        ("--as-of", &mut query.as_of),
        ("--supplier", &mut query.supplier),
        ("--from", &mut query.from),
        ("--to", &mut query.to),
    ] {
        *slot = flags.get(flag).map(|s| (*s).to_owned());
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
    let result = rt.block_on(async {
        if kind == "list" {
            api.payable_original_list(flags["--company"], &query).await
        } else {
            api.payable_original_mutation(
                flags["--company"],
                if kind == "register" {
                    PayableOriginalMutation::Register
                } else {
                    PayableOriginalMutation::Pay
                },
                serde_json::to_vec(&body).unwrap(),
            )
            .await
        }
    });
    match result {
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
            if exit == 0 && !json_mode && kind == "list" {
                return Output {
                    stdout: human_list(&value),
                    stderr: String::new(),
                    exit,
                };
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
