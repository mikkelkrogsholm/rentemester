//! Original bank command flags, confirmation and principal assertion; no client proof/math.
use crate::dated_reports::Output;
use rentemester_client::BankOriginalMutation;
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "bank")
        && args.get(1).is_some_and(|s| {
            matches!(
                s.as_str(),
                "suggest-matches" | "link-journal" | "correction-plan" | "correction-apply"
            )
        })
}
pub fn help() -> Value {
    json!({"usage":"bank suggest-matches [--bank-transaction-id ID] [--max N]; bank link-journal --bank-transaction-id ID --journal-entry-id ID --match-method METHOD [--source-reference TEXT] [--note TEXT] --confirm yes; bank correction-plan --bank-transaction-id ID --replacement-journal-entry-id ID; bank correction-apply --bank-transaction-id ID --replacement-journal-entry-id ID --expected-reconciliation-id ID --plan-hash HASH --reason TEXT --idempotency-key KEY --principal user:ID|service-account:ID --confirm yes", "transport":"Each requires --company SLUG, --server ORIGIN (or global --local-profile), exactly one --cookie-file PATH/--service-key-file PATH. Optional --json/--allow-loopback-http. Principal is only an expected authenticated identity assertion, never actor authority. Server owns full current proof, scores/hash/receipt. No automatic retry; retain exact key/reason after unknown outcome."})
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
    let command = args[1].as_str();
    let write = matches!(command, "link-journal" | "correction-apply");
    let mut flags = BTreeMap::new();
    let (mut json_mode, mut http) = (false, false);
    let mut cursor = 2;
    let selectors = match command {
        "suggest-matches" => vec!["--bank-transaction-id", "--max"],
        "link-journal" => vec![
            "--bank-transaction-id",
            "--journal-entry-id",
            "--match-method",
            "--source-reference",
            "--note",
            "--confirm",
        ],
        "correction-plan" => vec!["--bank-transaction-id", "--replacement-journal-entry-id"],
        _ => vec![
            "--bank-transaction-id",
            "--replacement-journal-entry-id",
            "--expected-reconciliation-id",
            "--plan-hash",
            "--reason",
            "--idempotency-key",
            "--principal",
            "--confirm",
        ],
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
            && !selectors.contains(&key)
        {
            return failure(&format!("Unknown/duplicate bank flag {key}"), false, 2);
        }
        let Some(value) = args.get(cursor + 1).filter(|s| !s.starts_with("--")) else {
            return failure(&format!("Missing value for {key}"), false, 2);
        };
        if flags.insert(key, value.as_str()).is_some() {
            return failure(&format!("Duplicate {key}"), false, 2);
        }
        cursor += 2;
    }
    if write && flags.get("--confirm") != Some(&"yes") {
        return failure(
            "Bank write requires the exact confirmation --confirm yes",
            false,
            2,
        );
    }
    for key in ["--company", "--server"] {
        if flags.get(key).is_none_or(|v| v.is_empty()) {
            return failure(&format!("Explicit {key} required"), false, 2);
        }
    }
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") {
        return failure("Choose exactly one cookie-file/service-key-file", false, 2);
    }
    let mut body = json!({"confirm":true});
    for (flag, key) in [
        ("--bank-transaction-id", "bankTransactionId"),
        ("--journal-entry-id", "journalEntryId"),
        (
            "--replacement-journal-entry-id",
            "replacementJournalEntryId",
        ),
        ("--max", "max"),
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
    let required = match command {
        "suggest-matches" => vec![],
        "link-journal" => vec![
            "--bank-transaction-id",
            "--journal-entry-id",
            "--match-method",
        ],
        "correction-plan" => vec!["--bank-transaction-id", "--replacement-journal-entry-id"],
        _ => vec![
            "--bank-transaction-id",
            "--replacement-journal-entry-id",
            "--expected-reconciliation-id",
            "--plan-hash",
            "--reason",
            "--idempotency-key",
            "--principal",
        ],
    };
    for key in required {
        if flags.get(key).is_none_or(|v| v.is_empty()) {
            return failure(&format!("Required {key}"), false, 2);
        }
    }
    for (flag, key) in [
        ("--match-method", "matchMethod"),
        ("--source-reference", "sourceReference"),
        ("--note", "note"),
        ("--expected-reconciliation-id", "expectedReconciliationId"),
        ("--plan-hash", "planHash"),
        ("--reason", "reason"),
        ("--idempotency-key", "idempotencyKey"),
    ] {
        if let Some(text) = flags.get(flag) {
            body[key] = json!(text);
        }
    }
    if let Some(principal) = flags.get("--principal") {
        let Some((kind, subject)) = principal.split_once(':').filter(|(kind, subject)| {
            matches!(*kind, "user" | "service-account") && !subject.trim().is_empty()
        }) else {
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
    let response = rt.block_on(async {
        match command {
            "suggest-matches" => {
                api.bank_original_suggestions(
                    flags["--company"],
                    body["bankTransactionId"].as_i64(),
                    body["max"].as_u64(),
                )
                .await
            }
            "correction-plan" => {
                api.bank_original_correction_plan(
                    flags["--company"],
                    body["bankTransactionId"].as_i64().unwrap(),
                    body["replacementJournalEntryId"].as_i64().unwrap(),
                )
                .await
            }
            _ => {
                api.bank_original_mutation(
                    flags["--company"],
                    if command == "link-journal" {
                        BankOriginalMutation::LinkJournal
                    } else {
                        BankOriginalMutation::CorrectionApply
                    },
                    serde_json::to_vec(&body).unwrap(),
                )
                .await
            }
        }
    });
    match response {
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
            if exit == 0 && !json_mode && command == "suggest-matches" {
                let mut text = String::new();
                for row in value["rows"].as_array().unwrap() {
                    text.push_str(&format!(
                        "Banktransaktion {} | {} | {} {} | {}\n",
                        row["bankTransactionId"],
                        row["date"].as_str().unwrap_or(""),
                        row["amount"],
                        row["currency"].as_str().unwrap_or(""),
                        row["text"].as_str().unwrap_or("")
                    ));
                    for suggestion in row["suggestions"].as_array().into_iter().flatten() {
                        text.push_str(&format!(
                            "  {} | bilag {} | score {} | {}\n",
                            suggestion["kind"].as_str().unwrap_or(""),
                            suggestion["documentId"],
                            suggestion["confidence"],
                            suggestion["reasons"]
                        ));
                    }
                    if row["suggestions"].as_array().is_some_and(Vec::is_empty) {
                        if let Some(reason) = row["unsafeMatchReason"].as_str() {
                            text.push_str(&format!("  Ingen sikre forslag. {reason}\n"));
                        } else {
                            text.push_str("  Ingen sikre forslag.\n");
                        }
                    }
                }
                if text.is_empty() {
                    text = "Ingen uafstemte banktransaktioner for det valgte filter.\n".into();
                }
                return Output {
                    stdout: text,
                    stderr: String::new(),
                    exit,
                };
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
