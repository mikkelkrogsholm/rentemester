use serde_json::Value;
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn discoverable_six_commands_and_strict_global_company_selectors() {
    let r = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    for n in [
        "consolidation",
        "group-report-profiles",
        "group-consolidated-report",
        "propose-consolidation-profile",
        "approve-consolidation-profile",
        "revoke-consolidation-profile",
    ] {
        assert!(
            v["commands"]
                .as_array()
                .unwrap()
                .contains(&Value::String(n.into()))
        );
    }
    for args in [
        vec!["consolidation"],
        vec!["journal", "--company", "synthetic", "--profile-id", "forbidden"],
        vec![
            "group-report-profiles",
            "--company",
            "synthetic",
            "--as-of",
            "2026-01-01",
        ],
        vec!["group-report-profiles", "--confirm"],
        vec![
            "consolidation",
            "--company",
            "synthetic",
            "--from",
            "2026-01-01",
        ],
        vec![
            "approve-consolidation-profile",
            "--company",
            "synthetic",
            "--preview-reviewed",
            "--confirm",
        ],
    ] {
        let r = Command::new(BIN).arg("remote").args(args).output().unwrap();
        assert_eq!(r.status.code(), Some(2));
    }
}
#[test]
fn global_read_has_no_company_default_and_preserves_decimal_strings() {
    let path = std::env::temp_dir().join(format!(
        "rm-cli-consolidation-{}.cookie",
        std::process::id()
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&path)
        .unwrap()
        .write_all(b"session=private-consolidation-cookie\n")
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 4096];
        let n = stream.read(&mut bytes).unwrap();
        assert!(bytes[..n].starts_with(b"GET /api/group-report-profiles?asOf=2026-06-01 HTTP/1.1"));
        let reply = r#"{"ok":true,"scope":"consolidation-reconciliation","rows":[{"difference":"0.00","left":{"balance":"125.50","sourceRefs":[{"amount":"125.50"}]} }],"futureEvidence":{"retained":true}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        )
        .unwrap();
    });
    let r = Command::new(BIN)
        .args([
            "remote",
            "group-report-profiles",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            path.to_str().unwrap(),
            "--as-of",
            "2026-06-01",
        ])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    task.join().unwrap();
    assert!(r.status.success());
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(v["data"]["rows"][0]["left"]["balance"], "125.50");
    assert_eq!(v["data"]["futureEvidence"]["retained"], true);
    assert!(!String::from_utf8_lossy(&r.stdout).contains("private-consolidation-cookie"));
}
#[test]
fn confirmed_mapping_approval_sends_exact_body_once_and_reports_unknown() {
    let path = std::env::temp_dir().join(format!(
        "rm-cli-consolidation-write-{}.cookie",
        std::process::id()
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&path)
        .unwrap()
        .write_all(b"session=private-consolidation-cookie\n")
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let body = format!(
        r#"{{ "profileId":"synthetic-map", "profileHash":"{}", "confirm":true }}
"#,
        "a".repeat(64)
    );
    let expected = body.clone();
    let task =
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = vec![];
            loop {
                let mut chunk = [0; 4096];
                let n = stream.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n")
                    && bytes.len() >= end + 4 + expected.len()
                {
                    assert!(bytes.starts_with(
                        b"POST /api/companies/synthetic/consolidation/approve HTTP/1.1"
                    ));
                    assert_eq!(&bytes[end + 4..], expected.as_bytes());
                    break;
                }
            }
            write!(
                stream,
                "HTTP/1.1 503 Service Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n"
            )
            .unwrap();
            drop(stream);
            listener.set_nonblocking(true).unwrap();
            thread::sleep(std::time::Duration::from_millis(70));
            assert!(listener.accept().is_err());
        });
    let mut child = Command::new(BIN)
        .args([
            "remote",
            "approve-consolidation-profile",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            path.to_str().unwrap(),
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
    let r = child.wait_with_output().unwrap();
    std::fs::remove_file(path).unwrap();
    task.join().unwrap();
    assert_eq!(r.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&r.stdout).contains("outcome_unknown"));
    assert!(!String::from_utf8_lossy(&r.stdout).contains("private-consolidation-cookie"));
}
