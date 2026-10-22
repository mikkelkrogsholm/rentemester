//! Original invoice read flags/positional search and practical server-owned result display.
use crate::dated_reports::Output;
use rentemester_client::{InvoiceOriginalKind, InvoiceOriginalQuery};
use serde_json::{Number, Value, json};
use std::collections::BTreeMap;
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "invoice")
        && args
            .get(1)
            .is_some_and(|s| matches!(s.as_str(), "list" | "find" | "status" | "overdue"))
}
pub fn help() -> Value {
    json!({"usage":"invoice list [--status STATUS] [--from DATE] [--to DATE] [--customer-cvr CVR] [--customer TEXT] [--invoice-number NUMBER] [--min-amount NUMBER] [--max-amount NUMBER] [--as-of DATE]; invoice find [POSITIONAL WORDS...] [--customer TEXT] [--invoice-number NUMBER] [--amount NUMBER] [--as-of DATE]; invoice status (--document-id ID | --invoice-number NUMBER) [--as-of DATE]; invoice overdue [--min-days N] [--as-of DATE]", "transport":"Explicit --company SLUG --server ORIGIN or global --local-profile; exactly one --cookie-file PATH/--service-key-file PATH. Optional --json/--allow-loopback-http. Status documentId takes precedence; missing selector2, unmatched1. Find amount is exact original-currency gross amount; negative bounds and blank filters are valid source inputs. Server owns all current histories/aging/FX separation. No confirmation, actor, keys, local calculations, fiscal default or retries."})
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
                "Fakturastatus\n{}\n",
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
        .unwrap_or_else(|| "Invoice read refused".into());
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

fn status_da(value: &Value) -> String {
    match value.as_str().unwrap_or("") {
        "open" => "åben".into(),
        "paid" => "betalt".into(),
        "credited" => "krediteret".into(),
        "refunded" => "refunderet".into(),
        "overpaid" => "overbetalt".into(),
        "written_off" => "afskrevet".into(),
        _ => show(value),
    }
}
fn human(kind: InvoiceOriginalKind, value: &Value) -> String {
    if matches!(kind, InvoiceOriginalKind::Status) {
        return format!(
            "Fakturastatus {} — {}\n{}\n",
            show(&value["invoiceNumber"]),
            status_da(&value["status"]),
            serde_json::to_string_pretty(value).unwrap()
        );
    }
    let heading = match kind {
        InvoiceOriginalKind::Find => format!("Fakturatræf ({})", show(&value["count"])),
        InvoiceOriginalKind::Overdue => format!(
            "Forfaldne fakturaer pr. {} ({})",
            show(&value["asOfDate"]),
            show(&value["count"])
        ),
        _ => format!("Fakturaer ({})", show(&value["count"])),
    };
    let mut lines = vec![heading];
    let rows = value["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if rows.is_empty() {
        lines.push("Ingen fakturaer for det valgte filter.".into());
    }
    for row in rows {
        let overdue = if row["isOverdue"] == true {
            format!(" | {} dage forfalden", show(&row["overdueDays"]))
        } else {
            String::new()
        };
        lines.push(format!("\nFaktura {} — {}\n  Fakturadato: {} | Forfald: {}\n  Beløb (inkl. moms): {} {} | Åben saldo: {} {}\n  Status: {}{overdue}",show(&row["invoiceNumber"]),row["customerName"].as_str().unwrap_or("(ukendt kunde)"),show(&row["invoiceDate"]),show(&row["effectiveDueDate"]),show(&row["grossAmount"]),show(&row["currency"]),show(&row["openBalance"]),show(&row["currency"]),status_da(&row["status"])));
    }
    format!("{}\n", lines.join("\n"))
}
fn number(text: &str) -> Option<Number> {
    text.trim().parse::<Number>().ok().or_else(|| {
        text.trim()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .and_then(Number::from_f64)
    })
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
    let kind = match args[1].as_str() {
        "list" => InvoiceOriginalKind::List,
        "find" => InvoiceOriginalKind::Find,
        "status" => InvoiceOriginalKind::Status,
        _ => InvoiceOriginalKind::Overdue,
    };
    let mut flags = BTreeMap::new();
    let (mut json_mode, mut http) = (false, false);
    let mut words = vec![];
    let mut cursor = 2;
    let allowed = match kind {
        InvoiceOriginalKind::List => vec![
            "--status",
            "--from",
            "--to",
            "--customer-cvr",
            "--customer",
            "--invoice-number",
            "--min-amount",
            "--max-amount",
            "--as-of",
        ],
        InvoiceOriginalKind::Find => vec!["--customer", "--invoice-number", "--amount", "--as-of"],
        InvoiceOriginalKind::Status => vec!["--document-id", "--invoice-number", "--as-of"],
        InvoiceOriginalKind::Overdue => vec!["--min-days", "--as-of"],
    };
    while cursor < args.len() {
        if args[cursor] == "--" && matches!(kind, InvoiceOriginalKind::Find) {
            words.extend(args[cursor + 1..].iter().map(String::as_str));
            break;
        }
        let (key, attached) = crate::original_args::option(&args[cursor]);
        if key == "--json" && !json_mode && attached.is_none() {
            json_mode = true;
            cursor += 1;
            continue;
        }
        if key == "--allow-loopback-http" && !http && attached.is_none() {
            http = true;
            cursor += 1;
            continue;
        }
        if !key.starts_with("--") && matches!(kind, InvoiceOriginalKind::Find) {
            words.push(args[cursor].as_str());
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
            return failure(&format!("Unknown/duplicate invoice flag {key}"), false, 2);
        }
        let Some((value, next)) = crate::original_args::value(args, cursor) else {
            return failure(&format!("Missing value for {key}"), false, 2);
        };
        if flags.insert(key, value).is_some() {
            return failure(&format!("Duplicate {key}"), false, 2);
        }
        cursor = next;
    }
    if matches!(kind, InvoiceOriginalKind::Status)
        && !flags.contains_key("--document-id")
        && flags
            .get("--invoice-number")
            .is_none_or(|s| s.trim().is_empty())
    {
        return failure(
            "Missing required --document-id <n> or --invoice-number <no>",
            false,
            2,
        );
    }
    for key in ["--server", "--company"] {
        if flags.get(key).is_none_or(|s| s.is_empty()) {
            return failure(&format!("Required {key}"), false, 2);
        }
    }
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") {
        return failure("Choose exactly one cookie-file/service-key-file", false, 2);
    }
    let mut query = InvoiceOriginalQuery::default();
    for (flag, slot) in [
        ("--status", &mut query.status),
        ("--from", &mut query.from),
        ("--to", &mut query.to),
        ("--customer-cvr", &mut query.customer_cvr),
        ("--customer", &mut query.customer),
        ("--invoice-number", &mut query.invoice_number),
        ("--as-of", &mut query.as_of),
    ] {
        *slot = flags.get(flag).map(|s| (*s).to_owned());
    }
    if !words.is_empty() {
        query.query = Some(words.join(" "));
    }
    for (flag, slot) in [
        ("--min-amount", &mut query.min_amount),
        ("--max-amount", &mut query.max_amount),
        ("--amount", &mut query.amount),
    ] {
        if let Some(text) = flags.get(flag) {
            let Some(value) = number(text) else {
                return failure(&format!("Finite numeric {flag} required"), false, 2);
            };
            *slot = Some(value);
        }
    }
    if let Some(text) = flags.get("--document-id") {
        let Some(id) = text
            .parse::<i64>()
            .ok()
            .filter(|n| (1..=9007199254740991).contains(n))
        else {
            return failure("Positive safe document ID required", false, 2);
        };
        query.document_id = Some(id);
    }
    if let Some(text) = flags.get("--min-days") {
        let Some(days) = text.parse::<u64>().ok().filter(|n| *n <= 9007199254740991) else {
            return failure("Nonnegative safe min-days required", false, 2);
        };
        query.min_days = Some(days);
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
    match rt.block_on(api.invoice_original(flags["--company"], kind, &query)) {
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
            if exit == 0 && !json_mode {
                return Output {
                    stdout: human(kind, &value),
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
