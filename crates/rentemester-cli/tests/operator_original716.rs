//! Synthetic original group overview blackbox. Common fixture answers protocol
//! discovery first; the integration owner must add the delivered route to its registry.
#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use serde_json::{Value, json};
use std::{io::{Read, Write}, process::Command};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn call(args: &[&str]) -> std::process::Output {
    Command::new(BIN).args(args).env_remove("RENTEMESTER_COMPANY")
        .env_remove("RENTEMESTER_WORKSPACE").output().unwrap()
}
#[test]
fn unverified_remote_workspace_is_not_a_file_capability() {
    let output = call(&["group", "overview", "--workspace=/synthetic/no-access", "--as-of=2026-06-01", "--server=http://127.0.0.1:9", "--allow-loopback-http", "--cookie-file=/synthetic/missing"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--workspace requires an explicit verified --local-profile"));
    assert!(output.stdout.is_empty());
}
#[test]
fn invalid_date_is_usage_before_credentials() {
    let output = call(&["--as-of=2026-02-30", "group", "overview", "--cookie-file=/synthetic/missing"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("real ISO date"));
}
#[test]
fn remote_overview_preserves_structure_without_financial_sums() {
    let listener = common::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let expected = json!({"ok":true,"scope":"structure-status-only","consolidationStatus":"not-available","consolidatedFigures":null,"rawCompanySums":null,"asOf":"2026-06-01","manifestStatus":"ready","blockers":["structure overview deliberately contains no financial consolidation","use an approved reporting profile and the consolidated-report contract for figures"],"groups":[{"id":"synthetic","name":"Synthetic group","partial":false,"readiness":"ready","visibleMemberships":[{"id":"synthetic-member","companySlug":"synthetic-a","validFrom":"2026-01-01","archived":false}],"visibleOwnership":[],"blockers":[]}]});
    let reply = expected.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = vec![];
        while !bytes.ends_with(b"\r\n\r\n") {
            let mut byte = [0]; stream.read_exact(&mut byte).unwrap(); bytes.push(byte[0]);
            assert!(bytes.len() < 16384);
        }
        let request = String::from_utf8(bytes).unwrap();
        assert!(request.starts_with("GET /api/group/overview?asOf=2026-06-01 HTTP/1.1"));
        assert!(request.to_lowercase().contains("cookie: session=synthetic"));
        assert!(request.to_lowercase().contains("x-rentemester-manifest:"));
        let body = reply.to_string();
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    });
    let path = std::env::temp_dir().join(format!("operator716-cookie-{}", std::process::id()));
    let mut options = std::fs::OpenOptions::new(); options.write(true).create_new(true);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    let mut file = options.open(&path).unwrap(); file.write_all(b"session=synthetic").unwrap(); drop(file);
    let output = call(&["--server", &origin, "group", "--as-of=2026-06-01", "overview", "--cookie-file", path.to_str().unwrap(), "--allow-loopback-http", "--json"]);
    std::fs::remove_file(path).unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(serde_json::from_slice::<Value>(&output.stdout).unwrap(), expected);
    assert!(output.stderr.is_empty());
    server.join().unwrap();
}
