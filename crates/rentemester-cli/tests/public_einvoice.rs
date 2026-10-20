use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn public_invoice_cli_discovery_and_confirmation_boundaries() {
    let output = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let help: Value = serde_json::from_slice(&output.stdout).unwrap();
    for name in [
        "public-invoice-config",
        "public-invoice-submissions",
        "register-public-invoice",
        "send-public-invoice",
        "refresh-public-invoice-status",
    ] {
        assert!(help["commands"].as_array().unwrap().contains(&json!(name)));
    }
    assert_eq!(
        help["publicEInvoice"]["bodySchemas"]["register"]["required"],
        json!(["confirm"])
    );
    assert_eq!(
        help["publicEInvoice"]["bodySchemas"]["send"]["properties"]["invoiceDocumentId"]["maximum"],
        json!(9007199254740991u64)
    );
    for args in [
        vec!["public-invoice-config", "--confirm"],
        vec!["send-public-invoice"],
        vec![
            "send-public-invoice",
            "--company-key",
            "forbidden",
            "--confirm",
        ],
        vec!["register-public-invoice", "--id", "7", "--confirm"],
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
fn public_invoice_cli_retains_known_uncertainty_without_resend_or_false_delivery() {
    use std::os::unix::fs::OpenOptionsExt;
    let cookie = std::env::temp_dir().join(format!(
        "rm-public-invoice-cli-{}.cookie",
        std::process::id()
    ));
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap()
        .write_all(b"session=private-public-invoice-cookie")
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let body = r#"{ "invoiceDocumentId":7, "confirm":true }"#;
    let task = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut wire = vec![];
        loop {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            wire.extend_from_slice(&b[..n]);
            if wire.ends_with(body.as_bytes()) {
                break;
            }
        }
        assert!(wire.starts_with(b"POST /api/companies/synthetic/invoices/send-public HTTP/1.1"));
        let reply=json!({"ok":true,"submission":{"state":"uncertain","transmissionId":null,"redeliveryBlocked":true}}).to_string();
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        )
        .unwrap();
        drop(s);
        listener.set_nonblocking(true).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(60));
        assert!(listener.accept().is_err());
    });
    let mut p = Command::new(BIN)
        .args([
            "remote",
            "send-public-invoice",
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
    p.stdin.take().unwrap().write_all(body.as_bytes()).unwrap();
    let output = p.wait_with_output().unwrap();
    std::fs::remove_file(cookie).unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["submission"]["state"], "uncertain");
    assert_eq!(value["data"]["submission"]["redeliveryBlocked"], true);
    assert!(value["data"]["submission"].get("delivered").is_none());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private-public-invoice-cookie"));
    task.join().unwrap();
}
