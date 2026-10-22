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
fn once(count: Arc<AtomicUsize>, handle: thread::JoinHandle<()>) {
    handle.join().unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[test]
fn exception_original_cli_help_usage_global_positions() {
    for args in [
        vec!["exceptions", "--help"],
        vec!["--cookie-file=/missing", "exceptions", "list", "--help"],
        vec!["--company=synthetic", "exceptions", "--help"],
    ] {
        let out = Command::new(BIN).args(args).output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(String::from_utf8_lossy(&out.stdout).contains("exceptions resolve"));
    }
    for args in [
        vec!["exceptions", "resolve"],
        vec!["exceptions", "resolve", "--id=0"],
        vec!["exceptions", "resolve", "--id=1", "--confirm=yes"],
        vec!["exceptions", "list", "--include-archived=true"],
    ] {
        let out = Command::new(BIN).args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(!out.stderr.is_empty());
    }
}
#[test]
fn exception_original_cli_flat_success_business_human_and_standalone_archive() {
    let path = std::env::temp_dir().join(format!("rm163-exception-cookie-{}", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&path)
        .unwrap()
        .write_all(b"session=synthetic")
        .unwrap();
    for (args, target, body, value, exit, human) in [
        (
            vec!["list"],
            "GET /api/companies/synthetic/exceptions/original?includeArchived=false HTTP/1.1",
            None,
            json!({"ok":true,"count":0,"rows":[],"errors":[]}),
            0,
            false,
        ),
        (
            vec!["list", "--json"],
            "GET /api/companies/synthetic/exceptions/original?includeArchived=false HTTP/1.1",
            None,
            json!({"ok":true,"count":0,"rows":[],"errors":[]}),
            0,
            false,
        ),
        (
            vec![
                "list",
                "--include-archived",
                "--status=all",
                "--format=human",
            ],
            "GET /api/companies/synthetic/exceptions/original?includeArchived=true&status=all HTTP/1.1",
            None,
            json!({"ok":true,"count":0,"rows":[],"errors":[]}),
            0,
            true,
        ),
        (
            vec!["list", "--status=bad", "--json"],
            "GET /api/companies/synthetic/exceptions/original?includeArchived=false&status=bad HTTP/1.1",
            None,
            json!({"ok":false,"count":0,"rows":[],"errors":["status must be open, resolved, or all when present"]}),
            1,
            false,
        ),
        (
            vec!["resolve", "--id=9", "--json"],
            "POST /api/companies/synthetic/exceptions/original/9/resolve HTTP/1.1",
            Some(b"{}".to_vec()),
            json!({"ok":true,"resolved":true,"errors":[]}),
            0,
            false,
        ),
        (
            vec!["resolve", "--id=9", "--note=review", "--json"],
            "POST /api/companies/synthetic/exceptions/original/9/resolve HTTP/1.1",
            Some(br#"{"note":"review"}"#.to_vec()),
            json!({"ok":false,"resolved":false,"code":"NOT_FOUND","errors":["exception 9 does not exist"]}),
            1,
            false,
        ),
    ] {
        let status = if value["code"] == "NOT_FOUND" {
            422
        } else {
            200
        };
        let (origin, count, t) = server(status, value.to_string(), target, body);
        let out = Command::new(BIN)
            .args([
                "--company=synthetic",
                "--server",
                &origin,
                "--cookie-file",
                path.to_str().unwrap(),
                "--allow-loopback-http",
                "exceptions",
            ])
            .args(args)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(exit),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
        if human {
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                "Ingen undtagelser i køen for det valgte filter.\n"
            );
        } else {
            assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), value);
        }
        once(count, t);
    }
    std::fs::remove_file(path).unwrap();
}
