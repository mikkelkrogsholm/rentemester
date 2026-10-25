#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use common::TcpListener;
use serde_json::Value;
use std::{
    io::{Read, Write},
    process::Command,
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[cfg(unix)]
#[test]
fn budget_cli_preserves_full_source_response() {
    use std::os::unix::fs::OpenOptionsExt;
    let path = std::env::temp_dir().join(format!("rm-budget-cli-{}.cookie", std::process::id()));
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
    let pdf = br#"{"ok":true,"budget":{"lines":[{"id":7,"accountNo":"E","period":"2026-01","amount":23.45}],"futureEvidence":true}}"#;
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 4096];
        let n = stream.read(&mut bytes).unwrap();
        assert!(
            bytes[..n].starts_with(b"GET /api/companies/synthetic/budget?year=2026%2F27 HTTP/1.1")
        );
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
            "budget",
            "--year",
            "2026/27",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            path.to_str().unwrap(),
            "--company",
            "synthetic",
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
fn budget_mileage_cli_discovery_and_explicit_selectors() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    for name in [
        "budget",
        "budget-vs-actual",
        "budget-dimension-actuals",
        "set-budget",
        "mileage",
        "register-mileage",
    ] {
        assert!(
            v["commands"]
                .as_array()
                .unwrap()
                .contains(&Value::String(name.into()))
        );
    }
    for args in [
        vec!["budget"],
        vec!["mileage"],
        vec!["set-budget", "--year", "2026", "--confirm"],
        vec![
            "budget-dimension-actuals",
            "--year",
            "2026",
            "--member",
            "x",
        ],
        vec!["register-mileage", "--year", "2026", "--confirm"],
        vec!["health", "--dimension", "x"],
    ] {
        let out = Command::new(BIN).arg("remote").args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}

#[cfg(unix)]
#[test]
fn budget_revision_cli_exact_body_without_inner_confirm_unknown_outcome() {
    use std::{os::unix::fs::OpenOptionsExt, process::Stdio};
    let path = std::env::temp_dir().join(format!(
        "rm-budget-generate-cli-{}.cookie",
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
    let body = "{ \"accountNo\":\"E\",\"period\":\"2026-01\",\"amount\":23.45 }\n";
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
        assert!(data.starts_with(b"POST /api/companies/synthetic/budget HTTP/1.1"));
        let end = data.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(&data[end..], body.as_bytes());
        stream
            .write_all(b"HTTP/1.1 503 Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n")
            .unwrap();
    });
    let mut child = Command::new(BIN)
        .args([
            "remote",
            "set-budget",
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
    task.join().unwrap();
}

#[cfg(unix)]
#[test]
fn mileage_cli_register_preserves_native_body_and_advisory() {
    use std::{os::unix::fs::OpenOptionsExt, process::Stdio};
    let path = std::env::temp_dir().join(format!(
        "rm-mileage-register-cli-{}.cookie",
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
    let body = r#"{ "tripDate":"2026-01-31","purpose":"Synthetic visit","fromLocation":"A","toLocation":"B","kilometers":0.15,"vehicle":"Synthetic car","driver":"Synthetic driver","ratePerKm":9,"rateBasis":"Synthetic chosen basis","confirm":true }"#;
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
        assert!(data.starts_with(b"POST /api/companies/synthetic/mileage HTTP/1.1"));
        let end = data.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(&data[end..], body.as_bytes());
        let reply = r#"{"ok":true,"mileage":{"mileageEntryId":7,"amountBasis":1.35,"warnings":["native advisory"]}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        )
        .unwrap();
    });
    let mut child = Command::new(BIN)
        .args([
            "remote",
            "register-mileage",
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
    assert_eq!(output.status.code(), Some(0));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["mileage"]["warnings"][0], "native advisory");
    assert_eq!(value["data"]["mileage"]["amountBasis"], 1.35);
    task.join().unwrap();
}
