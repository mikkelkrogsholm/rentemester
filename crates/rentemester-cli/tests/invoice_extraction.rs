#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use serde_json::{Value, json};
use std::process::Command;
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn canonical_namespace_is_discoverable_and_confirmation_and_selectors_are_strict() {
    for args in [
        vec!["documents", "--help"],
        vec!["documents", "extract-invoice", "--help"],
        vec!["documents", "invoice-extraction", "--help"],
    ] {
        let output = Command::new(BIN).args(args).output().unwrap();
        assert!(output.status.success());
        let help: Value = serde_json::from_slice(&output.stdout).unwrap();
        let help = help.get("invoiceExtraction").unwrap_or(&help);
        assert!(
            help["commands"]
                .as_array()
                .unwrap()
                .iter()
                .all(|command| command.as_str().unwrap().contains("--document-id"))
        );
        assert!(
            help["recovery"]
                .as_str()
                .unwrap()
                .contains("outcome_unknown")
        );
    }
    let output = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let help: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        help["invoiceExtraction"]["commands"],
        json!(["extract-invoice", "invoice-extraction"])
    );
    for (command, extra) in [
        ("extract-invoice", vec!["--document-id", "7"]),
        (
            "extract-invoice",
            vec!["--document-id", "7", "--confirm", "yes"],
        ),
        (
            "invoice-extraction",
            vec!["--document-id", "7", "--confirm"],
        ),
        ("invoice-extraction", vec!["--document-id", "0"]),
        (
            "invoice-extraction",
            vec!["--document-id", "9007199254740992"],
        ),
        ("invoice-extraction", vec!["--document-id", "+7"]),
        (
            "invoice-extraction",
            vec!["--document-id", "7", "--id", "8"],
        ),
        (
            "invoice-extraction",
            vec!["--document-id", "7", "--year", "2026"],
        ),
        (
            "invoice-extraction",
            vec!["--document-id", "7", "--actor", "user:forged"],
        ),
        (
            "invoice-extraction",
            vec!["--document-id", "7", "--document-id", "8"],
        ),
    ] {
        let out = Command::new(BIN)
            .args([
                "documents",
                command,
                "--server",
                "http://127.0.0.1:9",
                "--allow-loopback-http",
                "--company",
                "synthetic",
            ])
            .args(extra)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
    let output = Command::new(BIN)
        .args(["remote", "health", "--document-id", "7"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    // Explicit remote target remains mandatory even through the namespace adapter.
    let output = Command::new(BIN)
        .args([
            "documents",
            "invoice-extraction",
            "--company",
            "synthetic",
            "--document-id",
            "7",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}
#[cfg(unix)]
mod http {
    use super::common::TcpListener;
    use super::*;
    use std::{
        io::{Read, Write},
        os::unix::fs::OpenOptionsExt,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
        thread,
        time::{Duration, Instant},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Cookie(PathBuf);
    impl Cookie {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "rm70-cli-extraction-{}-{}.cookie",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
                .unwrap()
                .write_all(b"session=private-extraction-cli-secret")
                .unwrap();
            Self(path)
        }
    }
    impl Drop for Cookie {
        fn drop(&mut self) {
            std::fs::remove_file(&self.0).unwrap();
        }
    }
    fn endpoint(
        method: &'static str,
        suffix: &'static str,
        status: &'static str,
        body: &'static str,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let task = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(pair) => break pair,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("CLI request missing: {e}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut wire = vec![];
            loop {
                let mut chunk = [0; 4096];
                let n = stream.read(&mut chunk).unwrap();
                assert!(n > 0);
                wire.extend_from_slice(&chunk[..n]);
                assert!(wire.len() < 8192);
                if let Some(end) = wire.windows(4).position(|v| v == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&wire[..end]);
                    let length = headers
                        .lines()
                        .find_map(|s| {
                            s.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(|v| v.parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if wire.len() < end + 4 + length {
                        continue;
                    }
                    assert!(headers.starts_with(&format!(
                        "{method} /api/companies/synthetic/documents/7/{suffix} HTTP/1.1"
                    )));
                    assert!(
                        headers
                            .to_ascii_lowercase()
                            .contains("cookie: session=private-extraction-cli-secret")
                    );
                    assert!(!headers.to_ascii_lowercase().contains("idempotency-key:"));
                    assert_eq!(
                        &wire[end + 4..],
                        if method == "POST" {
                            br#"{"confirm":true}"#.as_slice()
                        } else {
                            b""
                        }
                    );
                    break;
                }
            }
            write!(stream,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection:close\r\n\r\n{body}",body.len()).unwrap();
            drop(stream);
            let listener = listener.into_inner();
            thread::sleep(Duration::from_millis(75));
            assert!(
                listener.accept().is_err(),
                "an extraction request must not be automatically retried"
            );
        });
        (origin, task)
    }
    fn call(command: &str, origin: &str, cookie: &Cookie) -> std::process::Output {
        let mut child = Command::new(BIN);
        child.args([
            "remote",
            command,
            "--server",
            origin,
            "--allow-loopback-http",
            "--cookie-file",
            cookie.0.to_str().unwrap(),
            "--company",
            "synthetic",
            "--document-id",
            "7",
        ]);
        if command == "extract-invoice" {
            child.arg("--confirm");
        }
        child.output().unwrap()
    }
    fn value(output: &std::process::Output) -> Value {
        assert!(!String::from_utf8_lossy(&output.stdout).contains("private-extraction-cli-secret"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private-extraction-cli-secret"));
        serde_json::from_slice(&output.stdout).unwrap()
    }
    #[test]
    fn native_remote_posts_once_and_preserves_conflicts_and_exact_source_fields() {
        const BODY: &str = r#"{"ok":true,"extraction":{"attemptId":7,"status":"needs_resolution","fields":[{"key":"grossAmount","value":"9007199254740993.01","sourceText":"Total 9007199254740993.01","confidence":0.9,"page":1}],"conflicts":["gross amount conflicts with supplied metadata"],"resolutions":[]}}"#;
        let cookie = Cookie::new();
        let (origin, task) = endpoint("POST", "extract-invoice", "200 OK", BODY);
        let output = call("extract-invoice", &origin, &cookie);
        task.join().unwrap();
        assert!(output.status.success());
        assert_eq!(
            value(&output)["data"],
            serde_json::from_str::<Value>(BODY).unwrap()
        );
    }
    #[test]
    fn readonly_namespace_preserves_null_and_current_authority_refusal() {
        for (status, body, exit) in [
            ("200 OK", r#"{"ok":true,"extraction":null}"#, 0),
            (
                "403 Forbidden",
                r#"{"ok":false,"code":"access_denied","errors":["Current permission required"]}"#,
                1,
            ),
        ] {
            let cookie = Cookie::new();
            let (origin, task) = endpoint("GET", "invoice-extraction", status, body);
            let output = call("invoice-extraction", &origin, &cookie);
            task.join().unwrap();
            assert_eq!(output.status.code(), Some(exit));
            assert_eq!(
                value(&output)["data"],
                serde_json::from_str::<Value>(body).unwrap()
            );
        }
    }
    #[test]
    fn off_provider_is_known_and_other_lost_write_results_remain_unknown_without_retry() {
        for (body, known) in [
            (
                r#"{"ok":false,"code":"EXTRACTION_PROVIDER_UNAVAILABLE","errors":["Provider is not configured"]}"#,
                true,
            ),
            (r#"{"ok":false,"code":"temporarily_unavailable"}"#, false),
        ] {
            let cookie = Cookie::new();
            let (origin, task) = endpoint("POST", "extract-invoice", "503 Unavailable", body);
            let output = call("extract-invoice", &origin, &cookie);
            task.join().unwrap();
            assert_eq!(output.status.code(), Some(1));
            let value = value(&output);
            if known {
                assert_eq!(value["data"]["code"], "EXTRACTION_PROVIDER_UNAVAILABLE");
            } else {
                assert_eq!(value["code"], "outcome_unknown");
            }
        }
    }
}
