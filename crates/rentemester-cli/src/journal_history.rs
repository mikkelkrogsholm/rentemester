//! Original machine-readable journal history, transported from one server snapshot.
use rentemester_client::JournalHistoryQuery;
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn invalid() -> (u8, Error) {
    (
        2,
        Error::new(
            "invalid_input",
            "use journal list [--json] --server ORIGIN --cookie-file PATH --company SLUG; see journal list --help",
        ),
    )
}
pub fn help() -> Value {
    json!({"ok":true,"usage":"journal list [--json] --server ORIGIN --cookie-file PATH --company SLUG [--allow-loopback-http]","output":"Bare snake_case array with all 12 original stored journal header fields, descending ID, all native fiscal years in one admitted read snapshot. Null DKK header amounts and original posted/reversed status are preserved. No archive merge or local fallback.","humanOutput":"Danish journal details use server-owned amountDkk or bookedDebitTotal; no local line aggregation.","exitCodes":{"0":"success","1":"server/transport/protocol refusal","2":"usage"}})
}
type Parsed<'a> = (BTreeMap<&'a str, &'a str>, bool, bool);

fn parse(args: &[String]) -> Result<Parsed<'_>, (u8, Error)> {
    let mut flags = BTreeMap::new();
    let (mut json, mut http) = (false, false);
    let mut cursor = 0;
    while cursor < args.len() {
        let key = args[cursor].as_str();
        match key {
            "--json" if !json => {
                json = true;
                cursor += 1;
            }
            "--allow-loopback-http" if !http => {
                http = true;
                cursor += 1;
            }
            "--server" | "--cookie-file" | "--company" => {
                let value = args
                    .get(cursor + 1)
                    .filter(|s| !s.is_empty() && !s.starts_with("--"))
                    .ok_or_else(invalid)?;
                if flags.insert(key, value.as_str()).is_some() {
                    return Err(invalid());
                }
                cursor += 2;
            }
            _ => return Err(invalid()),
        }
    }
    if ["--server", "--cookie-file", "--company"]
        .iter()
        .any(|k| !flags.contains_key(k))
    {
        return Err(invalid());
    }
    Ok((flags, http, json))
}
const MAP: [(&str, &str); 12] = [
    ("id", "id"),
    ("entryNo", "entry_no"),
    ("transactionDate", "transaction_date"),
    ("text", "text"),
    ("currency", "currency"),
    ("amountForeign", "amount_foreign"),
    ("amountDkk", "amount_dkk"),
    ("fxRateToDkk", "fx_rate_to_dkk"),
    ("documentId", "document_id"),
    ("sourceBankTransactionId", "source_bank_transaction_id"),
    ("status", "status"),
    ("reversalOfEntryId", "reversal_of_entry_id"),
];
fn snake(page: Value) -> Result<Value, (u8, Error)> {
    let rows = page["entries"]
        .as_array()
        .ok_or_else(|| (1, Error::new("protocol", "journal history entries missing")))?;
    Ok(Value::Array(
        rows.iter()
            .map(|row| {
                Value::Object(
                    MAP.iter()
                        .map(|(from, to)| ((*to).into(), row[*from].clone()))
                        .collect(),
                )
            })
            .collect(),
    ))
}
fn kroner(value: &Value) -> Result<String, (u8, Error)> {
    let text = if let Some(s) = value.as_str() {
        s.to_owned()
    } else {
        value.to_string()
    };
    let amount = rentemester_engine::money::Money::parse(&text)
        .map_err(|_| (1, Error::new("protocol", "invalid server journal amount")))?
        .decimal();
    let (whole, fraction) = amount
        .split_once('.')
        .ok_or_else(|| (1, Error::new("protocol", "invalid server journal amount")))?;
    let digits = whole.strip_prefix('-').unwrap_or(whole);
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(c);
    }
    Ok(format!(
        "{}{},{} kr.",
        if whole.starts_with('-') { "-" } else { "" },
        grouped,
        fraction
    ))
}
fn human(page: Value) -> Result<Value, (u8, Error)> {
    let rows = page["entries"]
        .as_array()
        .ok_or_else(|| (1, Error::new("protocol", "journal history entries missing")))?;
    let mut lines = vec![format!("Finansposteringer ({})", rows.len())];
    if rows.is_empty() {
        lines.push("Ingen finansposteringer registreret.".into());
    }
    for row in rows {
        lines.push(String::new());
        lines.push(format!(
            "#{} — {}",
            row["entryNo"].as_str().unwrap_or("—"),
            row["transactionDate"].as_str().unwrap_or("—")
        ));
        lines.push(format!("  Tekst: {}", row["text"].as_str().unwrap_or("—")));
        let amount = if row["amountDkk"].is_null() {
            &row["bookedDebitTotal"]
        } else {
            &row["amountDkk"]
        };
        let mut label = format!("  Beløb: {}", kroner(amount)?);
        let currency = row["currency"].as_str().unwrap_or("DKK").to_uppercase();
        if currency != "DKK" {
            label.push_str(&format!(
                " ({} {}, kurs {})",
                row["amountForeign"], currency, row["fxRateToDkk"]
            ));
        }
        lines.push(label);
        lines.push(format!(
            "  Status: {}",
            match row["status"].as_str() {
                Some("posted") => "bogført",
                Some("reversed") => "tilbageført",
                _ => "—",
            }
        ));
        for (key, label) in [
            ("documentId", "Bilag"),
            ("sourceBankTransactionId", "Kilde-banktransaktion"),
            ("reversalOfEntryId", "Tilbagefører postering"),
        ] {
            if !row[key].is_null() {
                lines.push(format!("  {label}: {}", row[key]));
            }
        }
    }
    Ok(Value::String(lines.join("\n")))
}
pub fn run(args: &[String]) -> Result<Value, (u8, Error)> {
    if args == ["--help"] {
        return Ok(help());
    }
    let (flags, http, json) = parse(args)?;
    let api = crate::remote::credential_client(flags["--server"], http, &flags, false)?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let page = runtime
        .block_on(api.journal_history(flags["--company"], &JournalHistoryQuery::default(), true))
        .map_err(crate::remote::client_error)?;
    if json { snake(page) } else { human(page) }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn strict_parse_and_help_preserve_machine_and_human_modes() {
        for extra in [
            vec!["--limit", "500"],
            vec!["--from", "2026-10-20"],
            vec!["--confirm"],
            vec!["--json", "--json"],
            vec!["--service-key-file", "x"],
            vec!["--company", "duplicate"],
        ] {
            let mut input = args(&[
                "--server",
                "http://127.0.0.1:9",
                "--cookie-file",
                "missing",
                "--company",
                "synthetic",
            ]);
            input.extend(args(&extra));
            assert_eq!(run(&input).unwrap_err().0, 2);
        }
        for extra in [vec![], vec!["--json"]] {
            let mut input = args(&[
                "--server",
                "https://synthetic.invalid",
                "--cookie-file",
                "missing",
                "--company",
                "synthetic",
            ]);
            input.extend(args(&extra));
            let parsed = parse(&input).unwrap();
            assert_eq!(parsed.2, !extra.is_empty());
        }
        assert_eq!(run(&args(&["--help"])).unwrap()["ok"], true);
        assert!(
            crate::run(args(&["--help"])).unwrap()["commands"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["name"] == "journal list")
        );
        assert_eq!(
            crate::run(args(&["journal", "list", "--help"])).unwrap(),
            help()
        );
    }
    #[test]
    fn snake_json_preserves_original_headers_and_human_uses_server_debit_total() {
        let row = json!({"id":3,"entryNo":"SYN-3","transactionDate":"2026-10-20","text":"Synthetic expense","currency":"DKK","amountForeign":null,"amountDkk":null,"fxRateToDkk":null,"documentId":null,"sourceBankTransactionId":9,"status":"posted","reversalOfEntryId":null,"bookedDebitTotal":"1234.50"});
        let page = json!({"entries":[row]});
        let wire = snake(page.clone()).unwrap();
        assert!(wire.is_array());
        assert_eq!(wire[0].as_object().unwrap().len(), 12);
        assert_eq!(wire[0]["amount_dkk"], Value::Null);
        assert_eq!(wire[0]["source_bank_transaction_id"], 9);
        assert_eq!(wire[0]["status"], "posted");
        let text = human(page).unwrap();
        let text = text.as_str().unwrap();
        assert!(text.contains("Beløb: 1.234,50 kr."));
        assert!(text.contains("Status: bogført"));
        assert!(text.contains("Kilde-banktransaktion: 9"));
        let foreign = json!({"entries":[{"entryNo":"SYN-FX","transactionDate":"2026-10-20","text":"Synthetic FX carrying value","currency":"EUR","amountForeign":50.05,"amountDkk":371.98,"fxRateToDkk":7.43208,"status":"reversed","reversalOfEntryId":3,"bookedDebitTotal":375.38}]});
        let text = human(foreign).unwrap();
        let text = text.as_str().unwrap();
        assert!(text.contains("Beløb: 371,98 kr. (50.05 EUR, kurs 7.43208)"));
        assert!(text.contains("Status: tilbageført"));
        assert!(text.contains("Tilbagefører postering: 3"));
        assert_eq!(
            human(json!({"entries":[]})).unwrap(),
            "Finansposteringer (0)\nIngen finansposteringer registreret."
        );
    }
    #[test]
    fn all_headers_roundtrip_and_cli_exit_classification() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };
        let dir = std::env::temp_dir().join(format!("rm-journal-history-{}", std::process::id()));
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
        for status in [200, 403] {
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
                let body = if status == 200 {
                    r#"{"ok":true,"journalHistory":{"entries":[],"total":0,"count":0,"limit":0,"offset":0,"hasMore":false}}"#
                } else {
                    r#"{"ok":false,"code":"access_denied"}"#
                };
                write!(
                    socket,
                    "HTTP/1.1 {status} Test\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                String::from_utf8(request).unwrap()
            });
            let result = crate::run(args(&[
                "journal",
                "list",
                "--json",
                "--server",
                &origin,
                "--cookie-file",
                cookie.to_str().unwrap(),
                "--company",
                "synthetic",
                "--allow-loopback-http",
            ]));
            if status == 200 {
                assert_eq!(result.unwrap(), json!([]));
            } else {
                assert_eq!(result.unwrap_err().0, 1);
            }
            let request = task.join().unwrap();
            assert!(
                request
                    .starts_with("GET /api/companies/synthetic/journal/entries?all=true HTTP/1.1")
            );
            assert!(request.to_lowercase().contains("cookie: session=synthetic"));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
