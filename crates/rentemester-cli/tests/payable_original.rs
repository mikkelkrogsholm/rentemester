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
            "rm146-cli-{}-{}",
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
            "payable",
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
fn original_three_commands_use_source_fields_filters_and_flat_rules_without_client_keys() {
    let f = Fixture::new();
    for (kind, extra, data, rule) in [
        (
            "register",
            vec![
                "--document-id",
                "4",
                "--bill-date",
                "2026-01-01",
                "--due-date",
                "2026-01-31",
                "--expense-account",
                "expense",
                "--vat-treatment",
                "exempt",
                "--note",
                " raw note ",
            ],
            json!({"payableId":7,"entryId":8,"entryNo":9}),
            Some("DK-PAYABLE-001"),
        ),
        (
            "pay",
            vec![
                "--payable-id",
                "7",
                "--bank-transaction-id",
                "11",
                "--amount",
                "50.25",
                "--date",
                "2026-02-01",
                "--payment-account",
                "bank",
            ],
            json!({"paymentId":12,"openBalance":74.75}),
            Some("DK-PAYABLE-PAYMENT-001"),
        ),
        (
            "list",
            vec![
                "--status",
                "overdue",
                "--as-of",
                "2026-04-01",
                "--supplier",
                " SYN ",
                "--vendor-id",
                "3",
                "--from",
                "2026-03-01",
                "--to",
                "2026-01-01",
                "--min-days",
                "0",
            ],
            json!({"count":0,"rows":[],"totalOpenBalance":0,"overdueOpenBalance":0,"notYetDueOpenBalance":0,"asOfDate":"2026-04-01"}),
            None,
        ),
    ] {
        let mut value = json!({"ok":true,"data":data,"errors":[]});
        if let Some(rule) = rule {
            value["appliedRules"] = json!([rule]);
        }
        let (origin, t) = server(200, value);
        let mut args = extra;
        args.push("--json");
        let out = call(kind, &origin, &f, kind == "pay", &args);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let mut expected = data;
        expected["ok"] = json!(true);
        expected["errors"] = json!([]);
        if let Some(rule) = rule {
            expected["appliedRules"] = json!([rule]);
        }
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap(),
            expected
        );
        assert!(out.stderr.is_empty());
        let (header, body) = t.join().unwrap();
        assert!(header.contains(&format!("/payables/original/{kind}")));
        if kind == "register" {
            assert_eq!(body["expenseAccount"], "expense");
            assert!(body.get("expenseAccountNo").is_none());
            assert_eq!(body["confirm"], true);
        }
        if kind == "pay" {
            assert_eq!(body["amount"], 50.25);
            assert_eq!(body["date"], "2026-02-01");
            assert_eq!(body["paymentAccount"], "bank");
            assert!(body.get("paymentDate").is_none());
        }
        assert!(body.get("idempotencyKey").is_none());
        if kind == "list" {
            assert!(
                header.contains("supplier=+SYN+")
                    && header.contains("minDays=0")
                    && !header.contains("1970")
            );
        }
    }
}
#[test]
fn strict_source_flags_business_streams_and_human_display_server_totals_and_aging() {
    let f = Fixture::new();
    for (kind, args) in [
        (
            "pay",
            vec![
                "--payable-id",
                "7",
                "--bank-transaction-id",
                "11",
                "--confirm",
                "yes",
            ],
        ),
        (
            "pay",
            vec![
                "--payable-id",
                "7",
                "--bank-transaction-id",
                "11",
                "--idempotency-key",
                "key",
            ],
        ),
        ("list", vec!["--principal", "user:forged"]),
        ("list", vec!["--min-days", "-1"]),
        (
            "pay",
            vec![
                "--payable-id",
                "7",
                "--payable-id",
                "8",
                "--bank-transaction-id",
                "11",
            ],
        ),
        ("register", vec![]),
    ] {
        let out = call(kind, "http://127.0.0.1:9", &f, false, &args);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
    }
    let value = json!({"ok":false,"errors":["server-owned invalid date"],"code":"business"});
    let (origin, t) = server(200, value.clone());
    let out = call(
        "list",
        &origin,
        &f,
        false,
        &["--as-of", "2026-02-30", "--json"],
    );
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), value);
    assert!(out.stderr.is_empty());
    t.join().unwrap();
    let (origin, t) = server(
        200,
        json!({"ok":true,"data":{"count":1,"asOfDate":"2026-04-01","totalOpenBalance":1234.5,"overdueOpenBalance":1000,"notYetDueOpenBalance":234.5,"rows":[{"payableId":7,"documentId":4,"supplierName":"Synthetic supplier","billNo":"S-7","billDate":"2026-01-01","dueDate":"2026-01-31","grossAmount":125,"paidAmount":50,"openBalance":75,"isOverdue":true,"overdueDays":60,"agingBucket":"31-60","status":"open"}]},"errors":[]}),
    );
    let out = call("list", &origin, &f, false, &["--format", "human"]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("Kreditorliste (1)")
            && text.contains("1.234,50 kr.")
            && text.contains("75,00 kr.")
            && text.contains("Forfalden 60 dage (interval 31-60)")
    );
    t.join().unwrap();
    for kind in ["register", "list", "pay"] {
        assert_eq!(
            Command::new(BIN)
                .args(["payable", kind, "--help"])
                .output()
                .unwrap()
                .status
                .code(),
            Some(0)
        );
    }
}
