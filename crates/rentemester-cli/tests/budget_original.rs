use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn original_discovery_and_usage_stay_separate_from_native() {
    for args in [
        vec!["budget", "--help"],
        vec!["budget", "set", "--help"],
        vec!["--help"],
    ] {
        let o = Command::new(BIN).args(args).output().unwrap();
        assert!(o.status.success());
        assert!(String::from_utf8_lossy(&o.stdout).contains("budget"));
    }
    for args in [
        vec!["budget", "unknown"],
        vec!["budget", "list", "--unknown"],
        vec!["budget", "set", "--amount"],
    ] {
        let o = Command::new(BIN).args(args).output().unwrap();
        assert_eq!(o.status.code(), Some(2));
    }
}

use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
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
#[test]
fn semantic_missing_invalid_and_json_read_refusals_exit1() {
    let cookie = std::env::temp_dir().join(format!("budget-cli-{}.cookie", std::process::id()));
    std::fs::write(&cookie, b"session=synthetic\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cookie, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    for (command, flags, path, reply, body) in [
        (
            "set",
            vec![],
            "POST /api/companies/synthetic/original/budget/set ",
            json!({"ok":false,"appliedRules":["budget:set"],"errors":["account required","amount required"]}),
            Some(json!({"accountNo":"","period":"","confirm":true})),
        ),
        (
            "set",
            vec!["--amount", "garbage"],
            "POST /api/companies/synthetic/original/budget/set ",
            json!({"ok":false,"appliedRules":["budget:set"],"errors":["amount required"]}),
            Some(json!({"accountNo":"","period":"","confirm":true})),
        ),
        (
            "set",
            vec!["--amount", "-1"],
            "POST /api/companies/synthetic/original/budget/set ",
            json!({"ok":false,"appliedRules":["budget:set"],"errors":["amount must be nonnegative"]}),
            Some(json!({"accountNo":"","period":"","amount":-1.0,"confirm":true})),
        ),
        (
            "vs-actual",
            vec![],
            "GET /api/companies/synthetic/original/budget/vs-actual?from=&to= ",
            json!({"ok":false,"appliedRules":["budget:vs-actual"],"periodStart":"","periodEnd":"","lines":[],"totalBudget":0,"totalActual":0,"totalVariance":0,"errors":["range required"]}),
            None,
        ),
        (
            "forecast",
            vec!["--months", "bad"],
            "GET /api/companies/synthetic/original/budget/forecast?startDate=&months=bad ",
            json!({"ok":false,"appliedRules":["liquidity-forecast"],"openingBalance":0,"closingBalance":0,"periods":[],"errors":["months required"]}),
            None,
        ),
        (
            "forecast-13-week",
            vec!["--weeks", "bad"],
            "GET /api/companies/synthetic/original/budget/forecast-13-week?startDate=&weeks=bad ",
            json!({"ok":false,"appliedRules":["liquidity-forecast-13-week-v1"],"openingCash":0,"lowestPoint":0,"completeness":{"included":[],"excluded":[]},"periods":[],"errors":["weeks invalid"]}),
            None,
        ),
    ] {
        let (origin, count, t) = server(
            422,
            reply.to_string(),
            path,
            body.map(|v| serde_json::to_vec(&v).unwrap()),
        );
        let o = Command::new(BIN)
            .args([
                "budget",
                command,
                "--company",
                "synthetic",
                "--server",
                &origin,
                "--allow-loopback-http",
                "--cookie-file",
                cookie.to_str().unwrap(),
                "--json",
            ])
            .args(flags)
            .output()
            .unwrap();
        assert_eq!(
            o.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert_eq!(serde_json::from_slice::<Value>(&o.stdout).unwrap(), reply);
        t.join().unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
    std::fs::remove_file(cookie).unwrap();
}
