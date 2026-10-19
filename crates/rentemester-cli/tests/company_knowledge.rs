use serde_json::Value;
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn discoverability_and_explicit_scope() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    for name in [
        "company-knowledge",
        "propose-company-knowledge",
        "review-company-knowledge",
        "supersede-company-knowledge",
    ] {
        assert!(
            v["commands"]
                .as_array()
                .unwrap()
                .contains(&Value::String(name.into()))
        );
    }
    for args in [
        vec!["company-knowledge", "--confirm"],
        vec!["company-knowledge", "--include-proposed", "yes"],
        vec![
            "propose-company-knowledge",
            "--as-of",
            "2026-01-01",
            "--confirm",
        ],
        vec!["review-company-knowledge"],
    ] {
        let out = Command::new(BIN).arg("remote").args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}
#[test]
#[cfg(unix)]
fn read_exact_selector_and_full_history() {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::env::temp_dir().join(format!("rm-knowledge-cli-{}.cookie", std::process::id()));
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&file)
        .unwrap()
        .write_all(b"session=knowledge-private")
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut b = [0; 4096];
        let n = s.read(&mut b).unwrap();
        assert!(b[..n].starts_with(
            b"GET /api/companies/synthetic/knowledge?includeProposed=true&asOf=2027-01-01 HTTP/1.1"
        ));
        let reply = r#"{"ok":true,"context":{"conflicts":["operating_status"],"safeForProductBehavior":false},"history":[{"futureNativeEvidence":true}]}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        )
        .unwrap();
    });
    let out = Command::new(BIN)
        .args([
            "remote",
            "company-knowledge",
            "--company",
            "synthetic",
            "--as-of",
            "2027-01-01",
            "--include-proposed",
            "true",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    std::fs::remove_file(file).unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains("knowledge-private"));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["context"]["safeForProductBehavior"], false);
    assert_eq!(v["data"]["history"][0]["futureNativeEvidence"], true);
    task.join().unwrap();
}
