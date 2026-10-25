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
            "rm147-cli-{}-{}",
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

fn call(kind: &str, origin: &str, f: &Fixture, service: bool, extra: &[&str]) -> Output {
    Command::new(BIN)
        .args([
            "invoice",
            kind,
            "--server",
            origin,
            "--allow-loopback-http",
            "--company",
            "synthetic",
            if service {
                "--service-key-file"
            } else {
                "--cookie-file"
            },
            f.0.join(if service { "key" } else { "cookie" })
                .to_str()
                .unwrap(),
        ])
        .args(extra)
        .output()
        .unwrap()
}
#[test]
fn original_four_cli_reads_preserve_positional_find_exact_amount_selectors_and_flat_data() {
    let f = Fixture::new();
    for (kind, extra, contains, data) in [
        (
            "list",
            vec![
                "--customer-cvr",
                " dk123 ",
                "--customer",
                " ",
                "--invoice-number",
                " Exact ",
                "--min-amount",
                "-12.34",
                "--max-amount",
                "0",
            ],
            vec!["customerCvr=+dk123+", "customer=+", "minAmount=-12.34"],
            json!({"count":0,"rows":[]}),
        ),
        (
            "find",
            vec![
                "two",
                "literal",
                "words",
                "--amount",
                "-12.34",
                "--customer",
                "Syn",
                "--as-of",
                "2020-01-01",
            ],
            vec![
                "query=two+literal+words",
                "amount=-12.34",
                "asOf=2020-01-01",
            ],
            json!({"count":0,"query":"two literal words","rows":[]}),
        ),
        (
            "status",
            vec!["--document-id", "7", "--invoice-number", "ignored"],
            vec!["documentId=7", "invoiceNumber=ignored"],
            json!({"invoiceDocumentId":7,"currency":"EUR","claimOpenBalance":null,"claimCurrency":"DKK","registeredClaimBalanceDkk":100,"payments":[{"amount":12}]}),
        ),
        (
            "overdue",
            vec!["--min-days", "0", "--as-of", "2026-04-01"],
            vec!["minDays=0", "asOf=2026-04-01"],
            json!({"count":0,"rows":[]}),
        ),
    ] {
        let (origin, t) = server(200, json!({"ok":true,"data":data,"errors":[]}));
        let mut args = extra;
        args.push("--json");
        let out = call(kind, &origin, &f, kind == "status", &args);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let mut expected = data;
        expected["ok"] = json!(true);
        expected["errors"] = json!([]);
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap(),
            expected
        );
        let (header, body) = t.join().unwrap();
        assert!(header.starts_with(&format!(
            "GET /api/companies/synthetic/invoices/original/{kind}?"
        )));
        assert!(contains.into_iter().all(|s| header.contains(s)));
        assert!(body.is_null());
    }
}
#[test]
fn missing_selector_usage_unmatched_business_and_practical_currency_safe_human() {
    let f = Fixture::new();
    for (kind, args) in [
        ("status", vec![]),
        ("find", vec!["--min-amount", "1"]),
        ("list", vec!["--confirm", "yes"]),
        ("list", vec!["--status", "all", "--status", "paid"]),
        ("overdue", vec!["--min-days", "-1"]),
        ("list", vec!["--min-amount", "NaN"]),
    ] {
        let out = call(kind, "http://127.0.0.1:9", &f, false, &args);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
    }
    let value = json!({"ok":false,"errors":["invoice document 999 does not exist"]});
    let (origin, t) = server(200, value.clone());
    let out = call(
        "status",
        &origin,
        &f,
        false,
        &["--document-id", "999", "--json"],
    );
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), value);
    assert!(out.stderr.is_empty());
    t.join().unwrap();
    let (origin, t) = server(
        200,
        json!({"ok":true,"data":{"count":1,"rows":[{"invoiceNumber":"INV-7","customerName":"Synthetic customer","invoiceDate":"2027-01-01","effectiveDueDate":"2027-01-31","grossAmount":125,"openBalance":75,"currency":"EUR","status":"open","isOverdue":true,"overdueDays":12}]},"errors":[]}),
    );
    let out = call("list", &origin, &f, false, &["--format", "human"]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("Fakturaer (1)")
            && text.contains("INV-7")
            && text.contains("75 EUR")
            && text.contains("12 dage forfalden")
    );
    assert!(!text.contains("kr."));
    t.join().unwrap();
    for kind in ["list", "find", "status", "overdue"] {
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
fn exact_422_receipts_exit_one_for_all_original_reads() {
    let f = Fixture::new();
    for (kind, selectors) in [
        ("list", vec![]),
        ("find", vec!["literal"]),
        ("status", vec!["--document-id", "7"]),
        ("overdue", vec![]),
    ] {
        let receipt = json!({"ok":false,"code":"ORIGINAL_SOURCE_MISSING","errors":["server-owned source refusal"],"appliedRules":["original.source"]});
        let (origin, t) = server(422, receipt.clone());
        let mut args = selectors;
        args.push("--json");
        let out = call(kind, &origin, &f, kind == "status", &args);
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap(),
            receipt
        );
        assert!(out.stderr.is_empty());
        let (header, body) = t.join().unwrap();
        assert!(header.starts_with(&format!(
            "GET /api/companies/synthetic/invoices/original/{kind}"
        )));
        assert!(body.is_null());
    }
}
