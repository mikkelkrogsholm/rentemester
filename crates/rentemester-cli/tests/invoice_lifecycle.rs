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
            "rm148-cli-{}-{}",
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

fn call(kind: &str, origin: &str, f: &Fixture, extra: &[&str], input: Option<&str>) -> Output {
    let mut command = Command::new(BIN);
    command
        .args([
            "invoice",
            kind,
            "--server",
            origin,
            "--allow-loopback-http",
            "--company",
            "synthetic",
            "--cookie-file",
            f.0.join("cookie").to_str().unwrap(),
        ])
        .args(extra);
    if let Some(input) = input {
        let mut child = command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    } else {
        command.output().unwrap()
    }
}
#[test]
fn original_seven_commands_keep_raw_file_stdin_create_essentials_and_server_results() {
    let f = Fixture::new();
    let payload = json!({"invoiceType":"full","issueDate":"2026-01-01","lines":[{"description":"Synthetic","quantity":2,"unitPriceExVat":50,"lineTotalExVat":100}],"totals":{"netAmount":100,"vatRate":0.25,"vatAmount":25,"grossAmount":125}});
    let path = f.0.join("payload.json");
    fs::write(&path, payload.to_string()).unwrap();
    for (kind, extra, input) in [
        (
            "issue",
            vec!["--input", path.to_str().unwrap(), "--customer-id", "2"],
            None,
        ),
        (
            "create",
            vec![
                "--issue-date",
                "2026-01-01",
                "--line",
                "Synthetic|2|50;Second|1|10",
                "--buyer-name",
                " Synthetic buyer ",
                "--seller-vat",
                " DK123 ",
            ],
            None,
        ),
        (
            "render",
            vec!["--document-id", "7", "--invoice-number", "fallback"],
            None,
        ),
        (
            "post",
            vec![
                "--document-id",
                "7",
                "--revenue-account",
                "synthetic-income",
            ],
            None,
        ),
        (
            "settle-bank",
            vec!["--input", "-"],
            Some("{\"invoiceDocumentId\":7,\"bankTransactionReference\":\"in-ref\",\"amount\":50}"),
        ),
        (
            "credit-note",
            vec!["--input", "-"],
            Some(
                "{\"originalInvoiceNumber\":\"INV-7\",\"issueDate\":\"2026-01-02\",\"reason\":\"credit\",\"grossAmount\":25}",
            ),
        ),
        (
            "refund-bank",
            vec!["--input", "-"],
            Some(
                "{\"invoiceDocumentId\":7,\"bankTransactionId\":11,\"refundDate\":\"2026-01-03\",\"amount\":25}",
            ),
        ),
    ] {
        let data = json!({"documentId":7,"entryId":8,"entryNo":9,"pdfStoredPath":"/api/source.pdf","computed":{"net":110,"vatRate":0.25,"vatRatePercent":25,"vat":27.5,"gross":137.5}});
        let (origin, t) = server(
            200,
            json!({"ok":true,"data":data,"errors":[],"appliedRules":["server-rule"]}),
        );
        let mut args = extra;
        args.push("--json");
        let out = call(kind, &origin, &f, &args, input);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let mut expected = data;
        expected["ok"] = json!(true);
        expected["errors"] = json!([]);
        expected["appliedRules"] = json!(["server-rule"]);
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap(),
            expected
        );
        let (header, body) = t.join().unwrap();
        assert!(header.starts_with(&format!(
            "POST /api/companies/synthetic/invoices/original/{kind} "
        )));
        assert_eq!(body["confirm"], true);
        if kind == "issue" {
            assert_eq!(body["payload"], payload);
        }
        if kind == "create" {
            assert_eq!(body["payload"]["vatRatePercent"], 25);
            assert_eq!(
                body["payload"]["lines"][0],
                json!({"description":"Synthetic","quantity":2,"unitPriceExVat":50})
            );
            assert!(body["payload"].get("totals").is_none());
            assert_eq!(body["payload"]["buyer"]["name"], "Synthetic buyer");
        }
        if kind == "post" {
            assert_eq!(body["revenueAccountNo"], "synthetic-income");
        }
        if let Some(input) = input {
            assert_eq!(
                body["payload"],
                serde_json::from_str::<Value>(input).unwrap()
            );
        }
    }
}
#[test]
fn malformed_input_forbidden_flags_and_server_business_refusals_preserve_exits() {
    let f = Fixture::new();
    for (kind, args, input) in [
        ("issue", vec!["--input", "-"], Some("[]")),
        ("issue", vec!["--input", "-"], Some("not json")),
        ("issue", vec!["--input", "missing"], None),
        ("post", vec!["--document-id", "7", "--confirm", "yes"], None),
        (
            "post",
            vec!["--document-id", "7", "--idempotency-key", "key"],
            None,
        ),
        ("render", vec![], None),
    ] {
        let out = call(kind, "http://127.0.0.1:9", &f, &args, input);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
    }
    let value =
        json!({"ok":false,"code":"account_selection_required","errors":["Select revenue account"]});
    let (origin, t) = server(200, value.clone());
    let out = call("post", &origin, &f, &["--document-id", "7", "--json"], None);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), value);
    assert!(out.stderr.is_empty());
    let (_, body) = t.join().unwrap();
    assert!(body.get("revenueAccountNo").is_none());
    for kind in [
        "issue",
        "create",
        "render",
        "post",
        "settle-bank",
        "credit-note",
        "refund-bank",
    ] {
        assert_eq!(
            Command::new(BIN)
                .args(["invoice", kind, "--help"])
                .output()
                .unwrap()
                .status
                .code(),
            Some(0)
        );
    }
}
