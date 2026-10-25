//! Synthetic original-command checks; integration owns root registration and compiled execution.
#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::Command,
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(args)
        .env_remove("RENTEMESTER_COMPANY")
        .output()
        .unwrap()
}
#[test]
fn source_usage_refusals_precede_credentials_and_files() {
    for (args, message) in [
        (
            vec!["opening-balance", "post"],
            "Missing required --input <file.json>",
        ),
        (
            vec!["compliance", "report"],
            "Missing required --out <file.html>",
        ),
        (
            vec!["gdpr", "erase", "--as-of=2099-01-01"],
            "Unknown argument",
        ),
        (vec!["gdpr", "erase", "--confirm"], "Unknown argument"),
        (
            vec!["retention", "status", "--format=xml"],
            "--format must be either json or human",
        ),
    ] {
        let out = run(&args);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains(message));
    }
}
#[cfg(unix)]
fn private_cookie() -> std::path::PathBuf {
    use std::os::unix::fs::OpenOptionsExt;
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let p = std::env::temp_dir().join(format!(
        "remaining716-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&p)
        .unwrap()
        .write_all(b"session=synthetic")
        .unwrap();
    p
}
#[cfg(unix)]
fn fixture(args: &[&str], route: &str, response: Value) -> (std::process::Output, Value) {
    let listener = common::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let route = route.to_string();
    let task = thread::spawn(move || {
        // common performs anonymous protocol handshake before accepting a business call.
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = Vec::new();
        let mut end = None;
        let mut length = 0;
        loop {
            let mut buf = [0; 4096];
            let n = stream.read(&mut buf).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
            if end.is_none() {
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    end = Some(i + 4);
                    for l in String::from_utf8_lossy(&bytes[..i]).lines() {
                        if let Some(s) = l.to_lowercase().strip_prefix("content-length:") {
                            length = s.trim().parse().unwrap();
                        }
                    }
                }
            }
            if end.is_some_and(|i| bytes.len() >= i + length) {
                break;
            }
        }
        let header = String::from_utf8_lossy(&bytes[..end.unwrap()]);
        assert!(header.starts_with(&route), "{header}");
        assert!(header.to_lowercase().contains("cookie: session=synthetic"));
        assert!(header.to_lowercase().contains("x-rentemester-manifest:"));
        let body = if length == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[end.unwrap()..]).unwrap()
        };
        let response = response.to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
        body
    });
    let cookie = private_cookie();
    let out = Command::new(BIN)
        .args(args)
        .args([
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    let body = task.join().unwrap();
    std::fs::remove_file(cookie).unwrap();
    (out, body)
}
#[cfg(unix)]
#[test]
fn export_uses_original717_and_preserves_literal_subject_and_domain_refusal() {
    let core = json!({"ok":false,"asOf":"invalid","subject":{"cvr":"--literal=a=b","name":null},"records":[],"appliedRules":["GDPR-SUBJECT-EXPORT"],"errors":["asOf must be a valid YYYY-MM-DD date"]});
    let (out, body) = fixture(
        &[
            "--company=synthetic",
            "gdpr",
            "--subject=--literal=a=b",
            "export",
            "--as-of=invalid",
        ],
        "POST /api/companies/synthetic/original-gdpr-export HTTP/1.1",
        json!({"ok":true,"result":core}),
    );
    assert_eq!(
        body,
        json!({"cvr":"--literal=a=b","asOf":"invalid","confirm":true})
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stderr.is_empty());
    assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), core);
}
#[cfg(unix)]
#[test]
fn retention_keeps_original_date_refusal_not_native_metadata() {
    let core = json!({"ok":false,"asOf":"bad","appliedRules":["DK-BOOKKEEPING-RETENTION-001"],"rows":[],"errors":["asOf must be YYYY-MM-DD"]});
    let (out, body) = fixture(
        &["retention", "status", "--company=synthetic", "--as-of=bad"],
        "GET /api/companies/synthetic/retention-status?asOf=bad HTTP/1.1",
        json!({"ok":true,"retentionStatus":core}),
    );
    assert!(body.is_null());
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), core);
}
#[cfg(unix)]
#[test]
fn erase_uses_current_server_date_and_preserves_retained_rows() {
    let core = json!({"ok":true,"asOf":"2026-01-01","subject":{"cvr":"SYNTHETIC","name":null},"appliedRules":["GDPR-SUBJECT-ERASURE","DK-BOOKKEEPING-RETENTION-001"],"erasedCount":0,"refusedCount":1,"alreadyErasedCount":0,"erased":[],"refused":[{"source":"journal_entries","sourceRowId":1,"reason":"under retention"}],"errors":[],"coverage":{"complete":false}});
    let (out, body) = fixture(
        &["gdpr", "erase", "--company=synthetic", "--cvr=SYNTHETIC"],
        "POST /api/companies/synthetic/gdpr/erase HTTP/1.1",
        json!({"ok":true,"gdprErasure":core}),
    );
    assert_eq!(body, json!({"confirm":true,"cvr":"SYNTHETIC"}));
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), core);
}
#[cfg(unix)]
#[test]
fn explicit_local_company_path_is_not_sent_to_remote() {
    let out = run(&[
        "retention",
        "status",
        "--company=/private/synthetic/company",
        "--server=http://127.0.0.1:1",
        "--allow-loopback-http",
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("verified --local-profile"));
}
#[test]
fn original716_compliance_date_usage_precedes_credentials() {
    let out = run(&[
        "compliance",
        "report",
        "--company=synthetic",
        "--out=unused-synthetic.html",
        "--as-of=bad",
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--as-of must be YYYY-MM-DD"));
}
#[cfg(unix)]
#[test]
fn original716_actor_assertion_sent_as_assertion_not_authority() {
    let (out, body) = fixture(
        &[
            "--actor=user:forged",
            "gdpr",
            "erase",
            "--company=synthetic",
            "--name=--literal=a",
        ],
        "POST /api/companies/synthetic/original716-erase HTTP/1.1",
        json!({"ok":true,"result":{"ok":false,"errors":["Current actor assertion did not match"]}}),
    );
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        body,
        json!({"expectedActor":"user:forged","confirm":true,"name":"--literal=a"})
    );
}
#[cfg(unix)]
#[test]
fn original716_export_private_file_and_existing_symlink_refusal() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = private_cookie();
    std::fs::remove_file(&dir).unwrap();
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let core = json!({"ok":true,"asOf":"2026-01-01","subject":{"cvr":"--a=b","name":null},"records":[{"sensitive":"synthetic"}],"errors":[]});
    let (out, _) = fixture(
        &[
            "gdpr",
            "export",
            "--company=synthetic",
            "--cvr=--a=b",
            "--out",
            dir.to_str().unwrap(),
        ],
        "POST /api/companies/synthetic/original-gdpr-export HTTP/1.1",
        json!({"ok":true,"result":core}),
    );
    assert_eq!(out.status.code(), Some(0));
    let path = dir.join("gdpr-export---a_b-2026-01-01.json");
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap(),
        core
    );
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::remove_file(&path).unwrap();
    let protected = dir.join("protected");
    std::fs::write(&protected, b"unchanged").unwrap();
    symlink(&protected, &path).unwrap();
    let (out, _) = fixture(
        &[
            "gdpr",
            "export",
            "--company=synthetic",
            "--cvr=--a=b",
            "--out",
            dir.to_str().unwrap(),
        ],
        "POST /api/companies/synthetic/original-gdpr-export HTTP/1.1",
        json!({"ok":true,"result":core}),
    );
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(std::fs::read(&protected).unwrap(), b"unchanged");
    std::fs::remove_dir_all(dir).unwrap();
}
#[cfg(unix)]
#[test]
fn original716_opening_positive_canonical_receipt_and_unknown_malformed() {
    for malformed in [false, true] {
        let listener = common::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = thread::spawn(move || {
            for operation in ["plan", "apply"] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut bytes = vec![];
                let (end, length) = loop {
                    let mut b = [0; 4096];
                    let n = stream.read(&mut b).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&b[..n]);
                    if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&bytes[..i]);
                        let length: usize = header
                            .lines()
                            .find_map(|l| {
                                l.to_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse().unwrap())
                            })
                            .unwrap();
                        if bytes.len() >= i + 4 + length {
                            break (i + 4, length);
                        }
                    }
                };
                let header = String::from_utf8_lossy(&bytes[..end]);
                assert!(header.starts_with(&format!(
                    "POST /api/companies/synthetic/opening-balance/{operation} HTTP/1.1"
                )));
                assert!(header.to_lowercase().contains("cookie: session=synthetic"));
                let body: Value = serde_json::from_slice(&bytes[end..end + length]).unwrap();
                let response=if operation=="plan" {assert_eq!(body["lines"][0]["debitAmount"],"10.25");json!({"ok":true,"plan":{"status":"ready","planHash":"a".repeat(64),"blockers":[]}})}else{assert_eq!(body["confirm"],true);assert_eq!(body["planHash"],"a".repeat(64));assert_eq!(body["input"]["lines"][1]["creditAmount"],"10.25");if malformed {json!({"ok":true,"openingBalance":{}})}else{json!({"ok":true,"openingBalance":{"journalEntryId":7,"journalEntryNo":"2026-00007","journalEntryHash":"b".repeat(64),"cutOverDate":"2026-01-01"}})}}.to_string();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
            }
        });
        let input = private_cookie();
        std::fs::write(&input,serde_json::to_vec(&json!({"cutOverDate":"2026-01-01","lines":[{"accountNo":"A","debitAmount":10.25},{"accountNo":"E","creditAmount":10.25}]})).unwrap()).unwrap();
        let cookie = private_cookie();
        let out = run(&[
            "opening-balance",
            "post",
            "--company=synthetic",
            "--input",
            input.to_str().unwrap(),
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--json",
        ]);
        task.join().unwrap();
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        if malformed {
            assert_eq!(out.status.code(), Some(1));
            assert!(v["errors"][0].as_str().unwrap().contains("outcome unknown"));
        } else {
            assert_eq!(out.status.code(), Some(0));
            assert_eq!(v["entryId"], 7);
            assert_eq!(v["entryHash"], "b".repeat(64));
            assert!(v.get("appliedRules").is_none());
        }
        std::fs::remove_file(input).unwrap();
        std::fs::remove_file(cookie).unwrap();
    }
}
