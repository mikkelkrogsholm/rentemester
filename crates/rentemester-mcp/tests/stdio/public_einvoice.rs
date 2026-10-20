use super::*;
#[test]
fn public_invoice_tools_publish_five_strict_source_operations() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 214);
    for (name, read) in [
        ("public_invoice_config", true),
        ("public_invoice_submissions", true),
        ("register_public_invoice", false),
        ("send_public_invoice", false),
        ("refresh_public_invoice_status", false),
    ] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(t["annotations"]["readOnlyHint"], read);
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        if !read {
            assert_eq!(t["inputSchema"]["properties"]["body"]["maxLength"], 16384);
            assert_eq!(
                t["inputSchema"]["properties"]["confirm"],
                json!({"const":true})
            );
        }
    }
}
#[test]
fn public_invoice_stdio_confirmation_and_authority_fields_refuse_locally() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "public_invoice_config",
                json!({"company":"synthetic","confirm":true}),
            ),
            call(
                3,
                "register_public_invoice",
                json!({"company":"synthetic","body":"{\"confirm\":true}"}),
            ),
            call(
                4,
                "send_public_invoice",
                json!({"company":"synthetic","body":"{\"invoiceDocumentId\":0,\"confirm\":true}","confirm":true}),
            ),
            call(
                5,
                "refresh_public_invoice_status",
                json!({"company":"synthetic","body":"{\"invoiceDocumentId\":7,\"confirm\":false}","confirm":true}),
            ),
            call(
                6,
                "register_public_invoice",
                json!({"company":"synthetic","body":"{\"confirm\":true}","confirm":true,"companyKey":"forbidden"}),
            ),
        ],
    ));
    for row in &rows[1..] {
        assert!(row.get("error").is_some() || row["result"]["isError"] == true);
        assert_ne!(
            row["result"]["structuredContent"]["error"]["code"],
            "transport"
        );
        assert_ne!(
            row["result"]["structuredContent"]["error"]["code"],
            "outcome_unknown"
        );
    }
}
#[test]
fn public_invoice_stdio_keeps_exact_post_body_and_prepared_receipt() {
    let reply = json!({"ok":true,"submission":{"state":"prepared","transmissionId":"synthetic-transmission","redeliveryBlocked":true}});
    let text = reply.to_string();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{text}",
        text.len()
    );
    let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
    let body = r#"{ "confirm":true,"invoiceDocumentId":7 }"#;
    let rows = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "send_public_invoice",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(rows[1]["result"]["structuredContent"]["data"], reply);
    let wire = request.recv().unwrap();
    assert!(wire.starts_with(b"POST /api/companies/synthetic/invoices/send-public HTTP/1.1"));
    assert!(wire.ends_with(body.as_bytes()));
    task.join().unwrap();
}
