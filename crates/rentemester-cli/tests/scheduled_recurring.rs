use common::TcpListener;
#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn scheduled_cli_discovery_and_command_specific_selectors() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    for name in [
        "scheduled-recurring",
        "scheduled-recurring-plan",
        "enable-scheduled-recurring",
        "pause-scheduled-recurring",
        "resume-scheduled-recurring",
        "cancel-scheduled-recurring",
        "catch-up-scheduled-recurring",
        "skip-scheduled-recurring",
        "dispatch-scheduled-recurring",
        "reconsent-scheduled-recurring",
    ] {
        assert!(v["commands"].as_array().unwrap().contains(&json!(name)));
    }
    assert_eq!(
        v["scheduledRecurring"]["bodySchemas"]["dispatch"]["properties"]["confirm"]["const"],
        true
    );
    for args in [
        vec!["scheduled-recurring", "--confirm"],
        vec!["scheduled-recurring", "--id", "7"],
        vec!["scheduled-recurring-plan"],
        vec!["scheduled-recurring-plan", "--id", "0"],
        vec!["dispatch-scheduled-recurring"],
        vec!["reconsent-scheduled-recurring"],
        vec!["reconsent-scheduled-recurring", "--confirm", "--id", "7"],
        vec!["dispatch-scheduled-recurring", "--confirm", "--id", "7"],
        vec!["scheduled-recurring", "--as-of", "2026-01-01"],
    ] {
        let out = Command::new(BIN).arg("remote").args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}
#[cfg(unix)]
#[test]
fn scheduled_cli_exact_dispatch_once_and_unknown_outcome() {
    use std::os::unix::fs::OpenOptionsExt;
    let cookie =
        std::env::temp_dir().join(format!("rm-scheduled-cli-{}.cookie", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic-private")
        .unwrap();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let body = format!(
        " {{\"templateId\":7,\"planHash\":\"{}\",\"periodIndex\":0,\"confirm\":true}}\n",
        "a".repeat(64)
    );
    let expected = body.clone();
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut wire = vec![];
        loop {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            wire.extend_from_slice(&b[..n]);
            if wire.ends_with(expected.as_bytes()) {
                break;
            }
        }
        assert!(
            wire.starts_with(
                b"POST /api/companies/synthetic/scheduled-recurring/dispatch HTTP/1.1"
            )
        );
        s.write_all(b"HTTP/1.1 503 Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n")
            .unwrap();
        drop(s);
        let l = l.into_inner();
        l.set_nonblocking(true).unwrap();
        thread::sleep(std::time::Duration::from_millis(70));
        assert!(l.accept().is_err());
    });
    let mut child = Command::new(BIN)
        .args([
            "remote",
            "dispatch-scheduled-recurring",
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
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("outcome_unknown"));
    assert!(!text.contains("synthetic-private"));
    t.join().unwrap();
}

#[cfg(unix)]
#[test]
fn scheduled_cli_exact_reconsent_once_and_unknown_outcome() {
    use std::os::unix::fs::OpenOptionsExt;
    let cookie =
        std::env::temp_dir().join(format!("rm-reconsent-cli-{}.cookie", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic-private")
        .unwrap();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let body = format!(
        " {{\"templateId\":7,\"planHash\":\"{}\",\"reason\":\" explicit restored consent \",\"confirm\":true}}\n",
        "a".repeat(64)
    );
    let expected = body.clone();
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut wire = vec![];
        loop {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            wire.extend_from_slice(&b[..n]);
            if wire.ends_with(expected.as_bytes()) {
                break;
            }
        }
        assert!(
            wire.starts_with(
                b"POST /api/companies/synthetic/scheduled-recurring/reconsent HTTP/1.1"
            )
        );
        s.write_all(b"HTTP/1.1 503 Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n")
            .unwrap();
        drop(s);
        let l = l.into_inner();
        l.set_nonblocking(true).unwrap();
        thread::sleep(std::time::Duration::from_millis(70));
        assert!(l.accept().is_err());
    });
    let mut child = Command::new(BIN)
        .args([
            "remote",
            "reconsent-scheduled-recurring",
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
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("outcome_unknown"));
    assert!(!text.contains("synthetic-private"));
    t.join().unwrap();
}
