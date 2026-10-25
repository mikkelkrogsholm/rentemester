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
        "ownership",
        "ownership-history",
        "propose-ownership",
        "review-ownership",
        "apply-ownership",
    ] {
        assert!(
            v["commands"]
                .as_array()
                .unwrap()
                .contains(&Value::String(n.into()))
        );
    }
    for args in [
        vec!["ownership"],
        vec!["ownership-history", "--confirm"],
        vec![
            "propose-ownership",
            "--snapshot-id",
            "forbidden",
            "--confirm",
        ],
        vec!["review-ownership", "--as-of", "2026-06-01", "--confirm"],
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
    let file = std::env::temp_dir().join(format!("rm-ownership-cli-{}.cookie", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&file)
        .unwrap()
        .write_all(b"session=private-ownership-cookie")
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut b = [0; 4096];
        let n = s.read(&mut b).unwrap();
        assert!(
            b[..n].starts_with(b"GET /api/companies/synthetic/ownership?asOf=2026-06-01 HTTP/1.1")
        );
        let reply = r#"{"ok":true,"asOf":"2026-06-01","facts":[],"partial":true,"consolidation":{"eligible":false,"reason":"hidden endpoints"},"futureSourceEvidence":true}"#;
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
            "ownership",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            file.to_str().unwrap(),
            "--company",
            "synthetic",
            "--as-of",
            "2026-06-01",
        ])
        .output()
        .unwrap();
    std::fs::remove_file(file).unwrap();
    assert!(r.status.success());
    assert!(!String::from_utf8_lossy(&r.stdout).contains("private-ownership-cookie"));
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(v["data"]["futureSourceEvidence"], true);
    assert_eq!(v["data"]["partial"], true);
    assert_eq!(v["data"]["consolidation"]["eligible"], false);
    t.join().unwrap();
}
