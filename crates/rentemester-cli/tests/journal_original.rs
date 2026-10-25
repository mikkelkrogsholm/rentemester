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
            "rm160-journal-cli-{}-{}",
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
fn result(kind: &str) -> Value {
    let mut v = json!({"ok":true,"entryId":17,"entryNo":"JE-000017","entryHash":"a".repeat(64),"appliedRules":["DK-BOOKKEEPING-APPEND-ONLY-001"],"errors":[]});
    if kind == "reverse" {
        v["originalEntryId"] = json!(16);
    }
    if kind == "dry-run" {
        v["previousHash"] = json!("GENESIS");
        v["accountEffects"] = json!([
            {"accountNo":"1000","accountName":"Synthetic asset","balanceBefore":0,"balanceAfter":1250.5,"delta":1250.5},
            {"accountNo":"2000","accountName":"Synthetic bank","balanceBefore":0,"balanceAfter":-1250.5,"delta":-1250.5}
        ]);
    }
    v
}
// Count every accepted request, including unexpected retries and selector downloads.
fn server(status: u16, response: String) -> (String, thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        let mut last = std::time::Instant::now();
        while last.elapsed() < if requests.is_empty() { std::time::Duration::from_secs(5) } else { std::time::Duration::from_millis(400) } {
            let (mut stream, _) = match listener.accept() {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            last = std::time::Instant::now();
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let end = loop {
                let mut b = [0; 4096];
                let n = stream.read(&mut b).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&b[..n]);
                if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let header = String::from_utf8_lossy(&bytes[..end]);
            let target = header.lines().next().unwrap().to_owned();
            assert!(!header.to_lowercase().contains("idempotency"));
            assert!(header.contains("session=synthetic") || header.contains("synthetic-key"));
            let length: usize = header
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|s| s.trim().parse().unwrap())
                })
                .unwrap_or(0);
            while bytes.len() < end + length {
                let mut b = [0; 4096];
                let n = stream.read(&mut b).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&b[..n]);
            }
            requests.push(json!({"target":target,"body":serde_json::from_slice::<Value>(&bytes[end..end+length]).unwrap()}));
            write!(stream,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
        }
        requests
    });
    (origin, handle)
}
fn call(kind: &str, origin: &str, f: &Fixture, service: bool, extra: &[&str]) -> Output {
    Command::new(BIN)
        .env_remove("RENTEMESTER_COMPANY")
        .args([
            "--format=json",
            "journal",
            kind,
            "--server",
            origin,
            "--allow-loopback-http",
            "--company=synthetic",
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
fn numeric_post_and_rollback_only_dry_run_use_flat_results() {
    let f = Fixture::new();
    let input = json!({"transactionDate":"2026-05-16","text":"Synthetic transfer","currency":"dkk","sourceBankTransactionId":4,"lines":[{"accountNo":"1000","debitAmount":1250.5},{"accountNo":"2000","creditAmount":1250.5}]});
    let path = f.0.join("input.json");
    fs::write(&path, input.to_string()).unwrap();
    for kind in ["post", "dry-run"] {
        let expected = result(kind);
        let (origin, handle) = server(200, expected.to_string());
        let output = call(
            kind,
            &origin,
            &f,
            kind == "dry-run",
            &["--input", path.to_str().unwrap()],
        );
        assert_eq!(
            output.status.code(),
            Some(0),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            expected
        );
        let requests = handle.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0]["target"],
            format!("POST /api/companies/synthetic/journal/original/{kind} HTTP/1.1")
        );
        let mut body = json!({"input":input});
        if kind == "post" {
            body["confirm"] = json!(true);
        }
        assert_eq!(requests[0]["body"], body);
    }
}
#[test]
fn reverse_selectors_remain_intact_and_server_selection_usage_is_stderr() {
    let f = Fixture::new();
    for flags in [
        vec![
            "--entry-id",
            "16",
            "--entry-no",
            " JE-16 ",
            "--match-text",
            " exact ",
            "--match-date",
            "2026-05-16",
            "--match-document-id",
            "3",
        ],
        vec!["--entry-no", " JE-16 "],
        vec![
            "--match-text",
            " exact ",
            "--match-date",
            "2026-05-16",
            "--match-document-id",
            "3",
        ],
    ] {
        for status in [200, 422] {
            let expected = if status == 200 {
                result("reverse")
            } else {
                json!({"ok":false,"code":"journal_selection_usage","errors":["Multiple journal entries matched"],"appliedRules":[]})
            };
            let (origin, handle) = server(status, expected.to_string());
            let mut extra = flags.clone();
            extra.extend(["--date", "2026-05-17", "--reason", "Correction"]);
            let output = call("reverse", &origin, &f, false, &extra);
            assert_eq!(
                output.status.code(),
                Some(if status == 200 { 0 } else { 2 })
            );
            if status == 422 {
                assert!(output.stdout.is_empty());
                assert!(!output.stderr.is_empty());
            }
            let requests = handle.join().unwrap();
            assert_eq!(requests.len(), 1);
            let body = &requests[0]["body"];
            assert_eq!(body["confirm"], true);
            for pair in flags.chunks_exact(2) {
                let key = match pair[0] {
                    "--entry-id" => "entryId",
                    "--entry-no" => "entryNo",
                    "--match-text" => "matchText",
                    "--match-date" => "matchDate",
                    _ => "matchDocumentId",
                };
                let expected = if matches!(key, "entryId" | "matchDocumentId") {
                    json!(pair[1].parse::<u64>().unwrap())
                } else {
                    json!(pair[1])
                };
                assert_eq!(body[key], expected);
            }
        }
    }
}
#[test]
fn business_authority_and_unknown_writes_never_retry() {
    let f = Fixture::new();
    let path = f.0.join("input.json");
    fs::write(&path, "{\"lines\":[],\"transactionDate\":\"not-a-date\"}").unwrap();
    for (kind,status,response,code) in [
        ("post",200,json!({"ok":false,"appliedRules":["RULE"],"errors":["At least two lines required"]}).to_string(),None),
        ("post",422,json!({"ok":false,"code":"journal_validation","appliedRules":[],"errors":["Invalid transaction date"]}).to_string(),Some("journal_validation")),
        ("post",403,"private authority detail".into(),Some("server_refused")),
        ("post",500,"private failure".into(),Some("outcome_unknown")),
        ("post",200,"{\"ok\":true}".into(),Some("outcome_unknown")),
        ("dry-run",200,"{\"ok\":true}".into(),Some("protocol")),
        ("post",422,"{\"ok\":false,\"code\":\"journal_selection_usage\",\"errors\":[42]}".into(),Some("server_refused")),
    ] {
        let (origin,handle)=server(status,response);
        let output=call(kind,&origin,&f,false,&["--input",path.to_str().unwrap()]);
        assert_eq!(output.status.code(),Some(1));
        let value:Value=serde_json::from_slice(&output.stdout).unwrap();
        if let Some(code)=code {assert_eq!(value["code"],code);}
        assert!(!String::from_utf8_lossy(&output.stdout).contains("private"));
        assert_eq!(handle.join().unwrap().len(),1);
    }
}
#[test]
fn local_usage_precedes_unreadable_profile_and_credentials() {
    let f = Fixture::new();
    for (name, content) in [("bad", "{"), ("array", "[]"), ("scalar", "42")] {
        fs::write(f.0.join(name), content).unwrap();
    }
    let bad = f.0.join("bad");
    let array = f.0.join("array");
    let scalar = f.0.join("scalar");
    for extra in [
        vec!["post", "--input", "absent"],
        vec!["post", "--input", bad.to_str().unwrap()],
        vec!["dry-run", "--input", array.to_str().unwrap()],
        vec!["post", "--input", scalar.to_str().unwrap()],
        vec!["post", "--confirm", "yes"],
        vec!["post", "--idempotency-key", "hidden"],
        vec!["reverse", "--date", "2026-05-17", "--reason", "x"],
        vec![
            "reverse",
            "--match-text",
            "x",
            "--match-document-id",
            "0",
            "--date",
            "2026-05-17",
            "--reason",
            "x",
        ],
    ] {
        let output = Command::new(BIN)
            .arg("journal")
            .args(extra)
            .args([
                "--local-profile",
                "/not-opened/profile",
                "--cookie-file",
                "/not-opened/cookie",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}
