#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use common::TcpListener;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn server(
    status: u16,
    response: String,
    target: &'static str,
    body: Option<Vec<u8>>,
) -> (String, Arc<AtomicUsize>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    let handle = thread::spawn(move || {
        let start = Instant::now();
        let mut last: Option<Instant> = None;
        loop {
            if let Some(t) = last {
                if t.elapsed() >= Duration::from_millis(150) {
                    break;
                }
            } else {
                assert!(
                    start.elapsed() < Duration::from_secs(5),
                    "startup deadline exceeded"
                );
            }
            let (mut s, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            seen.fetch_add(1, Ordering::SeqCst);
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut bytes = Vec::new();
            let end = loop {
                let mut chunk = [0; 4096];
                let n = s.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
            assert!(headers.starts_with(target), "{headers}");
            assert!(headers.to_lowercase().contains("cookie: session=synthetic"));
            let length = headers
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|s| s.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            while bytes.len() < end + length {
                let mut chunk = [0; 4096];
                let n = s.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
            }
            assert_eq!(&bytes[end..], body.as_deref().unwrap_or_default());
            write!(s,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
            last = Some(Instant::now());
        }
    });
    (origin, count, handle)
}
fn cookie(root: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = root.join("cookie");
    std::fs::write(&path, "session=synthetic").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    path
}

fn temporary() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "original-accrual-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    root
}
#[test]
fn pure_help_and_exact_valued_confirmation_before_credentials() {
    for op in ["register", "depreciate", "write-off", "register-report"] {
        let o = Command::new(BIN)
            .args([
                "--company=synthetic",
                "asset",
                op,
                "--help",
                "--cookie-file",
                "/private/not-opened",
                "--local-profile",
                "/private/not-opened-profile",
            ])
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        assert_eq!(
            serde_json::from_slice::<Value>(&o.stdout).unwrap()["ok"],
            true
        );
    }
    for extra in [
        vec!["--confirm"],
        vec!["--confirm", "YES"],
        vec!["--confirm=true"],
        vec!["--confirm", "true"],
        vec!["--method", "linear"],
        vec!["--idempotency-key", "key"],
    ] {
        let out = Command::new(BIN)
            .args([
                "asset",
                "write-off",
                "--server",
                "http://127.0.0.1:9",
                "--allow-loopback-http",
                "--company",
                "synthetic",
                "--cookie-file",
                "/private/not-opened",
            ])
            .args(extra)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(!String::from_utf8_lossy(&out.stderr).contains("Could not read"));
    }
    let o = Command::new(BIN).arg("--help").output().unwrap();
    assert!(
        String::from_utf8_lossy(&o.stdout)
            .contains("asset register|depreciate|write-off|register-report")
    );
}
#[test]
fn four_routes_flat_exact_results_no_retry() {
    let root = temporary();
    let credential = cookie(&root);
    for (op, flags, target, body, response, status) in [
        (
            "register",
            vec![
                "--name",
                "Synthetic",
                "--category",
                "Equipment",
                "--acquisition-date",
                "2026-01-01",
                "--cost",
                "125",
                "--useful-life-months",
                "3",
                "--document-id",
                "7",
            ],
            "POST /api/companies/synthetic/asset/original/register ",
            Some(
                json!({"name":"Synthetic","category":"Equipment","acquisitionDate":"2026-01-01","cost":125,"usefulLifeMonths":3,"documentId":7,"confirm":true}),
            ),
            json!({"ok":true,"assetId":4,"totalPeriods":3,"periodAmount":41.66,"appliedRules":["DK-ASSET-DEPR-001"],"errors":[]}),
            200,
        ),
        (
            "depreciate",
            vec!["--asset-id", "4", "--period", "1", "--date", "2026-01-01"],
            "POST /api/companies/synthetic/asset/original/4/depreciate ",
            Some(json!({"period":1,"date":"2026-01-01","confirm":true})),
            json!({"ok":true,"assetId":4,"periodIndex":1,"periodAmount":41.66,"entryId":1,"entryNo":"2026-0001","entryHash":"a".repeat(64),"appliedRules":["DK-ASSET-DEPR-001"],"errors":[]}),
            200,
        ),
        (
            "write-off",
            vec![
                "--name",
                "Synthetic",
                "--category",
                "Equipment",
                "--acquisition-date",
                "2026-01-01",
                "--cost",
                "250",
                "--document-id",
                "8",
                "--expense-account",
                "expense",
                "--date",
                "2026-01-02",
                "--threshold-source",
                "Advisor assessment",
                "--confirm",
                "yes",
            ],
            "POST /api/companies/synthetic/asset/original/write-off ",
            Some(
                json!({"name":"Synthetic","category":"Equipment","acquisitionDate":"2026-01-01","cost":250,"documentId":8,"expenseAccount":"expense","date":"2026-01-02","thresholdRuleSource":"Advisor assessment","confirmImmediateWriteOff":true,"confirm":true}),
            ),
            json!({"ok":false,"appliedRules":["DK-ASSET-WRITEOFF-001"],"errors":["Purchase documentation is missing"]}),
            422,
        ),
        (
            "register-report",
            vec![],
            "GET /api/companies/synthetic/asset/original/register-report ",
            None,
            json!({"ok":true,"assets":[],"totals":{"cost":0,"accumulatedDepreciation":0,"netBookValue":0},"errors":[]}),
            200,
        ),
    ] {
        let (origin, count, t) = server(
            status,
            response.to_string(),
            target,
            body.map(|v| serde_json::to_vec(&v).unwrap()),
        );
        let out = Command::new(BIN)
            .args([
                "--company=synthetic",
                "asset",
                op,
                "--server",
                &origin,
                "--allow-loopback-http",
                "--cookie-file",
                credential.to_str().unwrap(),
                "--json",
            ])
            .args(flags)
            .output()
            .unwrap();
        t.join().unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(
            out.status.code(),
            Some(if status == 422 { 1 } else { 0 }),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap(),
            response
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
