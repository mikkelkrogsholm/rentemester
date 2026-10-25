#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use common::TcpListener;
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rm142-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let fixture = Self(path);
        for (name, bytes) in [
            ("cookie", b"session=synthetic".as_slice()),
            ("key", b"synthetic-key".as_slice()),
        ] {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options
                .open(fixture.0.join(name))
                .unwrap()
                .write_all(bytes)
                .unwrap();
        }
        fixture
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn server(status: u16, value: Value) -> (String, thread::JoinHandle<Value>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = vec![];
        let end = loop {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let header = String::from_utf8_lossy(&bytes[..end]);
        assert!(header.starts_with("POST /api/companies/synthetic/bank/import-original HTTP/1.1"));
        let length: usize = header
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|s| s.trim().parse().unwrap())
            })
            .unwrap();
        while bytes.len() < end + length {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
        }
        let body = value.to_string();
        write!(
            s,
            "HTTP/1.1 {status} Mock\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        serde_json::from_slice(&bytes[end..end + length]).unwrap()
    });
    (origin, t)
}

fn call(origin: &str, f: &Fixture, extra: &[&str]) -> Output {
    Command::new(BIN)
        .args([
            "bank",
            "import",
            "--server",
            origin,
            "--allow-loopback-http",
            "--company",
            "synthetic",
            "--cookie-file",
            f.0.join("cookie").to_str().unwrap(),
        ])
        .args(extra)
        .output()
        .unwrap()
}
#[test]
fn original_file_command_preserves_bytes_and_flattens_only_server_receipt() {
    let f = Fixture::new();
    let path = f.0.join("statement.csv");
    fs::write(&path, [239, 187, 191, 248, 13, 10]).unwrap();
    let data = json!({"importBatchId":"BANK-server","sourceFileHash":"server-hash","imported":2,"skippedDuplicates":1,"skippedDuplicateRows":["server duplicate"],"exceptionsCreated":0,"firstBalance":-42.5});
    let (origin, t) = server(200, json!({"ok":true,"data":data,"errors":[]}));
    let out = call(
        &origin,
        &f,
        &[
            "--file",
            path.to_str().unwrap(),
            "--account",
            " synthetic ",
            "--profile",
            "danske-bank",
            "--json",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut expected = data;
    expected["ok"] = json!(true);
    expected["errors"] = json!([]);
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        expected
    );
    assert!(out.stderr.is_empty());
    let body = t.join().unwrap();
    assert_eq!(body["csvBase64"], "77u/+A0K");
    assert_eq!(body["account"], " synthetic ");
    assert!(body.get("csvContent").is_none());
}
#[test]
fn original_import_usage_business_and_unknown_outcomes_keep_streams_and_exit_codes() {
    let f = Fixture::new();
    let path = f.0.join("statement.csv");
    fs::write(&path, b"date;text;amount\n2026-01-01;Synthetic;1").unwrap();
    let refusal = json!({"ok":false,"errors":["bank account 'unknown' does not exist"]});
    let (origin, t) = server(200, refusal.clone());
    let out = call(&origin, &f, &["--file", path.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        refusal
    );
    assert!(out.stderr.is_empty());
    t.join().unwrap();
    let (origin, t) = server(503, json!({"ok":false}));
    let out = call(&origin, &f, &["--file", path.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["code"],
        "outcome_unknown"
    );
    t.join().unwrap();
    for extra in [&[][..], &["--file"][..], &["--file", "x", "--confirm"][..]] {
        let out = call("http://127.0.0.1:9", &f, extra);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
    }
    let out = call(
        "http://127.0.0.1:9",
        &f,
        &["--file", f.0.join("missing").to_str().unwrap(), "--json"],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("file could not be opened"));
    let out = Command::new(BIN)
        .args(["bank", "import", "--help"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
}
