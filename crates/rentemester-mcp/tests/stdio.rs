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
    // Drain stdout while feeding stdin: discovery and document frames can exceed pipe capacity.
    let bytes = bytes.to_vec();
    let mut stdin = child.stdin.take().unwrap();
    let writer = thread::spawn(move || stdin.write_all(&bytes));
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap().unwrap();
    output
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
    assert_eq!(tools.len(), 65);
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
            call(
                7,
                "delete_customer",
                json!({"company":"demo","body":"{\"confirm\":true}","confirm":true}),
            ),
            call(
                8,
                "delete_vendor",
                json!({"company":"demo","id":7,"body":"{\"confirm\":true}","confirm":false}),
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
    for name in ["delete_customer", "delete_vendor"] {
        let deletion = tools.iter().find(|t| t["name"] == name).unwrap();
        assert!(
            deletion["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("id"))
        );
        assert_eq!(
            deletion["inputSchema"]["properties"]["body"]["contentSchema"]["properties"]["confirm"]
                ["const"],
            true
        );
        assert_eq!(deletion["annotations"]["destructiveHint"], true);
        assert_eq!(deletion["annotations"]["idempotentHint"], false);
    }
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
#[test]
fn bank_payables_tools_publish_strict_actual_schemas_and_require_confirmation() {
    let output = run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            call(
                3,
                "pay_payable",
                json!({"company":"demo","body":"{}","confirm":true}),
            ),
            call(4, "bank_accounts", json!({"company":"demo","year":"2026"})),
            call(
                5,
                "register_payable",
                json!({"company":"demo","body":"{}","confirm":false}),
            ),
            call(6, "payables", json!({"company":"demo","status":null})),
            call(
                7,
                "expense_booking_options",
                json!({"company":"demo","id":0}),
            ),
        ],
    );
    let values = parsed(&output);
    let tools = values[1]["result"]["tools"].as_array().unwrap();
    let import = tools.iter().find(|t| t["name"] == "import_bank").unwrap();
    assert_eq!(
        import["inputSchema"]["properties"]["body"]["contentSchema"]["required"],
        json!(["csvContent", "confirm"])
    );
    let pay = tools.iter().find(|t| t["name"] == "pay_payable").unwrap();
    assert_eq!(pay["inputSchema"]["properties"]["id"]["minimum"], 1);
    assert_eq!(pay["annotations"]["idempotentHint"], false);
    for result in &values[2..] {
        assert_eq!(result["error"]["code"], -32602);
    }
}
#[test]
fn supplier_payment_mcp_keeps_exact_body_and_target() {
    let (origin, request, task) =
        http("HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}");
    let body = "{ \"bankTransactionId\":9, \"confirm\":true }";
    let output = run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "pay_payable",
                json!({"company":"demo","id":7,"body":body,"confirm":true}),
            ),
        ],
    );
    let values = parsed(&output);
    assert_eq!(values[1]["result"]["isError"], false);
    let wire = request.recv().unwrap();
    assert!(wire.starts_with(b"POST /api/companies/demo/payables/7/pay "));
    assert!(wire.ends_with(body.as_bytes()));
    task.join().unwrap();
}
#[test]
fn period_tools_publish_proof_contract_and_reject_unconfirmed_or_mixed_args() {
    let output = run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            call(
                3,
                "review_period",
                json!({"company":"demo","body":"{}","confirm":false}),
            ),
            call(4, "periods", json!({"company":"demo","year":"2025"})),
            call(
                5,
                "period_close_readiness",
                json!({"company":"demo","from":"2025-01-01"}),
            ),
        ],
    );
    let values = parsed(&output);
    let tools = values[1]["result"]["tools"].as_array().unwrap();
    let close = tools.iter().find(|t| t["name"] == "close_period").unwrap();
    assert_eq!(
        close["inputSchema"]["properties"]["body"]["contentSchema"]["properties"]["reviewId"]["minimum"],
        1
    );
    assert_eq!(close["annotations"]["idempotentHint"], false);
    for v in &values[2..] {
        assert_eq!(v["error"]["code"], -32602);
    }
}
#[test]
fn period_close_mcp_preserves_exact_original_proof() {
    let (origin, request, task) =
        http("HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}");
    let body = r#"{ "periodStart":"2025-01-01","periodEnd":"2025-03-31","reviewId":9,"packetHash":"reviewed-proof","confirm":true }"#;
    let output = run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "close_period",
                json!({"company":"demo","body":body,"confirm":true}),
            ),
        ],
    );
    let values = parsed(&output);
    assert_eq!(values[1]["result"]["isError"], false);
    let wire = request.recv().unwrap();
    assert!(wire.starts_with(b"POST /api/companies/demo/periods/close "));
    assert!(wire.ends_with(body.as_bytes()));
    task.join().unwrap();
}

#[test]
fn receivables_tools_forward_exact_native_bodies_and_are_discoverable() {
    for (name, route, body) in [
        (
            "settle_invoice",
            "settle",
            r#"{ "invoiceDocumentId":1,"bankTransactionId":2,"confirm":true }"#,
        ),
        (
            "credit_invoice",
            "credit-note",
            r#"{"invoiceDocumentId":1,"issueDate":"2026-10-19","reason":"Synthetic","grossAmount":25.01,"confirm":true}"#,
        ),
        (
            "refund_invoice",
            "refund",
            r#"{"invoiceDocumentId":1,"bankTransactionReference":"synthetic outgoing","confirm":true}"#,
        ),
    ] {
        let (origin, wire, task) =
            http("HTTP/1.1 200 OK\r\nContent-Length:11\r\nConnection:close\r\n\r\n{\"ok\":true}");
        let output = run(
            &origin,
            &[
                init("2025-11-25"),
                ready(),
                call(
                    2,
                    name,
                    json!({"company":"synthetic","body":body,"confirm":true}),
                ),
            ],
        );
        let values = parsed(&output);
        assert_eq!(values[1]["result"]["isError"], false);
        let request = wire.recv().unwrap();
        assert!(request.starts_with(
            format!("POST /api/companies/synthetic/invoices/{route} HTTP/1.1").as_bytes()
        ));
        assert!(request.ends_with(body.as_bytes()));
        task.join().unwrap();
    }
    let output = run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    );
    let values = parsed(&output);
    let tools = values[1]["result"]["tools"].as_array().unwrap();
    for name in ["settle_invoice", "credit_invoice", "refund_invoice"] {
        let tool = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(tool["annotations"]["idempotentHint"], false);
        assert_eq!(
            tool["inputSchema"]["properties"]["body"]["contentSchema"]["properties"]["confirm"]["const"],
            true
        );
    }
}
#[test]
fn receivables_refusal_unknown_and_mixed_selector_remain_structured() {
    for (response, expected) in [
        (
            "HTTP/1.1 422 Unprocessable Entity\r\nContent-Length:12\r\nConnection:close\r\n\r\n{\"ok\":false}",
            "server_refused",
        ),
        (
            "HTTP/1.1 503 Unavailable\r\nContent-Length:12\r\nConnection:close\r\n\r\n{\"ok\":false}",
            "outcome_unknown",
        ),
    ] {
        let (origin, _wire, task) = http(response);
        let output = run(
            &origin,
            &[
                init("2025-11-25"),
                ready(),
                call(
                    2,
                    "credit_invoice",
                    json!({"company":"synthetic","confirm":true,"body":r#"{"invoiceDocumentId":1,"issueDate":"2026-10-19","reason":"Synthetic","confirm":true}"#}),
                ),
            ],
        );
        let values = parsed(&output);
        assert_eq!(values[1]["result"]["isError"], true);
        assert_eq!(
            values[1]["result"]["structuredContent"]["error"]["code"],
            expected
        );
        assert!(
            values[1]["result"]["structuredContent"]["error"]
                .get("idempotencyKey")
                .is_none()
        );
        task.join().unwrap();
    }
    let output = run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "settle_invoice",
                json!({"company":"synthetic","confirm":true,"body":r#"{"invoiceDocumentId":1,"bankTransactionId":2,"bankTransactionReference":"both","confirm":true}"#}),
            ),
        ],
    );
    let values = parsed(&output);
    assert_eq!(
        values[1]["result"]["structuredContent"]["error"]["code"],
        "invalid_input"
    );
}

#[test]
fn issued_pdf_mcp_publishes_read_only_contract_and_preserves_binary_original() {
    let (origin, wire, task) = http(
        "HTTP/1.1 200 OK\r\nContent-Type:application/pdf\r\nContent-Length:19\r\nConnection:close\r\n\r\n%PDF-1.7\nÅ\0\n%%EOF\n",
    );
    let output = run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(2, "invoice_pdf", json!({"company":"synthetic","id":7})),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}),
        ],
    );
    let values = parsed(&output);
    assert_eq!(values[1]["result"]["isError"], false);
    let pdf = &values[1]["result"]["structuredContent"]["data"]["pdf"];
    assert_eq!(pdf["contentType"], "application/pdf");
    assert_eq!(
        serde_json::from_value::<Vec<u8>>(pdf["bytes"].clone()).unwrap(),
        "%PDF-1.7\nÅ\0\n%%EOF\n".as_bytes()
    );
    let request = wire.recv().unwrap();
    assert!(request.starts_with(b"GET /api/companies/synthetic/invoices/7/pdf HTTP/1.1"));
    task.join().unwrap();
    let tool = values[2]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "invoice_pdf")
        .unwrap();
    assert_eq!(tool["inputSchema"]["required"], json!(["company", "id"]));
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["annotations"]["destructiveHint"], false);
}
#[test]
fn issued_pdf_mcp_rejects_nonpositive_or_extra_write_arguments_before_network() {
    let messages = [
        json!({"company":"synthetic","id":0}),
        json!({"company":"synthetic","id":-1}),
        json!({"company":"synthetic","id":"7"}),
        json!({"company":"synthetic"}),
        json!({"company":"synthetic","id":7,"confirm":true}),
        json!({"company":"synthetic","id":7,"body":"{}"}),
    ];
    for args in messages {
        let output = run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, "invoice_pdf", args)],
        );
        let values = parsed(&output);
        assert_eq!(values[1]["error"]["code"], -32602);
    }
}

#[test]
fn commitments_discovery_readonly_plan_and_confirmed_proof_contract() {
    let output = run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            call(
                3,
                "plan_supplier_commitment",
                json!({"company":"demo","body":"{}","confirm":true}),
            ),
            call(
                4,
                "supplier_commitment_matches",
                json!({"company":"demo","startDate":"2026-01-01"}),
            ),
            call(
                5,
                "approve_supplier_commitment",
                json!({"company":"demo","body":"{}","confirm":false}),
            ),
            call(
                6,
                "supplier_commitments",
                json!({"company":"demo","year":"2026"}),
            ),
        ],
    );
    let values = parsed(&output);
    let tools = values[1]["result"]["tools"].as_array().unwrap();
    let plan = tools
        .iter()
        .find(|t| t["name"] == "plan_supplier_commitment")
        .unwrap();
    assert_eq!(plan["annotations"]["readOnlyHint"], true);
    assert_eq!(plan["inputSchema"]["additionalProperties"], false);
    assert!(plan["inputSchema"]["properties"].get("confirm").is_none());
    let change = tools
        .iter()
        .find(|t| t["name"] == "change_supplier_commitment")
        .unwrap();
    assert_eq!(
        change["inputSchema"]["properties"]["body"]["contentSchema"]["properties"]["expectedPayloadHash"]
            ["minLength"],
        64
    );
    assert_eq!(change["annotations"]["idempotentHint"], false);
    for value in &values[2..] {
        assert_eq!(value["error"]["code"], -32602);
    }
}
#[test]
fn commitment_status_change_mcp_preserves_body_and_does_not_generate_key() {
    let (origin, request, task) =
        http("HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}");
    let body = format!(
        r#"{{ "commitmentId":"synthetic-contract","action":"paused","reason":"Synthetic reviewed pause","expectedPayloadHash":"{}","confirm":true }}"#,
        "a".repeat(64)
    );
    let output = run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "change_supplier_commitment",
                json!({"company":"demo","body":body,"confirm":true}),
            ),
        ],
    );
    let values = parsed(&output);
    assert_eq!(values[1]["result"]["isError"], false);
    let wire = request.recv().unwrap();
    assert!(wire.starts_with(b"POST /api/companies/demo/supplier-commitments/change "));
    assert!(wire.ends_with(body.as_bytes()));
    task.join().unwrap();
}

#[test]
fn annual_tool_is_readonly_and_preserves_preparation_blockers() {
    let (origin, request, task) = http(
        "HTTP/1.1 200 OK\r\nContent-Length: 73\r\nConnection:close\r\n\r\n{\"ok\":true,\"annualReport\":{\"report\":{\"ok\":false,\"errors\":[\"open year\"]}}}",
    );
    let output = run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "annual_report",
                json!({"company":"synthetic","fiscalYearStart":"2020-01-01","fiscalYearEnd":"2020-12-31"}),
            ),
        ],
    );
    let values = parsed(&output);
    assert_eq!(
        values[1]["result"]["structuredContent"]["data"],
        json!({"ok":true,"annualReport":{"report":{"ok":false,"errors":["open year"]}}})
    );
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(wire.starts_with("GET /api/companies/synthetic/annual-report?fiscalYearStart=2020-01-01&fiscalYearEnd=2020-12-31 HTTP/1.1"));
    task.join().unwrap();
    let discovery = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tool = discovery[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "annual_report")
        .unwrap();
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
}
#[test]
fn annual_tool_rejects_bad_dates_and_extra_mutation_fields_without_http() {
    for args in [
        json!({"company":"synthetic"}),
        json!({"company":"synthetic","fiscalYearStart":"2025-02-29","fiscalYearEnd":"2025-12-31"}),
        json!({"company":"synthetic","fiscalYearStart":"2025-12-31","fiscalYearEnd":"2025-01-01"}),
        json!({"company":"synthetic","fiscalYearStart":"2025-01-01","fiscalYearEnd":"2025-12-31","confirm":true}),
    ] {
        let values = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, "annual_report", args)],
        ));
        assert_eq!(values[1]["error"]["code"], -32602);
    }
}

#[test]
fn reviewed_submit_is_exact_and_read_tools_are_discoverable() {
    let (origin, request, task) = http(
        "HTTP/1.1 200 OK\r\nContent-Length: 68\r\nConnection:close\r\n\r\n{\"ok\":true,\"accountingDraft\":{\"id\":\"bank-one\",\"status\":\"submitted\"}}",
    );
    let body = "{ \"expectedEventHash\": \"server-hash\" }\n";
    let values = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "submit_draft",
                json!({"company":"synthetic","draftId":"bank-one","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        values[1]["result"]["structuredContent"]["data"]["accountingDraft"]["status"],
        "submitted"
    );
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(
        wire.starts_with(
            "POST /api/companies/synthetic/accounting-drafts/bank-one/submit HTTP/1.1"
        )
    );
    assert!(wire.ends_with(body));
    assert!(!wire.contains("idempotency"));
    task.join().unwrap();
    let values = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = values[1]["result"]["tools"].as_array().unwrap();
    for name in ["reviewed_bank", "accounting_drafts", "posting_rules"] {
        assert_eq!(
            tools.iter().find(|t| t["name"] == name).unwrap()["annotations"]["readOnlyHint"],
            true
        );
    }
    for name in [
        "create_bank_draft",
        "submit_draft",
        "reject_draft",
        "approve_draft",
    ] {
        assert_eq!(
            tools.iter().find(|t| t["name"] == name).unwrap()["annotations"]["readOnlyHint"],
            false
        );
    }
}
#[test]
fn reviewed_tools_reject_attribution_path_escape_and_mixed_selectors() {
    let values = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "create_bank_draft",
                json!({"company":"synthetic","draftId":null,"body":"{}","confirm":true}),
            ),
        ],
    ));
    assert_eq!(values[1]["error"]["code"], -32602);

    for (name, args) in [
        (
            "reviewed_bank",
            json!({"company":"synthetic","confirm":true}),
        ),
        (
            "posting_rules",
            json!({"company":"synthetic","year":"2026"}),
        ),
        (
            "submit_draft",
            json!({"company":"synthetic","body":"{}","confirm":true}),
        ),
        (
            "approve_draft",
            json!({"company":"synthetic","draftId":"../bad","body":"{}","confirm":true}),
        ),
        (
            "approve_draft",
            json!({"company":"synthetic","draftId":"bank-one","body":"{}","confirm":true,"actor":"reviewer"}),
        ),
        (
            "create_bank_draft",
            json!({"company":"synthetic","draftId":"bank-one","body":"{}","confirm":true}),
        ),
    ] {
        let values = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert_eq!(values[1]["error"]["code"], -32602);
    }
}

#[test]
fn document_ingestion_uses_large_dedicated_frame_and_exact_native_body() {
    let (origin, request, task) = http(
        "HTTP/1.1 200 OK\r\nContent-Length: 52\r\nConnection:close\r\n\r\n{\"ok\":true,\"document\":{\"id\":7,\"documentNo\":\"SYN-1\"}}",
    );
    let body = format!(
        "{{\"confirm\":true,\"fileName\":\"synthetic.txt\",\"fileBase64\":\"{}\",\"metadata\":{{\"source\":\"synthetic\"}}}}",
        "YQ==".repeat(300000)
    );
    let values = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "ingest_document",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        values[1]["result"]["structuredContent"]["data"]["document"]["id"],
        7
    );
    let data = request.recv().unwrap();
    let end = data.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    assert!(data.starts_with(b"POST /api/companies/synthetic/documents/ingest HTTP/1.1"));
    assert_eq!(&data[end..], body.as_bytes());
    task.join().unwrap();
}
#[test]
fn document_frame_exception_cannot_relax_generic_admission_or_execute_invalid_uploads() {
    for method in ["ping", "initialize", "tools/call"] {
        let message = json!({"jsonrpc":"2.0","id":1,"method":method,"params":{"name":"me","padding":"x".repeat(1048576)}});
        let output = run("http://127.0.0.1:9", &[message]);
        assert!(!output.status.success());
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["error"]["code"], -32700);
    }
    // Dedicated frame admitted, but body exceeds35MiB: reject before any HTTP mutation.
    let values = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "ingest_document",
                json!({"company":"synthetic","body":"x".repeat(35*1024*1024+1),"confirm":true}),
            ),
        ],
    ));
    assert_eq!(values[1]["error"]["code"], -32602);
    let values = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "ingest_document",
                json!({"company":"synthetic","body":"x".repeat(1048577),"confirm":true,"actor":"other"}),
            ),
        ],
    ));
    assert_eq!(values[1]["error"]["code"], -32602);
}
#[test]
fn document_original_mcp_preserves_non_pdf_binary_and_strict_read_schema() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (send, request) = mpsc::channel();
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 4096];
        let n = stream.read(&mut bytes).unwrap();
        send.send(bytes[..n].to_vec()).unwrap();
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: 5\r\nConnection:close\r\n\r\n\xff\xd8\xff\0x").unwrap();
    });
    let values = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "document_original",
                json!({"company":"synthetic","id":7}),
            ),
        ],
    ));
    assert_eq!(
        values[1]["result"]["structuredContent"]["data"]["original"]["contentType"],
        "image/jpeg"
    );
    let bytes: Vec<u8> = serde_json::from_value(
        values[1]["result"]["structuredContent"]["data"]["original"]["bytes"].clone(),
    )
    .unwrap();
    assert_eq!(bytes, b"\xff\xd8\xff\0x");
    assert!(
        request
            .recv()
            .unwrap()
            .starts_with(b"GET /api/companies/synthetic/documents/7/file HTTP/1.1")
    );
    task.join().unwrap();
    for args in [
        json!({"company":"synthetic","id":0}),
        json!({"company":"synthetic","id":9007199254740992i64}),
        json!({"company":"synthetic","id":7,"confirm":true}),
    ] {
        let values = parsed(&run(
            "http://127.0.0.1:9",
            &[
                init("2025-11-25"),
                ready(),
                call(2, "document_original", args),
            ],
        ));
        assert_eq!(values[1]["error"]["code"], -32602);
    }
}

#[test]
fn document_frame_has_absolute_bound_before_parsing_or_http() {
    let input = vec![b' '; 2 * 35 * 1024 * 1024 + 65536 + 1];
    let output = run_raw("http://127.0.0.1:9", &input);
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"], -32700);
}

#[test]
fn bank_reconciliation_link_forwards_reviewed_proof_exactly_once() {
    let (origin, request, task) = http(
        "HTTP/1.1 200 OK\r\nContent-Length: 69\r\nConnection:close\r\n\r\n{\"ok\":true,\"reconciliation\":{\"id\":1,\"effect\":\"link_only_no_posting\"}}",
    );
    let body = format!(
        "{{ \"bankTransactionId\":7,\"journalEntryId\":8,\"planHash\":\"{}\",\"note\":\"Synthetic reviewed source\",\"confirm\":true }}\n",
        "a".repeat(64)
    );
    let values = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "bank_reconciliation_link",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        values[1]["result"]["structuredContent"]["data"]["reconciliation"]["effect"],
        "link_only_no_posting"
    );
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(wire.starts_with("POST /api/companies/synthetic/bank/reconciliation-link HTTP/1.1"));
    assert!(wire.ends_with(&body));
    assert!(!wire.contains("idempotency"));
    task.join().unwrap();
}
#[test]
fn bank_reconciliation_schema_forbids_mixed_selectors_and_oversize() {
    let values = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = values[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(
        tools
            .iter()
            .find(|t| t["name"] == "bank_reconciliation_candidates")
            .unwrap()["annotations"]["readOnlyHint"],
        true
    );
    assert_eq!(
        tools
            .iter()
            .find(|t| t["name"] == "bank_reconciliation_link")
            .unwrap()["annotations"]["idempotentHint"],
        false
    );
    for (name, args) in [
        (
            "bank_reconciliation_candidates",
            json!({"company":"synthetic","bankTransactionId":0}),
        ),
        (
            "bank_reconciliation_candidates",
            json!({"company":"synthetic","bankTransactionId":7,"year":"2026"}),
        ),
        (
            "bank_reconciliation_candidates",
            json!({"company":"synthetic","bankTransactionId":9007199254740992i64}),
        ),
        (
            "bank_reconciliation_link",
            json!({"company":"synthetic","body":"{}","confirm":true,"actor":"other"}),
        ),
        (
            "bank_reconciliation_link",
            json!({"company":"synthetic","body":"x".repeat(16385),"confirm":true}),
        ),
    ] {
        let values = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert_eq!(values[1]["error"]["code"], -32602);
    }
}

#[test]
fn correction_mcp_unknown_keeps_original_key_and_exact_body() {
    let (origin, request, task) =
        http("HTTP/1.1 503 Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");
    let body=r#"{ "bankTransactionId":7,"replacementJournalEntryId":8,"expectedReconciliationId":"append-only:1","planHash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","reason":"  Synthetic correction  ","idempotencyKey":"  nøgle-🚀  ","confirm":true }
"#.to_owned();
    let values = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "bank_reconciliation_correction",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    let error = &values[1]["result"]["structuredContent"]["error"];
    assert_eq!(error["code"], "outcome_unknown");
    assert_eq!(error["idempotencyKey"], "  nøgle-🚀  ");
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(
        wire.starts_with("POST /api/companies/synthetic/bank/reconciliation-correction HTTP/1.1")
    );
    assert!(wire.ends_with(&body));
    task.join().unwrap();
}
#[test]
fn correction_mcp_selectors_extra_fields_and_native_size_are_strict() {
    let values = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = values[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(
        tools
            .iter()
            .find(|t| t["name"] == "bank_reconciliation_correction_plan")
            .unwrap()["annotations"]["readOnlyHint"],
        true
    );
    for (name, args) in [
        (
            "bank_reconciliation_correction_plan",
            json!({"company":"synthetic","bankTransactionId":7}),
        ),
        (
            "bank_reconciliation_correction_plan",
            json!({"company":"synthetic","bankTransactionId":7,"replacementJournalEntryId":0}),
        ),
        (
            "bank_reconciliation_correction_plan",
            json!({"company":"synthetic","bankTransactionId":7,"replacementJournalEntryId":8,"confirm":true}),
        ),
        (
            "bank_reconciliation_correction",
            json!({"company":"synthetic","body":"{}","confirm":true,"actor":"other"}),
        ),
        (
            "bank_reconciliation_correction",
            json!({"company":"synthetic","body":"x".repeat(16385),"confirm":true}),
        ),
    ] {
        let values = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert_eq!(values[1]["error"]["code"], -32602);
    }
}

#[test]
fn recurring_mcp_exact_generate_once_and_unknown_semantics() {
    let (origin, request, task) =
        http("HTTP/1.1 503 Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");
    let body = "{ \"asOfDate\":\"2026-01-01\",\"confirm\":true }\n";
    let v = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "generate_recurring_invoice",
                json!({"company":"synthetic","id":7,"body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        v[1]["result"]["structuredContent"]["error"]["code"],
        "outcome_unknown"
    );
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(
        wire.starts_with("POST /api/companies/synthetic/recurring-invoices/7/generate HTTP/1.1")
    );
    assert!(wire.ends_with(body));
    task.join().unwrap();
}
#[test]
fn recurring_tools_discovery_and_strict_read_write_admission() {
    let v = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = v[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 65);
    assert_eq!(
        tools
            .iter()
            .find(|t| t["name"] == "recurring_invoices")
            .unwrap()["annotations"]["readOnlyHint"],
        true
    );
    for (name, args) in [
        (
            "recurring_invoices",
            json!({"company":"synthetic","confirm":true}),
        ),
        (
            "create_recurring_invoice",
            json!({"company":"synthetic","id":7,"body":"{}","confirm":true}),
        ),
        (
            "generate_recurring_invoice",
            json!({"company":"synthetic","id":0,"body":"{}","confirm":true}),
        ),
        (
            "retire_recurring_invoice",
            json!({"company":"synthetic","id":7,"body":" ".repeat(256*1024+1),"confirm":true}),
        ),
    ] {
        let v = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert!(v[1].get("error").is_some(), "{v:?}");
    }
}
