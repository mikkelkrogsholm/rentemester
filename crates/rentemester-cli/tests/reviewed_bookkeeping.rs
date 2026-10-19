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
fn submit_cli_preserves_exact_native_body_without_injecting_confirmation() {
    use std::os::unix::fs::OpenOptionsExt;
    let path = std::env::temp_dir().join(format!("rm-reviewed-cli-{}.cookie", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap()
        .write_all(b"session=private-reviewed-secret")
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let pdf = br#"{"ok":true,"accountingDraft":{"id":"bank-one","status":"submitted"}}"#;
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 4096];
        let n = stream.read(&mut bytes).unwrap();
        assert!(bytes[..n].starts_with(
            b"POST /api/companies/synthetic/accounting-drafts/bank-one/submit HTTP/1.1"
        ));
        assert!(
            String::from_utf8_lossy(&bytes[..n])
                .ends_with("{ \"expectedEventHash\": \"server-hash\" }\n")
        );
        assert!(
            String::from_utf8_lossy(&bytes[..n])
                .to_lowercase()
                .contains("cookie: session=private-reviewed-secret")
        );
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection:close\r\n\r\n",pdf.len()).unwrap();
        stream.write_all(pdf).unwrap();
    });
    let mut child = Command::new(BIN)
        .args([
            "remote",
            "submit-draft",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            path.to_str().unwrap(),
            "--company",
            "synthetic",
            "--draft-id",
            "bank-one",
            "--confirm",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{ \"expectedEventHash\": \"server-hash\" }\n")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(!text.contains("private-reviewed-secret"));
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["data"], serde_json::from_slice::<Value>(pdf).unwrap());
    task.join().unwrap();
}
#[test]
fn reviewed_cli_discovery_and_write_flags_are_explicit() {
    let help = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let help: Value = serde_json::from_slice(&help.stdout).unwrap();
    for command in [
        "reviewed-bank",
        "accounting-drafts",
        "posting-rules",
        "create-bank-draft",
        "submit-draft",
        "reject-draft",
        "approve-draft",
    ] {
        assert!(
            help["commands"]
                .as_array()
                .unwrap()
                .contains(&Value::String(command.into()))
        );
    }
    for (command, extra) in [
        ("submit-draft", vec![]),
        ("submit-draft", vec!["--confirm"]),
        ("approve-draft", vec!["--confirm", "--draft-id", "../bad"]),
        ("reviewed-bank", vec!["--confirm"]),
        ("accounting-drafts", vec!["--year", "2026"]),
        ("posting-rules", vec!["--draft-id", "bank-one"]),
        ("create-bank-draft", vec!["--confirm", "--id", "1"]),
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
