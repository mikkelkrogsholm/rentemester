//! Original invoice lifecycle parsing only; no totals, VAT/FX, accounting or local company writes.
use crate::dated_reports::Output;
use rentemester_client::{InvoiceLifecycleKind, validate_invoice_lifecycle};
use serde_json::{Number, Value, json};
use std::{collections::BTreeMap, io::Read};
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "invoice")
        && args.get(1).is_some_and(|s| {
            matches!(
                s.as_str(),
                "issue"
                    | "create"
                    | "render"
                    | "post"
                    | "settle-bank"
                    | "credit-note"
                    | "refund-bank"
            )
        })
}
pub fn help() -> Value {
    json!({"usage":"invoice issue --input FILE|- [--customer-id ID]; invoice create --issue-date DATE --line 'description|quantity|unitPrice;...' [--vat-rate PERCENT(default25)] [--customer-id ID] [--buyer-name/--buyer-address/--buyer-vat TEXT] [--seller-name/--seller-address/--seller-vat TEXT] [--currency CODE] [--due-date DATE] [--invoice-number NUMBER]; invoice render|post --document-id ID|--invoice-number NUMBER (post requires --revenue-account ACCOUNT); invoice settle-bank|credit-note|refund-bank --input FILE|-", "transport":"Explicit --company SLUG --server ORIGIN or global --local-profile, exactly one --cookie-file PATH/--service-key-file PATH. Optional --json/--allow-loopback-http. Command invocation confirms; no --confirm, key, actor or principal. Bounded256KiB JSON object; no local files written, arithmetic/default account, hash generation or automatic retry. Server computes create totals and validates all issue-supplied totals; render verifies existing immutable PDF."})
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
                "Faktura\n{}\n",
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

fn number(text: &str) -> Option<Number> {
    text.trim().parse::<Number>().ok().or_else(|| {
        text.trim()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .and_then(Number::from_f64)
    })
}
fn read_input(path: &str) -> Result<(Value, Vec<u8>), &'static str> {
    let mut bytes = vec![];
    if path == "-" {
        std::io::stdin()
            .take(262145)
            .read_to_end(&mut bytes)
            .map_err(|_| "Could not read input JSON")?;
    } else {
        std::fs::File::open(path)
            .map_err(|_| "Could not open input JSON")?
            .take(262145)
            .read_to_end(&mut bytes)
            .map_err(|_| "Could not read input JSON")?;
    }
    if bytes.len() > 262144 {
        return Err("Input JSON exceeds256KiB");
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Input must be valid JSON object")?;
    if !value.is_object() {
        return Err("Input must be one JSON object");
    }
    Ok((value, bytes))
}
pub fn execute(args: &[String]) -> Output {
    if args.iter().skip(2).any(|s| s == "--help") {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&help()).unwrap()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let kind = match args[1].as_str() {
        "issue" => InvoiceLifecycleKind::Issue,
        "create" => InvoiceLifecycleKind::Create,
        "render" => InvoiceLifecycleKind::Render,
        "post" => InvoiceLifecycleKind::Post,
        "settle-bank" => InvoiceLifecycleKind::SettleBank,
        "credit-note" => InvoiceLifecycleKind::CreditNote,
        _ => InvoiceLifecycleKind::RefundBank,
    };
    let mut flags = BTreeMap::new();
    let (mut json_mode, mut http) = (false, false);
    let mut cursor = 2;
    let allowed = match kind {
        InvoiceLifecycleKind::Issue => vec!["--input", "--customer-id"],
        InvoiceLifecycleKind::Create => vec![
            "--issue-date",
            "--line",
            "--vat-rate",
            "--customer-id",
            "--buyer-name",
            "--buyer-address",
            "--buyer-vat",
            "--seller-name",
            "--seller-address",
            "--seller-vat",
            "--currency",
            "--due-date",
            "--invoice-number",
        ],
        InvoiceLifecycleKind::Render => vec!["--document-id", "--invoice-number"],
        InvoiceLifecycleKind::Post => {
            vec!["--document-id", "--invoice-number", "--revenue-account"]
        }
        _ => vec!["--input"],
    };
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
        ]
        .contains(&key)
            && !allowed.contains(&key)
        {
            return failure(
                &format!("Unknown/duplicate invoice lifecycle flag {key}"),
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
    for key in ["--server", "--company"] {
        if flags.get(key).is_none_or(|s| s.is_empty()) {
            return failure(&format!("Required {key}"), false, 2);
        }
    }
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") {
        return failure("Choose exactly one cookie-file/service-key-file", false, 2);
    }
    let mut body = json!({"confirm":true});
    if let Some(text) = flags.get("--customer-id") {
        let Some(id) = text
            .parse::<i64>()
            .ok()
            .filter(|n| (1..=9007199254740991).contains(n))
        else {
            return failure("Positive safe customer-id required", false, 2);
        };
        body["customerId"] = json!(id);
    }
    let mut raw_payload = None;
    match kind {
        InvoiceLifecycleKind::Render | InvoiceLifecycleKind::Post => {
            if !flags.contains_key("--document-id")
                && flags
                    .get("--invoice-number")
                    .is_none_or(|s| s.trim().is_empty())
            {
                return failure(
                    "Missing required --document-id or --invoice-number",
                    false,
                    2,
                );
            }
            if let Some(text) = flags.get("--document-id") {
                let Some(id) = text
                    .parse::<i64>()
                    .ok()
                    .filter(|n| (1..=9007199254740991).contains(n))
                else {
                    return failure("Positive safe document-id required", false, 2);
                };
                body["documentId"] = json!(id);
            }
            if let Some(text) = flags.get("--invoice-number") {
                body["invoiceNumber"] = json!(text);
            }
            if let Some(text) = flags.get("--revenue-account") {
                body["revenueAccountNo"] = json!(text);
            }
        }
        InvoiceLifecycleKind::Create => {
            let Some(date) = flags.get("--issue-date").filter(|s| !s.is_empty()) else {
                return failure("Missing required --issue-date", false, 2);
            };
            let mut lines = vec![];
            for segment in flags
                .get("--line")
                .copied()
                .unwrap_or("")
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                let parts = segment.split('|').map(str::trim).collect::<Vec<_>>();
                if parts.len() != 3 {
                    return failure(
                        "Each line must have description|quantity|unitPrice",
                        json_mode,
                        1,
                    );
                }
                let (Some(quantity), Some(price)) = (number(parts[1]), number(parts[2])) else {
                    return failure(
                        "Each line quantity/unit price must be numeric",
                        json_mode,
                        1,
                    );
                };
                lines.push(
                    json!({"description":parts[0],"quantity":quantity,"unitPriceExVat":price}),
                );
            }
            if lines.is_empty() {
                return failure(
                    "At least one --line description|quantity|unitPrice is required",
                    json_mode,
                    1,
                );
            }
            let Some(vat) = number(flags.get("--vat-rate").copied().unwrap_or("25")) else {
                return failure("--vat-rate must be numeric percent", false, 2);
            };
            let mut payload = json!({"invoiceType":"full","vatTreatment":"standard","issueDate":date,"lines":lines,"vatRatePercent":vat,"currency":flags.get("--currency").map(|s|s.trim()).filter(|s|!s.is_empty()).unwrap_or("DKK"),"seller":{},"buyer":{}});
            for (prefix, party) in [("seller", "seller"), ("buyer", "buyer")] {
                for (suffix, key) in [
                    ("name", "name"),
                    ("address", "address"),
                    ("vat", "vatOrCvr"),
                ] {
                    let flag = format!("--{prefix}-{suffix}");
                    if let Some(text) = flags
                        .get(flag.as_str())
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                    {
                        payload[party][key] = json!(text);
                    }
                }
            }
            for (flag, key) in [
                ("--due-date", "dueDate"),
                ("--invoice-number", "invoiceNumber"),
            ] {
                if let Some(text) = flags.get(flag).map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    payload[key] = json!(text);
                }
            }
            body["payload"] = payload;
        }
        _ => {
            let Some(path) = flags.get("--input").filter(|s| !s.is_empty()) else {
                return failure("Missing required --input FILE|-", false, 2);
            };
            body["payload"] = match read_input(path) {
                Ok((value, raw)) => {
                    raw_payload = Some(raw);
                    value
                }
                Err(e) => return failure(e, false, 2),
            };
        }
    }
    let bytes = if let Some(raw) = raw_payload {
        // Keep duplicate keys visible to strict server validation instead of normalizing a file.
        body.as_object_mut().unwrap().remove("payload");
        let mut bytes = serde_json::to_vec(&body).unwrap();
        bytes.pop();
        bytes.extend_from_slice(b",\"payload\":");
        bytes.extend_from_slice(&raw);
        bytes.push(b'}');
        bytes
    } else {
        serde_json::to_vec(&body).unwrap()
    };
    if let Err(e) = validate_invoice_lifecycle(kind, &bytes) {
        return failure(&e.to_string(), false, 2);
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
    match rt.block_on(api.invoice_lifecycle(flags["--company"], kind, bytes)) {
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
