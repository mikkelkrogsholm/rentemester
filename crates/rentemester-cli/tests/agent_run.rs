//! Original agent invocation against the existing passive protocol HTTP fixture pattern.
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn wire<S: Read + Write>(s: &mut S, expected: &str, status: u16, result: &Value) -> Vec<u8> {
    let mut b = vec![];
    let mut end = None;
    let mut length = 0;
    loop {
        let mut buf = [0; 4096];
        let n = s.read(&mut buf).unwrap();
        assert!(n > 0);
        b.extend_from_slice(&buf[..n]);
        if end.is_none() {
            if let Some(i) = b.windows(4).position(|w| w == b"\r\n\r\n") {
                end = Some(i + 4);
                for l in String::from_utf8_lossy(&b[..i]).lines() {
                    if let Some(v) = l.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                }
            }
        }
        if end.is_some_and(|i| b.len() >= i + length) {
            break;
        }
    }
    let text = String::from_utf8_lossy(&b);
    assert!(text.starts_with(expected), "{text}");
    if expected.starts_with("GET /api/protocol ") {
        assert!(!text.to_ascii_lowercase().contains("cookie:"));
    } else {
        assert!(
            text.to_ascii_lowercase()
                .contains("cookie: session=synthetic")
        );
        assert!(
            text.to_ascii_lowercase()
                .contains("x-rentemester-manifest:")
        );
    }
    if status == 0 {
        return b[end.unwrap()..].to_vec();
    }
    let body = result.to_string();
    write!(s,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection:close\r\n\r\n{body}",body.len()).unwrap();
    b[end.unwrap()..].to_vec()
}
fn bounded_accept(l: &TcpListener) -> Option<std::net::TcpStream> {
    let at = std::time::Instant::now();
    loop {
        match l.accept() {
            Ok((s, _)) => {
                s.set_nonblocking(false).unwrap();
                return Some(s);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if at.elapsed() > std::time::Duration::from_secs(5) {
                    return None;
                }
                thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(e) => panic!("{e}"),
        }
    }
}
fn run_fixture(
    args: &[&str],
    steps: Vec<(&'static str, u16, Value)>,
) -> (std::process::Output, Vec<Vec<u8>>) {
    use std::os::unix::fs::OpenOptionsExt;
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let cookie = std::env::temp_dir().join(format!(
        "rm-company716-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic")
        .unwrap();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    l.set_nonblocking(true).unwrap();
    let task = thread::spawn(move || {
        let mut manifest: Value =
            serde_json::from_str(include_str!("../../../protocol/http-v1.json")).unwrap();
        manifest["operations"].as_array_mut().unwrap().push(json!({"method":"POST","path":"/api/companies/{slug}/agent/original/run","capability":"POST /api/companies/{slug}/agent/original/run"}));
        let mut bodies = vec![];
        for (path, status, v) in steps {
            let Some(mut s) = bounded_accept(&l) else {
                return bodies;
            };
            s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            wire(&mut s, "GET /api/protocol HTTP/1.1", 200, &manifest);
            let Some(mut s) = bounded_accept(&l) else {
                return bodies;
            };
            s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            bodies.push(wire(&mut s, path, status, &v));
        }
        let deadline = std::time::Instant::now();
        while deadline.elapsed() < std::time::Duration::from_millis(80) {
            assert!(
                matches!(l.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
                "unexpected retry"
            );
            thread::sleep(std::time::Duration::from_millis(5));
        }
        bodies
    });
    let out = Command::new(BIN)
        .args([
            "agent",
            "run",
            "--company",
            "synthetic",
            "--as-of",
            "2026-02-01",
        ])
        .args(args)
        .args([
            "--server",
            &origin,
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--allow-loopback-http",
        ])
        .output()
        .unwrap();
    std::fs::remove_file(cookie).unwrap();
    (out, task.join().unwrap())
}
fn report() -> Value {
    json!({"ok":true,"actor":"service:synthetic","asOf":"2026-02-01","company":"synthetic","phases":["ingest","book","route","payables","reconcile","deadlines","report"],"documentsIngested":0,"documentsRejected":0,"bankTransactionsImported":0,"expensesBooked":[],"payablesMatched":[],"accrualRecognitionsDue":0,"openExceptions":[],"upcomingDeadlines":[],"summary":["0 bilag ingested"],"errors":[]})
}
#[test]
fn parse_help_and_missing_asof_before_credentials() {
    for args in [
        vec!["agent", "--help"],
        vec!["--cookie-file", "/not/opened", "agent", "run", "--help"],
    ] {
        let o = Command::new(BIN).args(args).output().unwrap();
        assert!(o.status.success());
        assert!(String::from_utf8_lossy(&o.stdout).contains("actual authenticated"));
    }
    for flags in [
        vec![],
        vec!["--confirm"],
        vec!["--as-of", "2026-01-01", "--format", "csv"],
    ] {
        let o = Command::new(BIN)
            .args(["agent", "run"])
            .args(flags)
            .args(["--cookie-file", "/not/opened"])
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(2));
        assert!(o.stdout.is_empty());
    }
}
#[test]
fn one_post_flat_failure_unknown_and_no_retry() {
    for mode in 0..4 {
        let mut v = report();
        let status = match mode {
            1 => {
                v["ok"] = json!(false);
                v["errors"] = json!(["bank import failed"]);
                v["documentsIngested"] = json!(2);
                v["phases"] = json!(["ingest"]);
                422
            }
            2 => {
                v["extra"] = json!(true);
                200
            }
            3 => 0,
            _ => 200,
        };
        let (out, bodies) = run_fixture(
            &["--json"],
            vec![(
                "POST /api/companies/synthetic/agent/original/run HTTP/1.1",
                status,
                v.clone(),
            )],
        );
        assert_eq!(bodies.len(), 1);
        let body: Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert_eq!(body, json!({"asOf":"2026-02-01","confirm":true}));
        assert_eq!(out.status.code(), Some(if mode == 0 { 0 } else { 1 }));
        if mode < 2 {
            assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), v);
        } else {
            assert!(out.stdout.is_empty());
            assert!(String::from_utf8_lossy(&out.stderr).contains("unknown"));
        }
    }
}
#[test]
fn human_sections_currency_and_whole_inbox_failure() {
    let mut v = report();
    v["expensesBooked"] = json!([{"documentNo":"d","documentId":1,"bankTransactionId":1,"supplier":"Synthetic","amount":1234.5,"currency":"DKK","expenseAccount":"test","vatTreatment":"standard","label":"test","journalEntryNo":null},{"documentNo":"e","documentId":2,"bankTransactionId":2,"supplier":"Foreign","amount":100,"currency":"eur","expenseAccount":"test","vatTreatment":"standard","label":"test","journalEntryNo":"entry"}]);
    let (out, bodies) = run_fixture(
        &[
            "--format",
            "human",
            "--inbox",
            "/missing-agent-inbox",
            "--bank-csv",
            "/missing-bank",
        ],
        vec![(
            "POST /api/companies/synthetic/agent/original/run HTTP/1.1",
            200,
            v,
        )],
    );
    assert_eq!(bodies.len(), 1);
    let packaged: Value = serde_json::from_slice(&bodies[0]).unwrap();
    assert_eq!(packaged["inbox"]["state"], "failed");
    assert_eq!(packaged["bank"]["state"], "failed");
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("1.234,50 kr."));
    assert!(text.contains("100,00 EUR"));
    let headings = [
        "— Bogført automatisk —",
        "— Kreditorposter afregnet automatisk —",
        "— Exception-kø (kræver et menneske) —",
        "— Kommende deadlines —",
        "=== Kørsel afsluttet ===",
    ];
    let positions: Vec<_> = headings.iter().map(|h| text.find(h).unwrap()).collect();
    assert!(positions.windows(2).all(|p| p[0] < p[1]));
}
#[test]
fn sorted_bytes_sidecars_invalid_values_and_all_or_nothing_packaging() {
    let dir = std::env::temp_dir().join(format!("agent716-files-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, bytes) in [
        ("b.PDF", vec![0, 255, 1]),
        ("a.txt", b"hello".to_vec()),
        (".hidden.txt", vec![1]),
        ("ignored.csv", vec![2]),
    ] {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    // Parsed but semantically invalid metadata is intentionally left for individual server rejection.
    std::fs::write(dir.join("a.json"), "null").unwrap();
    std::fs::write(dir.join("b.json"), "42").unwrap();
    let path = dir.to_str().unwrap();
    let (_, bodies) = run_fixture(
        &["--inbox", path, "--bank-csv", "/missing-bank"],
        vec![(
            "POST /api/companies/synthetic/agent/original/run HTTP/1.1",
            200,
            report(),
        )],
    );
    let body: Value = serde_json::from_slice(&bodies[0]).unwrap();
    let items = body["inbox"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["fileName"], "a.txt");
    assert_eq!(items[0]["fileBase64"], "aGVsbG8=");
    assert_eq!(items[0]["metadata"], Value::Null);
    assert_eq!(items[1]["fileBase64"], "AP8B");
    assert_eq!(items[1]["metadata"], 42);
    assert_eq!(body["bank"]["state"], "failed");
    for broken in [Some("{"), None] {
        if let Some(b) = broken {
            std::fs::write(dir.join("b.json"), b).unwrap();
        } else {
            std::fs::remove_file(dir.join("b.json")).unwrap();
        }
        let (_, bodies) = run_fixture(
            &["--inbox", path],
            vec![(
                "POST /api/companies/synthetic/agent/original/run HTTP/1.1",
                200,
                report(),
            )],
        );
        let body: Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert_eq!(body["inbox"]["state"], "failed");
        assert!(body["inbox"].get("items").is_none());
    }
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn unsupported_protocol_refuses_before_post() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    l.set_nonblocking(true).unwrap();
    let task = thread::spawn(move || {
        let mut s = bounded_accept(&l).unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut manifest: Value =
            serde_json::from_str(include_str!("../../../protocol/http-v1.json")).unwrap();
        manifest["operations"]
            .as_array_mut()
            .unwrap()
            .retain(|op| op["path"] != "/api/companies/{slug}/agent/original/run");
        wire(&mut s, "GET /api/protocol HTTP/1.1", 200, &manifest);
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_millis(150) {
            assert!(
                matches!(l.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock),
                "unexpected POST after failed admission"
            );
            thread::sleep(std::time::Duration::from_millis(5));
        }
    });
    let rt = tokio::runtime::Runtime::new().unwrap();
    let api = rentemester_client::ApiClient::new(&origin, true, Some("session=synthetic".into()))
        .unwrap();
    let result = rt.block_on(
        api.original_agent_run("synthetic", &json!({"asOf":"2026-02-01","confirm":true})),
    );
    assert_eq!(result, Err(rentemester_client::Error::Protocol));
    task.join().unwrap();
}
#[test]
fn invalid_date_and_uninitialized_company_are_flat_business_reports() {
    for message in [
        "--as-of must be a valid YYYY-MM-DD date (got: invalid)",
        "no initialised company at synthetic (run 'rentemester init' first)",
    ] {
        let mut v = report();
        v["ok"] = json!(false);
        v["phases"] = json!([]);
        v["errors"] = json!([message]);
        let (out, _) = run_fixture(
            &["--json"],
            vec![(
                "POST /api/companies/synthetic/agent/original/run HTTP/1.1",
                422,
                v.clone(),
            )],
        );
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), v);
    }
}

#[test]
fn wrong_company_success_is_unknown_without_retry() {
    let mut v = report();
    v["company"] = json!("another-company");
    let (out, bodies) = run_fixture(
        &["--json"],
        vec![(
            "POST /api/companies/synthetic/agent/original/run HTTP/1.1",
            200,
            v,
        )],
    );
    assert_eq!(bodies.len(), 1);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown"));
}

#[test]
fn mismatched_asserted_actor_success_is_unknown_without_retry() {
    let (out, bodies) = run_fixture(
        &["--json", "--actor", "user:expected"],
        vec![(
            "POST /api/companies/synthetic/agent/original/run HTTP/1.1",
            200,
            report(),
        )],
    );
    assert_eq!(bodies.len(), 1);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown"));
}
