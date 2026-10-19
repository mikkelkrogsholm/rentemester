use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    thread,
    time::Duration,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn invoice_email_discovery_and_scope_confirmation_are_exact() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    for name in [
        "invoice-email-status",
        "invoice-email-send",
        "invoice-email-reconcile",
        "invoice-email-retry",
    ] {
        assert!(v["commands"].as_array().unwrap().contains(&json!(name)));
    }
    assert_eq!(
        v["invoiceEmail"]["bodySchemas"]["invoice-email-send"]["properties"]["confirm"]["const"],
        true
    );
    for args in [
        vec!["invoice-email-status", "--confirm"],
        vec![
            "invoice-email-status",
            "--company",
            "synthetic",
            "--id",
            "7",
        ],
        vec!["invoice-email-send", "--company", "synthetic"],
        vec![
            "invoice-email-retry",
            "--company",
            "synthetic",
            "--confirm",
            "--id",
            "7",
        ],
    ] {
        assert_eq!(
            Command::new(BIN)
                .arg("remote")
                .args(args)
                .output()
                .unwrap()
                .status
                .code(),
            Some(2)
        );
    }
}
#[cfg(unix)]
#[test]
fn invoice_email_cli_preserves_canonical_known_refusal_exit_and_never_retries() {
    use std::os::unix::fs::OpenOptionsExt;
    let cookie = std::env::temp_dir().join(format!(
        "rm-invoice-email-cli-{}.cookie",
        std::process::id()
    ));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic-private")
        .unwrap();
    for (command, path, body, status, response, exit) in [
        (
            "invoice-email-send",
            "invoices/send-email",
            json!({"invoiceDocumentId":7,"to":"buyer@example.invalid","confirm":true}),
            "409 Conflict",
            r#"{"ok":false,"code":"delivery_unknown","delivery":{"state":"unknown","history":[{"future":"retained"}]}}"#,
            1,
        ),
        (
            "invoice-email-reconcile",
            "invoices/email-deliveries/reconcile",
            json!({"messageId":format!("<{}@rentemester.local>","a".repeat(32)),"expectedStateHash":"b".repeat(64),"reason":"Reviewed SMTP evidence","evidence":"SMTP-log synthetic001","resolution":"not_accepted","confirm":true}),
            "200 OK",
            r#"{"ok":true,"delivery":{"state":"refused","history":[{"humanResolution":"not_accepted"}]}}"#,
            0,
        ),
        (
            "invoice-email-retry",
            "invoices/email-deliveries/retry",
            json!({"messageId":format!("<{}@rentemester.local>","a".repeat(32)),"expectedStateHash":"b".repeat(64),"reason":"Reviewed SMTP refusal","confirm":true}),
            "503 Unavailable",
            r#"{"secret":"must-not-leak"}"#,
            1,
        ),
    ] {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", l.local_addr().unwrap());
        let bytes = format!(" {body}\n");
        let expected = bytes.clone();
        let t =
            thread::spawn(move || {
                let (mut s, _) = l.accept().unwrap();
                s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut wire = vec![];
                loop {
                    let mut b = [0; 4096];
                    let n = s.read(&mut b).unwrap();
                    assert!(n > 0);
                    wire.extend_from_slice(&b[..n]);
                    if wire.ends_with(expected.as_bytes()) {
                        break;
                    }
                }
                assert!(wire.starts_with(
                    format!("POST /api/companies/synthetic/{path} HTTP/1.1").as_bytes()
                ));
                write!(
                    s,
                    "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{response}",
                    response.len()
                )
                .unwrap();
                drop(s);
                l.set_nonblocking(true).unwrap();
                thread::sleep(Duration::from_millis(60));
                assert!(l.accept().is_err());
            });
        let mut child = Command::new(BIN)
            .args([
                "remote",
                command,
                "--server",
                &origin,
                "--allow-loopback-http",
                "--cookie-file",
                cookie.to_str().unwrap(),
                "--company",
                "synthetic",
                "--confirm",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(bytes.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.code(), Some(exit));
        assert!(out.stderr.is_empty());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(!v.to_string().contains("must-not-leak"));
        assert!(!v.to_string().contains("synthetic-private"));
        if status.starts_with("503") {
            assert_eq!(v["code"], "outcome_unknown");
        } else {
            assert_eq!(v["data"], serde_json::from_str::<Value>(response).unwrap());
        }
        t.join().unwrap();
    }
    std::fs::remove_file(cookie).unwrap();
}
