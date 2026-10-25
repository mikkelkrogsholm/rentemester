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
            "rm145-cli-{}-{}",
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

fn call(command: &str, origin: &str, f: &Fixture, service: bool, extra: &[&str]) -> Output {
    Command::new(BIN)
        .args([
            "bank",
            command,
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
            "--document-id",
            "3",
            "--bank-transaction-id",
            "7",
            "--bill-date",
            "2026-01-01",
            "--due-date",
            "2026-01-31",
            "--expense-account",
            "expense",
        ])
        .args(extra)
        .output()
        .unwrap()
}
#[test]
fn original_plan_apply_flags_preserve_full_server_hash_receipt_and_assertion() {
    let f = Fixture::new();
    let hash = "a".repeat(64);
    for (write, data) in [
        (
            false,
            json!({"plan":{"planHash":"full-native","sourceProof":{"kept":true}}}),
        ),
        (
            true,
            json!({"id":4,"idempotent":false,"originalJournalEntryId":8,"reversalJournalEntryId":9,"sourcePrincipal":{"kind":"service-account","subjectId":"service-7"},"idempotency":{"replayed":true,"receiptId":"server"}}),
        ),
    ] {
        let (origin, t) = server(200, json!({"ok":true,"data":data,"errors":[]}));
        let extra = if write {
            vec![
                "--plan-hash",
                &hash,
                "--reason",
                " raw reason ",
                "--idempotency-key",
                " raw key ",
                "--principal",
                "service-account:service-7",
                "--confirm",
                "yes",
                "--json",
            ]
        } else {
            vec![
                "--vat-treatment",
                "exempt",
                "--vendor-id",
                "2",
                "--note",
                " raw note ",
                "--json",
            ]
        };
        let out = call(
            if write {
                "direct-payable-apply"
            } else {
                "direct-payable-plan"
            },
            &origin,
            &f,
            write,
            &extra,
        );
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
        assert!(out.stderr.is_empty());
        let (header, body) = t.join().unwrap();
        assert!(header.contains("/bank/original/direct-payable-"));
        if write {
            assert_eq!(
                body["expectedPrincipal"],
                json!({"kind":"service-account","subjectId":"service-7"})
            );
            assert_eq!(body["reason"], " raw reason ");
            assert_eq!(body["idempotencyKey"], " raw key ");
            assert!(body.get("actor").is_none());
        } else {
            assert!(body.get("confirm").is_none());
            assert!(body.get("expectedPrincipal").is_none());
            assert_eq!(body["note"], " raw note ");
        }
    }
}
#[test]
fn exact_confirmation_principal_duplicate_usage_and_business_streams() {
    let f = Fixture::new();
    for extra in [
        vec![],
        vec!["--confirm", "true"],
        vec![
            "--confirm",
            "yes",
            "--plan-hash",
            "a",
            "--reason",
            "r",
            "--idempotency-key",
            "k",
        ],
        vec!["--confirm", "yes", "--document-id", "4"],
    ] {
        let out = call(
            "direct-payable-apply",
            "http://127.0.0.1:9",
            &f,
            false,
            &extra,
        );
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
    }
    let out = call(
        "direct-payable-plan",
        "http://127.0.0.1:9",
        &f,
        false,
        &["--principal", "user:forged"],
    );
    assert_eq!(out.status.code(), Some(2));
    let value = json!({"ok":false,"code":"business","errors":["server-owned failure"]});
    let (origin, t) = server(200, value.clone());
    let out = call("direct-payable-plan", &origin, &f, false, &["--json"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), value);
    assert!(out.stderr.is_empty());
    t.join().unwrap();
    for command in ["direct-payable-plan", "direct-payable-apply"] {
        assert_eq!(
            Command::new(BIN)
                .args(["bank", command, "--help"])
                .output()
                .unwrap()
                .status
                .code(),
            Some(0)
        );
    }
}
