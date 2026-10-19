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
fn annual_cli_preserves_full_blocker_response_without_confirmation() {
    use std::os::unix::fs::OpenOptionsExt;
    let path = std::env::temp_dir().join(format!("rm-annual-cli-{}.cookie", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap()
        .write_all(b"session=private-annual-secret")
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let pdf = br#"{"ok":true,"annualReport":{"report":{"ok":false,"errors":["open year"]}}}"#;
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 4096];
        let n = stream.read(&mut bytes).unwrap();
        assert!(bytes[..n].starts_with(b"GET /api/companies/synthetic/annual-report?fiscalYearStart=2020-01-01&fiscalYearEnd=2020-12-31 HTTP/1.1"));
        assert!(
            String::from_utf8_lossy(&bytes[..n])
                .to_lowercase()
                .contains("cookie: session=private-annual-secret")
        );
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection:close\r\n\r\n",pdf.len()).unwrap();
        stream.write_all(pdf).unwrap();
    });
    let result = Command::new(BIN)
        .args([
            "remote",
            "annual-report",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            path.to_str().unwrap(),
            "--company",
            "synthetic",
            "--fiscal-year-start",
            "2020-01-01",
            "--fiscal-year-end",
            "2020-12-31",
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
    assert!(!text.contains("private-annual-secret"));
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["data"], serde_json::from_slice::<Value>(pdf).unwrap());
    task.join().unwrap();
}
#[test]
fn annual_cli_discovers_command_and_refuses_missing_invalid_or_mixed_selectors() {
    let help = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let help: Value = serde_json::from_slice(&help.stdout).unwrap();
    assert!(
        help["commands"]
            .as_array()
            .unwrap()
            .contains(&Value::String("annual-report".into()))
    );
    for extra in [
        vec![],
        vec![
            "--fiscal-year-start",
            "2025-02-29",
            "--fiscal-year-end",
            "2025-12-31",
        ],
        vec![
            "--fiscal-year-start",
            "2025-12-31",
            "--fiscal-year-end",
            "2025-01-01",
        ],
        vec![
            "--fiscal-year-start",
            "2025-01-01",
            "--fiscal-year-end",
            "2025-12-31",
            "--confirm",
        ],
        vec![
            "--fiscal-year-start",
            "2025-01-01",
            "--fiscal-year-end",
            "2025-12-31",
            "--year",
            "2025",
        ],
    ] {
        let output = Command::new(BIN)
            .args([
                "remote",
                "annual-report",
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
