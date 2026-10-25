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
            "rm141-cli-{}-{}",
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
fn server(value: Value) -> (String, thread::JoinHandle<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = vec![];
        while !request.ends_with(b"\r\n\r\n") {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            request.extend(&b[..n]);
        }
        let body = value.to_string();
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        String::from_utf8(request).unwrap()
    });
    (origin, t)
}
fn call(prefix: &[&str], origin: &str, fixture: &Fixture, service: bool, extra: &[&str]) -> Output {
    Command::new(BIN)
        .args(prefix)
        .args([
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
            fixture
                .0
                .join(if service { "key" } else { "cookie" })
                .to_str()
                .unwrap(),
        ])
        .args(extra)
        .output()
        .unwrap()
}
#[test]
fn all_three_original_json_commands_flatten_only_canonical_core_and_list_is_unpaged() {
    let fixture = Fixture::new();
    let list = json!({"ok":true,"data":{"rows":[{"id":7,"amount":-12.34,"currency":"EUR","journalEntryId":null}],"count":1},"errors":[]});
    let report = json!({"ok":true,"data":{"periodStart":"2020-01-01","periodEnd":"2027-12-31","matchedCount":0,"unmatchedCount":1,"matchedAmountTotal":0,"unmatchedAmountTotal":-12.34,"matched":[],"unmatched":[]},"errors":[],"appliedRules":["DK-BOOKKEEPING-RECONCILIATION-001"]});
    let accounts = json!({"ok":true,"data":{"accounts":[{"id":2,"slug":"fx","currency":"EUR","active":false}],"count":1},"errors":[]});
    for (prefix, response, extra, path) in [
        (
            vec!["bank", "list"],
            list,
            vec![
                "--json",
                "--text-match",
                "A & B",
                "--amount",
                "-12.345",
                "--account",
                "fx",
            ],
            "/bank/transactions?textMatch=A+%26+B&amount=-12.345&account=fx",
        ),
        (
            vec!["reconcile", "bank"],
            report,
            vec!["--json", "--from", "2020-01-01", "--to", "2027-12-31"],
            "/bank/reconciliation?from=2020-01-01&to=2027-12-31",
        ),
        (
            vec!["bank-account", "list"],
            accounts,
            vec!["--json"],
            "/bank/accounts",
        ),
    ] {
        let (origin, t) = server(response.clone());
        let output = call(&prefix, &origin, &fixture, false, &extra);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        assert!(output.stdout.ends_with(b"\n"));
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        let mut expected = response["data"].clone();
        expected["ok"] = json!(true);
        expected["errors"] = json!([]);
        if let Some(rules) = response.get("appliedRules") {
            expected["appliedRules"] = rules.clone();
        }
        assert_eq!(value, expected);
        let request = t.join().unwrap();
        assert!(request.contains(path), "{request}");
        assert!(!request.contains("limit="));
        assert!(!request.contains("offset="));
        assert!(!request.contains("year="));
    }
}
#[test]
fn business_messages_streams_usage_and_human_original_units_are_preserved() {
    let fixture = Fixture::new();
    let refusal =
        json!({"ok":false,"code":"NOT_FOUND","errors":["bank account 'unknown' does not exist"]});
    for json_mode in [true, false] {
        let (origin, t) = server(refusal.clone());
        let extra = if json_mode {
            vec!["--account", "unknown", "--json"]
        } else {
            vec!["--account", "unknown"]
        };
        let output = call(&["bank", "list"], &origin, &fixture, false, &extra);
        assert_eq!(output.status.code(), Some(1));
        if json_mode {
            assert_eq!(
                serde_json::from_slice::<Value>(&output.stdout).unwrap(),
                refusal
            );
            assert!(output.stderr.is_empty());
        } else {
            assert!(output.stdout.is_empty());
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("bank account 'unknown' does not exist")
            );
        }
        t.join().unwrap();
    }
    for args in [
        vec!["bank", "list", "--json"],
        vec!["reconcile", "bank", "--json"],
        vec!["bank", "list", "--limit", "5"],
    ] {
        let output = Command::new(BIN).args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    let output = call(
        &["bank", "list"],
        "http://127.0.0.1:9",
        &fixture,
        false,
        &["--amount", "NaN", "--json"],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let report = json!({"ok":true,"data":{"periodStart":"2020-01-01","periodEnd":"2027-12-31","matchedCount":1,"unmatchedCount":0,"matchedAmountTotal":-123.45,"unmatchedAmountTotal":0,"matched":[{"bankTransactionId":7,"transactionDate":"2020-01-02","amount":-2,"text":"synthetic"}],"unmatched":[]},"errors":[],"appliedRules":["DK-BOOKKEEPING-RECONCILIATION-001"]});
    let (origin, t) = server(report);
    let output = call(
        &["reconcile", "bank"],
        &origin,
        &fixture,
        true,
        &["--from", "2020-01-01", "--to", "2027-12-31"],
    );
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Bankafstemning"));
    assert!(text.contains("-123.45 originale valutaenheder"));
    assert!(!text.contains("DKK"));
    assert!(
        t.join()
            .unwrap()
            .contains("x-rentemester-service-key: synthetic-key")
    );
    let output = Command::new(BIN)
        .args(["bank", "list", "--help"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
}
