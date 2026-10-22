//! Original report CLI stream contract; all financial values come from the server.
use rentemester_engine::money::Money;
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[derive(Debug)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub exit: u8,
}
fn failure(message: impl AsRef<str>, exit: u8) -> Output {
    Output {
        stdout: String::new(),
        stderr: format!("{}\n", message.as_ref()),
        exit,
    }
}
pub fn help() -> Value {
    json!({"usage":["report trial-balance|profit-loss --server ORIGIN --cookie-file PATH --company SLUG --from YYYY-MM-DD --to YYYY-MM-DD [--json] [--allow-loopback-http]","report balance --server ORIGIN --cookie-file PATH --company SLUG --as-of YYYY-MM-DD [--json] [--allow-loopback-http]"],"scope":"Original flat numeric reports; explicit authenticated server, no local fallback. --service-key-file may replace cookie-file for scoped CompanyRead. Calendar/business refusals preserve original report and exit1. JSON stdout; human failures stderr; missing selectors stderr/exit2. The experimental exact-string trial-balance command is separate."})
}
struct Parsed<'a> {
    kind: &'a str,
    flags: BTreeMap<&'a str, &'a str>,
    json: bool,
    http: bool,
}
fn parse(args: &[String]) -> Result<Parsed<'_>, String> {
    let kind = args
        .first()
        .map(String::as_str)
        .filter(|s| matches!(*s, "trial-balance" | "profit-loss" | "balance"))
        .ok_or("Expected report trial-balance|profit-loss|balance; see report --help")?;
    let mut flags = BTreeMap::new();
    let (mut json, mut http) = (false, false);
    let mut cursor = 1;
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
            "--server" | "--cookie-file" | "--service-key-file" | "--company" | "--from"
            | "--to" | "--as-of" => {
                let (value, next) = crate::original_args::value(args, cursor)
                    .filter(|(value, _)| !value.is_empty())
                    .ok_or_else(|| format!("Missing value for {key}"))?;
                if flags.insert(key, value).is_some() {
                    return Err(format!("Duplicate flag {key}"));
                }
                cursor = next;
            }
            _ => {
                return Err(format!(
                    "Unknown or duplicate report flag {key}; see report --help"
                ));
            }
        }
    }
    if kind == "balance" {
        if !flags.contains_key("--as-of") {
            return Err("Missing required --as-of <YYYY-MM-DD>".into());
        }
        if flags.contains_key("--from") || flags.contains_key("--to") {
            return Err("Balance accepts --as-of only".into());
        }
    } else {
        if !flags.contains_key("--from") || !flags.contains_key("--to") {
            return Err("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>".into());
        }
        if flags.contains_key("--as-of") {
            return Err("Period reports accept --from/--to only".into());
        }
    }
    if !flags.contains_key("--server") || !flags.contains_key("--company") {
        return Err("Explicit --server and --company required".into());
    }
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") {
        return Err("Choose exactly one --cookie-file or --service-key-file".into());
    }
    Ok(Parsed {
        kind,
        flags,
        json,
        http,
    })
}
pub fn execute(args: &[String]) -> Output {
    if args == ["--help"]
        || args
            .iter()
            .skip(1)
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
        Ok(i) => i,
        Err(e) => return failure(e, 2),
    };
    let api = match crate::remote::credential_client(
        input.flags["--server"],
        input.http,
        &input.flags,
        false,
    ) {
        Ok(api) => api,
        Err((code, e)) => return failure(format!("{}: {}", e.code, e.message), code),
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(_) => return failure("transport: could not start HTTP runtime", 1),
    };
    let result = runtime.block_on(api.dated_report(
        input.flags["--company"],
        input.kind,
        input.flags.get("--from").copied(),
        input.flags.get("--to").copied(),
        input.flags.get("--as-of").copied(),
    ));
    match result {
        Ok(report) => render(input.kind, &report, input.json),
        Err(error) => {
            let (code, error) = crate::remote::client_error(error);
            failure(format!("{}: {}", error.code, error.message), code)
        }
    }
}
fn text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| "—".into())
}
fn kroner(value: &Value) -> String {
    if !value.is_number() {
        return "—".into();
    }
    let Ok(amount) = Money::parse(&value.to_string()) else {
        return "—".into();
    };
    let amount = amount.decimal();
    let Some((whole, fraction)) = amount.split_once('.') else {
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
fn rows(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}
fn section(lines: &mut Vec<String>, heading: &str, data: &Value) {
    lines.push(format!("{heading}:"));
    if rows(&data["lines"]).next().is_none() {
        lines.push("  (ingen)".into());
    }
    for row in rows(&data["lines"]) {
        lines.push(format!(
            "  {} {}: {}",
            text(&row["accountNo"]),
            text(&row["name"]),
            kroner(&row["amount"])
        ));
    }
    lines.push(format!(
        "{:<33}{}",
        format!("  {heading} i alt:"),
        kroner(&data["total"])
    ));
}
fn render(kind: &str, report: &Value, json: bool) -> Output {
    let failed = report["ok"] == false;
    if json {
        return Output {
            stdout: format!("{}\n", serde_json::to_string_pretty(report).unwrap()),
            stderr: String::new(),
            exit: u8::from(failed),
        };
    }
    let title = match kind {
        "trial-balance" => "Saldobalance",
        "profit-loss" => "Resultatopgørelse",
        _ => "Balance",
    };
    let mut lines = Vec::new();
    if failed {
        lines.push(format!("✘ {title} failed"));
        if rows(&report["errors"]).next().is_none() {
            lines.push("  → Kommandoen fejlede uden en specifik fejlbesked.".into());
        }
        for error in rows(&report["errors"]) {
            lines.push(format!("  → {}", text(error)));
        }
        if rows(&report["warnings"]).next().is_some() {
            lines.push("  Advarsler:".into());
            for warning in rows(&report["warnings"]) {
                lines.push(format!("    - {}", text(warning)));
            }
        }
        return failure(lines.join("\n"), 1);
    }
    match kind {
        "trial-balance" => {
            lines.push(format!(
                "Saldobalance for perioden {} til {}",
                text(&report["periodStart"]),
                text(&report["periodEnd"])
            ));
            lines.push(String::new());
            if rows(&report["accounts"]).next().is_none() {
                lines.push("Ingen konti har bevægelser i perioden.".into());
            }
            for row in rows(&report["accounts"]) {
                lines.push(format!(
                    "  {} {}",
                    text(&row["accountNo"]),
                    text(&row["name"])
                ));
                lines.push(format!(
                    "      Debet {} | Kredit {} | Saldo {}",
                    kroner(&row["debit"]),
                    kroner(&row["credit"]),
                    kroner(&row["balance"])
                ));
            }
            lines.push(String::new());
            lines.push(format!(
                "  Debet i alt:                   {}",
                kroner(&report["totalDebit"])
            ));
            lines.push(format!(
                "  Kredit i alt:                  {}",
                kroner(&report["totalCredit"])
            ));
            lines.push(
                if report["balanced"] == true {
                    "  Saldobalancen går op (debet = kredit)."
                } else {
                    "  ADVARSEL: saldobalancen går ikke op (debet ≠ kredit)."
                }
                .into(),
            );
        }
        "profit-loss" => {
            lines.push(format!(
                "Resultatopgørelse for perioden {} til {}",
                text(&report["periodStart"]),
                text(&report["periodEnd"])
            ));
            lines.push(String::new());
            for (key, label, total) in [
                ("income", "Indtægter", "totalIncome"),
                ("expense", "Omkostninger", "totalExpense"),
            ] {
                lines.push(format!("{label}:"));
                if rows(&report[key]).next().is_none() {
                    lines.push("  (ingen)".into());
                }
                for row in rows(&report[key]) {
                    lines.push(format!(
                        "  {} {}: {}",
                        text(&row["accountNo"]),
                        text(&row["name"]),
                        kroner(&row["amount"])
                    ));
                }
                lines.push(format!(
                    "{:<33}{}",
                    format!("  {label} i alt:"),
                    kroner(&report[total])
                ));
                lines.push(String::new());
            }
            let result = kroner(&report["result"]);
            lines.push(if result.starts_with('-') {
                format!(
                    "Periodens resultat: underskud på {}",
                    result.trim_start_matches('-')
                )
            } else if Money::parse(&report["result"].to_string())
                .is_ok_and(|v| v != Money::default())
            {
                format!("Periodens resultat: overskud på {result}")
            } else {
                "Periodens resultat: 0,00 kr.".into()
            });
        }
        _ => {
            lines.push(format!("Balance pr. {}", text(&report["asOfDate"])));
            lines.push(String::new());
            for (key, label) in [
                ("assets", "Aktiver"),
                ("liabilities", "Passiver"),
                ("equity", "Egenkapital"),
            ] {
                section(&mut lines, label, &report[key]);
                if key != "equity" {
                    lines.push(String::new());
                }
            }
            lines.push(format!(
                "{:<33}{}",
                "  Periodens resultat:",
                kroner(&report["periodResult"])
            ));
            lines.push(String::new());
            lines.push(format!(
                "  Aktiver i alt:                 {}",
                kroner(&report["totalAssets"])
            ));
            lines.push(format!(
                "  Passiver og egenkapital i alt: {}",
                kroner(&report["totalLiabilitiesAndEquity"])
            ));
            lines.push(
                if report["balanced"] == true {
                    "  Balancen går op (aktiver = passiver + egenkapital)."
                } else {
                    "  ADVARSEL: balancen går ikke op."
                }
                .into(),
            );
        }
    }
    if rows(&report["warnings"]).next().is_some() {
        lines.push(String::new());
        lines.push("Advarsler:".into());
        for warning in rows(&report["warnings"]) {
            lines.push(format!("  - {}", text(warning)));
        }
    }
    Output {
        stdout: format!("{}\n", lines.join("\n")),
        stderr: String::new(),
        exit: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }
    fn trial(ok: bool) -> Value {
        json!({"ok":ok,"appliedRules":["financial-statements:trial-balance"],"periodStart":"2025-12-31","periodEnd":"2026-01-01","accounts":[],"totalDebit":0,"totalCredit":0,"balanced":ok,"linesConsidered":0,"journalEntryCount":0,"errors":if ok{json!([])}else{json!(["periodStart must be YYYY-MM-DD"])}})
    }
    #[test]
    fn json_and_human_success_and_business_error_streams_are_original() {
        for ok in [true, false] {
            let report = trial(ok);
            let out = render("trial-balance", &report, true);
            assert_eq!(out.exit, if ok { 0 } else { 1 });
            assert!(out.stderr.is_empty());
            assert_eq!(
                out.stdout,
                format!("{}\n", serde_json::to_string_pretty(&report).unwrap())
            );
            assert_eq!(serde_json::from_str::<Value>(&out.stdout).unwrap(), report);
            assert!(!report.as_object().unwrap().contains_key("data"));
            let human = render("trial-balance", &report, false);
            if ok {
                assert!(human.stderr.is_empty());
                assert!(
                    human
                        .stdout
                        .starts_with("Saldobalance for perioden 2025-12-31 til 2026-01-01\n")
                );
                assert!(
                    human
                        .stdout
                        .contains("Ingen konti har bevægelser i perioden.")
                );
            } else {
                assert_eq!(human.exit, 1);
                assert!(human.stdout.is_empty());
                assert_eq!(
                    human.stderr,
                    "✘ Saldobalance failed\n  → periodStart must be YYYY-MM-DD\n"
                );
            }
        }
    }
    #[test]
    fn original_human_statement_money_equity_and_signs_are_formatting_only() {
        let pl = json!({"ok":true,"periodStart":"2026-01-01","periodEnd":"2026-02-01","income":[{"accountNo":"I","name":"Synthetic revenue","type":"income","amount":10}],"expense":[{"accountNo":"E","name":"Synthetic costs","type":"expense","amount":1244.50}],"totalIncome":10,"totalExpense":1244.50,"result":-1234.50,"errors":[]});
        let out = render("profit-loss", &pl, false);
        assert!(out.stdout.contains("Resultatopgørelse for perioden"));
        assert!(out.stdout.contains("I Synthetic revenue: 10,00 kr."));
        assert!(
            out.stdout
                .contains("Periodens resultat: underskud på 1.234,50 kr.")
        );
        let bs = json!({"ok":true,"asOfDate":"2026-10-20","assets":{"lines":[],"total":150},"liabilities":{"lines":[],"total":25},"equity":{"lines":[{"accountNo":"Q","name":"Synthetic capital","type":"equity","amount":100}],"total":100},"periodResult":25,"totalAssets":150,"totalLiabilitiesAndEquity":150,"balanced":true,"errors":[]});
        let out = render("balance", &bs, false);
        assert!(out.stdout.starts_with("Balance pr. 2026-10-20\n"));
        assert!(out.stdout.contains("Q Synthetic capital: 100,00 kr."));
        assert!(
            out.stdout
                .contains("Egenkapital i alt:             100,00 kr.")
        );
        assert!(
            out.stdout
                .contains("Periodens resultat:            25,00 kr.")
        );
        assert!(out.stdout.contains("Balancen går op"));
    }
    #[test]
    fn missing_flags_usage2_is_stderr_only_but_bad_calendar_is_forwarded() {
        for input in [
            vec!["trial-balance"],
            vec!["balance", "--from", "2026-01-01"],
            vec![
                "trial-balance",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-02",
                "--as-of",
                "x",
            ],
            vec!["balance", "--as-of", "2026-10-20", "--confirm"],
        ] {
            let out = execute(&args(&input));
            assert_eq!(out.exit, 2);
            assert!(out.stdout.is_empty());
            assert!(!out.stderr.is_empty());
        }
        let input = args(&[
            "trial-balance",
            "--server",
            "https://synthetic.invalid",
            "--cookie-file",
            "missing",
            "--company",
            "synthetic",
            "--from",
            "2026-02-30",
            "--to",
            "2025-01-01",
        ]);
        let parsed = parse(&input).unwrap();
        assert_eq!(parsed.flags["--from"], "2026-02-30");
        assert_eq!(parsed.flags["--to"], "2025-01-01");
        assert_eq!(execute(&args(&["--help"])).exit, 0);
        assert!(
            crate::run(args(&["--help"])).unwrap()["commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["name"] == "report trial-balance|profit-loss|balance")
        );
    }
    #[test]
    fn actual_sdk_cli_dispatch_keeps_business1_and_server_refusal_streams() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };
        let dir = std::env::temp_dir().join(format!("rm-dated-report-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let cookie = dir.join("cookie");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(&cookie)
            .unwrap()
            .write_all(b"session=synthetic")
            .unwrap();
        for (status, json_mode) in [(200, true), (200, false), (403, true)] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let task = thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = vec![];
                while !request.ends_with(b"\r\n\r\n") {
                    let mut b = [0; 4096];
                    let n = socket.read(&mut b).unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&b[..n]);
                }
                let report = if status == 200 {
                    trial(false)
                } else {
                    json!({"ok":false,"code":"access_denied"})
                };
                let body = report.to_string();
                write!(
                    socket,
                    "HTTP/1.1 {status} Test\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                String::from_utf8(request).unwrap()
            });
            let mut input = args(&[
                "trial-balance",
                "--server",
                &origin,
                "--cookie-file",
                cookie.to_str().unwrap(),
                "--allow-loopback-http",
                "--company",
                "synthetic",
                "--from",
                "2026-02-30",
                "--to",
                "2026-01-01",
            ]);
            if json_mode {
                input.push("--json".into());
            }
            let out = execute(&input);
            assert_eq!(out.exit, 1);
            if status == 200 && json_mode {
                assert_eq!(
                    serde_json::from_str::<Value>(&out.stdout).unwrap(),
                    trial(false)
                );
                assert!(out.stderr.is_empty());
            } else {
                assert!(out.stdout.is_empty());
                assert!(!out.stderr.is_empty());
            }
            let request = task.join().unwrap();
            assert!(request.starts_with("GET /api/companies/synthetic/reports/trial-balance?from=2026-02-30&to=2026-01-01 HTTP/1.1"));
            assert!(!out.stdout.contains("session=synthetic"));
            assert!(!out.stderr.contains("session=synthetic"));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
