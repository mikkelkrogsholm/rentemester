use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-mcp");
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Cookie(PathBuf);
impl Cookie {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rm-mcp-{}-{}.cookie",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(&path)
            .unwrap()
            .write_all(b"session=never-print-this\n")
            .unwrap();
        Self(path)
    }
}
impl Drop for Cookie {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn run(origin: &str, messages: &[Value]) -> Output {
    let lines: String = messages.iter().map(|v| format!("{v}\n")).collect();
    run_raw(origin, lines.as_bytes())
}
fn run_raw(origin: &str, bytes: &[u8]) -> Output {
    let cookie = Cookie::new();
    let mut child = Command::new(BIN)
        .args([
            "--server",
            origin,
            "--allow-loopback-http",
            "--cookie-file",
            cookie.0.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    child.wait_with_output().unwrap()
}
fn parsed(output: &Output) -> Vec<Value> {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("never-print-this"));
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn init(version: &str) -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"test","version":"1"}}})
}
fn ready() -> Value {
    json!({"jsonrpc":"2.0","method":"notifications/initialized"})
}
fn call(id: i32, name: &str, args: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}})
}
fn http(response: &'static str) -> (String, mpsc::Receiver<Vec<u8>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (send, recv) = mpsc::channel();
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut buffer = [0; 4096];
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
            if let Some(index) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..index]).to_lowercase();
                let length = headers
                    .lines()
                    .find_map(|v| v.strip_prefix("content-length: "))
                    .map(|v| v.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= index + 4 + length {
                    break;
                }
            }
        }
        send.send(bytes).unwrap();
        stream.write_all(response.as_bytes()).unwrap();
    });
    (origin, recv, task)
}
#[test]
fn lifecycle_negotiation_discovery_notifications_and_validation() {
    let output = run(
        "http://127.0.0.1:9",
        &[
            json!({"jsonrpc":"2.0","id":"pre","method":"tools/list"}),
            init("2099-01-01"),
            json!({"jsonrpc":"2.0","id":"waiting","method":"tools/list"}),
            ready(),
            json!({"jsonrpc":"2.0","method":"notifications/unknown"}),
            json!({"jsonrpc":"2.0","id":2,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}),
            call(4, "not_a_tool", json!({})),
            call(5, "me", json!({"bad":true})),
            call(6, "me", json!([])),
            call(7, "journal", json!({"company":"demo","year":null})),
            json!({"jsonrpc":"2.0","id":2,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":8,"method":"unknown"}),
        ],
    );
    let values = parsed(&output);
    assert_eq!(values.len(), 11);
    assert_eq!(values[0]["error"]["code"], -32002);
    assert_eq!(values[1]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(values[1]["result"]["capabilities"], json!({"tools":{}}));
    assert_eq!(values[2]["error"]["code"], -32002);
    assert_eq!(values[3]["result"], json!({}));
    let tools = values[4]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 19);
    let write = tools.iter().find(|v| v["name"] == "post_journal").unwrap();
    assert_eq!(
        write["inputSchema"]["required"],
        json!(["company", "envelope"])
    );
    assert_eq!(write["annotations"]["readOnlyHint"], false);
    for value in &values[5..9] {
        assert_eq!(value["error"]["code"], -32602);
    }
    assert_eq!(values[9]["error"]["code"], -32600);
    assert_eq!(values[10]["error"]["code"], -32601);
}
#[test]
fn jsonrpc_framing_rejects_batches_parse_errors_and_invalid_ids() {
    let output = run_raw(
        "http://127.0.0.1:9",
        b"not-json\n[]\n{\"jsonrpc\":\"2.0\",\"id\":null,\"method\":\"ping\"}\n",
    );
    let values = parsed(&output);
    assert_eq!(values.len(), 3);
    assert_eq!(values[0]["error"]["code"], -32700);
    assert_eq!(values[1]["error"]["code"], -32600);
    assert_eq!(values[2]["error"]["code"], -32600);
}
#[test]
fn authenticated_http_report_crosses_real_subprocess_boundary() {
    let (origin, request, task) = http(
        "HTTP/1.1 200 OK\r\nContent-Length: 21\r\nConnection: close\r\n\r\n{\"total\":\"123456.01\"}",
    );
    let output = run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "trial_balance",
                json!({"company":"demo","year":"2026","account":"1000 &x=2"}),
            ),
        ],
    );
    let values = parsed(&output);
    assert_eq!(values[1]["result"]["isError"], false);
    assert_eq!(
        values[1]["result"]["structuredContent"]["data"]["total"],
        "123456.01"
    );
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(wire.starts_with(
        "GET /api/companies/demo/trial-balance?year=2026&account=1000+%26x%3D2 HTTP/1.1"
    ));
    assert!(
        wire.to_lowercase()
            .contains("\r\ncookie: session=never-print-this\r\n")
    );
    task.join().unwrap();
}
#[test]
fn missing_or_invalid_auth_is_clear_and_secret_free() {
    let output = Command::new(BIN)
        .args(["--server", "http://127.0.0.1:9", "--allow-loopback-http"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--cookie-file"));
    let (origin, request, task) = http(
        "HTTP/1.1 401 Unauthorized\r\nContent-Length: 16\r\nConnection: close\r\n\r\nnever-print-this",
    );
    let output = run(
        &origin,
        &[init("2025-11-25"), ready(), call(2, "me", json!({}))],
    );
    let values = parsed(&output);
    let error = &values[1]["result"];
    assert_eq!(error["isError"], true);
    assert_eq!(
        error["structuredContent"]["error"]["code"],
        "server_refused"
    );
    assert_eq!(error["structuredContent"]["error"]["status"], 401);
    request.recv().unwrap();
    task.join().unwrap();
}
#[test]
fn uncertain_journal_preserves_exact_envelope_and_original_key() {
    let (origin, request, task) =
        http("HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let envelope = "{ \"confirm\":true,\"input\":{\"idempotencyKey\":\"original-key\",\"request\":{\"amount\":\"9007199254740993.01\"}} }";
    let output = run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "post_journal",
                json!({"company":"demo","envelope":envelope}),
            ),
        ],
    );
    let values = parsed(&output);
    let error = &values[1]["result"]["structuredContent"]["error"];
    assert_eq!(error["code"], "outcome_unknown");
    assert_eq!(error["idempotencyKey"], "original-key");
    assert!(request.recv().unwrap().ends_with(envelope.as_bytes()));
    task.join().unwrap();
}
#[test]
fn malformed_mutation_never_contacts_server_and_notification_never_executes_write() {
    let output = run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":"post_journal","arguments":{"company":"demo","envelope":"{}"}}}),
            call(
                2,
                "reverse_journal",
                json!({"company":"demo","envelope":"{}"}),
            ),
        ],
    );
    let values = parsed(&output);
    assert_eq!(values.len(), 2);
    assert_eq!(values[1]["result"]["isError"], true);
    assert_eq!(
        values[1]["result"]["structuredContent"]["error"]["code"],
        "invalid_input"
    );
}

#[test]
fn domain_discovery_is_strict_and_preview_attestation_is_explicit() {
    let output = run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            call(
                3,
                "issue_invoice",
                json!({"company":"demo","body":"{}","confirm":true}),
            ),
            call(
                4,
                "create_customer",
                json!({"company":"demo","body":"{\"name\":\"Synthetic\"}","confirm":false}),
            ),
            call(5, "documents", json!({"company":"demo","year":"2026"})),
            call(
                6,
                "update_vendor",
                json!({"company":"demo","id":null,"body":"{}","confirm":true}),
            ),
        ],
    );
    let values = parsed(&output);
    let tools = values[1]["result"]["tools"].as_array().unwrap();
    let issue = tools.iter().find(|t| t["name"] == "issue_invoice").unwrap();
    assert_eq!(
        issue["inputSchema"]["properties"]["previewReviewed"]["const"],
        true
    );
    assert_eq!(
        issue["inputSchema"]["properties"]["body"]["contentSchema"]["additionalProperties"],
        false
    );
    assert_eq!(issue["annotations"]["idempotentHint"], false);
    let preview = tools
        .iter()
        .find(|t| t["name"] == "preview_invoice")
        .unwrap();
    assert_eq!(preview["annotations"]["readOnlyHint"], true);
    for result in &values[2..] {
        assert_eq!(result["error"]["code"], -32602);
    }
}
#[test]
fn domain_write_forwards_exact_body_without_confirmation_injection() {
    let (origin, request, task) = http(
        "HTTP/1.1 200 OK\r\nContent-Length: 31\r\nConnection: close\r\n\r\n{\"ok\":true,\"customer\":{\"id\":7}}",
    );
    let body = "{ \"name\":\"Synthetic MCP buyer\", \"email\":null }";
    let output = run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "create_customer",
                json!({"company":"demo","body":body,"confirm":true}),
            ),
        ],
    );
    let values = parsed(&output);
    assert_eq!(
        values[1]["result"]["structuredContent"]["data"]["customer"]["id"],
        7
    );
    let wire = request.recv().unwrap();
    assert!(wire.starts_with(b"POST /api/companies/demo/customers "));
    assert!(wire.ends_with(body.as_bytes()));
    task.join().unwrap();
}
