use serde_json::{Value, json};

// Existing 312 bindings plus seven original document-party tools.
const EXPECTED_NATIVE_TOOLS: usize = 337;

#[test]
fn pdf_tools_require_confirmation_and_keep_reads_bounded() {
    let output=run("http://127.0.0.1:9",&[
        init("2025-11-25"),ready(),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        call(3,"parse_document",json!({"company":"synthetic","id":7})),
        call(4,"document_parsed_text",json!({"company":"synthetic","id":7,"limit":11})),
        call(5,"document_parse_status",json!({"company":"synthetic","id":7,"confirm":true})),
        call(6,"parse_pending_documents",json!({"company":"synthetic","confirm":true,"limit":101})),
    ]);
    let rows=parsed(&output);
    let tools=rows[1]["result"]["tools"].as_array().unwrap();
    for (name,read) in [("parse_document",false),("document_parse_status",true),("document_parsed_text",true),("parse_pending_documents",false)] {
        let tool=tools.iter().find(|t|t["name"]==name).unwrap();
        assert_eq!(tool["annotations"]["readOnlyHint"],read);
    }
    for row in &rows[2..] {assert!(row["result"]["isError"]==true||row.get("error").is_some());}
}
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
    assert_eq!(tools.len(), EXPECTED_NATIVE_TOOLS);
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
                "journal",
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
        "GET /api/companies/demo/journal?year=2026&account=1000+%26x%3D2 HTTP/1.1"
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
        json!(["confirm"])
    );
    assert_eq!(import["inputSchema"]["properties"]["body"]["contentSchema"]["oneOf"], json!([
        {"required":["csvContent"],"not":{"required":["csvBase64"]}},
        {"required":["csvBase64"],"not":{"required":["csvContent"]}}
    ]));
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
    assert_eq!(tools.len(), EXPECTED_NATIVE_TOOLS);
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

#[test]
fn asset_mcp_exact_explicit_period_and_unknown_semantics() {
    let (origin, request, task) =
        http("HTTP/1.1 503 Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");
    let body = "{ \"transactionDate\":\"2026-01-01\",\"periodIndex\":1,\"confirm\":true }\n";
    let v = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "depreciate_asset",
                json!({"company":"synthetic","id":7,"body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        v[1]["result"]["structuredContent"]["error"]["code"],
        "outcome_unknown"
    );
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(wire.starts_with("POST /api/companies/synthetic/assets/7/depreciate HTTP/1.1"));
    assert!(wire.ends_with(body));
    task.join().unwrap();
}
#[test]
fn asset_readonly_discovery_and_native_body_gates() {
    let v = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = v[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), EXPECTED_NATIVE_TOOLS);
    for name in ["assets", "next_asset_depreciation"] {
        assert_eq!(
            tools.iter().find(|t| t["name"] == name).unwrap()["annotations"]["readOnlyHint"],
            true
        );
    }
    for (name, args) in [
        ("assets", json!({"company":"synthetic","confirm":true})),
        (
            "next_asset_depreciation",
            json!({"company":"synthetic","id":0}),
        ),
        (
            "register_asset",
            json!({"company":"synthetic","id":7,"body":"{}","confirm":true}),
        ),
        (
            "depreciate_asset",
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

#[test]
fn accrual_mcp_exact_explicit_period_and_unknown_semantics() {
    let (origin, request, task) =
        http("HTTP/1.1 503 Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");
    let body = "{ \"transactionDate\":\"2026-01-01\",\"periodIndex\":1,\"confirm\":true }\n";
    let v = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "recognize_accrual",
                json!({"company":"synthetic","id":7,"body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        v[1]["result"]["structuredContent"]["error"]["code"],
        "outcome_unknown"
    );
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(wire.starts_with("POST /api/companies/synthetic/accruals/7/recognize HTTP/1.1"));
    assert!(wire.ends_with(body));
    task.join().unwrap();
}
#[test]
fn accrual_readonly_discovery_and_native_body_gates() {
    let v = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = v[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), EXPECTED_NATIVE_TOOLS);
    for name in ["accruals", "accrual_schedule"] {
        assert_eq!(
            tools.iter().find(|t| t["name"] == name).unwrap()["annotations"]["readOnlyHint"],
            true
        );
    }
    for (name, args) in [
        ("accruals", json!({"company":"synthetic","confirm":true})),
        ("accrual_schedule", json!({"company":"synthetic","id":0})),
        (
            "register_accrual",
            json!({"company":"synthetic","id":7,"body":"{}","confirm":true}),
        ),
        (
            "recognize_accrual",
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

#[test]
fn budget_mcp_native_revision_has_no_injected_inner_confirmation() {
    let (origin, request, task) =
        http("HTTP/1.1 503 Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");
    let body = "{ \"accountNo\":\"E\",\"period\":\"2026-01\",\"amount\":23.45 }\n";
    let v = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "set_budget",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        v[1]["result"]["structuredContent"]["error"]["code"],
        "outcome_unknown"
    );
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(wire.starts_with("POST /api/companies/synthetic/budget HTTP/1.1"));
    assert!(wire.ends_with(body));
    assert!(!wire.contains("idempotencyKey"));
    task.join().unwrap();
}
#[test]
fn mileage_mcp_preserves_native_advisory_and_caller_rate() {
    let reply = r#"{"ok":true,"mileage":{"mileageEntryId":7,"amountBasis":1.35,"warnings":["native rate advisory"],"appliedRules":["native-rule"]}}"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
        reply.len()
    );
    let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
    let body = r#"{ "tripDate":"2026-01-31","purpose":"Synthetic visit","fromLocation":"A","toLocation":"B","kilometers":0.15,"vehicle":"Synthetic car","driver":"Synthetic driver","ratePerKm":9,"rateBasis":"Synthetic chosen basis","confirm":true }"#;
    let v = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "register_mileage",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        v[1]["result"]["structuredContent"]["data"],
        serde_json::from_str::<Value>(reply).unwrap()
    );
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(wire.starts_with("POST /api/companies/synthetic/mileage HTTP/1.1"));
    assert!(wire.ends_with(body));
    task.join().unwrap();
}
#[test]
fn budget_mileage_discovery_year_scope_and_body_limits_are_strict() {
    let v = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = v[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), EXPECTED_NATIVE_TOOLS);
    for name in [
        "budget",
        "budget_vs_actual",
        "budget_dimension_actuals",
        "mileage",
    ] {
        assert_eq!(
            tools.iter().find(|t| t["name"] == name).unwrap()["annotations"]["readOnlyHint"],
            true
        );
    }
    let comparison=tools.iter().find(|t|t["name"]=="budget_vs_actual").unwrap();
    assert_eq!(tools.iter().filter(|t|t["name"]=="budget_vs_actual").count(),1);
    let forms=comparison["inputSchema"]["oneOf"].as_array().unwrap();
    assert_eq!(forms.len(),2);
    assert_eq!(forms[0]["required"],json!(["company","year"]));
    assert_eq!(forms[0]["properties"]["year"]["minLength"],1);
    assert_eq!(forms[0]["properties"]["year"]["maxLength"],128);
    assert_eq!(forms[1]["required"],json!(["company","from","to"]));
    for form in forms {assert_eq!(form["additionalProperties"],false);}
    assert!(forms[0]["properties"].get("from").is_none());
    assert!(forms[1]["properties"].get("year").is_none());
    for (name, args) in [
        ("budget", json!({"company":"synthetic"})),
        (
            "mileage",
            json!({"company":"synthetic","year":"2026","confirm":true}),
        ),
        (
            "set_budget",
            json!({"company":"synthetic","body":" ".repeat(16*1024+1),"confirm":true}),
        ),
        (
            "register_mileage",
            json!({"company":"synthetic","body":" ".repeat(32*1024+1),"confirm":true}),
        ),
    ] {
        let v = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert!(v[1].get("error").is_some(), "{v:?}");
    }
}

#[test]
fn privacy_mcp_export_is_audited_and_full_unknown_coverage_is_preserved() {
    let reply = r#"{"ok":true,"gdpr":{"export":{"coverage":{"complete":false},"records":[{"source":"audit_log","retentionUnknown":true,"retainUntil":null,"erasable":false}]}}}"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
        reply.len()
    );
    let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
    let body = "{ \"cvr\":\"  SYN-SUBJECT  \",\"asOf\":\"2026-01-31\",\"confirm\":true }\n";
    let v = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "gdpr_export",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        v[1]["result"]["structuredContent"]["data"],
        serde_json::from_str::<Value>(reply).unwrap()
    );
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(wire.starts_with("POST /api/companies/synthetic/gdpr/export HTTP/1.1"));
    assert!(wire.ends_with(body));
    task.join().unwrap();
}
#[test]
fn privacy_mcp_overlay_unknown_is_not_silently_retried() {
    let (origin, request, task) =
        http("HTTP/1.1 503 Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");
    let body = r#"{"name":"Synthetic subject","confirm":true}"#;
    let v = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "gdpr_erase",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        v[1]["result"]["structuredContent"]["error"]["code"],
        "outcome_unknown"
    );
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(wire.starts_with("POST /api/companies/synthetic/gdpr/erase HTTP/1.1"));
    assert!(wire.ends_with(body));
    task.join().unwrap();
}
#[test]
fn privacy_discovery_audited_annotation_and_date_body_gates() {
    let v = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = v[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), EXPECTED_NATIVE_TOOLS);
    for (name, readonly) in [
        ("retention", true),
        ("gdpr_export", false),
        ("gdpr_erase", false),
    ] {
        assert_eq!(
            tools.iter().find(|t| t["name"] == name).unwrap()["annotations"]["readOnlyHint"],
            readonly
        );
    }
    for (name, args) in [
        (
            "retention",
            json!({"company":"synthetic","asOf":"9999-12-31"}),
        ),
        (
            "gdpr_export",
            json!({"company":"synthetic","body":"{}","confirm":false}),
        ),
        (
            "gdpr_erase",
            json!({"company":"synthetic","body":" ".repeat(16*1024+1),"confirm":true}),
        ),
    ] {
        let v = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert!(
            v[1].get("error").is_some() || v[1]["result"]["isError"] == true,
            "{v:?}"
        );
    }
}

#[test]
fn setup_mcp_role_uses_exact_reviewed_account_and_full_resolution() {
    let reply = r#"{"ok":true,"resolution":{"ok":true,"role":"input_vat","accountNo":"V","version":2,"confirmedBy":"user:synthetic","confirmationSource":"explicit"}}"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
        reply.len()
    );
    let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
    let body = r#"{ "role":"input_vat","accountNo":"V","confirm":true }"#;
    let v = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "confirm_account_role",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        v[1]["result"]["structuredContent"]["data"],
        serde_json::from_str::<Value>(reply).unwrap()
    );
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(wire.starts_with("POST /api/companies/synthetic/account-roles HTTP/1.1"));
    assert!(wire.ends_with(body));
    task.join().unwrap();
}
#[test]
fn setup_mcp_account_creation_does_not_inject_confirm_keys_or_defaults() {
    let (origin, request, task) =
        http("HTTP/1.1 503 Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");
    let body = r#"{"accountNo":"V","name":"Synthetic VAT","type":"vat","normalBalance":"debit"}"#;
    let v = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "create_account",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        v[1]["result"]["structuredContent"]["error"]["code"],
        "outcome_unknown"
    );
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(wire.starts_with("POST /api/companies/synthetic/accounts HTTP/1.1"));
    assert!(wire.ends_with(body));
    task.join().unwrap();
}
#[test]
fn setup_discovery_current_company_read_and_strict_outer_admission() {
    let v = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = v[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), EXPECTED_NATIVE_TOOLS);
    assert_eq!(
        tools
            .iter()
            .find(|t| t["name"] == "company_profile")
            .unwrap()["annotations"]["readOnlyHint"],
        true
    );
    for (name, args) in [
        (
            "company_profile",
            json!({"company":"synthetic","year":"2026"}),
        ),
        (
            "update_company_profile",
            json!({"company":"synthetic","body":"{}","confirm":false}),
        ),
        (
            "confirm_account_role",
            json!({"company":"synthetic","body":" ".repeat(16*1024+1),"confirm":true}),
        ),
    ] {
        let v = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert!(
            v[1].get("error").is_some() || v[1]["result"]["isError"] == true,
            "{v:?}"
        );
    }
}

#[test]
fn company_creation_is_workspace_scoped_exact_and_unknown_without_replay() {
    let (origin, request, task)=http("HTTP/1.1 503 Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");
    let body="{ \"name\":\" Synthetic Separate Company \",\"fiscalYearStartMonth\":\"7\",\"vatPeriodType\":null }\n";
    let v=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"create_company",json!({"body":body,"confirm":true}))]));
    assert_eq!(v[1]["result"]["structuredContent"]["error"]["code"],"outcome_unknown");
    let wire=String::from_utf8(request.recv().unwrap()).unwrap();assert!(wire.starts_with("POST /api/companies HTTP/1.1"));assert!(wire.ends_with(body));task.join().unwrap();
    let v=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})]));
    let tool=v[1]["result"]["tools"].as_array().unwrap().iter().find(|t|t["name"]=="create_company").unwrap();
    assert_eq!(tool["annotations"]["readOnlyHint"],false);assert_eq!(tool["inputSchema"]["additionalProperties"],false);assert!(tool["inputSchema"]["properties"].get("company").is_none());
    for args in [json!({"body":"{}","confirm":false}),json!({"body":"{}","confirm":true,"company":"old"}),json!({"body":"{\"name\":\"Synthetic\",\"idempotencyKey\":\"invented\"}","confirm":true})] {
        let v=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),call(2,"create_company",args)]));assert!(v[1].get("error").is_some()||v[1]["result"]["isError"]==true);
    }
}
#[test]
fn saft_export_discovery_confirmation_and_native_shape_are_explicit(){
 let out=run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),call(3,"saft_export",json!({"company":"synthetic","body":"{}","confirm":false}))]);
 let rows=parsed(&out);let tools=rows[1]["result"]["tools"].as_array().unwrap();let t=tools.iter().find(|t|t["name"]=="saft_export").unwrap();assert_eq!(t["annotations"]["readOnlyHint"],false);assert_eq!(t["inputSchema"]["properties"]["body"]["maxLength"],4096);assert!(rows[2]["result"]["isError"]==true||rows[2].get("error").is_some());
}
#[test]
fn saft_export_stdio_returns_exact_tar_bytes(){
 let binary="x".repeat(1024);let reply=format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-tar\r\nContent-Length: 1024\r\nConnection: close\r\n\r\n{binary}");let(origin,request,task)=http(Box::leak(reply.into_boxed_str()));
 let body=r#"{ "periodStart":"2026-01-01","periodEnd":"2026-12-31","confirm":true }"#;
 let out=run(&origin,&[init("2025-11-25"),ready(),call(2,"saft_export",json!({"company":"synthetic","body":body,"confirm":true}))]);let rows=parsed(&out);assert_eq!(rows[1]["result"]["structuredContent"]["data"]["export"]["bytes"],json!(vec![120;1024]));let req=request.recv().unwrap();assert!(req.ends_with(body.as_bytes()));assert!(req.starts_with(b"POST /api/companies/synthetic/saft-export HTTP/1.1"));task.join().unwrap();
}

#[test]fn reminder_claim_tools_read_and_exact_write_are_discoverable(){let out=run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),call(3,"settle_invoice_claims",json!({"company":"synthetic","body":"{}","confirm":false}))]);let rows=parsed(&out);let tools=rows[1]["result"]["tools"].as_array().unwrap();for(name,read)in[("invoice_reminder_claims",true),("register_invoice_reminder",false),("post_invoice_reminder",false),("settle_invoice_claims",false)]{let t=tools.iter().find(|t|t["name"]==name).unwrap();assert_eq!(t["annotations"]["readOnlyHint"],read);}assert!(rows[2]["result"]["isError"]==true||rows[2].get("error").is_some());}
#[test]fn reminder_claim_canonical_stdio_read_preserves_full_native_body(){let reply=r#"{"ok":true,"reminderClaims":{"principalOpenBalance":0,"ledgerBackedClaimBalance":30,"pendingUnpostedReminders":false,"delivery":{"available":false,"sent":false},"claimPayments":[{"claimPaymentId":9}],"unknownFutureFact":{"available":false}}}"#;let response=format!("HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection: close\r\n\r\n{}",reply.len(),reply);let(origin,request,task)=http(Box::leak(response.into_boxed_str()));let out=run(&origin,&[init("2025-11-25"),ready(),call(2,"invoice_reminder_claims",json!({"company":"synthetic","id":7}))]);let rows=parsed(&out);assert_eq!(rows[1]["result"]["structuredContent"]["data"],serde_json::from_str::<Value>(reply).unwrap());assert!(request.recv().unwrap().starts_with(b"GET /api/companies/synthetic/invoices/7/reminder-claims HTTP/1.1"));task.join().unwrap();}

#[test]
fn workspace_backup_discovery_confirmation_and_forbidden_selectors(){
 let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})]));
 let tools=rows[1]["result"]["tools"].as_array().unwrap();assert_eq!(tools.len(),EXPECTED_NATIVE_TOOLS);let tool=tools.iter().find(|t|t["name"]=="workspace_backup").unwrap();assert_eq!(tool["annotations"]["readOnlyHint"],false);assert_eq!(tool["annotations"]["idempotentHint"],false);assert_eq!(tool["inputSchema"],json!({"type":"object","required":["confirm"],"properties":{"confirm":{"const":true}},"additionalProperties":false}));
 for args in [json!({}),json!({"confirm":false}),json!({"confirm":"true"}),json!({"confirm":true,"company":"synthetic"}),json!({"confirm":true,"year":"2026"}),json!({"confirm":true,"id":1}),json!({"confirm":true,"body":"{}"}),json!({"confirm":true,"path":"/tmp/forbidden"}),json!({"confirm":true,"key":"invented"})]{let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),call(2,"workspace_backup",args)]));assert!(rows[1].get("error").is_some()||rows[1]["result"]["isError"]==true);}
}
#[test]
fn workspace_backup_stdio_exact_post_preserves_artifact_bytes_and_refusal_classes(){
 for(status,mime,expected)in[("200 OK","application/vnd.rentemester.native-backup+json",None),("403 Forbidden","application/json",Some("server_refused")),("503 Unavailable","application/json",Some("outcome_unknown")),("200 OK","application/json",Some("outcome_unknown"))]{
  let reply=format!("HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: 5\r\nConnection: close\r\n\r\n{{}}\n  ");let(origin,request,task)=http(Box::leak(reply.into_boxed_str()));let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"workspace_backup",json!({"confirm":true}))]));
  if let Some(code)=expected{assert_eq!(rows[1]["result"]["structuredContent"]["error"]["code"],code);}else{assert_eq!(rows[1]["result"]["structuredContent"]["data"]["backup"],json!({"contentType":"application/vnd.rentemester.native-backup+json","bytes":[123,125,10,32,32]}));}
  let wire=request.recv().unwrap();assert!(wire.starts_with(b"POST /api/workspace/backup HTTP/1.1\r\n"));let split=wire.windows(4).position(|v|v==b"\r\n\r\n").unwrap()+4;assert_eq!(&wire[split..],br#"{"confirm":true}"#);task.join().unwrap();
 }
}

#[test]
fn company_knowledge_tools_are_discoverable_and_authority_fields_refused(){
 let output=run("http://127.0.0.1:1",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),call(3,"company_knowledge",json!({"company":"a","actor":"user:forged"})),call(4,"propose_company_knowledge",json!({"company":"a","body":"{\"predicate\":\"markets\",\"source\":{\"kind\":\"user\",\"ref\":\"s\"},\"validFrom\":\"2026-01-01\",\"confirm\":true}","confirm":true}))]);
 let v=parsed(&output);let tools=v[1]["result"]["tools"].as_array().unwrap();for n in ["company_knowledge","propose_company_knowledge","review_company_knowledge","supersede_company_knowledge"]{assert!(tools.iter().any(|t|t["name"]==n));}assert!(v[2]["error"].is_object());assert!(v[3]["result"]["isError"].as_bool().unwrap());
}
#[test]
fn company_knowledge_mcp_exact_review_once_and_full_response(){
 let (origin,recv,task)=http("HTTP/1.1 200 OK\r\nContent-Length: 53\r\nConnection:close\r\n\r\n{\"ok\":true,\"assertion\":{\"futureNativeEvidence\":true}}");
 let body="{ \"assertionId\":\"knowledge-synthetic\", \"decision\":\"approved\", \"confirm\":true }\n";
 let output=run(&origin,&[init("2025-11-25"),ready(),call(2,"review_company_knowledge",json!({"company":"synthetic","body":body,"confirm":true}))]);
 let wire=String::from_utf8(recv.recv().unwrap()).unwrap();assert!(wire.starts_with("POST /api/companies/synthetic/knowledge/review HTTP/1.1"));assert!(wire.ends_with(body));let v=parsed(&output);assert_eq!(v[1]["result"]["structuredContent"]["data"]["assertion"]["futureNativeEvidence"],true);task.join().unwrap();
}
#[test]fn party_discovery_and_current_scope_arguments_are_strict(){let out=run("http://127.0.0.1:1",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),call(3,"workspace_parties",json!({"company":"a","visibleCompanies":["a","hidden"]})),call(4,"create_workspace_party",json!({"company":"a","partyId":"id","body":"{}","confirm":true}))]);let v=parsed(&out);let tools=v[1]["result"]["tools"].as_array().unwrap();for n in ["workspace_parties","workspace_party","create_workspace_party","link_workspace_party_role","assert_workspace_party_field","assert_workspace_party_alias"]{assert!(tools.iter().any(|t|t["name"]==n));}assert!(v[2]["error"].is_object());assert!(v[3]["error"].is_object());}
#[test]fn party_assertion_exact_body_once_with_full_response(){let reply=r#"{"ok":true,"party":{"assertions":[{"field":"name","review_state":"approved"}],"unknownFutureEvidence":true}}"#;let response=format!("HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",reply.len());let(origin,rx,t)=http(Box::leak(response.into_boxed_str()));let body=r#"{ "field":"name","value":"Synthetic revised","source":"registry","observedAt":"2026-01-01","reviewState":"approved","confirm":true }"#;let out=run(&origin,&[init("2025-11-25"),ready(),call(2,"assert_workspace_party_field",json!({"company":"synthetic","partyId":"party-synthetic","body":body,"confirm":true}))]);let wire=String::from_utf8(rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap()).unwrap();assert!(wire.starts_with("POST /api/companies/synthetic/workspace-parties/party-synthetic/assertion HTTP/1.1"));assert!(wire.ends_with(body));let v=parsed(&out);assert_eq!(v[1]["result"]["structuredContent"]["data"],serde_json::from_str::<Value>(reply).unwrap());t.join().unwrap();}

#[test]
fn workspace_inbox_discovery_and_strict_selection_contracts() {
 let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})]));
 let tools=rows[1]["result"]["tools"].as_array().unwrap();assert_eq!(tools.len(),EXPECTED_NATIVE_TOOLS);
 for(name,read)in[("workspace_inbox",true),("workspace_inbox_source",true),("ingest_workspace_inbox",false),("assign_workspace_inbox",false),("complete_workspace_inbox",false)]{let t=tools.iter().find(|t|t["name"]==name).unwrap();assert_eq!(t["annotations"]["readOnlyHint"],read);assert_eq!(t["inputSchema"]["additionalProperties"],false);}
 for(name,args)in[
 ("workspace_inbox",json!({"company":"synthetic","year":"2026"})),
 ("workspace_inbox_source",json!({"company":"synthetic","sourceId":"source","confirm":true})),
 ("ingest_workspace_inbox",json!({"company":"synthetic","sourceId":"source","body":"{}","confirm":true})),
 ("assign_workspace_inbox",json!({"company":"synthetic","body":"{}","confirm":true})),
 ("complete_workspace_inbox",json!({"company":"synthetic","sourceId":"source","body":"{}","confirm":true,"actor":"forged"}))]{let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),call(2,name,args)]));assert_eq!(rows[1]["error"]["code"],-32602);}
}
#[test]
fn workspace_inbox_ingest_only_large_frame_exact_body_and_unknown_key() {
 let(origin,request,task)=http("HTTP/1.1 503 Service Unavailable\r\nContent-Length: 11\r\nConnection:close\r\n\r\n{\"ok\":true}");
 let body=format!("{{ \"confirm\":true,\"idempotencyKey\":\" original inbox key \",\"bytesBase64\":\"{}\",\"filename\":\"source.txt\",\"mimeType\":\"text/plain\",\"transport\":\"upload\",\"receivedAt\":\"2026-01-01\",\"metadata\":{{\"documentType\":\"cash_register_receipt\"}} }}\n","YWFh".repeat(300000));
 let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"ingest_workspace_inbox",json!({"company":"synthetic","body":body,"confirm":true}))]));
 let error=&rows[1]["result"]["structuredContent"]["error"];assert_eq!(error["code"],"outcome_unknown");assert_eq!(error["idempotencyKey"]," original inbox key ");
 let wire=request.recv().unwrap();let end=wire.windows(4).position(|w|w==b"\r\n\r\n").unwrap()+4;assert!(wire.starts_with(b"POST /api/companies/synthetic/workspace-inbox HTTP/1.1"));assert_eq!(&wire[end..],body.as_bytes());task.join().unwrap();
 for name in ["assign_workspace_inbox","complete_workspace_inbox","workspace_inbox"]{
  let output=run("http://127.0.0.1:9",&[call(2,name,json!({"company":"synthetic","sourceId":"source","body":" ".repeat(1048576),"confirm":true}))]);assert!(!output.status.success());let row:Value=serde_json::from_slice(&output.stdout).unwrap();assert_eq!(row["error"]["code"],-32700);
 }
}
#[test]
fn workspace_inbox_missing_confirmation_or_oversized_body_never_executes() {
 for args in [json!({"company":"synthetic","body":"{}","confirm":false}),json!({"company":"synthetic","body":" ".repeat(rentemester_client::MAX_WORKSPACE_INBOX_BODY+1),"confirm":true})]{
  let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),call(2,"ingest_workspace_inbox",args)]));assert_eq!(rows[1]["result"]["structuredContent"]["error"]["code"],"invalid_input");
 }
}

#[test]
fn corporate_discovery_and_strict_scope_confirmation_contracts(){
 let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})]));let tools=rows[1]["result"]["tools"].as_array().unwrap();assert_eq!(tools.len(),EXPECTED_NATIVE_TOOLS);
 for(name,read)in[("corporate_records",true),("corporate_record",true),("corporate_record_original",true),("ingest_corporate_record",false),("link_corporate_record",false),("enrich_corporate_record",false),("supersede_corporate_record",false)]{let t=tools.iter().find(|t|t["name"]==name).unwrap();assert_eq!(t["annotations"]["readOnlyHint"],read);assert_eq!(t["inputSchema"]["additionalProperties"],false);}
 for(name,args)in[("corporate_records",json!({"company":"synthetic","year":"2026"})),("corporate_record_original",json!({"company":"synthetic","recordId":"record","confirm":true})),("ingest_corporate_record",json!({"company":"synthetic","recordId":"record","body":"{}","confirm":true})),("ingest_corporate_record",json!({"company":"synthetic","recordId":null,"body":"{}","confirm":true})),("link_corporate_record",json!({"company":"synthetic","body":"{}","confirm":true})),("enrich_corporate_record",json!({"company":"synthetic","recordId":"record","body":"{}","confirm":true,"actor":"forged"}))]{let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),call(2,name,args)]));assert_eq!(rows[1]["error"]["code"],-32602);}
}
#[test]
fn corporate_original_preserves_native_base64_sha_json_without_fabricated_wrapper(){
 let(origin,request,task)=http("HTTP/1.1 200 OK\r\nContent-Length: 64\r\nConnection:close\r\n\r\n{\"ok\":true,\"bytesBase64\":\"AP8=\",\"sha256\":\"native\",\"future\":true}");
 let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"corporate_record_original",json!({"company":"synthetic","recordId":"record+synthetic"}))]));let data=&rows[1]["result"]["structuredContent"]["data"];assert_eq!(data["bytesBase64"],"AP8=");assert_eq!(data["sha256"],"native");assert!(data.get("original").is_none());assert!(request.recv().unwrap().starts_with(b"GET /api/companies/synthetic/corporate-records/record%2Bsynthetic/download HTTP/1.1"));task.join().unwrap();
}
#[test]
fn corporate_ingest_large_dedicated_frame_exact_body_and_unknown_outcome(){
 let(origin,request,task)=http("HTTP/1.1 503 Service Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");
 let body=format!("{{ \"confirm\":true,\"type\":\"articles\",\"bytesBase64\":\"{}\",\"filename\":\"source.txt\",\"source\":\"Synthetic source\",\"receivedAt\":\"2026-01-01\",\"uploader\":\"Synthetic uploader\",\"links\":null }}\n","YWFh".repeat(300000));
 let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"ingest_corporate_record",json!({"company":"synthetic","body":body,"confirm":true}))]));let error=&rows[1]["result"]["structuredContent"]["error"];assert_eq!(error["code"],"outcome_unknown");assert!(error.get("idempotencyKey").is_none());let wire=request.recv().unwrap();let end=wire.windows(4).position(|w|w==b"\r\n\r\n").unwrap()+4;assert!(wire.starts_with(b"POST /api/companies/synthetic/corporate-records HTTP/1.1"));assert_eq!(&wire[end..],body.as_bytes());task.join().unwrap();
 for name in ["link_corporate_record","enrich_corporate_record","supersede_corporate_record","corporate_record_original"]{let output=run("http://127.0.0.1:9",&[call(2,name,json!({"company":"synthetic","recordId":"record","body":" ".repeat(1048576),"confirm":true}))]);assert!(!output.status.success());let row:Value=serde_json::from_slice(&output.stdout).unwrap();assert_eq!(row["error"]["code"],-32700);}
}
#[test]
fn corporate_body_and_specific_frame_bounds_refuse_before_execution(){
 for body in ["{}".to_owned()," ".repeat(rentemester_client::MAX_CORPORATE_INGEST_BODY+1)]{let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),call(2,"ingest_corporate_record",json!({"company":"synthetic","body":body,"confirm":true}))]));assert_eq!(rows[1]["result"]["structuredContent"]["error"]["code"],"invalid_input");}
 let output=run("http://127.0.0.1:9",&[call(2,"ingest_corporate_record",json!({"company":"synthetic","body":" ".repeat(2*rentemester_client::MAX_CORPORATE_INGEST_BODY+65536),"confirm":true}))]);assert!(!output.status.success());let row:Value=serde_json::from_slice(&output.stdout).unwrap();assert_eq!(row["error"]["code"],-32700);
}

#[test]
fn ownership_discovery_explicit_dates_independent_review_and_strict_arguments(){
 let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})]));let tools=rows[1]["result"]["tools"].as_array().unwrap();assert_eq!(tools.len(),EXPECTED_NATIVE_TOOLS);
 for(name,read)in[("ownership",true),("ownership_history",true),("propose_ownership",false),("review_ownership",false),("apply_ownership",false)]{let t=tools.iter().find(|t|t["name"]==name).unwrap();assert_eq!(t["annotations"]["readOnlyHint"],read);assert_eq!(t["inputSchema"]["additionalProperties"],false);}
 for(name,args)in[("ownership",json!({"company":"synthetic"})),("ownership_history",json!({"company":"synthetic","confirm":true})),("propose_ownership",json!({"company":"synthetic","body":"{}","confirm":true,"principal":"forged"})),("review_ownership",json!({"company":"synthetic","body":"{}","confirm":true,"snapshotId":"mixed"}))]{let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),call(2,name,args)]));assert_eq!(rows[1]["error"]["code"],-32602);}
}
#[test]
fn ownership_apply_exact_source_hashes_unknown_and_canonical_read_passthrough(){
 let(origin,request,task)=http("HTTP/1.1 503 Service Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");
 let body=format!(r#"{{ "snapshotId":"ownership-source","snapshotHash":"{}","diffHash":"{}","confirm":true }}"#,"a".repeat(64),"b".repeat(64));
 let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"apply_ownership",json!({"company":"synthetic","body":body,"confirm":true}))]));let error=&rows[1]["result"]["structuredContent"]["error"];assert_eq!(error["code"],"outcome_unknown");assert!(error.get("idempotencyKey").is_none());let wire=request.recv().unwrap();let end=wire.windows(4).position(|w|w==b"\r\n\r\n").unwrap()+4;assert!(wire.starts_with(b"POST /api/companies/synthetic/ownership/apply HTTP/1.1"));assert_eq!(&wire[end..],body.as_bytes());task.join().unwrap();
 let(origin,request,task)=http("HTTP/1.1 200 OK\r\nContent-Length: 120\r\nConnection:close\r\n\r\n{\"ok\":true,\"asOf\":\"2026-06-01\",\"facts\":[],\"partial\":true,\"consolidation\":{\"eligible\":false,\"reason\":\"hidden endpoints\"}}");
 let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"ownership",json!({"company":"synthetic","asOf":"2026-06-01"}))]));let data=&rows[1]["result"]["structuredContent"]["data"];assert_eq!(data["partial"],true);assert_eq!(data["consolidation"]["eligible"],false);assert!(request.recv().unwrap().starts_with(b"GET /api/companies/synthetic/ownership?asOf=2026-06-01 HTTP/1.1"));task.join().unwrap();
}
#[test]
fn ownership_proposal_dedicated_frame_preserves_exact_body_and_other_limits(){
 let(origin,request,task)=http("HTTP/1.1 200 OK\r\nContent-Length:11\r\nConnection:close\r\n\r\n{\"ok\":true}");
 let body=format!("{{\"source\":\"Synthetic source\",\"observedAt\":\"2026-01-01\",\"facts\":[{{\"owner\":{{\"kind\":\"party\",\"partyId\":\"party-synthetic\"}},\"ownedCompanySlug\":\"synthetic\",\"validFrom\":\"2000-01-01\",\"economicBasisPoints\":1000,\"controlType\":\"equity\",\"jurisdiction\":\"DK\",\"evidenceRefs\":[\"synthetic-brief\"]}}],\"confirm\":true}}{}"," ".repeat(1_100_000));
 let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"propose_ownership",json!({"company":"synthetic","body":body,"confirm":true}))]));assert_eq!(rows[1]["result"]["structuredContent"]["ok"],true);let wire=request.recv().unwrap();let end=wire.windows(4).position(|w|w==b"\r\n\r\n").unwrap()+4;assert!(wire.starts_with(b"POST /api/companies/synthetic/ownership/propose HTTP/1.1"));assert_eq!(&wire[end..],body.as_bytes());task.join().unwrap();
 for name in ["review_ownership","apply_ownership","ownership_history"]{let output=run("http://127.0.0.1:9",&[call(2,name,json!({"company":"synthetic","body":" ".repeat(1048576),"confirm":true}))]);assert!(!output.status.success());let row:Value=serde_json::from_slice(&output.stdout).unwrap();assert_eq!(row["error"]["code"],-32700);}
 let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),call(2,"propose_ownership",json!({"company":"synthetic","body":" ".repeat(rentemester_client::MAX_OWNERSHIP_PROPOSE_BODY+1),"confirm":true}))]));assert_eq!(rows[1]["result"]["structuredContent"]["error"]["code"],"invalid_input");
 let output=run("http://127.0.0.1:9",&[call(2,"propose_ownership",json!({"company":"synthetic","body":" ".repeat(2*rentemester_client::MAX_OWNERSHIP_PROPOSE_BODY+65536),"confirm":true}))]);assert!(!output.status.success());let row:Value=serde_json::from_slice(&output.stdout).unwrap();assert_eq!(row["error"]["code"],-32700);
}

#[test]
fn intercompany_six_tools_preserve_prior_registry_and_strict_companyless_global_read(){
 let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})]));let tools=rows[1]["result"]["tools"].as_array().unwrap();assert_eq!(tools.len(),EXPECTED_NATIVE_TOOLS);
 for(name,read)in[("intercompany",true),("group_reconciliation",true),("apply_group_manifest",false),("propose_intercompany_mapping",false),("approve_intercompany_mapping",false),("revoke_intercompany_mapping",false)]{let t=tools.iter().find(|t|t["name"]==name).unwrap();assert_eq!(t["annotations"]["readOnlyHint"],read);assert_eq!(t["inputSchema"]["additionalProperties"],false);if !read{assert_eq!(t["inputSchema"]["properties"]["body"]["maxLength"],262144);}}
 for name in ["ownership","corporate_records","workspace_inbox","company_knowledge","companies","post_journal"]{assert!(tools.iter().any(|t|t["name"]==name));}
 for(name,args)in[("group_reconciliation",json!({})),("group_reconciliation",json!({"company":"synthetic","asOf":"2026-01-01"})),("intercompany",json!({"company":"synthetic","confirm":true})),("approve_intercompany_mapping",json!({"company":"synthetic","body":"{}","confirm":true,"actor":"forged"}))]{let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),call(2,name,args)]));assert_eq!(rows[1]["error"]["code"],-32602);}
}
#[test]
fn intercompany_unknown_mutation_exact_body_and_decimal_source_projection(){
 let(origin,request,task)=http("HTTP/1.1 503 Service Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");let body=format!(r#"{{ "mappingId":"synthetic-map","mappingHash":"{}","confirm":true }}"#,"a".repeat(64));let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"approve_intercompany_mapping",json!({"company":"synthetic","body":body,"confirm":true}))]));assert_eq!(rows[1]["result"]["structuredContent"]["error"]["code"],"outcome_unknown");let wire=request.recv().unwrap();let end=wire.windows(4).position(|w|w==b"\r\n\r\n").unwrap()+4;assert!(wire.starts_with(b"POST /api/companies/synthetic/intercompany/approve HTTP/1.1"));assert_eq!(&wire[end..],body.as_bytes());task.join().unwrap();
 let(origin,request,task)=http(concat!("HTTP/1.1 200 OK\r\nContent-Length:185\r\nConnection:close\r\n\r\n",r#"{"ok":true,"scope":"intercompany-reconciliation","asOf":"2026-06-01","rows":[{"difference":"0.00","left":{"balance":"125.50","sourceRefs":[{"amount":"125.50"}]}}],"futureEvidence":true}"#));let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"group_reconciliation",json!({"asOf":"2026-06-01"}))]));let data=&rows[1]["result"]["structuredContent"]["data"];assert_eq!(data["rows"][0]["difference"],"0.00");assert_eq!(data["rows"][0]["left"]["balance"],"125.50");assert_eq!(data["rows"][0]["left"]["sourceRefs"][0]["amount"],"125.50");assert_eq!(data["futureEvidence"],true);assert!(request.recv().unwrap().starts_with(b"GET /api/group-reconciliation?asOf=2026-06-01 HTTP/1.1"));task.join().unwrap();
}
#[test]
fn intercompany_native_confirm_size_and_ordinary_frame_limits_remain_fail_closed(){
 for args in [json!({"company":"synthetic","body":"{}","confirm":false}),json!({"company":"synthetic","body":" ".repeat(262145),"confirm":true})]{let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),call(2,"apply_group_manifest",args)]));assert_eq!(rows[1]["result"]["structuredContent"]["error"]["code"],"invalid_input");}
 for name in ["apply_group_manifest","propose_intercompany_mapping","approve_intercompany_mapping","revoke_intercompany_mapping"]{let output=run("http://127.0.0.1:9",&[call(2,name,json!({"company":"synthetic","body":" ".repeat(1048576),"confirm":true}))]);assert!(!output.status.success());let row:Value=serde_json::from_slice(&output.stdout).unwrap();assert_eq!(row["error"]["code"],-32700);}
}
#[test]
fn consolidation_six_tools_preserve_existing_registry_and_strict_global_selectors(){
 let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})]));let tools=rows[1]["result"]["tools"].as_array().unwrap();assert_eq!(tools.len(),EXPECTED_NATIVE_TOOLS);
 for(name,read)in[("consolidation",true),("group_report_profiles",true),("group_consolidated_report",true),("propose_consolidation_profile",false),("approve_consolidation_profile",false),("revoke_consolidation_profile",false)]{let t=tools.iter().find(|t|t["name"]==name).unwrap();assert_eq!(t["annotations"]["readOnlyHint"],read);assert_eq!(t["inputSchema"]["additionalProperties"],false);if !read{assert_eq!(t["inputSchema"]["properties"]["body"]["maxLength"],if name=="propose_consolidation_profile"{1048576}else{32768});}}
 for name in ["intercompany","group_reconciliation","propose_ownership","ingest_document","ingest_workspace_inbox","ingest_corporate_record","workspace_backup"]{assert!(tools.iter().any(|t|t["name"]==name));}
 for(name,args)in[("consolidation",json!({"company":"synthetic"})),("group_report_profiles",json!({"company":"synthetic","asOf":"2026-01-01"})),("group_consolidated_report",json!({"profileId":"p","asOf":"2026-01-01"})),("approve_consolidation_profile",json!({"company":"synthetic","body":"{}","confirm":true,"actor":"forged"}))]{let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),call(2,name,args)]));assert_eq!(rows[1]["error"]["code"],-32602);}
}
#[test]
fn consolidation_unknown_exact_hash_body_and_full_global_decimal_report(){
 let(origin,request,task)=http("HTTP/1.1 503 Service Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");let body=format!(r#"{{ "profileId":"synthetic-profile","profileHash":"{}","confirm":true }}"#,"a".repeat(64));let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"approve_consolidation_profile",json!({"company":"synthetic","body":body,"confirm":true}))]));assert_eq!(rows[1]["result"]["structuredContent"]["error"]["code"],"outcome_unknown");let wire=request.recv().unwrap();let end=wire.windows(4).position(|w|w==b"\r\n\r\n").unwrap()+4;assert!(wire.starts_with(b"POST /api/companies/synthetic/consolidation/approve HTTP/1.1"));assert_eq!(&wire[end..],body.as_bytes());task.join().unwrap();
 let reply=r#"{"ok":true,"lines":[{"amount":"125.50"}],"futureSource":{"redacted":false,"hash":"proof"}}"#;let response=format!("HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",reply.len());let(origin,request,task)=http(Box::leak(response.into_boxed_str()));let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"group_consolidated_report",json!({"profileId":"synthetic-profile","from":"2026-01-01","asOf":"2026-06-01"}))]));assert_eq!(rows[1]["result"]["structuredContent"]["data"],serde_json::from_str::<Value>(reply).unwrap());assert!(request.recv().unwrap().starts_with(b"GET /api/group-consolidated-report?profileId=synthetic-profile&from=2026-01-01&asOf=2026-06-01 HTTP/1.1"));task.join().unwrap();
}
#[test]
fn consolidation_only_proposal_gets_bounded_large_frame_and_native_bodies_stay_bounded(){

 let profile=json!({"version":1,"id":"synthetic-profile","groupId":"synthetic-group","currency":"DKK","validFrom":"2026-01-01","reportingLines":[{"id":"asset","label":"Asset","section":"asset","displayOrder":0},{"id":"liability","label":"Liability","section":"liability","displayOrder":1},{"id":"equity","label":"Equity","section":"equity","displayOrder":2,"role":"current-result"},{"id":"income","label":"Income","section":"income","displayOrder":3},{"id":"expense","label":"Expense","section":"expense","displayOrder":4}],"accountMappings":[{"id":"map","companySlug":"synthetic","accountNo":"source","reportingLineId":"asset","validFrom":"2026-01-01"}],"evidenceRefs":["reviewed-source"]});
 let body=format!("{}{}",json!({"profile":profile,"confirm":true}),"\n".repeat(600000));
 let(origin,request,task)=http("HTTP/1.1 200 OK\r\nContent-Length:11\r\nConnection:close\r\n\r\n{\"ok\":true}");let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"propose_consolidation_profile",json!({"company":"synthetic","body":body,"confirm":true}))]));assert_eq!(rows[1]["result"]["structuredContent"]["ok"],true);let wire=request.recv().unwrap();let end=wire.windows(4).position(|w|w==b"\r\n\r\n").unwrap()+4;assert!(wire.starts_with(b"POST /api/companies/synthetic/consolidation/propose HTTP/1.1"));assert_eq!(&wire[end..],body.as_bytes());task.join().unwrap();
 // A >1MiB escaped frame reaches only this proposal's native body validation (not HTTP).
 let body=format!("{}{}", "\\".repeat(540000)," ");let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),call(2,"propose_consolidation_profile",json!({"company":"synthetic","body":body,"confirm":true}))]));assert_eq!(rows[1]["result"]["structuredContent"]["error"]["code"],"invalid_input");
 for name in ["approve_consolidation_profile","revoke_consolidation_profile","group_report_profiles"]{let output=run("http://127.0.0.1:9",&[call(2,name,json!({"company":"synthetic","body":" ".repeat(1048576),"confirm":true}))]);assert!(!output.status.success());let row:Value=serde_json::from_slice(&output.stdout).unwrap();assert_eq!(row["error"]["code"],-32700);}
 for(name,limit)in[("propose_consolidation_profile",1048576),("approve_consolidation_profile",32768),("revoke_consolidation_profile",32768)]{let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),call(2,name,json!({"company":"synthetic","body":" ".repeat(limit+1),"confirm":true}))]));assert_eq!(rows[1]["result"]["structuredContent"]["error"]["code"],"invalid_input");}
 let output=run("http://127.0.0.1:9",&[call(2,"propose_consolidation_profile",json!({"company":"synthetic","body":" ".repeat(2*1048576+65536),"confirm":true}))]);assert!(!output.status.success());let row:Value=serde_json::from_slice(&output.stdout).unwrap();assert_eq!(row["error"]["code"],-32700);
}

#[test]
fn elimination_eight_tools_readonly_plan_strict_discovery_and_confirmation(){
 let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})]));let tools=rows[1]["result"]["tools"].as_array().unwrap();assert_eq!(tools.len(),EXPECTED_NATIVE_TOOLS);
 for(name,read)in[("eliminations",true),("group_eliminations",true),("plan_elimination",true),("propose_elimination",false),("approve_elimination",false),("reject_elimination",false),("apply_elimination",false),("reverse_elimination",false)]{let tool=tools.iter().find(|t|t["name"]==name).unwrap();assert_eq!(tool["annotations"]["readOnlyHint"],read);assert_eq!(tool["inputSchema"]["additionalProperties"],false);if name=="plan_elimination"{assert!(tool["inputSchema"]["properties"].get("confirm").is_none());}}
 for(name,args)in[("plan_elimination",json!({"company":"synthetic","body":"{}","confirm":true})),("group_eliminations",json!({"company":"synthetic","asOf":"2026-06-01"})),("propose_elimination",json!({"company":"synthetic","body":"{}"})),("apply_elimination",json!({"company":"synthetic","body":"{}","confirm":true,"actor":"forged"}))]{let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),call(2,name,args)]));assert_eq!(rows[1]["error"]["code"],-32602);}
}
#[test]
fn elimination_plan_and_write_exact_wire_unknown_semantics_and_ordinary_frames(){
 let input=r#"{ "id":"own","mappingId":"map","asOf":"2026-06-01","evidenceRefs":["ref"] }"#;
 let(origin,request,task)=http("HTTP/1.1 503 Service Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"plan_elimination",json!({"company":"synthetic","body":input}))]));assert_eq!(rows[1]["result"]["structuredContent"]["error"]["code"],"server_refused");let wire=request.recv().unwrap();assert!(wire.starts_with(b"POST /api/companies/synthetic/eliminations/plan HTTP/1.1"));assert!(wire.ends_with(input.as_bytes()));task.join().unwrap();
 let body=format!(r#"{{ "eliminationId":"own","payloadHash":"{}","confirm":true }}"#,"a".repeat(64));let(origin,request,task)=http("HTTP/1.1 503 Service Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"apply_elimination",json!({"company":"synthetic","body":body,"confirm":true}))]));assert_eq!(rows[1]["result"]["structuredContent"]["error"]["code"],"outcome_unknown");let wire=request.recv().unwrap();assert!(wire.starts_with(b"POST /api/companies/synthetic/eliminations/apply HTTP/1.1"));assert!(wire.ends_with(body.as_bytes()));task.join().unwrap();
 let out=run("http://127.0.0.1:9",&[call(2,"plan_elimination",json!({"company":"synthetic","body":" ".repeat(1048576)}))]);assert!(!out.status.success());let row:Value=serde_json::from_slice(&out.stdout).unwrap();assert_eq!(row["error"]["code"],-32700);
}
#[path="stdio/scheduled_recurring.rs"]
mod scheduled_recurring;

#[path="stdio/hosted_services.rs"]
mod hosted_services;

#[path="stdio/invoice_email.rs"]
mod invoice_email;

#[path="stdio/workspace_transfer.rs"]
mod workspace_transfer;

#[path="stdio/bookkeeping_batch.rs"]
mod bookkeeping_batch;

#[path="stdio/payable_correction.rs"]
mod payable_correction;

#[path="stdio/intercompany_dispositions.rs"]
mod intercompany_dispositions;

#[path="stdio/contact_import.rs"]
mod contact_import;

#[path="stdio/cfo_analytics.rs"]
mod cfo_analytics;

#[path="stdio/archive_import.rs"]
mod archive_import;
#[path="stdio/report_reads.rs"]
mod report_reads;

#[path="stdio/opening_balance.rs"]
mod opening_balance;

#[path="stdio/invoice_extraction.rs"]
mod invoice_extraction;

#[path="stdio/bank_profile.rs"]
mod bank_profile;

#[path="stdio/bilagsmail.rs"]
mod bilagsmail;

#[path="stdio/public_einvoice.rs"]
mod public_einvoice;

#[test]
fn portfolio_cashflow_tools_are_readonly_and_exactly_scoped() {
 let output=run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),call(3,"portfolio",json!({"company":"hidden"})),call(4,"cashflow",json!({"company":"demo","confirm":true}))]);
 let values=parsed(&output);let tools=values.iter().find(|v|v["id"]==2).unwrap()["result"]["tools"].as_array().unwrap();
 for name in ["portfolio","cashflow"]{let tool=tools.iter().find(|v|v["name"]==name).unwrap();assert_eq!(tool["annotations"]["readOnlyHint"],true);assert_eq!(tool["inputSchema"]["additionalProperties"],false);}
 for id in [3,4]{assert_eq!(values.iter().find(|v|v["id"]==id).unwrap()["error"]["code"],-32602);}
}

#[test]
fn eu_sales_and_vies_discovery_require_explicit_bounds_and_confirmation() {
    let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),call(3,"vies_validate",json!({"company":"synthetic","vatOrCvr":"DE123456789"})),call(4,"vies_validate",json!({"company":"synthetic","vatOrCvr":"DE123456789","confirm":true,"valid":true})),call(5,"eu_sales",json!({"company":"synthetic","from":"2026-01-01"}))]));
    let tools=rows[1]["result"]["tools"].as_array().unwrap();
    for (name,read) in [("eu_sales",true),("vies_validate",false)] {
        let tool=tools.iter().find(|t|t["name"]==name).unwrap();
        assert_eq!(tool["annotations"]["readOnlyHint"],read);
        assert_eq!(tool["inputSchema"]["additionalProperties"],false);
    }
    for row in &rows[2..] {assert_eq!(row["error"]["code"],-32602);}
}

#[test]
fn oss_discovery_is_read_only_and_requires_exact_date_arguments() {
 let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),call(3,"vat_oss_report",json!({"company":"synthetic","from":"2026-01-01"})),call(4,"vat_oss_report",json!({"company":"synthetic","from":"2026-01-01","to":"2026-06-30","confirm":true}))]));
 let tools=rows[1]["result"]["tools"].as_array().unwrap();
 let tool=tools.iter().find(|t|t["name"]=="vat_oss_report").unwrap();
 assert_eq!(tools.len(),EXPECTED_NATIVE_TOOLS);
 assert_eq!(tool["annotations"]["readOnlyHint"],true);
 assert_eq!(tool["inputSchema"]["additionalProperties"],false);
 assert_eq!(tool["inputSchema"]["required"],json!(["company","from","to"]));
 assert_eq!(rows[2]["error"]["code"],-32602);
 assert_eq!(rows[3]["error"]["code"],-32602);
}

#[test]
fn exception_tools_discover_source_contract_and_reject_unadmitted_writes() {
    let rows = parsed(&run("http://127.0.0.1:9", &[
        init("2025-11-25"), ready(),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        call(3,"exceptions_list",json!({"company":"synthetic","includeArchived":"true"})),
        call(4,"exception_resolve",json!({"company":"synthetic","id":1,"note":"review"})),
        call(5,"exception_resolve",json!({"company":"synthetic","id":1,"confirm":true,"actor":"agent:forged"})),
        call(6,"exception_resolve",json!({"company":"synthetic","id":0,"confirm":true})),
    ]));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(),EXPECTED_NATIVE_TOOLS);
    for (name,read) in [("exceptions_list",true),("exception_resolve",false)] {
        let tool=tools.iter().find(|tool|tool["name"]==name).unwrap();
        assert_eq!(tool["annotations"]["readOnlyHint"],read);
        assert_eq!(tool["inputSchema"]["additionalProperties"],false);
    }
    for row in &rows[2..] { assert!(row.get("error").is_some() || row["result"]["isError"]==true); }
}

#[test]
fn raw_bank_import_named_frame_forwards_large_body_without_relaxing_other_tools() {
    let (origin, request, task) = http("HTTP/1.1 200 OK\r\nContent-Length:11\r\nConnection:close\r\n\r\n{\"ok\":true}");
    let body=serde_json::to_string(&json!({"confirm":true,"csvBase64":"AAAA".repeat(280_000),"profile":"danske-bank"})).unwrap();
    let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"import_bank",json!({"company":"synthetic","body":body,"confirm":true}))]));
    assert_eq!(rows[1]["result"]["structuredContent"]["ok"],true);
    let wire=request.recv().unwrap();
    let end=wire.windows(4).position(|w|w==b"\r\n\r\n").unwrap()+4;
    assert!(wire.starts_with(b"POST /api/companies/synthetic/bank/import HTTP/1.1"));
    assert_eq!(&wire[end..],body.as_bytes());
    task.join().unwrap();
    let output=run("http://127.0.0.1:9",&[call(2,"create_bank_account",json!({"company":"synthetic","body":" ".repeat(1_100_000),"confirm":true}))]);
    assert!(!output.status.success());
    let value:Value=serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"],-32700);
}

#[test]
fn bank_match_read_discovery_and_source_business_refusal_are_preserved() {
    let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),call(3,"bank_suggest_matches",json!({"company":"synthetic","confirm":true})),call(4,"bank_suggest_matches",json!({"company":"synthetic","max":0}))]));
    let tools=rows[1]["result"]["tools"].as_array().unwrap();assert_eq!(tools.len(),EXPECTED_NATIVE_TOOLS);
    let tool=tools.iter().find(|tool|tool["name"]=="bank_suggest_matches").unwrap();
    assert_eq!(tool["annotations"]["readOnlyHint"],true);assert_eq!(tool["inputSchema"]["additionalProperties"],false);
    for row in &rows[2..]{assert!(row.get("error").is_some() || row["result"]["isError"]==true);}
    let reply=r#"{"ok":false,"code":"NOT_FOUND","errors":["unmatched bank transaction 7 does not exist"]}"#;
    let response=format!("HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",reply.len());
    let(origin,wire,task)=http(Box::leak(response.into_boxed_str()));
    let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"bank_suggest_matches",json!({"company":"synthetic","bankTransactionId":7,"max":2}))]));
    assert_eq!(rows[1]["result"]["structuredContent"],serde_json::from_str::<Value>(reply).unwrap(),"{rows:?}");
    assert_eq!(rows[1]["result"]["isError"],true);
    assert_eq!(serde_json::from_str::<Value>(rows[1]["result"]["content"][0]["text"].as_str().unwrap()).unwrap(),serde_json::from_str::<Value>(reply).unwrap());
    assert!(wire.recv().unwrap().starts_with(b"GET /api/companies/synthetic/bank/original/suggestions?bankTransactionId=7&max=2 HTTP/1.1"));task.join().unwrap();
}

#[test]
fn cvr_tools_preserve_inner_business_failures_and_explicit_provider_actions() {
    for (name, args, response, expected, verb, path) in [
        (
            "cvr_status",
            json!({}),
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"ok\":true,\"cvrStatus\":{\"configured\":false}}",
            json!({"configured":false}),
            "GET",
            "/api/system/cvr-status",
        ),
        (
            "cvr_lookup",
            json!({"company":"synthetic","cvr":"DK 12345678","confirm":true}),
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"ok\":true,\"cvr\":{\"ok\":false,\"cached\":false,\"company\":null,\"errors\":[\"synthetic provider unavailable\"]}}",
            json!({"ok":false,"cached":false,"company":null,"errors":["synthetic provider unavailable"]}),
            "GET",
            "/api/companies/synthetic/cvr-lookup?cvr=DK+12345678",
        ),
        (
            "company_sync_cvr",
            json!({"company":"synthetic","confirm":true}),
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"ok\":true,\"sync\":{\"ok\":false,\"errors\":[\"synthetic invalid company CVR\"]}}",
            json!({"ok":false,"errors":["synthetic invalid company CVR"]}),
            "POST",
            "/api/companies/synthetic/sync-cvr",
        ),
    ] {
        let (origin, requests, server) = http(response);
        let result = parsed(&run(
            &origin,
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        let value = result.iter().find(|r| r["id"] == 2).unwrap();
        assert_eq!(value["result"]["structuredContent"]["data"], expected);
        assert_eq!(
            value["result"]["isError"],
            expected.get("ok").is_some_and(|v| *v == false)
        );
        let request = requests.recv().unwrap();
        let request = String::from_utf8(request).unwrap();
        assert!(
            request.starts_with(&format!("{verb} {path} HTTP/1.1\r\n")),
            "{request}"
        );
        server.join().unwrap();
    }
}

#[test]
fn service_file_mcp_context_and_mutually_exclusive_flags() {
    let key = Cookie::new();
    std::fs::write(&key.0, b"synthetic-private-service\n").unwrap();
    let (origin, request, task) =
        http("HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}");
    let messages = [init("2025-11-25"), ready(), call(2, "me", json!({}))];
    let mut child = Command::new(BIN)
        .args([
            "--server",
            &origin,
            "--allow-loopback-http",
            "--service-key-file",
            key.0.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            messages
                .iter()
                .map(|v| format!("{v}\n"))
                .collect::<String>()
                .as_bytes(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let rows = parsed(&output);
    assert_eq!(rows[1]["result"]["structuredContent"]["data"]["ok"], true);
    let wire = String::from_utf8(request.recv().unwrap())
        .unwrap()
        .to_ascii_lowercase();
    assert!(wire.starts_with("get /api/me "));
    assert!(wire.contains("x-rentemester-service-key: synthetic-private-service\r\n"));
    assert!(!wire.contains("\r\ncookie:") && !wire.contains("\r\nauthorization:"));
    task.join().unwrap();
    assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic-private-service"));
    for flags in [
        vec![
            "--cookie-file",
            key.0.to_str().unwrap(),
            "--service-key-file",
            key.0.to_str().unwrap(),
        ],
        vec![
            "--service-key-file",
            key.0.to_str().unwrap(),
            "--service-key-file",
            key.0.to_str().unwrap(),
        ],
        vec!["--service-key", "synthetic-private-service"],
    ] {
        let output = Command::new(BIN)
            .args(["--server", "http://127.0.0.1:9", "--allow-loopback-http"])
            .args(flags)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("synthetic-private-service"));
    }
}

#[path="stdio/dinero_import.rs"]
mod dinero_import;

#[path="stdio/dashboard_artifact.rs"]
mod dashboard_artifact;

#[path="stdio/recurring_original.rs"]
mod recurring_original;

#[path="stdio/journal_original.rs"]
mod journal_original;

#[path="stdio/period_original.rs"]
mod period_original;

#[path="stdio/exceptions_original.rs"]
mod exceptions_original;

#[path="stdio/master_data_original.rs"]
mod master_data_original;

#[path="stdio/documents_original.rs"]
mod documents_original;

#[path = "stdio/document_party_original.rs"]
mod document_party_original;

#[path = "stdio/expense_original.rs"]
mod expense_original;

#[path="stdio/accrual_original.rs"]
mod accrual_original;

#[path="stdio/asset_original.rs"]
mod asset_original;

#[path = "stdio/mileage_original.rs"]
mod mileage_original;
#[path = "stdio/budget_original.rs"]
mod budget_original;
