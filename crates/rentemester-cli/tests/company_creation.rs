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
#[test]
fn new_company_discovery_and_workspace_only_confirmation() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        value["commands"]
            .as_array()
            .unwrap()
            .contains(&Value::String("create-company".into()))
    );
    for flags in [
        vec![],
        vec!["--confirm", "--company", "synthetic"],
        vec!["--confirm", "--year", "2026"],
        vec!["--confirm", "--id", "1"],
        vec!["--confirm", "--preview-reviewed"],
    ] {
        let output = Command::new(BIN)
            .args(["remote", "create-company"])
            .args(flags)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
    }
}
#[cfg(unix)]
#[test]
fn creation_cli_preserves_exact_request_and_unknown_result_without_echoing_secrets() {
    use std::os::unix::fs::OpenOptionsExt;
    let cookie =
        std::env::temp_dir().join(format!("rm-create-company-{}.cookie", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap()
        .write_all(b"session=private-company-secret")
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let body = "{ \"name\":\" Synthetic New Company \",\"vatPeriodType\":null }\n";
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut wire = vec![];
        loop {
            let mut buf = [0; 4096];
            let n = stream.read(&mut buf).unwrap();
            assert!(n > 0);
            wire.extend_from_slice(&buf[..n]);
            if wire.ends_with(body.as_bytes()) {
                break;
            }
        }
        assert!(wire.starts_with(b"POST /api/companies HTTP/1.1\r\n"));
        assert!(wire.ends_with(body.as_bytes()));
        stream
            .write_all(b"HTTP/1.1 503 Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n")
            .unwrap();
    });
    let mut child = Command::new(BIN)
        .args([
            "remote",
            "create-company",
            "--server",
            &origin,
            "--allow-loopback-http",
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
    let out = child.wait_with_output().unwrap();
    std::fs::remove_file(cookie).unwrap();
    task.join().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let output = String::from_utf8(out.stdout).unwrap();
    assert!(!output.contains("private-company-secret"));
    let value: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["code"], "outcome_unknown");
}
