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
fn bank_candidates_cli_uses_explicit_exact_selector_and_preserves_response() {
    use std::os::unix::fs::OpenOptionsExt;
    let path = std::env::temp_dir().join(format!(
        "rm-bank-candidates-cli-{}.cookie",
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
    let pdf = br#"{"ok":true,"reconciliationCandidates":{"plans":[],"correctionAvailable":false}}"#;
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 4096];
        let n = stream.read(&mut bytes).unwrap();
        assert!(bytes[..n].starts_with(b"GET /api/companies/synthetic/bank/reconciliation-candidates?bankTransactionId=7 HTTP/1.1"));
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
            "bank-reconciliation-candidates",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            path.to_str().unwrap(),
            "--company",
            "synthetic",
            "--bank-transaction-id",
            "7",
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
fn bank_link_cli_discovery_and_mixed_selectors_are_strict() {
    let output = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let help: Value = serde_json::from_slice(&output.stdout).unwrap();
    for command in ["bank-reconciliation-candidates", "bank-reconciliation-link"] {
        assert!(
            help["commands"]
                .as_array()
                .unwrap()
                .contains(&Value::String(command.into()))
        );
    }
    for (command, extra) in [
        ("bank-reconciliation-candidates", vec![]),
        (
            "bank-reconciliation-candidates",
            vec!["--bank-transaction-id", "0"],
        ),
        (
            "bank-reconciliation-candidates",
            vec!["--bank-transaction-id", "9007199254740992"],
        ),
        (
            "bank-reconciliation-candidates",
            vec!["--bank-transaction-id", "7", "--confirm"],
        ),
        (
            "bank-reconciliation-candidates",
            vec!["--bank-transaction-id", "7", "--year", "2026"],
        ),
        ("bank-reconciliation-link", vec![]),
        (
            "bank-reconciliation-link",
            vec!["--bank-transaction-id", "7", "--confirm"],
        ),
        ("bank-reconciliation-link", vec!["--id", "8", "--confirm"]),
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
