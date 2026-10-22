use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
    time::{Duration, Instant},
};
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rentemester-rust"))
        .args(args)
        .env_remove("RENTEMESTER_COMPANY")
        .env_remove("RENTEMESTER_WORKSPACE")
        .output()
        .unwrap()
}
#[test]
fn document_party_original_pure_help_global_flags_and_old_dispatcher() {
    for command in [
        "party-link-plan",
        "party-link-apply",
        "party-link-inspect",
        "party-link-list",
        "party-link-supersede",
        "internal-no-external-party",
        "internal-no-external-party-supersede",
    ] {
        let o = cli(&[
            "--company",
            "synthetic",
            "documents",
            "--cookie-file",
            "/does/not/exist",
            command,
            "--help",
        ]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let v: Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(v["ok"], true);
        assert!(o.stderr.is_empty());
    }
    let o = cli(&["documents", "list", "--help"]);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("documents ingest"));
    let o = cli(&["documents", "--help"]);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("party-link-plan"));
    let family: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(family["ok"], true);
    assert_eq!(family["commands"].as_array().unwrap().len(), 15);
    assert!(family.get("native").is_some());
    assert!(
        family["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().starts_with("documents ingest "))
    );
}
#[test]
fn document_party_original_usage_precedes_credentials_and_has_stderr_exit_two() {
    for args in [
        vec![
            "party-link-apply",
            "--document-id",
            "7",
            "--role",
            "supplier",
            "--plan-hash",
            &"a".repeat(64),
        ],
        vec!["party-link-inspect", "--document-id", "0"],
        vec!["party-link-list", "--status", "invented"],
        vec![
            "internal-no-external-party",
            "--document-id",
            "7",
            "--reason",
            " ",
            "--confirm",
            "yes",
        ],
        vec![
            "party-link-plan",
            "--document-id",
            "7",
            "--role",
            "supplier",
            "--actor",
            "forged",
        ],
    ] {
        let mut invocation = vec!["documents"];
        invocation.extend(args);
        invocation.extend([
            "--server",
            "https://example.invalid",
            "--company",
            "synthetic",
            "--cookie-file",
            "/not/opened",
        ]);
        let o = cli(&invocation);
        assert_eq!(o.status.code(), Some(2));
        assert!(o.stdout.is_empty());
        assert!(!o.stderr.is_empty());
    }
}
#[test]
fn document_party_original_concrete_process_snake_json_and_human_refusal() {
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "document-party-original-cli-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let key = root.join("key");
    fs::write(&key, "rm_svc_synthetic_process").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
    }
    for human in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut s = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline);
                        thread::yield_now();
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            s.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut b = [0; 2048];
                let n = s.read(&mut b).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&b[..n]);
                if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&bytes);
            assert!(
                request.starts_with("GET /api/companies/synthetic/documents/original/party-links ")
            );
            assert!(!request.contains("workspace"));
            let body=if human{json!({"ok":false,"errors":["SCOPE_MISMATCH"]})}else{json!({"ok":true,"links":[{"id":7,"document_no":"SYN-7","sha256_hash":"a".repeat(64),"linked":0,"resolution_state":"unresolved"}]})}.to_string();
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let o = cli(&[
            "--company",
            "synthetic",
            "documents",
            "--server",
            &origin,
            "party-link-list",
            "--service-key-file",
            key.to_str().unwrap(),
            "--allow-loopback-http",
            "--format",
            if human { "human" } else { "json" },
        ]);
        task.join().unwrap();
        if human {
            assert_eq!(o.status.code(), Some(1));
            assert!(o.stdout.is_empty());
            let text = String::from_utf8(o.stderr).unwrap();
            assert!(text.contains("SCOPE_MISMATCH"));
            assert!(!text.contains('{'));
        } else {
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
            let v: Value = serde_json::from_slice(&o.stdout).unwrap();
            assert_eq!(v["links"][0]["document_no"], "SYN-7");
            assert!(v["links"][0].get("documentNo").is_none());
        }
    }
    let o = cli(&[
        "documents",
        "party-link-list",
        "--company",
        "synthetic",
        "--company-slug",
        "other",
        "--server",
        "https://example.invalid",
        "--service-key-file",
        key.to_str().unwrap(),
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("differs"));
    fs::remove_dir_all(root).unwrap();
}
