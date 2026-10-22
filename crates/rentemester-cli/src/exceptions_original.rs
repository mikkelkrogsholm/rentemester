//! Original exception commands; server authentication owns review attribution.
use crate::dated_reports::Output;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::IsTerminal};
pub fn accepts(args: &[String]) -> bool {
    args.first().is_some_and(|s| s == "exceptions")
        && args
            .get(1)
            .is_some_and(|s| matches!(s.as_str(), "list" | "resolve"))
}
pub fn help() -> Value {
    json!({"commands":["exceptions list --company SLUG [--status open|resolved|all] [--include-archived]","exceptions resolve --company SLUG --id POSITIVE [--note TEXT]"],"connection":"--server ORIGIN or --local-profile PATH; --cookie-file PATH or --service-key-file PATH; --allow-loopback-http; --json or --format json|human","authority":"Explicit resolve invocation confirms one write. Server-owned identity; no stdin, caller actor, extra confirm, idempotency key or retry."})
}
fn as_text(v: &Value) -> String {
    match v {
        Value::String(s) if !s.is_empty() => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => "—".into(),
    }
}
fn human_list(value: &Value) -> String {
    let rows = value["rows"].as_array().unwrap();
    if rows.is_empty() {
        return "Ingen undtagelser i køen for det valgte filter.".into();
    }
    let open = rows.iter().filter(|r| r["status"] == "open").count();
    let mut lines = vec![
        format!(
            "Undtagelser: {} sag{}{}",
            rows.len(),
            if rows.len() == 1 { "" } else { "er" },
            if open > 0 {
                format!(" ({open} åben{})", if open == 1 { "" } else { "e" })
            } else {
                String::new()
            }
        ),
        String::new(),
    ];
    for row in rows {
        let severity = match row["severity"].as_str() {
            Some("low") => "lav".into(),
            Some("medium") => "middel".into(),
            Some("high") => "høj".into(),
            _ => as_text(&row["severity"]),
        };
        let status = match row["status"].as_str() {
            Some("open") => "åben".into(),
            Some("resolved") => "løst".into(),
            _ => as_text(&row["status"]),
        };
        lines.push(format!(
            "#{} — {}",
            as_text(&row["id"]),
            as_text(&row["message"])
        ));
        lines.push(format!(
            "  Type: {} | Alvorlighed: {severity} | Status: {status}",
            as_text(&row["type"])
        ));
        if row["requiredAction"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
        {
            lines.push(format!(
                "  Påkrævet handling: {}",
                as_text(&row["requiredAction"])
            ));
        }
        let mut refs = Vec::new();
        for (key, label) in [
            ("relatedBankTransactionId", "banktransaktion"),
            ("relatedDocumentId", "bilag"),
        ] {
            if !row[key].is_null() {
                refs.push(format!("{label} {}", as_text(&row[key])));
            }
        }
        if !refs.is_empty() {
            lines.push(format!("  Vedrører: {}", refs.join(", ")));
        }
        if row["archived"] == true {
            lines.push("  (i arkiveret/lukket periode)".into());
        }
        if row["status"] == "resolved" && row["resolvedAt"].as_str().is_some_and(|s| !s.is_empty())
        {
            lines.push(format!(
                "  Løst {}{}",
                as_text(&row["resolvedAt"]),
                row["resolvedBy"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map_or(String::new(), |s| format!(" af {s}"))
            ));
        }
        lines.push(String::new());
    }
    lines.pop();
    lines.join("\n")
}
fn output(value: Value, json_mode: bool, exit: u8, resolve: bool) -> Output {
    if json_mode && exit != 2 {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
            stderr: String::new(),
            exit,
        };
    }
    if exit == 0 {
        Output {
            stdout: format!(
                "{}\n",
                if value.get("rows").is_some() {
                    human_list(&value)
                } else {
                    "✔ Markerer en exception som løst.".to_owned()
                }
            ),
            stderr: String::new(),
            exit,
        }
    } else {
        Output {
            stdout: String::new(),
            stderr: if exit != 2 && resolve {
                format!(
                    "✘ Markerer en exception som løst. failed\n{}",
                    value["errors"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|e| format!("  → {}\n", e.as_str().unwrap()))
                        .collect::<String>()
                )
            } else {
                value["errors"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| format!("{}\n", e.as_str().unwrap()))
                    .collect()
            },
            exit,
        }
    }
}
fn usage(message: &str) -> Output {
    output(json!({"ok":false,"errors":[message]}), false, 2, false)
}

fn parse(args: &[String]) -> Result<BTreeMap<&str, &str>, Output> {
    let mut flags = BTreeMap::new();
    let mut cursor = 2;
    while cursor < args.len() {
        let (key, attached) = crate::original_args::option(&args[cursor]);
        let global = matches!(
            key,
            "--company"
                | "--server"
                | "--cookie-file"
                | "--service-key-file"
                | "--local-profile"
                | "--format"
                | "--json"
                | "--help"
                | "--allow-loopback-http"
        );
        let domain = match args.get(1).map(String::as_str) {
            Some("list") => matches!(key, "--status" | "--include-archived"),
            Some("resolve") => matches!(key, "--id" | "--note"),
            _ => false,
        };
        if !global && !domain {
            return Err(usage("Unknown exceptions flag"));
        }
        let boolean = matches!(
            key,
            "--json" | "--help" | "--allow-loopback-http" | "--include-archived"
        );
        let (value, next) = if boolean {
            if attached.is_some() {
                return Err(usage("Boolean flag does not take a value"));
            }
            ("true", cursor + 1)
        } else {
            crate::original_args::value(args, cursor).ok_or_else(|| usage("Missing flag value"))?
        };
        if flags.insert(key, value).is_some() {
            return Err(usage("Duplicate exceptions flag"));
        }
        cursor = next;
    }
    if !flags.contains_key("--help") && args.get(1).is_some_and(|s| s == "resolve") {
        positive_id(flags.get("--id").copied().unwrap_or(""))?;
        if flags.get("--note").is_some_and(|s| s.len() > 4096) {
            return Err(usage("Note exceeds4096 bytes"));
        }
    }
    Ok(flags)
}
fn positive_id(raw: &str) -> Result<i64, Output> {
    raw.parse::<i64>()
        .ok()
        .filter(|n| (1..=9_007_199_254_740_991).contains(n))
        .filter(|_| raw.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(|| usage("Missing required --id <n>; positive safe integer required"))
}
pub fn preflight(args: &[String]) -> Result<(), Output> {
    parse(args).map(|_| ())
}
pub fn execute(args: &[String]) -> Output {
    let flags = match parse(args) {
        Ok(f) => f,
        Err(o) => return o,
    };
    if flags.contains_key("--help") {
        return Output {
            stdout: format!("{}\n", help()),
            stderr: String::new(),
            exit: 0,
        };
    }
    for key in ["--company", "--server"] {
        if flags.get(key).is_none_or(|s| s.is_empty()) {
            return usage(&format!("Explicit {key} required"));
        }
    }
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") {
        return usage("Choose exactly one --cookie-file/--service-key-file");
    }
    let json_mode = flags.contains_key("--json")
        || (flags.get("--format") != Some(&"human") && !std::io::stdout().is_terminal());
    let api = match crate::remote::credential_client(
        flags["--server"],
        flags.contains_key("--allow-loopback-http"),
        &flags,
        false,
    ) {
        Ok(api) => api,
        Err((exit, e)) => {
            return output(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
                args[1] == "resolve",
            );
        }
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(_) => {
            return output(
                json!({"ok":false,"errors":["HTTP runtime unavailable"]}),
                json_mode,
                1,
                args[1] == "resolve",
            );
        }
    };
    let result = rt.block_on(async {
        if args[1] == "list" {
            api.exceptions_original_list(
                flags["--company"],
                flags.get("--status").copied(),
                flags.contains_key("--include-archived"),
            )
            .await
        } else {
            let mut body = json!({});
            if let Some(note) = flags.get("--note") {
                body["note"] = json!(note);
            }
            api.exceptions_original_resolve(
                flags["--company"],
                positive_id(flags["--id"]).unwrap(),
                serde_json::to_vec(&body).unwrap(),
                true,
            )
            .await
        }
    });
    match result {
        Ok(value) => {
            let exit = if value["ok"] == false { 1 } else { 0 };
            output(value, json_mode, exit, args[1] == "resolve")
        }
        Err(e) => {
            let (exit, e) = crate::remote::client_error(e);
            output(
                json!({"ok":false,"code":e.code,"errors":[e.message]}),
                json_mode,
                exit,
                args[1] == "resolve",
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exception_original_exact_human_source_report_and_resolution() {
        let rows = json!({"rows":[{"id":9,"type":"synthetic","severity":"high","status":"resolved","message":"Review fact","requiredAction":"Check source","relatedBankTransactionId":3,"relatedDocumentId":4,"archived":true,"resolvedAt":"2026-01-01","resolvedBy":"user:synthetic"}]});
        assert_eq!(
            human_list(&rows),
            "Undtagelser: 1 sag\n\n#9 — Review fact\n  Type: synthetic | Alvorlighed: høj | Status: løst\n  Påkrævet handling: Check source\n  Vedrører: banktransaktion 3, bilag 4\n  (i arkiveret/lukket periode)\n  Løst 2026-01-01 af user:synthetic"
        );
        assert_eq!(
            output(
                json!({"ok":true,"resolved":false,"errors":[]}),
                false,
                0,
                true
            )
            .stdout,
            "✔ Markerer en exception som løst.\n"
        );
        assert_eq!(
            output(json!({"ok":false,"errors":["actual"]}), false, 1, true).stderr,
            "✘ Markerer en exception som løst. failed\n  → actual\n"
        );
        assert_eq!(
            output(json!({"ok":false,"errors":["actual"]}), false, 1, false).stderr,
            "actual\n"
        );
    }
}
