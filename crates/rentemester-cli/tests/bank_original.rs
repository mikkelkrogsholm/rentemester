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
            "rm144-cli-{}-{}",
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
        ])
        .args(extra)
        .output()
        .unwrap()
}
#[test]
fn four_original_commands_flatten_only_server_data_and_assert_principal_without_actor() {
    let f = Fixture::new();
    let hash = "a".repeat(64);
    let cases = [
        (
            "suggest-matches",
            vec!["--max", "3"],
            json!({"count":0,"rows":[]}),
        ),
        (
            "correction-plan",
            vec![
                "--bank-transaction-id",
                "7",
                "--replacement-journal-entry-id",
                "9",
            ],
            json!({"plan":{"planHash":"server","sourceProof":{"kept":true}}}),
        ),
        (
            "link-journal",
            vec![
                "--bank-transaction-id",
                "7",
                "--journal-entry-id",
                "8",
                "--match-method",
                "manual-review",
                "--note",
                " raw note ",
                "--confirm",
                "yes",
            ],
            json!({"id":1,"idempotent":false}),
        ),
        (
            "correction-apply",
            vec![
                "--bank-transaction-id",
                "7",
                "--replacement-journal-entry-id",
                "9",
                "--expected-reconciliation-id",
                "current",
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
            ],
            json!({"id":2,"receipt":{"server":true}}),
        ),
    ];
    for (command, mut args, data) in cases {
        let response = json!({"ok":true,"data":data,"errors":[],"appliedRules":["server-rule"]});
        let (origin, t) = server(200, response);
        args.push("--json");
        let out = call(command, &origin, &f, command == "correction-apply", &args);
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
        assert!(out.stderr.is_empty());
        let (header, body) = t.join().unwrap();
        assert!(header.contains("/bank/original/"));
        if command == "correction-apply" {
            assert_eq!(
                body["expectedPrincipal"],
                json!({"kind":"service-account","subjectId":"service-7"})
            );
            assert_eq!(body["reason"], " raw reason ");
            assert_eq!(body["idempotencyKey"], " raw key ");
            assert!(body.get("actor").is_none());
        }
        if command == "link-journal" {
            assert!(body.get("planHash").is_none());
            assert!(body.get("sourceReference").is_none());
        }
    }
}
#[test]
fn exact_confirmation_usage_business_streams_and_human_server_scores() {
    let f = Fixture::new();
    for args in [
        vec![],
        vec!["--confirm", "true"],
        vec!["--confirm", "yes", "--bank-transaction-id", "7"],
        vec![
            "--confirm",
            "yes",
            "--bank-transaction-id",
            "0",
            "--journal-entry-id",
            "8",
            "--match-method",
            "manual-review",
        ],
    ] {
        let out = call("link-journal", "http://127.0.0.1:9", &f, false, &args);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
    }
    let value =
        json!({"ok":false,"code":"stale","errors":["server refusal"],"appliedRules":["rule"]});
    let (origin, t) = server(200, value.clone());
    let out = call(
        "correction-plan",
        &origin,
        &f,
        false,
        &[
            "--bank-transaction-id",
            "7",
            "--replacement-journal-entry-id",
            "9",
            "--json",
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), value);
    t.join().unwrap();
    let (origin, t) = server(
        200,
        json!({"ok":true,"data":{"count":2,"rows":[{"bankTransactionId":7,"date":"2026-10-01","amount":12.34,"currency":"EUR","text":"Synthetic","suggestions":[{"kind":"purchase_sale","documentId":42,"confidence":1.4,"reasons":["server-scored"]}]},{"bankTransactionId":8,"date":"2026-10-01","amount":-1,"currency":"DKK","text":"Unsafe synthetic","suggestions":[],"unsafeMatchReason":"server-owned unsafe reason"}]},"errors":[]}),
    );
    let out = call("suggest-matches", &origin, &f, false, &[]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("1.4")
            && text.contains("42")
            && text.contains("EUR")
            && text.contains("server-scored")
            && text.contains("server-owned unsafe reason")
    );
    t.join().unwrap();
    for command in [
        "suggest-matches",
        "link-journal",
        "correction-plan",
        "correction-apply",
    ] {
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
