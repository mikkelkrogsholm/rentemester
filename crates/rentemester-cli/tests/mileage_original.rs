use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn original_discovery_and_usage_stay_separate_from_native() {
    for args in [
        vec!["mileage", "--help"],
        vec!["mileage", "log", "--help"],
        vec!["--help"],
    ] {
        let o = Command::new(BIN).args(args).output().unwrap();
        assert!(o.status.success());
        assert!(String::from_utf8_lossy(&o.stdout).contains("mileage"));
    }
    for args in [
        vec!["mileage", "unknown"],
        vec!["mileage", "list", "--unknown"],
        vec!["mileage", "log", "--rate-per-km"],
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
fn missing_negative_and_empty_log_values_are_source_exit1_not_usage2() {
    let cookie = std::env::temp_dir().join(format!("mileage-cli-{}.cookie", std::process::id()));
    std::fs::write(&cookie, b"session=synthetic\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cookie, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    for args in [
        vec![],
        vec!["--km", "-1", "--rate-per-km", "0", "--purpose="],
        vec!["--km", "garbage"],
    ] {
        let km = if args.contains(&"-1") {
            json!(-1.0)
        } else {
            Value::Null
        };
        let rate = if args.contains(&"0") {
            json!(0.0)
        } else {
            Value::Null
        };
        let mut input = json!({"tripDate":"","purpose":"","fromLocation":"","toLocation":"","vehicle":"","driver":"","rateBasis":"","confirm":true});
        if !km.is_null() {
            input["kilometers"] = km;
        }
        if !rate.is_null() {
            input["ratePerKm"] = rate;
        }
        let refusal = json!({"ok":false,"appliedRules":["DK-MILEAGE-LOG-001"],"warnings":[],"errors":["tripDate is required","kilometers must be a positive number"]});
        let (origin, count, t) = server(
            422,
            refusal.to_string(),
            "POST /api/companies/synthetic/original/mileage/log ",
            Some(serde_json::to_vec(&input).unwrap()),
        );
        let o = Command::new(BIN)
            .args([
                "mileage",
                "log",
                "--company",
                "synthetic",
                "--server",
                &origin,
                "--allow-loopback-http",
                "--cookie-file",
                cookie.to_str().unwrap(),
                "--json",
            ])
            .args(args)
            .output()
            .unwrap();
        assert_eq!(
            o.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert_eq!(serde_json::from_slice::<Value>(&o.stdout).unwrap(), refusal);
        t.join().unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
    std::fs::remove_file(cookie).unwrap();
}
