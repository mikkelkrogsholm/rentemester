#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use common::TcpListener;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn opening_balance_discovery_and_mutation_controls_are_explicit() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    for name in [
        "opening-balance-plan",
        "opening-balance-apply",
        "opening-balance",
    ] {
        assert!(v["commands"].as_array().unwrap().contains(&json!(name)));
    }
    for args in [
        vec!["opening-balance-apply"],
        vec!["opening-balance-plan", "--confirm"],
        vec!["opening-balance", "--confirm"],
        vec!["opening-balance", "--import-id", "1"],
        vec!["opening-balance-plan", "--year", "2024"],
    ] {
        let out = Command::new(BIN).arg("remote").args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}
#[test]
fn actual_opening_cli_dispatch_sends_exact_native_body_and_documented_read_selectors() {
    let cookie = std::env::temp_dir().join(format!("rm-opening-cli-{}.cookie", std::process::id()));
    let mut opts = std::fs::OpenOptions::new();
    opts.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic")
        .unwrap();
    for command in [
        "opening-balance-plan",
        "opening-balance-apply",
        "opening-balance",
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut bytes = vec![];
            loop {
                let mut b = [0; 4096];
                let n = stream.read(&mut b).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&b[..n]);
                if let Some(end) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    let len = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .map(|n| n.parse::<usize>().unwrap())
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            let reply = r#"{"ok":true,"future":true}"#;
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type:application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{}",reply.len(),reply).unwrap();
            bytes
        });
        let mut process = Command::new(BIN);
        process
            .args([
                "remote",
                command,
                "--server",
                &origin,
                "--cookie-file",
                cookie.to_str().unwrap(),
                "--allow-loopback-http",
                "--company",
                "synthetic",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let input = json!({"cutOverDate":"2024-07-01","lines":[{"accountNo":"A","debitAmount":"100.00"},{"accountNo":"E","creditAmount":"100.00"}],"note":"Exact original"});
        let mut body = input.clone();
        let suffix = match command {
            "opening-balance-plan" => "opening-balance/plan",
            "opening-balance-apply" => {
                process.arg("--confirm");
                body = json!({"input":input,"planHash":"a".repeat(64),"idempotencyKey":"exact-caller-key","confirm":true});
                "opening-balance/apply"
            }
            _ => "opening-balance",
        };
        let bytes = serde_json::to_vec(&body).unwrap();
        let mut child = process.spawn().unwrap();
        if command == "opening-balance-plan" || command == "opening-balance-apply" {
            child.stdin.take().unwrap().write_all(&bytes).unwrap();
        } else {
            drop(child.stdin.take());
        }
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        let wire = task.join().unwrap();
        assert!(
            wire.starts_with(
                format!(
                    "{} /api/companies/synthetic/{suffix} HTTP/1.1",
                    if command == "opening-balance-plan" || command == "opening-balance-apply" {
                        "POST"
                    } else {
                        "GET"
                    }
                )
                .as_bytes()
            )
        );
        if command == "opening-balance-plan" || command == "opening-balance-apply" {
            assert!(wire.ends_with(&bytes));
        }
    }
    std::fs::remove_file(cookie).unwrap();
}
