#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use common::TcpListener;
use serde_json::Value;
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[cfg(unix)]
#[test]
fn all_receivables_cli_commands_send_exact_body_and_preserve_refusal_uncertainty() {
    use std::os::unix::fs::OpenOptionsExt;
    let directory = std::env::temp_dir().join(format!("rm-receivables-cli-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let cookie = directory.join("session.cookie");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic-secret")
        .unwrap();
    for (command, route, body, status, code) in [
        (
            "settle-invoice",
            "settle",
            r#"{ "invoiceDocumentId":1,"bankTransactionId":2,"confirm":true }"#,
            200,
            None,
        ),
        (
            "credit-invoice",
            "credit-note",
            r#"{"invoiceDocumentId":1,"issueDate":"2026-10-19","reason":"Synthetic","grossAmount":25.01,"confirm":true}"#,
            200,
            None,
        ),
        (
            "refund-invoice",
            "refund",
            r#"{"invoiceDocumentId":1,"bankTransactionReference":"synthetic","refundDate":"2026-10-19","confirm":true}"#,
            200,
            None,
        ),
        (
            "credit-invoice",
            "credit-note",
            r#"{"invoiceDocumentId":1,"issueDate":"2026-10-19","reason":"Synthetic","confirm":true}"#,
            422,
            Some("server_refused"),
        ),
        (
            "credit-invoice",
            "credit-note",
            r#"{"invoiceDocumentId":1,"issueDate":"2026-10-19","reason":"Synthetic","confirm":true}"#,
            503,
            Some("outcome_unknown"),
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut bytes = vec![];
            loop {
                let mut buf = [0; 4096];
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                if bytes.ends_with(body.as_bytes()) {
                    break;
                }
            }
            assert!(bytes.starts_with(
                format!("POST /api/companies/synthetic/invoices/{route} HTTP/1.1").as_bytes()
            ));
            let success = if status == 200 {
                r#"{"ok":true,"entryId":7}"#
            } else {
                r#"{"ok":false}"#
            };
            write!(stream,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{success}",success.len()).unwrap();
        });
        let mut child = Command::new(BIN)
            .args([
                "remote",
                command,
                "--server",
                &origin,
                "--allow-loopback-http",
                "--company",
                "synthetic",
                "--cookie-file",
                cookie.to_str().unwrap(),
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
        let result = child.wait_with_output().unwrap();
        assert_eq!(
            result.status.code(),
            Some(if code.is_some() { 1 } else { 0 })
        );
        let output = String::from_utf8(result.stdout).unwrap();
        assert!(!output.contains("synthetic-secret"));
        let value: Value = serde_json::from_str(&output).unwrap();
        if let Some(code) = code {
            assert_eq!(value["code"], code);
        } else {
            assert_eq!(value["data"]["entryId"], 7);
        }
        task.join().unwrap();
    }
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn invoice_lifecycle_cli_requires_confirmation_and_disallows_unrelated_flags() {
    for command in ["settle-invoice", "credit-invoice", "refund-invoice"] {
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
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        let unrelated = Command::new(BIN)
            .args([
                "remote",
                command,
                "--server",
                "http://127.0.0.1:9",
                "--allow-loopback-http",
                "--company",
                "synthetic",
                "--confirm",
                "--id",
                "7",
            ])
            .output()
            .unwrap();
        assert_eq!(unrelated.status.code(), Some(2));
    }
    let help = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let value: Value = serde_json::from_slice(&help.stdout).unwrap();
    assert_eq!(
        value["receivables"]["bodySchemas"]["credit"]["properties"]["confirm"]["const"],
        true
    );
}
