use super::*;
#[test]
fn invoice_email_discovery_162_exact_scope_body_and_no_unknown_argument_echo() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), EXPECTED_NATIVE_TOOLS);
    for name in [
        "invoice_email_status",
        "invoice_email_send",
        "invoice_email_reconcile",
        "invoice_email_retry",
    ] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(
            t["annotations"]["readOnlyHint"],
            name == "invoice_email_status"
        );
        if name != "invoice_email_status" {
            assert_eq!(t["annotations"]["idempotentHint"], false);
            assert_eq!(t["inputSchema"]["properties"]["body"]["maxLength"], 16384);
        }
    }
    for (name, args) in [
        (
            "invoice_email_status",
            json!({"company":"synthetic","confirm":true}),
        ),
        (
            "invoice_email_status",
            json!({"company":"synthetic","id":7}),
        ),
        (
            "invoice_email_send",
            json!({"company":"synthetic","body":"{}"}),
        ),
        (
            "invoice_email_retry",
            json!({"company":"synthetic","body":"{}","confirm":true,"smtpPassword":"must-not-echo"}),
        ),
        (
            "invoice_email_reconcile",
            json!({"company":"synthetic","body":" ".repeat(16385),"confirm":true}),
        ),
    ] {
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602);
        assert!(!rows[1].to_string().contains("must-not-echo"));
    }
}
#[test]
fn invoice_email_canonical_unknown_refused_and_reconciliation_receipts_are_full_mcp_results() {
    for (name, path, body, status, reply) in [
        (
            "invoice_email_send",
            "invoices/send-email",
            json!({"invoiceDocumentId":7,"to":"buyer@example.invalid","confirm":true}),
            "409 Conflict",
            r#"{"ok":false,"code":"delivery_unknown","delivery":{"state":"unknown","history":[{"future":"kept"}]}}"#,
        ),
        (
            "invoice_email_retry",
            "invoices/email-deliveries/retry",
            json!({"messageId":format!("<{}@rentemester.local>","a".repeat(32)),"expectedStateHash":"b".repeat(64),"reason":"Reviewed definite refusal","confirm":true}),
            "422 Unprocessable",
            r#"{"ok":false,"code":"delivery_refused","delivery":{"state":"refused","history":[{"future":"kept"}]}}"#,
        ),
        (
            "invoice_email_reconcile",
            "invoices/email-deliveries/reconcile",
            json!({"messageId":format!("<{}@rentemester.local>","a".repeat(32)),"expectedStateHash":"b".repeat(64),"reason":"Reviewed external evidence","evidence":"SMTP log synthetic001","resolution":"accepted","confirm":true}),
            "200 OK",
            r#"{"ok":true,"delivery":{"state":"accepted","humanResolution":"accepted","history":[]}}"#,
        ),
    ] {
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        );
        let (origin, request, t) = http(Box::leak(response.into_boxed_str()));
        let bytes = format!(" {body}\n");
        let rows = parsed(&run(
            &origin,
            &[
                init("2025-11-25"),
                ready(),
                call(
                    2,
                    name,
                    json!({"company":"synthetic","body":bytes,"confirm":true}),
                ),
            ],
        ));
        let canonical: Value = serde_json::from_str(reply).unwrap();
        assert_eq!(rows[1]["result"]["structuredContent"]["data"], canonical);
        assert_eq!(rows[1]["result"]["isError"], canonical["ok"] == false);
        let wire = String::from_utf8(request.recv().unwrap()).unwrap();
        assert!(wire.starts_with(&format!("POST /api/companies/synthetic/{path} HTTP/1.1")));
        assert!(wire.ends_with(&bytes));
        t.join().unwrap();
    }
}
