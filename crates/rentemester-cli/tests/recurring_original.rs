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
            "rm154-recurring-cli-{}-{}",
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

fn call(command: &str, origin: &str, f: &Fixture, extra: &[&str], service: bool) -> Output {
    Command::new(BIN)
        .args([
            "recurring-invoice",
            command,
            "--server",
            origin,
            "--allow-loopback-http",
            "--company",
            "synthetic",
            if service {
                "--service-key-file"
            } else {
                "--cookie-file"
            },
            f.0.join(if service { "key" } else { "cookie" })
                .to_str()
                .unwrap(),
            "--json",
        ])
        .args(extra)
        .output()
        .unwrap()
}
#[test]
fn original_recurring_cli_source_shapes_flat_output_and_service_authority() {
    let f = Fixture::new();
    let input = f.0.join("template.json");
    let payload = json!({"name":"Synthetic subscription","interval":"monthly","firstIssueDate":"2024-01-31","invoice":{"invoiceType":"full","seller":{"name":"Synthetic Seller","address":"S1","vatOrCvr":"87654321"},"buyer":{"name":"Synthetic Buyer","address":"S2"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100,"lineTotalExVat":100}],"totals":{"netAmount":100,"vatRate":0.25,"vatAmount":25,"grossAmount":125}}});
    fs::write(&input, payload.to_string()).unwrap();
    let expected = json!({"ok":true,"templateId":7,"appliedRules":["DK-RECURRING-INVOICE-TEMPLATE-001"],"errors":[]});
    let (origin, t) = server(200, expected.clone());
    let output = call(
        "create",
        &origin,
        &f,
        &["--input", input.to_str().unwrap()],
        true,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        expected
    );
    let (header, sent) = t.join().unwrap();
    assert!(
        header.starts_with("POST /api/companies/synthetic/recurring-invoices/original/create ")
    );
    assert!(
        header
            .to_lowercase()
            .contains("x-rentemester-service-key: synthetic-key")
    );
    assert!(!header.to_lowercase().contains("\r\ncookie:"));
    let mut confirmed = payload;
    confirmed["confirm"] = json!(true);
    assert_eq!(sent, confirmed);
    let expected = json!({"ok":true,"created":false,"templateId":7,"periodIndex":1,"documentId":11,"invoiceNumber":"2024-0001","issueDate":"2024-02-29","dueDate":"2024-03-30","appliedRules":["DK-RECURRING-INVOICE-GENERATE-001"],"errors":[]});
    let (origin, t) = server(200, expected.clone());
    let output = call(
        "generate",
        &origin,
        &f,
        &["--template-id", "7", "--as-of", "2024-02-29"],
        false,
    );
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        expected
    );
    let (header, sent) = t.join().unwrap();
    assert!(
        header.starts_with("POST /api/companies/synthetic/recurring-invoices/original/generate ")
    );
    assert_eq!(
        sent,
        json!({"templateId":7,"asOfDate":"2024-02-29","confirm":true})
    );
    let expected = json!({"ok":true,"count":0,"rows":[],"errors":[]});
    let (origin, t) = server(200, expected.clone());
    let output = call("list", &origin, &f, &["--include-inactive"], false);
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        expected
    );
    assert!(t.join().unwrap().0.starts_with(
        "GET /api/companies/synthetic/recurring-invoices/original/list?includeInactive=true "
    ));
}
#[test]
fn original_recurring_cli_business_usage_and_human_list_are_distinct() {
    let f = Fixture::new();
    let expected = json!({"ok":false,"appliedRules":["DK-RECURRING-INVOICE-GENERATE-001"],"errors":["template is inactive"]});
    let (origin, t) = server(200, expected.clone());
    let output = call(
        "generate",
        &origin,
        &f,
        &["--template-id", "7", "--as-of", "2024-02-29"],
        false,
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        expected
    );
    assert!(output.stderr.is_empty());
    t.join().unwrap();
    for (command, args) in [
        ("create", vec![]),
        (
            "generate",
            vec!["--template-id", "0", "--as-of", "2024-01-01"],
        ),
        ("list", vec!["--confirm", "yes"]),
        ("list", vec!["--actor", "forged"]),
        ("list", vec!["--include-inactive", "--include-inactive"]),
    ] {
        let output = call(command, "http://127.0.0.1:9", &f, &args, false);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    let (origin, t) = server(200, json!({"ok":true,"count":0,"rows":[],"errors":[]}));
    let output = Command::new(BIN)
        .args([
            "recurring-invoice",
            "list",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--company",
            "synthetic",
            "--cookie-file",
            f.0.join("cookie").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Recurring invoice templates (0)\nNo recurring invoice templates.\n"
    );
    t.join().unwrap();
}

#[test]
fn workspace_recurring_cli_keeps_partial_results_confirmation_and_usage() {
    let f = Fixture::new();
    let partial = json!({"ok":false,"companies":[{"slug":"blocked","status":"failed","reason":"run failed"},{"slug":"next","status":"processed","generated":2,"attempted":0,"hasMore":true,"remainingGenerations":3}],"errors":["recurring run failed for company blocked"],"hasMore":true,"remainingGenerations":3,"continuation":{"remainingGenerations":3,"companies":[{"slug":"next","remainingGenerations":3}]}});
    let (origin, t) = server(200, partial.clone());
    let run = |origin: &str, extra: &[&str]| {
        Command::new(BIN)
            .args([
                "recurring-invoice",
                "run-workspace",
                "--server",
                origin,
                "--allow-loopback-http",
                "--cookie-file",
                f.0.join("cookie").to_str().unwrap(),
                "--json",
            ])
            .args(extra)
            .output()
            .unwrap()
    };
    let output = run(
        &origin,
        &[
            "--workspace",
            "/configured/workspace",
            "--as-of",
            "2024-07-31",
            "--max-generations",
            "2",
            "--confirm",
            " YES ",
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        partial
    );
    let (headers, body) = t.join().unwrap();
    assert!(headers.starts_with("POST /api/recurring-invoices/run-workspace "));
    assert!(headers.to_lowercase().contains("cookie: session=synthetic"));
    assert_eq!(
        body,
        json!({"workspace":"/configured/workspace","asOfDate":"2024-07-31","maxGenerations":2,"confirm":true})
    );
    for confirm in [vec![], vec!["--confirm", "no"]] {
        let output = run("http://127.0.0.1:9", &confirm);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stderr.is_empty());
        assert!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap()["errors"][0]
                .as_str()
                .unwrap()
                .contains("--confirm yes")
        );
    }
    for extra in [
        vec!["--confirm", "yes"],
        vec![
            "--workspace",
            "w",
            "--as-of",
            "2024-01-01",
            "--confirm",
            "yes",
            "--company",
            "forged",
        ],
        vec![
            "--workspace",
            "w",
            "--as-of",
            "2024-01-01",
            "--confirm",
            "yes",
            "--max-generations",
            "0",
        ],
        vec![
            "--workspace",
            "w",
            "--as-of",
            "2024-01-01",
            "--confirm",
            "yes",
            "--service-key-file",
            "key",
        ],
    ] {
        let output = run("http://127.0.0.1:9", &extra);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn original_recurring_cli_transport422_preserves_only_valid_receipts() {
    let f = Fixture::new();
    let receipt = json!({"ok":false,"code":"SERVER_RECURRING_POLICY","errors":["template does not exist"],"appliedRules":["recurring-policy"]});
    for command in ["generate", "list"] {
        for (status, body, valid) in [
            (422, receipt.clone(), true),
            (200, receipt.clone(), true),
            (403, receipt.clone(), false),
            (422, json!({"ok":false,"errors":["private marker"]}), false),
            (
                422,
                json!({"ok":false,"code":" ","errors":["private marker"]}),
                false,
            ),
            (
                422,
                json!({"ok":false,"code":"BAD","errors":["private marker"],"data":{}}),
                false,
            ),
            (
                422,
                json!({"ok":true,"templateId":7,"appliedRules":[],"errors":[]}),
                false,
            ),
        ] {
            let (origin, t) = server(status, body.clone());
            let args = if command == "generate" {
                vec!["--template-id", "7", "--as-of", "2024-02-29"]
            } else {
                vec![]
            };
            let output = call(command, &origin, &f, &args, false);
            assert_eq!(output.status.code(), Some(1));
            assert!(output.stderr.is_empty());
            let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
            if valid {
                assert_eq!(actual, body);
            } else {
                assert_eq!(actual["ok"], false);
                assert!(actual.get("data").is_none());
                assert!(!String::from_utf8_lossy(&output.stdout).contains("private marker"));
                assert_ne!(actual["code"], "SERVER_RECURRING_POLICY");
            }
            let (header, sent) = t.join().unwrap();
            if command == "list" {
                assert!(header.starts_with("GET "));
                assert_eq!(sent, Value::Null);
            }
        }
    }
}
