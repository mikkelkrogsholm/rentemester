use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn seven_original_commands_discover_without_opening_credentials() {
    for op in [
        "plan", "apply", "list", "change", "match", "matches", "alerts",
    ] {
        let output = Command::new(BIN)
            .args([
                "--company=synthetic",
                "supplier-commitment",
                op,
                "--help",
                "--cookie-file=/not/opened",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("INLINE_JSON"));
        assert!(text.contains("NOT offline pure TypeScript"));
    }
}
#[test]
fn usage_and_confirmation_fail_before_connection_or_credentials() {
    for args in [
        vec!["supplier-commitment", "unknown"],
        vec!["supplier-commitment", "list", "--actor=forged"],
        vec!["supplier-commitment", "alerts"],
        vec!["supplier-commitment", "plan", "--input=fixture.json"],
        vec![
            "supplier-commitment",
            "change",
            "--id=synthetic",
            "--action=paused",
            "--reason=reviewed",
            "--confirm=no",
        ],
        vec![
            "supplier-commitment",
            "change",
            "--id=synthetic",
            "--action=paused",
            "--reason=reviewed",
        ],
        vec![
            "supplier-commitment",
            "match",
            "--input={\"commitmentId\":\"synthetic\",\"occurrenceDate\":\"2026-01-01\",\"evidence\":{\"kind\":\"payable\",\"id\":\"1\",\"amount\":1}}",
            "--confirm=yes",
        ],
        vec!["supplier-commitment", "list", "--id=forbidden"],
    ] {
        let output = Command::new(BIN)
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
            output.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(!String::from_utf8_lossy(&output.stderr).contains("credential"));
    }
}

#[test]
fn explicit_human_mode_summarizes_and_routes_failures_to_stderr() {
    use serde_json::json;
    use std::{
        io::{Read, Write},
        net::TcpListener,
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
                json!({"ok":true,"rows":[],"errors":[]})
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
                "supplier-commitment",
                "list",
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
            assert!(stdout.contains("Records: 0"));
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
