use common::TcpListener;
#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
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
        "workspace-inbox",
        "workspace-inbox-source",
        "ingest-workspace-inbox",
        "assign-workspace-inbox",
        "complete-workspace-inbox",
    ] {
        assert!(
            v["commands"]
                .as_array()
                .unwrap()
                .contains(&Value::String(n.into()))
        );
    }
    for args in [
        vec!["workspace-inbox-source"],
        vec!["workspace-inbox", "--confirm"],
        vec![
            "ingest-workspace-inbox",
            "--source-id",
            "forbidden",
            "--confirm",
        ],
        vec!["assign-workspace-inbox", "--year", "2026", "--confirm"],
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
    let file = std::env::temp_dir().join(format!("rm-inbox-cli-{}.cookie", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&file)
        .unwrap()
        .write_all(b"session=private-inbox-cookie")
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut b = [0; 4096];
        let n = s.read(&mut b).unwrap();
        assert!(
            b[..n].starts_with(
                b"GET /api/companies/synthetic/workspace-inbox/inbox-synthetic HTTP/1.1"
            )
        );
        let reply = r#"{"ok":true,"source":{"sourceId":"inbox-synthetic","roles":[{"companySlug":"synthetic","role":"supplier"}],"history":[{"event_type":"created"}],"unknownFutureEvidence":true}}"#;
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
            "workspace-inbox-source",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            file.to_str().unwrap(),
            "--company",
            "synthetic",
            "--source-id",
            "inbox-synthetic",
        ])
        .output()
        .unwrap();
    std::fs::remove_file(file).unwrap();
    assert!(r.status.success());
    assert!(!String::from_utf8_lossy(&r.stdout).contains("private-inbox-cookie"));
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(v["data"]["source"]["unknownFutureEvidence"], true);
    t.join().unwrap();
}

#[test]
#[cfg(unix)]
fn exact_ingress_body_and_original_key_cross_cli_unknown_without_retry() {
    use std::{os::unix::fs::OpenOptionsExt, process::Stdio};
    let file =
        std::env::temp_dir().join(format!("rm-inbox-cli-write-{}.cookie", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&file)
        .unwrap()
        .write_all(b"session=private-inbox-cookie")
        .unwrap();
    let body = r#"{ "confirm":true,"idempotencyKey":" original key ","bytesBase64":"U3ludGhldGlj","filename":"source.txt","mimeType":"text/plain","transport":"upload","receivedAt":"2026-01-01","metadata":{} }"#;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = vec![];
        loop {
            let mut b = [0; 4096];
            let n = stream.read(&mut b).unwrap();
            assert_ne!(n, 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let h = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let size = h
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .unwrap()
                    .parse::<usize>()
                    .unwrap();
                if bytes.len() >= end + 4 + size {
                    assert_eq!(&bytes[end + 4..], body.as_bytes());
                    break;
                }
            }
        }
        assert!(bytes.starts_with(b"POST /api/companies/synthetic/workspace-inbox HTTP/1.1"));
        stream
            .write_all(
                b"HTTP/1.1 503 Service Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n",
            )
            .unwrap();
        drop(stream);
        let listener = listener.into_inner();
        listener.set_nonblocking(true).unwrap();
        thread::sleep(std::time::Duration::from_millis(70));
        assert!(listener.accept().is_err());
    });
    let mut child = Command::new(BIN)
        .args([
            "remote",
            "ingest-workspace-inbox",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            file.to_str().unwrap(),
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
        .write_all(body.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    std::fs::remove_file(file).unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("private-inbox-cookie"));
    assert!(out.stderr.is_empty());
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["code"], "outcome_unknown");
    assert_eq!(value["idempotencyKey"], " original key ");
    t.join().unwrap();
}
