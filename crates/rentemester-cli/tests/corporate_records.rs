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
#[test]
fn discoverability_and_explicit_scopes() {
    let r = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    for n in [
        "corporate-records",
        "corporate-record",
        "corporate-record-original",
        "ingest-corporate-record",
        "link-corporate-record",
        "enrich-corporate-record",
        "supersede-corporate-record",
    ] {
        assert!(
            v["commands"]
                .as_array()
                .unwrap()
                .contains(&Value::String(n.into()))
        );
    }
    for args in [
        vec!["corporate-record-original"],
        vec!["corporate-records", "--confirm"],
        vec![
            "ingest-corporate-record",
            "--record-id",
            "forbidden",
            "--confirm",
        ],
        vec!["enrich-corporate-record", "--year", "2026", "--confirm"],
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
#[test]
#[cfg(unix)]
fn canonical_inspection_crosses_cli_process_without_secret_leak() {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::env::temp_dir().join(format!("rm-corporate-cli-{}.cookie", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&file)
        .unwrap()
        .write_all(b"session=private-corporate-cookie")
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut b = [0; 4096];
        let n = s.read(&mut b).unwrap();
        assert!(b[..n].starts_with(
            b"GET /api/companies/synthetic/corporate-records/record-synthetic/download HTTP/1.1"
        ));
        let reply = r#"{"ok":true,"bytesBase64":"AP8=","sha256":"native-source-hash","futureSourceEvidence":true}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        )
        .unwrap();
    });
    let r = Command::new(BIN)
        .args([
            "remote",
            "corporate-record-original",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            file.to_str().unwrap(),
            "--company",
            "synthetic",
            "--record-id",
            "record-synthetic",
        ])
        .output()
        .unwrap();
    std::fs::remove_file(file).unwrap();
    assert!(r.status.success());
    assert!(!String::from_utf8_lossy(&r.stdout).contains("private-corporate-cookie"));
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(v["data"]["bytesBase64"], "AP8=");
    assert_eq!(v["data"]["futureSourceEvidence"], true);
    assert!(v["data"].get("original").is_none());
    t.join().unwrap();
}
