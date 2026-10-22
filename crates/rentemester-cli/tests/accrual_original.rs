use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
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

#[test]
fn pure_help_before_credentials_and_exact_source_flags() {
    for op in ["register", "recognize", "register-report"] {
        let o = Command::new(BIN)
            .args([
                "--company=synthetic",
                "accrual",
                op,
                "--help",
                "--cookie-file",
                "/private/not-opened-cookie",
                "--local-profile",
                "/private/not-opened-profile",
            ])
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let v: Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(v["ok"], true);
        assert!(v.to_string().contains("--period N"));
    }
    for extra in [
        vec!["--period-index", "1"],
        vec!["--transaction-date", "2026-01-01"],
        vec!["--amount", "1"],
        vec!["--idempotency-key", "forged"],
        vec!["--confirm"],
    ] {
        let o = Command::new(BIN)
            .args([
                "accrual",
                "recognize",
                "--server",
                "http://127.0.0.1:9",
                "--allow-loopback-http",
                "--company",
                "synthetic",
                "--cookie-file",
                "/private/not-opened",
                "--accrual-id",
                "4",
                "--period",
                "1",
            ])
            .args(extra)
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(2));
        assert!(!String::from_utf8_lossy(&o.stderr).contains("Could not read"));
    }
    let o = Command::new(BIN).arg("--help").output().unwrap();
    assert!(
        String::from_utf8_lossy(&o.stdout).contains("accrual register|recognize|register-report")
    );
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
fn three_source_routes_flat_outputs_defaults_no_added_confirmation_or_key() {
    let root = temporary();
    let credential = cookie(&root);
    let cases = [
        (
            "register",
            vec![
                "--type",
                "prepaid_expense",
                "--description",
                "Synthetic prepayment",
                "--amount",
                "125.09",
                "--periods",
                "3",
                "--first-date",
                "2026-01-31",
                "--result-account",
                "expense",
            ],
            "POST /api/companies/synthetic/accrual/original/register ",
            Some(
                json!({"accrualType":"prepaid_expense","description":"Synthetic prepayment","totalAmount":125.09,"recognitionPeriods":3,"firstRecognitionDate":"2026-01-31","resultAccountNo":"expense","confirm":true}),
            ),
            json!({"ok":true,"entryId":9,"entryNo":"2026-0009","entryHash":"a".repeat(64),"appliedRules":["DK-BOOKKEEPING-ACCRUAL-001"],"errors":[],"accrualId":4,"accrualType":"prepaid_expense","totalPeriods":3,"periodAmount":41.69}),
        ),
        (
            "recognize",
            vec!["--accrual-id", "4", "--period", "2", "--date", "2026-01-01"],
            "POST /api/companies/synthetic/accrual/original/4/recognize ",
            Some(json!({"period":2,"date":"2026-01-01","confirm":true})),
            json!({"ok":true,"entryId":10,"entryNo":"2026-0010","entryHash":"b".repeat(64),"appliedRules":["DK-BOOKKEEPING-ACCRUAL-001"],"errors":[],"accrualId":4,"periodIndex":2,"periodAmount":41.69,"recognizedPeriods":1,"totalPeriods":3,"fullyRecognized":false}),
        ),
        (
            "register-report",
            vec![],
            "GET /api/companies/synthetic/accrual/original/register-report ",
            None,
            json!({"ok":true,"accruals":[],"totals":{"totalAmount":0,"recognizedAmount":0,"remainingAmount":0},"errors":[]}),
        ),
    ];
    for (op, flags, target, body, response) in cases {
        let (origin, count, t) = server(
            200,
            response.to_string(),
            target,
            body.map(|v| serde_json::to_vec(&v).unwrap()),
        );
        let out = Command::new(BIN)
            .args([
                "accrual",
                op,
                "--server",
                &origin,
                "--allow-loopback-http",
                "--company",
                "synthetic",
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
            Some(0),
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
