use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
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
            "rm143-cli-{}-{}",
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
        assert!(header.starts_with("POST /api/companies/synthetic/bank/accounts-original/"));
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

fn call(kind: &str, origin: &str, f: &Fixture, service: bool, extra: &[&str]) -> Output {
    Command::new(BIN)
        .args([
            "bank-account",
            kind,
            "--server",
            origin,
            "--allow-loopback-http",
            "--company",
            "synthetic",
            if service {
                "--service-key-file"
            } else {
                "--cookie-file"
            },
            f.0.join(if service { "key" } else { "cookie" })
                .to_str()
                .unwrap(),
        ])
        .args(extra)
        .output()
        .unwrap()
}
#[test]
fn create_update_preserve_empty_and_omitted_fields_full_receipt_and_credentials() {
    let f = Fixture::new();
    let account = json!({"id":8,"slug":"server-slug","name":"Server name","iban":null,"active":false,"createdAt":"server-date"});
    for (kind, service, extra, body) in [
        (
            "add",
            false,
            vec!["--name", " Input ", "--slug", "", "--currency", ""],
            json!({"confirm":true,"name":" Input ","slug":"","currency":""}),
        ),
        (
            "update",
            true,
            vec!["--account", " 8 ", "--iban", "", "--active", "false"],
            json!({"confirm":true,"account":" 8 ","iban":"","active":false}),
        ),
    ] {
        let (origin, t) = server(
            200,
            json!({"ok":true,"data":{"account":account},"errors":[]}),
        );
        let mut args = extra;
        args.push("--json");
        let out = call(kind, &origin, &f, service, &args);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap(),
            json!({"ok":true,"account":account,"errors":[]})
        );
        assert!(out.stderr.is_empty());
        assert_eq!(t.join().unwrap(), body);
    }
}
#[test]
fn strict_active_and_forbidden_fields_stop_with_usage2_and_business_unknown_keep1() {
    let f = Fixture::new();
    for extra in [
        vec!["--account", "8", "--active", "typo"],
        vec!["--account", "8", "--active", "FALSE"],
        vec!["--account", "8", "--slug", "new"],
        vec!["--account", "8", "--confirm"],
        vec![],
    ] {
        let out = call("update", "http://127.0.0.1:9", &f, false, &extra);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
    }
    let out = call(
        "add",
        "http://127.0.0.1:9",
        &f,
        false,
        &["--name", "Synthetic", "--active", "true"],
    );
    assert_eq!(out.status.code(), Some(2));
    let value = json!({"ok":false,"errors":["unsafe historical remap refused"]});
    let (origin, t) = server(200, value.clone());
    let out = call(
        "update",
        &origin,
        &f,
        false,
        &["--account", "8", "--ledger-account", "", "--json"],
    );
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), value);
    t.join().unwrap();
    let (origin, t) = server(503, json!({"ok":false}));
    let out = call("update", &origin, &f, false, &["--account", "8", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["code"],
        "outcome_unknown"
    );
    t.join().unwrap();
    for kind in ["add", "update"] {
        assert_eq!(
            Command::new(BIN)
                .args(["bank-account", kind, "--help"])
                .output()
                .unwrap()
                .status
                .code(),
            Some(0)
        );
    }
}
