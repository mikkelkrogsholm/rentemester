use serde_json::{Value, json};
use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn cfo_discovery_and_mutating_or_mixed_selector_refusals() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["commands"]
            .as_array()
            .unwrap()
            .contains(&json!("cfo-analytics"))
    );
    assert_eq!(
        v["cfoAnalytics"]["querySchema"]["additionalProperties"],
        false
    );
    for args in [
        vec!["--confirm"],
        vec!["--company", "left", "--company-slugs-json", "[\"right\"]"],
        vec!["--actor", "agent:test"],
        vec!["--body", "{}"],
    ] {
        let out = Command::new(BIN)
            .args(["remote", "cfo-analytics"])
            .args(args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}

#[test]
fn cfo_actual_cli_dispatch_admits_every_documented_filter_before_other_command_guards() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut bytes = vec![];
        while !bytes.ends_with(b"\r\n\r\n") {
            let mut buf = [0; 4096];
            let n = stream.read(&mut buf).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
        }
        let reply = r#"{"ok":true,"status":"ready","rows":[{"amount":"-0.01","sourceHash":"exact"}],"future":true}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{}",
            reply.len(),
            reply
        )
        .unwrap();
        bytes
    });
    let cookie = std::env::temp_dir().join(format!("rm-cfo-cli-{}.cookie", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic")
        .unwrap();
    let out = Command::new(BIN)
        .args([
            "remote",
            "cfo-analytics",
            "--server",
            &origin,
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--allow-loopback-http",
            "--scope",
            "portfolio",
            "--from",
            "2025-01-01",
            "--to",
            "2026-01-02",
            "--company-slugs-json",
            "[\"left\",\"right\"]",
            "--account",
            "1862",
            "--party",
            "party-id",
            "--dimension",
            "project:member",
            "--currency",
            "DKK",
            "--aggregate",
            "sum",
            "--cursor",
            "abc",
            "--limit",
            "1",
        ])
        .output()
        .unwrap();
    std::fs::remove_file(cookie).unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["data"]["rows"][0]["amount"], "-0.01");
    let bytes = task.join().unwrap();
    assert!(bytes.starts_with(b"GET /api/cfo-analytics?scope=portfolio&from=2025-01-01&to=2026-01-02&companySlug=left&companySlug=right&account=1862&party=party-id&dimension=project%3Amember&currency=DKK&aggregate=sum&cursor=abc&limit=1 HTTP/1.1"));
}
