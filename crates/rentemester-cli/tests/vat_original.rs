use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn five_source_commands_require_dates_before_http_and_reject_write_flags() {
    for op in [
        "report",
        "eu-sales-list",
        "oss-report",
        "momsangivelse",
        "filing",
    ] {
        let out = Command::new(BIN)
            .args(["vat", op, "--help"])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("No submission"));
        for flags in [
            vec![],
            vec!["--from=", "--to=2026-03-31"],
            vec!["--from=2026-01-01"],
            vec!["--from"],
        ] {
            let out = Command::new(BIN)
                .args(["vat", op])
                .args(flags)
                .output()
                .unwrap();
            assert_eq!(out.status.code(), Some(2));
            assert!(
                String::from_utf8_lossy(&out.stderr)
                    .contains("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>"),
                "{:?}",
                out
            );
        }
        for flag in ["--confirm", "--input", "--output", "--actor"] {
            let out = Command::new(BIN)
                .args(["vat", op, flag, "x"])
                .output()
                .unwrap();
            assert_eq!(out.status.code(), Some(2));
        }
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
fn filing_aliases_share_route_and_flat_json_business_refusal() {
    let cookie = std::env::temp_dir().join(format!("vat-cli-{}.cookie", std::process::id()));
    std::fs::write(&cookie, b"session=synthetic\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cookie, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let reply = json!({"ok":false,"errors":["selskabet er ikke momsregistreret"]});
    for command in ["momsangivelse", "filing"] {
        let (origin, count, t) = server(
            422,
            reply.to_string(),
            "GET /api/companies/synthetic/original/vat/filing?from=bad&to=earlier ",
            None,
        );
        let o = Command::new(BIN)
            .args([
                "vat",
                command,
                "--company",
                "synthetic",
                "--server",
                &origin,
                "--allow-loopback-http",
                "--cookie-file",
                cookie.to_str().unwrap(),
                "--json",
                "--from",
                "bad",
                "--to",
                "earlier",
            ])
            .output()
            .unwrap();
        t.join().unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(o.status.code(), Some(1));
        assert_eq!(serde_json::from_slice::<Value>(&o.stdout).unwrap(), reply);
    }
    std::fs::remove_file(cookie).unwrap();
}
