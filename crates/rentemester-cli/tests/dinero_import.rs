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
fn dineros_discovery_and_mutation_controls_are_explicit() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    for name in [
        "dinero-import-plan",
        "dinero-import-apply",
        "dinero-imports",
        "dinero-import-original",
        "imported-receivables",
    ] {
        assert!(v["commands"].as_array().unwrap().contains(&json!(name)));
    }
    for args in [
        vec!["dinero-import-apply"],
        vec!["dinero-import-plan", "--confirm"],
        vec!["dinero-imports", "--confirm"],
        vec!["dinero-imports", "--attempt-id", "1"],
    ] {
        let out = Command::new(BIN).arg("remote").args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}
#[test]
fn actual_dinero_cli_dispatch_sends_exact_native_body_and_documented_read_selectors() {
    let cookie = std::env::temp_dir().join(format!("rm-dinero-cli-{}.cookie", std::process::id()));
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
        "dinero-import-plan",
        "dinero-import-apply",
        "dinero-imports",
        "dinero-import-original",
        "imported-receivables",
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
        let mut body = json!({"fileName":"source.json","contentBase64":"e30="});
        let suffix = match command {
            "dinero-import-plan" => "imports/dinero/plan",
            "dinero-import-apply" => {
                process.arg("--confirm");
                body["planHash"] = json!("a".repeat(64));
                body["idempotencyKey"] = json!("caller-exact-key");
                body["confirm"] = json!(true);
                "imports/dinero/apply"
            }
            "dinero-imports" => "imports/dinero",
            "imported-receivables" => {
                process.args(["--as-of", "2025-12-31"]);
                "invoices/imported-receivables?asOf=2025-12-31"
            }
            _ => {
                process.args(["--attempt-id", "42"]);
                "imports/dinero/42/original"
            }
        };
        let bytes = serde_json::to_vec(&body).unwrap();
        let mut child = process.spawn().unwrap();
        if command.starts_with("dinero-import-p") || command == "dinero-import-apply" {
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
                    if command == "dinero-import-plan" || command == "dinero-import-apply" {
                        "POST"
                    } else {
                        "GET"
                    }
                )
                .as_bytes()
            )
        );
        if command == "dinero-import-plan" || command == "dinero-import-apply" {
            assert!(wire.ends_with(&bytes));
        }
    }
    std::fs::remove_file(cookie).unwrap();
}
