use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn remote_requires_explicit_server_and_never_falls_back_to_local() {
    let output = Command::new(BIN)
        .args(["remote", "health"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("explicit --server")
    );
}
#[test]
fn remote_health_uses_real_http_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 4096];
        let read = stream.read(&mut bytes).unwrap();
        assert!(bytes[..read].starts_with(b"GET /api/health HTTP/1.1"));
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"status\":\"ok\"}").unwrap();
    });
    let output = Command::new(BIN)
        .args([
            "remote",
            "health",
            "--server",
            &origin,
            "--allow-loopback-http",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["status"], "ok");
    task.join().unwrap();
}
#[cfg(unix)]
#[test]
fn cookie_file_permissions_refused_without_secret_disclosure() {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::temp_dir().join(format!("rentemester-cli-cookie-{}", std::process::id()));
    std::fs::write(&path, "session=do-not-display").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let output = Command::new(BIN)
        .args([
            "remote",
            "me",
            "--server",
            "http://127.0.0.1:9",
            "--allow-loopback-http",
            "--cookie-file",
            path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(output.status.code(), Some(2));
    let result = String::from_utf8(output.stdout).unwrap();
    assert!(result.contains("chmod 600"));
    assert!(!result.contains("do-not-display"));
}
#[test]
fn native_invoice_issue_requires_explicit_reviewed_confirmation() {
    for flags in [vec![], vec!["--confirm"], vec!["--preview-reviewed"]] {
        let mut command = Command::new(BIN);
        command.args([
            "remote",
            "issue-invoice",
            "--server",
            "http://127.0.0.1:9",
            "--allow-loopback-http",
            "--company",
            "demo",
        ]);
        command.args(flags);
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("preview-reviewed")
        );
    }
    let output = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let help: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        help["domainBodySchemas"]["invoicePost"]["additionalProperties"],
        false
    );
    assert!(
        help["domainWrites"]
            .as_str()
            .unwrap()
            .contains("do NOT support idempotency")
    );
}
#[cfg(unix)]
#[test]
fn native_customer_cli_forwards_exact_flat_body() {
    use std::{io::Write, os::unix::fs::OpenOptionsExt, process::Stdio};
    let cookie = std::env::temp_dir().join(format!("rm-domain-cli-{}.cookie", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap();
    file.write_all(b"session=synthetic-secret").unwrap();
    drop(file);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let body = "{ \"name\":\"Synthetic CLI buyer\", \"email\":null }";
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let size = stream.read(&mut chunk).unwrap();
            bytes.extend_from_slice(&chunk[..size]);
            if bytes.ends_with(body.as_bytes()) {
                break;
            }
            assert!(size > 0);
        }
        assert!(bytes.starts_with(b"POST /api/companies/demo/customers "));
        assert!(bytes.ends_with(body.as_bytes()));
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 31\r\nConnection: close\r\n\r\n{\"ok\":true,\"customer\":{\"id\":7}}").unwrap();
    });
    let mut child = Command::new(BIN)
        .args([
            "remote",
            "create-customer",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--company",
            "demo",
            "--cookie-file",
            cookie.to_str().unwrap(),
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
    let output = child.wait_with_output().unwrap();
    std::fs::remove_file(cookie).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["customer"]["id"], 7);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic-secret"));
    task.join().unwrap();
}
#[test]
fn bank_commands_document_native_bodies_and_reject_mixed_filters() {
    let output = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let help: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        help["bankBodySchemas"]["payPayable"]["required"],
        serde_json::json!(["bankTransactionId", "confirm"])
    );
    assert!(
        help["bankPayables"]
            .as_str()
            .unwrap()
            .contains("not arbitrary journal matching")
    );
    for args in [
        vec!["remote", "bank", "--status", "open"],
        vec!["remote", "pay-payable", "--confirm"],
        vec!["remote", "expense-booking-options", "--id", "0"],
        vec!["remote", "import-bank"],
        vec!["remote", "bank-accounts", "--year", "2026"],
    ] {
        let output = Command::new(BIN).args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
    }
}
#[test]
fn period_commands_expose_schemas_and_reject_missing_confirmation_or_bounds() {
    let help = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&help.stdout).unwrap();
    assert_eq!(
        value["periodBodySchemas"]["close"]["properties"]["reviewId"]["minimum"],
        1
    );
    for command in [
        "review-period",
        "close-period",
        "reopen-period",
        "report-period",
        "period-close-readiness",
    ] {
        let output = Command::new(BIN)
            .args([
                "remote",
                command,
                "--company",
                "demo",
                "--server",
                "http://127.0.0.1:9",
                "--allow-loopback-http",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
    }
}

#[cfg(unix)]
#[test]
fn native_period_review_cli_forwards_confirmed_exact_body() {
    use std::{io::Write, os::unix::fs::OpenOptionsExt, process::Stdio};
    let cookie = std::env::temp_dir().join(format!("rm-period-cli-{}.cookie", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap();
    file.write_all(b"session=synthetic-secret").unwrap();
    drop(file);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let body = r#"{ "periodStart":"2025-01-01","periodEnd":"2025-03-31","confirm":true }"#;
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let size = stream.read(&mut chunk).unwrap();
            bytes.extend_from_slice(&chunk[..size]);
            if bytes.ends_with(body.as_bytes()) {
                break;
            }
            assert!(size > 0);
        }
        assert!(bytes.starts_with(b"POST /api/companies/demo/periods/close-review "));
        assert!(bytes.ends_with(body.as_bytes()));
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 31\r\nConnection: close\r\n\r\n{\"ok\":true,\"customer\":{\"id\":7}}").unwrap();
    });
    let mut child = Command::new(BIN)
        .args([
            "remote",
            "review-period",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--company",
            "demo",
            "--cookie-file",
            cookie.to_str().unwrap(),
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
    let output = child.wait_with_output().unwrap();
    std::fs::remove_file(cookie).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["customer"]["id"], 7);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic-secret"));
    task.join().unwrap();
}

#[cfg(unix)]
#[test]
fn native_bank_creation_cli_accepts_explicit_company() {
    use std::{io::Write, os::unix::fs::OpenOptionsExt, process::Stdio};
    let cookie = std::env::temp_dir().join(format!("rm-bank-cli-{}.cookie", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap();
    file.write_all(b"session=synthetic-secret").unwrap();
    drop(file);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let body = r#"{ "name":"Synthetic bank", "ledgerAccountNo":"B" }"#;
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let size = stream.read(&mut chunk).unwrap();
            bytes.extend_from_slice(&chunk[..size]);
            if bytes.ends_with(body.as_bytes()) {
                break;
            }
            assert!(size > 0);
        }
        assert!(bytes.starts_with(b"POST /api/companies/demo/bank-accounts "));
        assert!(bytes.ends_with(body.as_bytes()));
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 31\r\nConnection: close\r\n\r\n{\"ok\":true,\"customer\":{\"id\":7}}").unwrap();
    });
    let mut child = Command::new(BIN)
        .args([
            "remote",
            "create-bank-account",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--company",
            "demo",
            "--cookie-file",
            cookie.to_str().unwrap(),
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
    let output = child.wait_with_output().unwrap();
    std::fs::remove_file(cookie).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["customer"]["id"], 7);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic-secret"));
    task.join().unwrap();
}
