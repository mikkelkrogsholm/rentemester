#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use common::TcpListener;
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
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
    server_response(status, value.to_string(), None)
}
fn server_response(
    status: u16,
    response: String,
    declared_length: Option<usize>,
) -> (String, thread::JoinHandle<(String, Value)>) {
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
        write!(
            s,
            "HTTP/1.1 {status} Mock\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{response}",
            declared_length.unwrap_or(response.len())
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
fn original_seven_commands_keep_raw_file_stdin_create_essentials_and_server_results() {
    let f = Fixture::new();
    let payload = json!({"invoiceType":"full","issueDate":"2026-01-01","lines":[{"description":"Synthetic","quantity":2,"unitPriceExVat":50,"lineTotalExVat":100}],"totals":{"netAmount":100,"vatRate":0.25,"vatAmount":25,"grossAmount":125}});
    let path = f.0.join("payload.json");
    fs::write(&path, payload.to_string()).unwrap();
    for (kind, extra, input) in [
        (
            "issue",
            vec!["--input", path.to_str().unwrap(), "--customer-id", "2"],
            None,
        ),
        (
            "create",
            vec![
                "--issue-date",
                "2026-01-01",
                "--line",
                "Synthetic|2|50;Second|1|10",
                "--buyer-name",
                " Synthetic buyer ",
                "--seller-vat",
                " DK123 ",
            ],
            None,
        ),
        (
            "render",
            vec!["--document-id", "7", "--invoice-number", "fallback"],
            None,
        ),
        (
            "post",
            vec![
                "--document-id",
                "7",
                "--revenue-account",
                "synthetic-income",
            ],
            None,
        ),
        (
            "settle-bank",
            vec!["--input", "-"],
            Some("{\"invoiceDocumentId\":7,\"bankTransactionReference\":\"in-ref\",\"amount\":50}"),
        ),
        (
            "credit-note",
            vec!["--input", "-"],
            Some(
                "{\"originalInvoiceNumber\":\"INV-7\",\"issueDate\":\"2026-01-02\",\"reason\":\"credit\",\"grossAmount\":25}",
            ),
        ),
        (
            "refund-bank",
            vec!["--input", "-"],
            Some(
                "{\"invoiceDocumentId\":7,\"bankTransactionId\":11,\"refundDate\":\"2026-01-03\",\"amount\":25}",
            ),
        ),
    ] {
        let data = json!({"documentId":7,"entryId":8,"entryNo":9,"pdfStoredPath":"/api/source.pdf","computed":{"net":110,"vatRate":0.25,"vatRatePercent":25,"vat":27.5,"gross":137.5}});
        let (origin, t) = server(
            200,
            json!({"ok":true,"data":data,"errors":[],"appliedRules":["server-rule"]}),
        );
        let mut args = extra;
        args.push("--json");
        let out = call(kind, &origin, &f, &args, input);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let mut expected = data;
        expected["ok"] = json!(true);
        expected["errors"] = json!([]);
        expected["appliedRules"] = json!(["server-rule"]);
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap(),
            expected
        );
        let (header, body) = t.join().unwrap();
        assert!(header.starts_with(&format!(
            "POST /api/companies/synthetic/invoices/original/{kind} "
        )));
        assert_eq!(body["confirm"], true);
        if kind == "issue" {
            assert_eq!(body["payload"], payload);
        }
        if kind == "create" {
            assert_eq!(body["payload"]["vatRatePercent"], 25);
            assert_eq!(
                body["payload"]["lines"][0],
                json!({"description":"Synthetic","quantity":2,"unitPriceExVat":50})
            );
            assert!(body["payload"].get("totals").is_none());
            assert_eq!(body["payload"]["buyer"]["name"], "Synthetic buyer");
        }
        if kind == "post" {
            assert_eq!(body["revenueAccountNo"], "synthetic-income");
        }
        if let Some(input) = input {
            assert_eq!(
                body["payload"],
                serde_json::from_str::<Value>(input).unwrap()
            );
        }
    }
}
#[test]
fn malformed_input_forbidden_flags_and_server_business_refusals_preserve_exits() {
    let f = Fixture::new();
    for (kind, args, input) in [
        ("issue", vec!["--input", "-"], Some("[]")),
        ("issue", vec!["--input", "-"], Some("not json")),
        ("issue", vec!["--input", "missing"], None),
        ("post", vec!["--document-id", "7", "--confirm", "yes"], None),
        (
            "post",
            vec!["--document-id", "7", "--idempotency-key", "key"],
            None,
        ),
        ("render", vec![], None),
    ] {
        let out = call(kind, "http://127.0.0.1:9", &f, &args, input);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
    }
    let value =
        json!({"ok":false,"code":"account_selection_required","errors":["Select revenue account"]});
    let (origin, t) = server(200, value.clone());
    let out = call("post", &origin, &f, &["--document-id", "7", "--json"], None);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), value);
    assert!(out.stderr.is_empty());
    let (_, body) = t.join().unwrap();
    assert!(body.get("revenueAccountNo").is_none());
    for kind in [
        "issue",
        "create",
        "render",
        "post",
        "settle-bank",
        "credit-note",
        "refund-bank",
    ] {
        assert_eq!(
            Command::new(BIN)
                .args(["invoice", kind, "--help"])
                .output()
                .unwrap()
                .status
                .code(),
            Some(0)
        );
    }
}

#[test]
fn apply_payment_file_stdin_flat_receipts_business_and_strict_usage() {
    let f = Fixture::new();
    let raw = " { \"invoiceNumber\" : \" raw INV \", \"paymentDate\":\"2026-02-03\", \"amount\":12.34, \"journalEntryId\":19, \"bankAccountNo\":\"bank-role\", \"receivableAccountNo\":\"historical-debtor\", \"note\":\" raw note \" } ";
    let path = f.0.join("payment.json");
    fs::write(&path, raw).unwrap();
    for from_file in [true, false] {
        let value = if from_file {
            json!({"ok":true,"data":{"paymentId":4,"journalEntryId":19,"invoiceDocumentId":7,"invoiceNumber":"INV","openBalance":22.66},"errors":[],"appliedRules":["source-rule"]})
        } else {
            json!({"ok":false,"code":"business","errors":["source rejected candidate journal"]})
        };
        let (origin, t) = server(200, value.clone());
        let out = call(
            "apply-payment",
            &origin,
            &f,
            &[
                "--input",
                if from_file {
                    path.to_str().unwrap()
                } else {
                    "-"
                },
                "--json",
            ],
            if from_file { None } else { Some(raw) },
        );
        assert_eq!(out.status.code(), Some(if from_file { 0 } else { 1 }));
        assert!(out.stderr.is_empty());
        let actual: Value = serde_json::from_slice(&out.stdout).unwrap();
        if from_file {
            assert_eq!(
                actual,
                json!({"ok":true,"paymentId":4,"journalEntryId":19,"invoiceDocumentId":7,"invoiceNumber":"INV","openBalance":22.66,"errors":[],"appliedRules":["source-rule"]})
            );
            assert!(actual.get("entryId").is_none());
        } else {
            assert_eq!(actual, value);
        }
        let (header, body) = t.join().unwrap();
        assert!(
            header.starts_with("POST /api/companies/synthetic/invoices/original/apply-payment ")
        );
        assert_eq!(
            body,
            json!({"confirm":true,"payload":serde_json::from_str::<Value>(raw).unwrap()})
        );
    }
    for (extra, input) in [
        (vec!["--input", "-", "--confirm", "yes"], raw),
        (vec!["--input", "-"], "{\"paymentDate\":\"2026-01-01\"}"),
        (
            vec!["--input", "-"],
            "{\"paymentDate\":\"2026-01-01\",\"amount\":1,\"actor\":\"forged\"}",
        ),
    ] {
        let out = call(
            "apply-payment",
            "http://127.0.0.1:9",
            &f,
            &extra,
            Some(input),
        );
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
    }
}

#[test]
fn repair_posting_explicit_flags_flat_source_receipts_business_and_usage_exits() {
    let f = Fixture::new();
    let base = [
        "--document-id",
        "7",
        "--invoice-number",
        " fallback INV ",
        "--legacy-journal-entry-id",
        "19",
        "--reason",
        " reviewed reason ",
        "--revenue-account",
        "reviewed-income",
        "--json",
    ];
    let data = json!({"invoiceDocumentId":7,"legacyJournalEntryId":19,"replacementJournalEntryId":20,"replacementJournalEntryNo":"J-20","reversalJournalEntryId":21,"reversalJournalEntryNo":"J-21","entryId":20,"entryNo":"J-20","entryHash":"server-hash"});
    for (success, human) in [(true, false), (false, false), (false, true)] {
        let value = if success {
            json!({"ok":true,"data":data,"errors":[],"appliedRules":["source-repair-rule"]})
        } else {
            json!({"ok":false,"code":"business","errors":["Legacy journal is not eligible for repair"],"appliedRules":["source-rule"]})
        };
        let (origin, t) = server(200, value.clone());
        let human_args = [&base[..base.len() - 1], &["--format", "human"]].concat();
        let out = call(
            "repair-posting",
            &origin,
            &f,
            if human { &human_args } else { &base },
            None,
        );
        assert_eq!(out.status.code(), Some(if success { 0 } else { 1 }));
        if human {
            assert!(out.stdout.is_empty());
            assert!(
                String::from_utf8_lossy(&out.stderr).contains("Legacy journal is not eligible")
            );
        } else {
            assert!(out.stderr.is_empty());
            let actual: Value = serde_json::from_slice(&out.stdout).unwrap();
            if success {
                let mut expected = data.clone();
                expected["ok"] = json!(true);
                expected["errors"] = json!([]);
                expected["appliedRules"] = json!(["source-repair-rule"]);
                assert_eq!(actual, expected);
            } else {
                assert_eq!(actual, value);
            }
        }
        let (header, body) = t.join().unwrap();
        assert!(
            header.starts_with("POST /api/companies/synthetic/invoices/original/repair-posting ")
        );
        assert_eq!(
            body,
            json!({"confirm":true,"documentId":7,"invoiceNumber":" fallback INV ","legacyJournalEntryId":19,"reason":"reviewed reason","revenueAccountNo":"reviewed-income"})
        );
    }
    for extra in [
        vec![
            "--document-id",
            "7",
            "--legacy-journal-entry-id",
            "19",
            "--reason",
            "reviewed",
        ],
        vec![
            "--document-id",
            "7",
            "--legacy-journal-entry-id",
            "0",
            "--reason",
            "reviewed",
            "--revenue-account",
            "income",
        ],
        vec![
            "--document-id",
            "7",
            "--legacy-journal-entry-id",
            "19",
            "--reason",
            " ",
            "--revenue-account",
            "income",
        ],
        vec![
            "--legacy-journal-entry-id",
            "19",
            "--reason",
            "reviewed",
            "--revenue-account",
            "income",
        ],
        vec![
            "--document-id",
            "7",
            "--legacy-journal-entry-id",
            "19",
            "--reason",
            "reviewed",
            "--revenue-account",
            "income",
            "--confirm",
            "yes",
        ],
        vec![
            "--document-id",
            "7",
            "--legacy-journal-entry-id",
            "19",
            "--reason",
            "reviewed",
            "--revenue-account",
            "income",
            "--reason",
            "duplicate",
        ],
    ] {
        let out = call("repair-posting", "http://127.0.0.1:9", &f, &extra, None);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
    }
}

#[test]
fn post_422_preserves_typed_vat_refusal_and_exit_one() {
    let f = Fixture::new();
    for rules in [None, Some(json!(["server-vat-rule"]))] {
        let mut receipt = json!({"ok":false,"code":"unsupported_invoice_vat_classification","errors":["Server rejected invoice VAT classification"]});
        if let Some(rules) = rules {
            receipt["appliedRules"] = rules;
        }
        let (origin, t) = server(422, receipt.clone());
        let out = call(
            "post",
            &origin,
            &f,
            &[
                "--document-id",
                "156",
                "--revenue-account",
                "synthetic-income",
                "--json",
            ],
            None,
        );
        assert_eq!(out.status.code(), Some(1));
        assert!(out.stderr.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap(),
            receipt
        );
        let (header, body) = t.join().unwrap();
        assert!(header.starts_with("POST /api/companies/synthetic/invoices/original/post "));
        assert_eq!(
            body,
            json!({"confirm":true,"documentId":156,"revenueAccountNo":"synthetic-income"})
        );
    }
}

#[test]
fn malformed_422_and_uncertain_post_responses_do_not_reflect_remote_bodies() {
    let f = Fixture::new();
    for (status, response, declared_length, code) in [
        (
            422,
            "remote-secret not JSON".to_owned(),
            None,
            "server_refused",
        ),
        (
            422,
            json!({"ok":true,"data":{},"errors":[],"code":"remote-secret"}).to_string(),
            None,
            "server_refused",
        ),
        (
            422,
            json!({"ok":false,"data":{},"errors":["remote-secret"],"code":"business"}).to_string(),
            None,
            "server_refused",
        ),
        (
            422,
            json!({"ok":false,"errors":["remote-secret"]}).to_string(),
            None,
            "server_refused",
        ),
        (
            422,
            json!({"ok":false,"code":"","errors":["remote-secret"]}).to_string(),
            None,
            "server_refused",
        ),
        (
            422,
            json!({"ok":false,"code":12,"errors":["remote-secret"]}).to_string(),
            None,
            "server_refused",
        ),
        (
            422,
            json!({"ok":false,"code":"business","errors":[12]}).to_string(),
            None,
            "server_refused",
        ),
        (
            422,
            json!({"ok":false,"code":"business","errors":["remote-secret"],"appliedRules":[12]})
                .to_string(),
            None,
            "server_refused",
        ),
        (
            422,
            json!({"ok":false,"code":"business","errors":["remote-secret"]}).to_string(),
            Some(1000),
            "server_refused",
        ),
        (
            403,
            json!({"ok":false,"code":"business","errors":["remote-secret"]}).to_string(),
            None,
            "server_refused",
        ),
        (
            408,
            json!({"ok":false,"code":"business","errors":["remote-secret"]}).to_string(),
            None,
            "outcome_unknown",
        ),
        (
            503,
            json!({"ok":false,"code":"business","errors":["remote-secret"]}).to_string(),
            None,
            "outcome_unknown",
        ),
        (
            200,
            json!({"ok":true,"data":{"secret":"remote-secret"},"errors":[]}).to_string(),
            Some(1000),
            "outcome_unknown",
        ),
        (200, String::new(), Some(1), "outcome_unknown"),
    ] {
        let (origin, t) = server_response(status, response, declared_length);
        let out = call(
            "post",
            &origin,
            &f,
            &["--document-id", "156", "--json"],
            None,
        );
        assert_eq!(out.status.code(), Some(1));
        let actual: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(actual["ok"], false);
        assert_eq!(actual["code"], code);
        assert!(actual.get("data").is_none());
        let output = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        for secret in ["remote-secret", "session=synthetic", origin.as_str()] {
            assert!(!output.contains(secret));
        }
        t.join().unwrap();
    }
}
