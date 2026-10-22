use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rm148-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let fixture = Self(path);
        for (name, bytes) in [
            ("cookie", b"session=synthetic".as_slice()),
            ("key", b"synthetic-key".as_slice()),
        ] {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options
                .open(fixture.0.join(name))
                .unwrap()
                .write_all(bytes)
                .unwrap();
        }
        fixture
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn server(status: u16, value: Value) -> (String, thread::JoinHandle<(String, Value)>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = vec![];
        let end = loop {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let header = String::from_utf8_lossy(&bytes[..end]).to_string();
        let length: usize = header
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse().unwrap())
            })
            .unwrap_or(0);
        while bytes.len() < end + length {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
        }
        let body = if length == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[end..end + length]).unwrap()
        };
        let response = value.to_string();
        write!(
            s,
            "HTTP/1.1 {status} Mock\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{response}",
            response.len()
        )
        .unwrap();
        (header, body)
    });
    (origin, t)
}

fn call(kind: &str, origin: &str, f: &Fixture, extra: &[&str], input: Option<&str>) -> Output {
    let mut command = Command::new(BIN);
    command
        .args([
            "invoice",
            kind,
            "--server",
            origin,
            "--allow-loopback-http",
            "--company",
            "synthetic",
            "--cookie-file",
            f.0.join("cookie").to_str().unwrap(),
        ])
        .args(extra);
    if let Some(input) = input {
        let mut child = command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    } else {
        command.output().unwrap()
    }
}

#[test]
fn all_eleven_original_claim_commands_keep_source_flags_omitted_defaults_and_flat_results() {
    let f = Fixture::new();
    for (command, extra, expected) in [
        (
            "remind",
            vec!["--date", "2026-02-03", "--note", " raw note "],
            json!({"documentId":7,"date":"2026-02-03","note":" raw note ","confirm":true}),
        ),
        (
            "post-reminder",
            vec![
                "--receivable-account",
                "debtor",
                "--reminder-income-account",
                "reminder",
            ],
            json!({"documentId":7,"receivableAccountNo":"debtor","reminderIncomeAccountNo":"reminder","confirm":true}),
        ),
        (
            "compensation",
            vec!["--as-of", "2026-02-03", "--amount-dkk", "250"],
            json!({"documentId":7,"asOf":"2026-02-03","amountDkk":250}),
        ),
        (
            "claim-compensation",
            vec!["--as-of", "2026-02-03"],
            json!({"documentId":7,"asOf":"2026-02-03","confirm":true}),
        ),
        (
            "post-compensation",
            vec![
                "--receivable-account",
                "debtor",
                "--compensation-income-account",
                "comp",
            ],
            json!({"documentId":7,"receivableAccountNo":"debtor","compensationIncomeAccountNo":"comp","confirm":true}),
        ),
        (
            "interest",
            vec!["--as-of", "2026-02-03"],
            json!({"documentId":7,"asOf":"2026-02-03"}),
        ),
        (
            "claim-interest",
            vec!["--as-of", "2026-02-03", "--reference-rate", "2.65"],
            json!({"documentId":7,"asOf":"2026-02-03","referenceRate":2.65,"confirm":true}),
        ),
        (
            "post-interest",
            vec![
                "--claim-id",
                "9",
                "--receivable-account",
                "debtor",
                "--interest-income-account",
                "interest",
            ],
            json!({"documentId":7,"claimId":9,"receivableAccountNo":"debtor","interestIncomeAccountNo":"interest","confirm":true}),
        ),
        ("interest-correction", vec![], json!({"documentId":7})),
        (
            "post-interest-correction",
            vec!["--reason", " reviewed reason "],
            json!({"documentId":7,"reason":" reviewed reason ","confirm":true}),
        ),
        (
            "settle-claim-bank",
            vec!["--input", "-"],
            json!({"payload":{"invoiceNumber":"INV","bankTransactionReference":" raw ref ","amount":12.34},"confirm":true}),
        ),
    ] {
        let value = json!({"ok":true,"data":{"sourceOperation":command,"remainingClaimOpenBalance":17.66,"canPost":false,"requiresRefund":true,"claimOpenBalance":null,"principalCurrency":"EUR","claimBalanceDkk":17},"errors":[],"appliedRules":["source-rule"]});
        let (origin, t) = server(200, value.clone());
        let mut args = extra;
        if command != "settle-claim-bank" {
            args.extend(["--document-id", "7"]);
        }
        args.push("--json");
        let out = call(
            command,
            &origin,
            &f,
            &args,
            if command == "settle-claim-bank" {
                Some(
                    " { \"invoiceNumber\":\"INV\", \"bankTransactionReference\":\" raw ref \" , \"amount\":12.34 } ",
                )
            } else {
                None
            },
        );
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}: {}",
            command,
            String::from_utf8_lossy(&out.stderr)
        );
        let mut flat = value["data"].clone();
        flat["ok"] = json!(true);
        flat["errors"] = json!([]);
        flat["appliedRules"] = json!(["source-rule"]);
        assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), flat);
        let (header, body) = t.join().unwrap();
        assert!(header.starts_with(&format!(
            "POST /api/companies/synthetic/invoices/original/{command} "
        )));
        assert_eq!(body, expected);
    }
}
#[test]
fn strict_claim_flags_business_and_input_failures_preserve_exit_streams() {
    let f = Fixture::new();
    for (command, args) in [
        ("remind", vec!["--document-id", "7"]),
        (
            "post-reminder",
            vec!["--document-id", "7", "--receivable-account", "debtor"],
        ),
        ("post-interest-correction", vec!["--document-id", "7"]),
        (
            "interest",
            vec![
                "--document-id",
                "7",
                "--as-of",
                "2026-01-01",
                "--confirm",
                "yes",
            ],
        ),
        (
            "remind",
            vec!["--document-id", "7", "--date", "2026-01-01", "--fee", "bad"],
        ),
        (
            "interest",
            vec![
                "--document-id",
                "7",
                "--as-of",
                "2026-01-01",
                "--as-of",
                "duplicate",
            ],
        ),
        (
            "settle-claim-bank",
            vec!["--input", "/nonexistent/rm151.json"],
        ),
    ] {
        let out = call(command, "http://127.0.0.1:9", &f, &args, None);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
    }
    for human in [false, true] {
        let value = json!({"ok":false,"code":"unsupported_foreign_interest","errors":["unsupported foreign interest"],"appliedRules":["source-rule"]});
        let (origin, t) = server(200, value.clone());
        let mut args = vec!["--invoice-number", "INV", "--as-of", "2026-01-01"];
        if !human {
            args.push("--json");
        }
        let out = call("interest", &origin, &f, &args, None);
        assert_eq!(out.status.code(), Some(1));
        if human {
            assert!(out.stdout.is_empty());
            assert!(String::from_utf8_lossy(&out.stderr).contains("unsupported foreign interest"));
        } else {
            assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), value);
            assert!(out.stderr.is_empty());
        }
        t.join().unwrap();
    }
}
