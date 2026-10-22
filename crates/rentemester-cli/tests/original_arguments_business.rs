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
            "rm158-business-{}-{}",
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
fn server(status: u16, value: Value) -> (String, thread::JoinHandle<(String, Value)>) {
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
        let header = String::from_utf8_lossy(&bytes[..end]).to_string();
        let length: usize = header
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse().unwrap())
            })
            .unwrap_or(0);
        while bytes.len() < end + length {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
        }
        let body = if length == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[end..end + length]).unwrap()
        };
        let response = value.to_string();
        write!(
            s,
            "HTTP/1.1 {status} Mock\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{response}",
            response.len()
        )
        .unwrap();
        (header, body)
    });
    (origin, t)
}

// The same small socket fixture as the family suites; argv shape is the only
// variable. Existing family suites retain the detailed refusal/body assertions.
fn invoke(
    root: &[&str],
    origin: &str,
    f: &Fixture,
    extra: &[&str],
    shape: usize,
    human: bool,
) -> Output {
    let cookie = f.0.join("cookie").to_str().unwrap().to_owned();
    let globals: Vec<String> = if shape == 3 {
        vec![
            format!("--server={origin}"),
            format!("--cookie-file={cookie}"),
            "--company=synthetic".into(),
            "--allow-loopback-http".into(),
        ]
    } else {
        [
            "--server",
            origin,
            "--cookie-file",
            &cookie,
            "--company",
            "synthetic",
            "--allow-loopback-http",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    };
    let mut args: Vec<String> = root.iter().map(|s| (*s).into()).collect();
    let position = match shape {
        1 => 0,
        2 => 1,
        _ => args.len(),
    };
    args.splice(position..position, globals);
    args.extend(extra.iter().map(|s| (*s).to_owned()));
    if shape == 0 && !human {
        args.push("--json".into());
    } else {
        args.extend([
            "--format".into(),
            if human { "human" } else { "json" }.into(),
        ]);
    }
    Command::new(BIN)
        .args(args)
        .env_remove("RENTEMESTER_COMPANY")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap()
}
fn success(out: &Output) {
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty());
}

#[test]
fn bank_read_global_positions_and_attached_literal_filters_keep_one_request() {
    let f = Fixture::new();
    for human in [false, true] {
        let mut canonical = None;
        for shape in 0..4 {
            let (origin, task) = server(
                200,
                json!({"ok":true,"data":{"rows":[],"count":0},"errors":[]}),
            );
            let filters = if shape == 3 {
                vec!["--amount=-12.34", "--text-match=--help=a=b"]
            } else {
                vec!["--amount", "-12.34", "--text-match=--help=a=b"]
            };
            let out = invoke(&["bank", "list"], &origin, &f, &filters, shape, human);
            success(&out);
            let (request, body) = task.join().unwrap();
            assert!(
                request.starts_with("GET /api/companies/synthetic/bank/transactions?"),
                "{request}"
            );
            assert!(request.contains("amount=-12.34"));
            assert!(request.contains("textMatch=--help%3Da%3Db"), "{request}");
            assert_eq!(body, Value::Null);
            if let Some(expected) = &canonical {
                assert_eq!(&out.stdout, expected);
            } else {
                canonical = Some(out.stdout);
            }
        }
    }
}

#[test]
fn confirmed_bank_write_preserves_literal_empty_and_equals_values() {
    let f = Fixture::new();
    for shape in 0..4 {
        let (origin, task) = server(
            200,
            json!({"ok":true,"data":{"id":1,"idempotent":false},"errors":[]}),
        );
        let out = invoke(
            &["bank", "link-journal"],
            &origin,
            &f,
            &[
                "--bank-transaction-id=7",
                "--journal-entry-id=8",
                "--match-method=manual-review",
                "--source-reference=",
                "--note=--help=a=b",
                "--confirm=yes",
            ],
            shape,
            false,
        );
        success(&out);
        let (request, body) = task.join().unwrap();
        assert!(request.starts_with("POST "), "{request}");
        assert_eq!(
            body,
            json!({"confirm":true,"bankTransactionId":7,"journalEntryId":8,"matchMethod":"manual-review","sourceReference":"","note":"--help=a=b"})
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap(),
            json!({"id":1,"idempotent":false,"ok":true,"errors":[]})
        );
    }
}

#[test]
fn payable_report_and_journal_keep_original_shapes_across_global_positions() {
    let f = Fixture::new();
    for (root, extra, response, expected) in [
        (
            vec!["payable", "list"],
            vec!["--supplier=--help", "--status=open"],
            json!({"ok":true,"data":{"count":0,"rows":[],"totalOpenBalance":0,"overdueOpenBalance":0,"notYetDueOpenBalance":0,"asOfDate":"2026-04-01"},"errors":[]}),
            json!({"ok":true,"count":0,"rows":[],"totalOpenBalance":0,"overdueOpenBalance":0,"notYetDueOpenBalance":0,"asOfDate":"2026-04-01","errors":[]}),
        ),
        (
            vec!["report", "trial-balance"],
            vec!["--from=2026-01-01", "--to=2026-02-01"],
            json!({"ok":true,"periodStart":"2026-01-01","periodEnd":"2026-02-01","accounts":[],"totalDebit":0,"totalCredit":0,"balanced":true,"linesConsidered":0,"journalEntryCount":0,"errors":[],"appliedRules":["financial-statements:trial-balance"]}),
            json!({"ok":true,"periodStart":"2026-01-01","periodEnd":"2026-02-01","accounts":[],"totalDebit":0,"totalCredit":0,"balanced":true,"linesConsidered":0,"journalEntryCount":0,"errors":[],"appliedRules":["financial-statements:trial-balance"]}),
        ),
        (
            vec!["journal", "list"],
            vec![],
            json!({"ok":true,"journalHistory":{"entries":[],"total":0,"count":0,"limit":0,"offset":0,"hasMore":false}}),
            json!([]),
        ),
    ] {
        for human in [false, true] {
            let mut canonical = None;
            for shape in 0..4 {
                let (origin, task) = server(200, response.clone());
                let out = invoke(&root, &origin, &f, &extra, shape, human);
                success(&out);
                let (request, body) = task.join().unwrap();
                assert!(request.starts_with("GET "));
                assert_eq!(body, Value::Null);
                if !human {
                    assert_eq!(
                        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
                        expected
                    );
                }
                if let Some(expected) = &canonical {
                    assert_eq!(&out.stdout, expected);
                } else {
                    canonical = Some(out.stdout);
                }
            }
        }
    }
}

#[test]
fn every_owned_family_help_and_malformed_arguments_are_offline() {
    for root in [
        ["bank", "list"],
        ["reconcile", "bank"],
        ["bank-account", "list"],
        ["bank", "suggest-matches"],
        ["bank", "link-journal"],
        ["bank", "correction-plan"],
        ["bank", "correction-apply"],
        ["bank", "direct-payable-plan"],
        ["bank", "direct-payable-apply"],
        ["bank", "import"],
        ["bank-account", "add"],
        ["bank-account", "update"],
        ["payable", "register"],
        ["payable", "pay"],
        ["payable", "list"],
        ["report", "trial-balance"],
        ["report", "profit-loss"],
        ["report", "balance"],
        ["report", "annual"],
        ["report", "tax"],
        ["journal", "list"],
        ["system", "export-saft"],
        ["system", "export-authority"],
        ["system", "export-accountant"],
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        // A help-looking token after the terminator is literal data. These
        // families accept no positional tail, so refuse before credentials/I/O.
        let out = Command::new(BIN)
            .args(root)
            .args([
                "--company",
                "synthetic",
                "--server",
                &origin,
                "--cookie-file",
                "/nonexistent/private",
                "--allow-loopback-http",
                "--",
                "--help",
            ])
            .env_remove("RENTEMESTER_COMPANY")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{root:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        if root == ["journal", "list"] {
            // Journal retains its established native run() error envelope.
            assert!(out.stderr.is_empty());
            let error: Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(error["ok"], false);
            assert_eq!(error["code"], "invalid_input");
        } else {
            assert!(out.stdout.is_empty());
            assert!(!out.stderr.is_empty());
        }
        assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
        for extra in [
            vec!["--help"],
            vec!["--unknown=value"],
            vec!["--format=xml"],
            vec!["--company="],
            vec!["--json=yes"],
        ] {
            let out = Command::new(BIN)
                .args(root)
                .args(&extra)
                .args([
                    "--server",
                    &origin,
                    "--cookie-file",
                    "never-read",
                    "--allow-loopback-http",
                ])
                .env_remove("RENTEMESTER_COMPANY")
                .stdin(std::process::Stdio::null())
                .output()
                .unwrap();
            assert_eq!(
                out.status.code(),
                Some(if extra == ["--help"] { 0 } else { 2 }),
                "{root:?} {extra:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(
                matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock)
            );
        }
    }
}
