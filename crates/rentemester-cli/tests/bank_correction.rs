use serde_json::Value;
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[cfg(unix)]
#[test]
fn correction_plan_cli_preserves_nested_source_plan() {
    use std::os::unix::fs::OpenOptionsExt;
    let path = std::env::temp_dir().join(format!(
        "rm-bank-correction-plan-cli-{}.cookie",
        std::process::id()
    ));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap()
        .write_all(b"session=private-bank-secret")
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let pdf = br#"{"ok":true,"plan":{"ok":true,"plan":{"history":[],"effect":"correction_evidence_only_no_posting"},"errors":[]}}"#;
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 4096];
        let n = stream.read(&mut bytes).unwrap();
        assert!(bytes[..n].starts_with(b"GET /api/companies/synthetic/bank/reconciliation-correction-plan?bankTransactionId=7&replacementJournalEntryId=8 HTTP/1.1"));
        assert!(
            String::from_utf8_lossy(&bytes[..n])
                .to_lowercase()
                .contains("cookie: session=private-bank-secret")
        );
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection:close\r\n\r\n",pdf.len()).unwrap();
        stream.write_all(pdf).unwrap();
    });
    let result = Command::new(BIN)
        .args([
            "remote",
            "bank-reconciliation-correction-plan",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            path.to_str().unwrap(),
            "--company",
            "synthetic",
            "--bank-transaction-id",
            "7",
            "--replacement-journal-entry-id",
            "8",
        ])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(!text.contains("private-bank-secret"));
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["data"], serde_json::from_slice::<Value>(pdf).unwrap());
    task.join().unwrap();
}
#[test]
fn correction_cli_requires_exact_selectors_and_confirmed_keyed_body() {
    let output = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let help: Value = serde_json::from_slice(&output.stdout).unwrap();
    for command in [
        "bank-reconciliation-correction-plan",
        "bank-reconciliation-correction",
    ] {
        assert!(
            help["commands"]
                .as_array()
                .unwrap()
                .contains(&Value::String(command.into()))
        );
    }
    for (command, extra) in [
        ("bank-reconciliation-correction-plan", vec![]),
        (
            "bank-reconciliation-correction-plan",
            vec!["--bank-transaction-id", "7"],
        ),
        (
            "bank-reconciliation-correction-plan",
            vec![
                "--bank-transaction-id",
                "7",
                "--replacement-journal-entry-id",
                "0",
            ],
        ),
        (
            "bank-reconciliation-correction-plan",
            vec![
                "--bank-transaction-id",
                "7",
                "--replacement-journal-entry-id",
                "8",
                "--confirm",
            ],
        ),
        ("bank-reconciliation-correction", vec![]),
        (
            "bank-reconciliation-correction",
            vec!["--confirm", "--bank-transaction-id", "7"],
        ),
        (
            "bank-reconciliation-candidates",
            vec!["--replacement-journal-entry-id", "8"],
        ),
    ] {
        let output = Command::new(BIN)
            .args([
                "remote",
                command,
                "--server",
                "http://127.0.0.1:9",
                "--allow-loopback-http",
                "--company",
                "synthetic",
            ])
            .args(extra)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
    }
}
#[cfg(unix)]
#[test]
fn correction_cli_unknown_outcome_keeps_original_unicode_key_and_body() {
    use std::{os::unix::fs::OpenOptionsExt, process::Stdio};
    let path = std::env::temp_dir().join(format!(
        "rm-bank-correction-cli-{}.cookie",
        std::process::id()
    ));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap()
        .write_all(b"session=synthetic")
        .unwrap();
    let body = r#"{ "bankTransactionId":7,"replacementJournalEntryId":8,"expectedReconciliationId":"append-only:1","planHash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","reason":"  Synthetic correction  ","idempotencyKey":"  nøgle-🚀  ","confirm":true }
"#;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut data = vec![];
        loop {
            let mut chunk = [0; 4096];
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0);
            data.extend_from_slice(&chunk[..n]);
            if data.ends_with(body.as_bytes()) {
                break;
            }
        }
        assert!(
            data.starts_with(
                b"POST /api/companies/synthetic/bank/reconciliation-correction HTTP/1.1"
            )
        );
        let end = data.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(&data[end..], body.as_bytes());
        stream
            .write_all(b"HTTP/1.1 503 Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n")
            .unwrap();
    });
    let mut child = Command::new(BIN)
        .args([
            "remote",
            "bank-reconciliation-correction",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            path.to_str().unwrap(),
            "--company",
            "synthetic",
            "--confirm",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(body.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(output.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["code"], "outcome_unknown");
    assert_eq!(value["idempotencyKey"], "  nøgle-🚀  ");
    task.join().unwrap();
}
