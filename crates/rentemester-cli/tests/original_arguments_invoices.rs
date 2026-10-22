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
const ARTIFACT: &str = "<source>æ &amp; exact</source>\n";
const HASH: &str = "82936549ab45c9664092f4552c5c03ac5a779a6e75087904a80ce094d7e01235";
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rm158-arguments-{}-{}",
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

fn invoke(
    f: &Fixture,
    origin: &str,
    root: &str,
    operation: &str,
    form: usize,
    extra: &[&str],
) -> Output {
    let cookie = f.0.join("cookie");
    let cookie = cookie.to_str().unwrap();
    let mut args = match form {
        0 => vec![
            root.into(),
            operation.into(),
            "--server".into(),
            origin.into(),
            "--company".into(),
            "synthetic".into(),
            "--cookie-file".into(),
            cookie.into(),
            "--json".into(),
        ],
        1 => vec![
            format!("--server={origin}"),
            "--company=synthetic".into(),
            format!("--cookie-file={cookie}"),
            "--format=json".into(),
            root.into(),
            operation.into(),
        ],
        _ => vec![
            root.into(),
            "--format".into(),
            "json".into(),
            "--company=synthetic".into(),
            operation.into(),
            format!("--server={origin}"),
            format!("--cookie-file={cookie}"),
        ],
    };
    args.push("--allow-loopback-http".into());
    args.extend(extra.iter().map(|s| s.to_string()));
    Command::new(BIN)
        .env_remove("RENTEMESTER_COMPANY")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn read_forms_preserve_negative_blank_and_leading_dash_data() {
    let f = Fixture::new();
    for form in 0..3 {
        let (origin, request) = server(
            200,
            json!({"ok":true,"data":{"count":0,"rows":[]},"errors":[]}),
        );
        let extra = if form == 0 {
            vec![
                "--customer=--help=literal",
                "--invoice-number",
                "",
                "--min-amount",
                "-12.34",
            ]
        } else {
            vec![
                "--customer=--help=literal",
                "--invoice-number=",
                "--min-amount=-12.34",
            ]
        };
        let out = invoke(&f, &origin, "invoice", "list", form, &extra);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let (header, body) = request.join().unwrap();
        assert!(header.contains("minAmount=-12.34"));
        assert!(header.contains("invoiceNumber="));
        if form != 0 {
            assert!(header.contains("customer=--help%3Dliteral"));
        }
        assert!(body.is_null());
    }
}

#[test]
fn terminator_words_are_literal_query_not_help_or_profile_options() {
    let f = Fixture::new();
    let (origin, request) = server(
        200,
        json!({"ok":true,"data":{"count":0,"rows":[]},"errors":[]}),
    );
    let out = invoke(
        &f,
        &origin,
        "invoice",
        "find",
        1,
        &[
            "--",
            "--help",
            "--local-profile=missing",
            "--company=other",
            "--format=csv",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let (header, _) = request.join().unwrap();
    assert!(header.starts_with("GET /api/companies/synthetic/"));
    assert!(
        header.contains("query=--help+--local-profile%3Dmissing+--company%3Dother+--format%3Dcsv")
    );
}

#[test]
fn mutation_forms_send_exact_payload_once_and_preserve_refusal_and_unknown() {
    let f = Fixture::new();
    for (status, receipt, exit) in [
        (
            200,
            json!({"ok":true,"data":{"registered":true},"errors":[]}),
            0,
        ),
        (
            422,
            json!({"ok":false,"code":"source_gate","errors":["source refusal"]}),
            1,
        ),
        (503, json!({"unexpected":"untrusted"}), 1),
    ] {
        for form in 0..3 {
            let (origin, request) = server(status, receipt.clone());
            let extra = if form == 0 {
                vec![
                    "--document-id",
                    "7",
                    "--date",
                    "2026-01-01",
                    "--note",
                    "-literal",
                ]
            } else {
                vec!["--document-id=7", "--date=2026-01-01", "--note=-literal"]
            };
            let out = invoke(&f, &origin, "invoice", "remind", form, &extra);
            assert_eq!(
                out.status.code(),
                Some(exit),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(out.stderr.is_empty());
            let value: Value = serde_json::from_slice(&out.stdout).unwrap();
            if status == 422 {
                assert_eq!(value, receipt);
            }
            if status == 503 {
                assert_eq!(value["code"], "outcome_unknown");
                assert!(!String::from_utf8_lossy(&out.stdout).contains("untrusted"));
            }
            let (header, body) = request.join().unwrap();
            assert!(header.starts_with("POST /api/companies/synthetic/invoices/original/remind "));
            assert_eq!(
                body,
                json!({"documentId":7,"date":"2026-01-01","note":"-literal","confirm":true})
            );
        }
    }
}

#[test]
fn all_six_families_offer_pure_help_and_financial_validation_is_offline() {
    for (root, operation) in [
        ("invoice", "list"),
        ("invoice", "remind"),
        ("invoice", "create"),
        ("invoice", "validate"),
        ("invoice", "transmit-digisense"),
        ("recurring-invoice", "list"),
        ("recurring-invoice", "run-workspace"),
    ] {
        let out = Command::new(BIN)
            .args(["--format=json", root, operation, "--help"])
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{root} {operation}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(serde_json::from_slice::<Value>(&out.stdout).is_ok());
    }
    let f = Fixture::new();
    let path = f.0.join("invoice.json");
    let payload = json!({"invoiceType":"full","issueDate":"2026-01-01","lines":[{"description":"Synthetic","quantity":2,"unitPriceExVat":50,"lineTotalExVat":100}],"totals":{"netAmount":100,"vatRate":0.25,"vatAmount":25,"grossAmount":125}});
    fs::write(&path, payload.to_string()).unwrap();
    let out = Command::new(BIN)
        .args([
            "--format=json",
            "invoice",
            "validate",
            &format!("--input={}", path.display()),
        ])
        .output()
        .unwrap();
    let expected = rentemester_engine::invoice_validation::validate(&payload);
    assert_eq!(
        out.status.code(),
        Some(if expected["ok"] == true { 0 } else { 1 })
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        expected
    );
}

#[test]
fn malformed_arguments_exit_two_without_requests() {
    let f = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    for extra in [
        vec!["--format=csv"],
        vec!["--format="],
        vec!["--json=false"],
        vec!["--customer"],
        vec!["--unknown=x"],
        vec!["--min-amount=no"],
        vec!["--customer=a", "--customer=b"],
    ] {
        let out = invoke(&f, &origin, "invoice", "list", 0, &extra);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

#[test]
fn remaining_family_positive_attached_forms_keep_existing_transport() {
    let f = Fixture::new();
    for (root, operation, extra, response, expected) in [
        (
            "invoice",
            "render",
            vec!["--document-id=7"],
            json!({"ok":true,"data":{"rendered":true},"errors":[]}),
            json!({"confirm":true,"documentId":7}),
        ),
        (
            "invoice",
            "imported-receivables",
            vec!["--as-of=2026-01-01"],
            json!({"ok":true,"importedReceivables":{"rows":[],"count":0}}),
            Value::Null,
        ),
        (
            "invoice",
            "transmit-digisense",
            vec!["--document-id=7", "--confirm=yes"],
            json!({"ok":true,"invoiceNumber":"INV","envelope":ARTIFACT,"envelopeSha256":HASH,"oioublSha256":HASH,"submissionReference":"source-ref","idempotencyKey":"source-key","status":"acknowledged","transmissionId":"actual-id","duplicate":false,"appliedRules":["submit"],"errors":[]}),
            json!({"confirm":true,"documentId":7}),
        ),
        (
            "recurring-invoice",
            "list",
            vec!["--include-inactive"],
            json!({"ok":true,"rows":[],"count":0,"errors":[]}),
            Value::Null,
        ),
    ] {
        for form in 0..3 {
            let (origin, request) = server(200, response.clone());
            let out = invoke(&f, &origin, root, operation, form, &extra);
            assert_eq!(
                out.status.code(),
                Some(0),
                "{operation}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(out.stderr.is_empty());
            assert!(serde_json::from_slice::<Value>(&out.stdout).unwrap()["ok"] == true);
            let (_, body) = request.join().unwrap();
            assert_eq!(body, expected);
        }
    }
}

#[test]
fn non_find_terminator_help_is_usage_without_credentials_or_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    for (root, operation) in [
        ("invoice", "list"),
        ("invoice", "create"),
        ("invoice", "remind"),
        ("invoice", "validate"),
        ("invoice", "transmit-digisense"),
        ("recurring-invoice", "list"),
        ("recurring-invoice", "run-workspace"),
    ] {
        let out = Command::new(BIN)
            .env_remove("RENTEMESTER_COMPANY")
            .args([
                root,
                operation,
                "--company",
                "synthetic",
                "--server",
                &origin,
                "--cookie-file",
                "/nonexistent/private",
                "--",
                "--help",
            ])
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{root} {operation}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}
