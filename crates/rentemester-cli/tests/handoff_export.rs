#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use common::TcpListener;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::Path,
    process::{Command, Output},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn package(authority: bool) -> (String, Vec<u8>) {
    let root = if authority {
        "myndigheds-eksport-synthetic"
    } else {
        "revisor-eksport-synthetic"
    }
    .to_owned();
    let manifest = json!({"manifestVersion":2,"packageType":if authority{"authority_export"}else{"accountant_handoff_export"},"periodStart":"2026-01-01","periodEnd":"2026-12-31","generatedAt":"2027-01-02T12:00:00Z","deadlineAt":"2027-01-30T12:00:00Z","appliedRules":["DK-BOOKKEEPING-AUTHORITY-EXPORT-001"],"counts":{"journalEntries":2,"bankTransactions":0,"documents":0,"copiedReadableDocuments":0},"files":{"documents":"documents.json","readableDocumentsDir":"documents-readable"},"outputs":[{"path":"documents.json","sizeBytes":2,"sha256":"4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"}],"copiedDocuments":[]});
    let bytes = rentemester_storage::accountant::tar(BTreeMap::from([
        (format!("{root}/documents.json"), b"[]".to_vec()),
        (
            format!("{root}/manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        ),
    ]))
    .unwrap();
    (root, bytes)
}
fn server(bytes: Vec<u8>) -> (String, thread::JoinHandle<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = vec![];
        loop {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            request.extend(&b[..n]);
            if let Some(p) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&request[..p]).to_lowercase();
                let len: usize = head
                    .lines()
                    .find_map(|s| s.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                if request.len() >= p + 4 + len {
                    break;
                }
            }
        }
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type:application/x-tar\r\nContent-Length:{}\r\nConnection:close\r\n\r\n",bytes.len()).unwrap();
        s.write_all(&bytes).unwrap();
        String::from_utf8(request).unwrap()
    });
    (origin, t)
}
fn call(command: &str, origin: &str, dir: &Path) -> Output {
    Command::new(BIN)
        .args([
            "system",
            command,
            "--server",
            origin,
            "--allow-loopback-http",
            "--cookie-file",
            dir.join("cookie").to_str().unwrap(),
            "--company",
            "synthetic",
            "--from",
            "2026-01-01",
            "--to",
            "2026-12-31",
            "--out",
            dir.join("out").to_str().unwrap(),
            "--requested-at",
            "2027-01-02T12:00:00+01:00",
            "--requester",
            "Synthetic agent",
            "--json",
        ])
        .output()
        .unwrap()
}
#[test]
fn original_commands_confirm_once_and_return_flat_metadata_with_verified_local_files() {
    for (authority, command, route) in [
        (true, "export-authority", "authority-export"),
        (false, "export-accountant", "accountant-export"),
    ] {
        let dir = std::env::temp_dir().join(format!("rm136-cli-{}-{command}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(dir.join("cookie"))
            .unwrap()
            .write_all(b"session=synthetic")
            .unwrap();
        let (root, bytes) = package(authority);
        let (origin, t) = server(bytes.clone());
        let output = call(command, &origin, &dir);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(metadata["ok"], true);
        assert_eq!(metadata["journalEntryCount"], 2);
        assert_eq!(metadata["deadlineAt"], "2027-01-30T12:00:00Z");
        assert!(metadata.get("data").is_none());
        assert!(dir.join("out").join(&root).join("manifest.json").is_file());
        let request = t.join().unwrap();
        assert!(request.starts_with(&format!("POST /api/companies/synthetic/{route} HTTP/1.1")));
        let body: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(
            body,
            json!({"periodStart":"2026-01-01","periodEnd":"2026-12-31","confirm":true,"requestedAt":"2027-01-02T12:00:00+01:00","requester":"Synthetic agent"})
        );
        let (origin, t) = server(bytes);
        let output = call(command, &origin, &dir);
        assert_eq!(output.status.code(), Some(1));
        let refused: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(refused["ok"], false);
        assert!(!refused["errors"].as_array().unwrap().is_empty());
        t.join().unwrap();
        assert!(dir.join("out").join(root).join("manifest.json").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn missing_flags_are_usage_two_and_help_succeeds() {
    let out = Command::new(BIN)
        .args(["system", "export-authority", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(!out.stderr.is_empty());
    let out = Command::new(BIN)
        .args(["system", "export-accountant", "--help"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty());
}
