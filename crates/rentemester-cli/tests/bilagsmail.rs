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
fn mailbox_commands_are_discoverable_and_reject_secret_argv() {
    let output = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let help: Value = serde_json::from_slice(&output.stdout).unwrap();
    for name in [
        "bilagsmail",
        "configure-bilagsmail",
        "delete-bilagsmail-config",
        "set-bilagsmail-alias",
        "poll-bilagsmail",
        "review-workspace-inbox",
        "workspace-inbox-original",
    ] {
        assert!(
            help["commands"]
                .as_array()
                .unwrap()
                .contains(&Value::String(name.into()))
        );
    }
    for args in [
        vec![
            "configure-bilagsmail",
            "--password",
            "forbidden",
            "--confirm",
        ],
        vec!["configure-bilagsmail"],
        vec!["bilagsmail", "--confirm"],
        vec!["workspace-inbox-original", "--confirm"],
        vec!["review-workspace-inbox"],
    ] {
        let output = Command::new(BIN).arg("remote").args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("forbidden"));
    }
}
#[cfg(unix)]
#[test]
fn mailbox_config_cli_forwards_stdin_without_returning_password_and_original_saves_private() {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("rm-mail-cli-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let cookie = root.join("session.cookie");
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap()
        .write_all(b"session=private-mail-cookie")
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let body = r#"{ "host":"imap.example.invalid","port":993,"username":"synthetic","password":"private-test-password" }"#;
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut b = [0; 4096];
            let n = stream.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if bytes.ends_with(body.as_bytes()) {
                break;
            }
        }
        assert!(
            bytes.starts_with(b"POST /api/companies/synthetic/bilagsmail/imap-config HTTP/1.1")
        );
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length:11\r\nConnection:close\r\n\r\n{\"ok\":true}",
            )
            .unwrap();
    });
    let mut process = Command::new(BIN)
        .args([
            "remote",
            "configure-bilagsmail",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--company",
            "synthetic",
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--confirm",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    process
        .stdin
        .take()
        .unwrap()
        .write_all(body.as_bytes())
        .unwrap();
    let result = process.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!String::from_utf8_lossy(&result.stdout).contains("private-test-password"));
    task.join().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut b = [0; 4096];
        let n = stream.read(&mut b).unwrap();
        assert!(
            b[..n].starts_with(
                b"GET /api/companies/synthetic/workspace-inbox/source/original HTTP/1.1"
            )
        );
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type:message/rfc822\r\nx-content-sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\r\nContent-Length:3\r\nConnection:close\r\n\r\nabc").unwrap();
    });
    let output = root.join("original.eml");
    let result = Command::new(BIN)
        .args([
            "remote",
            "workspace-inbox-original",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--company",
            "synthetic",
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--source-id",
            "source",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(std::fs::read(&output).unwrap(), b"abc");
    assert_eq!(
        std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let reply: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(reply["data"]["original"]["size"], 3);
    task.join().unwrap();
    std::fs::remove_file(output).unwrap();
    std::fs::remove_file(cookie).unwrap();
    std::fs::remove_dir(root).unwrap();
}
