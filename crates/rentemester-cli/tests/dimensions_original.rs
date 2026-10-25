#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use common::TcpListener;
use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn fourteen_operations_discover_without_credentials() {
    for op in [
        "define",
        "member",
        "definition-lifecycle",
        "member-lifecycle",
        "definitions",
        "members",
        "plan",
        "apply",
        "replace",
        "supersede",
        "list",
        "budget-plan",
        "budget-apply",
        "budgets",
    ] {
        let out = Command::new(BIN)
            .args([
                "--company=synthetic",
                "dimensions",
                op,
                "--help",
                "--cookie-file=/not/opened",
            ])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(String::from_utf8_lossy(&out.stdout).contains("--allocations INLINE_JSON"));
    }
}
#[test]
fn invalid_usage_precedes_credentials() {
    for args in [
        vec!["dimensions", "unknown"],
        vec!["dimensions", "definitions", "--principal=forged"],
        vec!["dimensions", "members", "--journal-line-id=1"],
        vec![
            "dimensions",
            "plan",
            "--journal-line-id=1",
            "--allocations=fixture.json",
        ],
        vec![
            "dimensions",
            "define",
            "--dimension-id=project",
            "--kind=project",
            "--name=Projects",
            "--confirm=no",
        ],
        vec![
            "dimensions",
            "define",
            "--dimension-id=project",
            "--kind=project",
            "--name=Projects",
        ],
        vec!["dimensions", "budgets", "--dimension-id=project"],
        vec!["dimensions", "definitions", "--json=false"],
    ] {
        let out = Command::new(BIN)
            .args(args)
            .args([
                "--server=http://127.0.0.1:1",
                "--allow-loopback-http",
                "--company=synthetic",
                "--cookie-file=/not/opened",
            ])
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stdout.is_empty());
    }
}
#[test]
fn explicit_human_mode_summarizes_and_routes_failures_to_stderr() {
    use serde_json::json;
    use std::{
        io::{Read, Write},
        };
    let file = std::env::temp_dir().join(format!("commitment-human-cookie-{}", std::process::id()));
    std::fs::write(&file, "session=synthetic").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    for success in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                let mut b = [0; 4096];
                let n = stream.read(&mut b).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&b[..n]);
            }
            let core = if success {
                json!({"ok":true,"definitions":[]})
            } else {
                json!({"ok":false,"errors":["Source unavailable"]})
            }
            .to_string();
            write!(
                stream,
                "HTTP/1.1 {} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{core}",
                if success { 200 } else { 422 },
                core.len()
            )
            .unwrap();
        });
        let out = Command::new(BIN)
            .args([
                "dimensions",
                "definitions",
                "--server",
                &origin,
                "--company",
                "synthetic",
                "--allow-loopback-http",
                "--cookie-file",
                file.to_str().unwrap(),
                "--format",
                "human",
            ])
            .output()
            .unwrap();
        worker.join().unwrap();
        assert_eq!(out.status.code(), Some(if success { 0 } else { 1 }));
        let stdout = String::from_utf8(out.stdout).unwrap();
        let stderr = String::from_utf8(out.stderr).unwrap();
        if success {
            assert!(stdout.contains("definitions: 0 records"));
            assert!(!stdout.contains('{'));
            assert!(stderr.is_empty());
        } else {
            assert!(stdout.is_empty());
            assert!(stderr.contains("Source unavailable"));
            assert!(!stderr.contains('{'));
        }
    }
    std::fs::remove_file(file).unwrap();
}
