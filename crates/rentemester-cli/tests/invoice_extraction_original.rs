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
fn once(count: Arc<AtomicUsize>, handle: thread::JoinHandle<()>) {
    handle.join().unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

fn root() -> std::path::PathBuf {
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "invoice-extraction-original-cli-{}-{:?}",
        std::process::id(),
        thread::current().id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}
fn cookie(root: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = root.join("cookie");
    std::fs::write(&path, "session=synthetic").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    path
}
#[test]
fn pure_help_and_usage_never_open_credentials_or_profile() {
    for op in ["extract-invoice", "invoice-extraction"] {
        let output = Command::new(BIN)
            .args([
                "--local-profile",
                "/private/not-opened",
                "documents",
                op,
                "--cookie-file",
                "/private/not-opened-cookie",
                "--help",
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        let help: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(help["commands"].as_array().unwrap().len(), 2);
        for flags in [
            vec!["--document-id", "0"],
            vec!["--document-id", "9007199254740992"],
            vec!["--document-id", "1", "--confirm", "yes"],
            vec!["--document-id", "1", "--principal", "forged"],
            vec!["--document-id", "1", "--json=false"],
        ] {
            let output = Command::new(BIN)
                .args([
                    "documents",
                    op,
                    "--local-profile",
                    "/private/not-opened",
                    "--cookie-file",
                    "/private/not-opened-cookie",
                ])
                .args(flags)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(2));
            assert!(output.stdout.is_empty());
        }
    }
    let output = Command::new(BIN)
        .args(["documents", "--help"])
        .output()
        .unwrap();
    let help: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(help["commands"].as_array().unwrap().len(), 17);
    assert!(
        help["native"]["usage"]
            .as_str()
            .unwrap()
            .starts_with("remote extract-invoice")
    );
}
#[test]
fn invocation_confirms_and_flat_results_are_not_wrapped() {
    let root = root();
    let credential = cookie(&root);
    for (op, response, target, body) in [
        (
            "extract-invoice",
            json!({"ok":true,"extraction":null}),
            "POST /api/companies/synthetic/documents/original/7/extract-invoice ",
            Some(
                serde_json::to_vec(&json!({"confirm":true,"expectedActor":"user:synthetic"}))
                    .unwrap(),
            ),
        ),
        (
            "invoice-extraction",
            json!({"ok":true,"extraction":null}),
            "GET /api/companies/synthetic/documents/original/7/invoice-extraction ",
            None,
        ),
        (
            "extract-invoice",
            json!({"ok":false,"errors":["EXTRACTION_SNAPSHOT_INVALID"]}),
            "POST /api/companies/synthetic/documents/original/7/extract-invoice ",
            Some(
                serde_json::to_vec(&json!({"confirm":true,"expectedActor":"user:synthetic"}))
                    .unwrap(),
            ),
        ),
    ] {
        let (origin, count, handle) = server(200, response.to_string(), target, body);
        let mut cmd = Command::new(BIN);
        cmd.env("RENTEMESTER_COMPANY", "synthetic")
            .env("RENTEMESTER_AGENT", "ignored-legacy-actor")
            .args([
                "--server",
                &origin,
                "documents",
                op,
                "--allow-loopback-http",
                "--cookie-file",
                credential.to_str().unwrap(),
                "--document-id=7",
            ]);
        if op == "extract-invoice" {
            cmd.args(["--actor", "user:synthetic"]);
        }
        let out = cmd.output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(if response["ok"] == true { 0 } else { 1 }),
            "{:?}",
            out
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap(),
            response
        );
        assert!(out.stderr.is_empty());
        once(count, handle);
    }
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn human_streams_and_unknown_mutation_are_not_retried() {
    let root = root();
    let credential = cookie(&root);
    for (status, response, expected_exit) in [
        (200, json!({"ok":true,"extraction":null}), 0),
        (
            200,
            json!({"ok":false,"errors":["EXTRACTION_SNAPSHOT_INVALID"]}),
            1,
        ),
        (500, json!({"ok":true,"extraction":null}), 1),
        (403, json!({"ok":true,"extraction":null}), 1),
    ] {
        let (origin, count, handle) = server(
            status,
            response.to_string(),
            "POST /api/companies/synthetic/documents/original/7/extract-invoice ",
            Some(b"{\"confirm\":true}".to_vec()),
        );
        let out = Command::new(BIN)
            .args([
                "documents",
                "extract-invoice",
                "--server",
                &origin,
                "--allow-loopback-http",
                "--company",
                "synthetic",
                "--cookie-file",
                credential.to_str().unwrap(),
                "--document-id",
                "7",
                "--format",
                "human",
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(expected_exit));
        if expected_exit == 0 {
            assert_eq!(
                String::from_utf8(out.stdout).unwrap(),
                "✔ Udtrækker citerbar fakturaevidens fra et gemt PDF-bilag.\n"
            );
            assert!(out.stderr.is_empty());
        } else {
            assert!(out.stdout.is_empty());
            let err = String::from_utf8(out.stderr).unwrap();
            assert!(err.starts_with("✘ Udtrækker"));
            if status == 500 {
                assert!(err.contains("outcome_unknown"));
            }
        }
        once(count, handle);
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn service_key_uses_existing_auth_and_never_infers_actor() {
    use std::os::unix::fs::PermissionsExt;
    let root = root();
    let key = root.join("service-key");
    std::fs::write(&key, "rm_svc_synthetic_process").unwrap();
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut s = loop {
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline);
                    thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("{e}"),
            }
        };
        s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let n = s.read(&mut chunk).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n")
                && bytes.len() >= end + 4 + b"{\"confirm\":true}".len()
            {
                break;
            }
        }
        let request = String::from_utf8(bytes).unwrap();
        assert!(
            request
                .starts_with("POST /api/companies/synthetic/documents/original/7/extract-invoice ")
        );
        assert!(
            request
                .to_lowercase()
                .contains("x-rentemester-service-key: rm_svc_synthetic_process")
        );
        assert!(request.ends_with("{\"confirm\":true}"));
        assert!(!request.contains("expectedActor"));
        let response = r#"{"ok":true,"extraction":null}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
            response.len()
        )
        .unwrap();
    });
    let out = Command::new(BIN)
        .env("RENTEMESTER_AGENT", "ignored")
        .args([
            "documents",
            "extract-invoice",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--company",
            "synthetic",
            "--service-key-file",
            key.to_str().unwrap(),
            "--document-id",
            "7",
            "--json",
        ])
        .output()
        .unwrap();
    task.join().unwrap();
    assert_eq!(out.status.code(), Some(0), "{:?}", out);
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        json!({"ok":true,"extraction":null})
    );
    std::fs::remove_dir_all(root).unwrap();
}
