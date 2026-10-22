//! Original claim flags only. Server owns amounts, rates, dates, accounts and evidence.
use crate::dated_reports::Output;
use rentemester_client::{
    OriginalInvoiceClaimKind as Kind, invoice_claims_original_schema,
    validate_invoice_claims_original,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "invoice")
        && args
            .get(1)
            .is_some_and(|s| Kind::ALL.iter().any(|k| k.suffix() == s))
}
pub fn help() -> Value {
    json!({"commands":Kind::ALL.map(|k|k.suffix()),"selectors":"--document-id ID | --invoice-number NUMBER (settle-claim-bank uses --input FILE|-)","flags":"remind --date DATE [--fee NUMBER] [--note TEXT]; compensation|claim-compensation --as-of DATE [--amount-dkk NUMBER] [claim: --note TEXT]; interest|claim-interest --as-of DATE [--reference-rate NUMBER] [claim: --note TEXT]; post-reminder [--reminder-id ID] [--date DATE] --receivable-account NO --reminder-income-account NO; post-compensation [--date DATE] --receivable-account NO --compensation-income-account NO; post-interest [--claim-id ID] [--date DATE] --receivable-account NO --interest-income-account NO; interest-correction; post-interest-correction [--date DATE] --reason TEXT; settle-claim-bank --input FILE|-","transport":"Explicit --company SLUG --server ORIGIN or global --local-profile; one --cookie-file/--service-key-file; --json/--allow-loopback-http. Mutation invocation confirms. No --confirm, keys or identity. Omitted fee/rate/amount/date/id defaults resolved only on server. Registrations do not post. No financial math, inferred accounts, local company writes or retries."})
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

fn flag(key: &str) -> Option<&'static str> {
    Some(match key {
        "documentId" => "--document-id",
        "invoiceNumber" => "--invoice-number",
        "date" => "--date",
        "asOf" => "--as-of",
        "fee" => "--fee",
        "note" => "--note",
        "reminderId" => "--reminder-id",
        "amountDkk" => "--amount-dkk",
        "referenceRate" => "--reference-rate",
        "claimId" => "--claim-id",
        "reason" => "--reason",
        "receivableAccountNo" => "--receivable-account",
        "reminderIncomeAccountNo" => "--reminder-income-account",
        "compensationIncomeAccountNo" => "--compensation-income-account",
        "interestIncomeAccountNo" => "--interest-income-account",
        _ => return None,
    })
}
pub fn execute(args: &[String]) -> Output {
    if args.iter().skip(2).any(|s| s == "--help") {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&help()).unwrap()),
            stderr: String::new(),
            exit: 0,
        };
    }
    let kind = Kind::ALL
        .into_iter()
        .find(|k| k.suffix() == args[1])
        .unwrap();
    let schema = invoice_claims_original_schema(kind);
    let properties = schema["properties"].as_object().unwrap();
    let (mut json_mode, mut http) = (false, false);
    let mut flags = BTreeMap::new();
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
        let allowed = [
            "--server",
            "--company",
            "--cookie-file",
            "--service-key-file",
        ]
        .contains(&key)
            || kind == Kind::SettleClaimBank && key == "--input"
            || properties.keys().any(|s| flag(s) == Some(key));
        if !allowed {
            return failure(
                &format!("Unknown/duplicate original claim flag {key}"),
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
        if flags.get(key).is_none_or(|v| v.is_empty()) {
            return failure(&format!("Required {key}"), false, 2);
        }
    }
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") {
        return failure("Choose exactly one cookie-file/service-key-file", false, 2);
    }
    let mut body = json!({});
    if !kind.is_read_only() {
        body["confirm"] = json!(true);
    }
    let bytes = if kind == Kind::SettleClaimBank {
        let Some(path) = flags.get("--input").filter(|s| !s.is_empty()) else {
            return failure("Missing required --input FILE|-", false, 2);
        };
        let raw = match read_input(path) {
            Ok((_, raw)) => raw,
            Err(error) => return failure(error, false, 2),
        };
        let mut bytes = b"{\"confirm\":true,\"payload\":".to_vec();
        bytes.extend(raw);
        bytes.push(b'}');
        bytes
    } else {
        if !flags.contains_key("--document-id")
            && flags
                .get("--invoice-number")
                .is_none_or(|s| s.trim().is_empty())
        {
            return failure("Required --document-id or --invoice-number", false, 2);
        }
        for (key, shape) in properties {
            let Some(flag) = flag(key) else {
                continue;
            };
            if let Some(text) = flags.get(flag) {
                body[key] = match shape["type"].as_str() {
                    Some("integer") => match text
                        .parse::<i64>()
                        .ok()
                        .filter(|n| (1..=9007199254740991).contains(n))
                    {
                        Some(id) => json!(id),
                        None => {
                            return failure(&format!("Positive safe {flag} required"), false, 2);
                        }
                    },
                    Some("number") => match text.trim().parse::<serde_json::Number>() {
                        Ok(number) => json!(number),
                        Err(_) => return failure(&format!("Numeric {flag} required"), false, 2),
                    },
                    _ => json!(text),
                };
            }
        }
        for key in schema["required"].as_array().unwrap() {
            let key = key.as_str().unwrap();
            if key == "confirm" {
                continue;
            }
            if !body
                .get(key)
                .is_some_and(|v| !v.is_string() || !v.as_str().unwrap().trim().is_empty())
            {
                return failure(&format!("Required {}", flag(key).unwrap()), false, 2);
            }
        }
        serde_json::to_vec(&body).unwrap()
    };
    if let Err(error) = validate_invoice_claims_original(kind, &bytes) {
        return failure(&error.to_string(), false, 2);
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
    match rt.block_on(api.invoice_claims_original(flags["--company"], kind, bytes)) {
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
