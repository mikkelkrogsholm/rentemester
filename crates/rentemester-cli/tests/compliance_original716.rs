//! Execute the actual CLI publication handler against a protocol-aware synthetic server.
#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::Command,
    thread,
};
#[cfg(unix)]
#[test]
fn original716_compliance_positive_file_hash_metadata_and_no_server_path() {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let dir = std::env::temp_dir().join(format!("compliance716-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let cookie = dir.join("cookie");
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic")
        .unwrap();
    let output = dir.join("literal=a.html");
    let listener = common::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let evidence = json!({"format":"rentemester-company-compliance/1","company":{"name":"Synthetic <Report>","cvr":null},"fiscalYear":{"displayLabel":"2026"},"audit":{"ok":true,"entries":0,"errors":[]},"backup":{"detailPermissionGranted":false,"reason":"owner required"},"retention":{"asOf":"2026-01-01","rows":[]},"sourcePeriods":{"closedCount":0,"lastClosedLabel":null},"gdpr":{"eventCount":0,"fingerprint":"sha256:synthetic"},"coverage":{"overall":{"inScopeCitedCount":0,"inScopeOperativeCount":0},"perSource":[],"rules":[],"closureErrors":[],"driftErrors":[],"scopeErrors":[],"uncitedRules":[]},"generatedAt":"2026-01-01T12:34:00.000Z","commitSha":"synthetic","ruleBundleVersion":"synthetic=1"});
    let html = rentemester_engine::compliance_report::original716::render(&evidence).unwrap();
    let hash = format!(
        "sha256:{}",
        rentemester_engine::compliance_report::fingerprint(&html)
    );
    let expected = html.clone();
    let fingerprint = hash.clone();
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = vec![];
        let (end, length) = loop {
            let mut b = [0; 4096];
            let n = stream.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&bytes[..i]);
                let len: usize = header
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= i + 4 + len {
                    break (i + 4, len);
                }
            }
        };
        let header = String::from_utf8_lossy(&bytes[..end]);
        assert!(
            header.starts_with(
                "POST /api/companies/synthetic/original716-compliance-report HTTP/1.1"
            )
        );
        assert!(header.to_lowercase().contains("cookie: session=synthetic"));
        let body: Value = serde_json::from_slice(&bytes[end..end + length]).unwrap();
        assert_eq!(body["asOf"], "2026-01-01");
        assert_eq!(body["generatedAt"], "2026-01-01T12:34:00.000Z");
        assert_eq!(body["expectedActor"], "user:synthetic");
        assert!(body.get("out").is_none());
        let response=json!({"ok":true,"result":{"ok":true,"html":html,"bytes":html.encode_utf16().count(),"fingerprint":fingerprint,"asOfDate":"2026-01-01","generatedAt":body["generatedAt"],"commitSha":body["commitSha"],"ruleBundleVersion":"synthetic=1"}}).to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
    });
    let out = Command::new(env!("CARGO_BIN_EXE_rentemester-rust"))
        .args([
            "compliance",
            "report",
            "--company=synthetic",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--out",
            output.to_str().unwrap(),
            "--as-of=2026-01-01",
            "--as-of-instant=2026-01-01T12:34:00.000Z",
            "--actor=user:synthetic",
            "--json",
        ])
        .output()
        .unwrap();
    task.join().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(result["fingerprint"], hash);
    assert_eq!(result["out"], output.to_str().unwrap());
    assert!(result.get("html").is_none());
    assert_eq!(std::fs::read_to_string(&output).unwrap(), expected);
    assert_eq!(
        std::fs::metadata(output).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::remove_dir_all(dir).unwrap();
}
