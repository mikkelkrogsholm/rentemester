use super::*;
#[test]
fn invoice_extraction_tools_publish_exact_source_names_bounds_and_confirmation() {
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
    for (name, read) in [
        ("documents_invoice_extraction", true),
        ("documents_extract_invoice", false),
    ] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(t["annotations"]["readOnlyHint"], read);
        assert_eq!(t["annotations"]["idempotentHint"], true);
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        assert_eq!(
            t["inputSchema"]["properties"]["documentId"],
            json!({"type":"integer","minimum":1,"maximum":9007199254740991u64})
        );
        let required = vec![json!("company"), json!("documentId")];
        if !read {
            assert_eq!(
                t["inputSchema"]["properties"]["confirm"],
                json!({"type":"boolean"})
            );
        } else {
            assert!(t["inputSchema"]["properties"].get("confirm").is_none());
        }
        assert_eq!(t["inputSchema"]["required"], Value::Array(required));
    }
}
#[test]
fn invoice_extraction_admission_rejects_missing_confirmation_extra_authority_and_unsafe_ids() {
    let mut messages = vec![init("2025-11-25"), ready()];
    let mut id = 2;
    for (name, args) in [
        (
            "documents_extract_invoice",
            json!({"company":"synthetic","documentId":7}),
        ),
        (
            "documents_extract_invoice",
            json!({"company":"synthetic","documentId":7,"confirm":false}),
        ),
        (
            "documents_extract_invoice",
            json!({"company":"synthetic","documentId":7,"confirm":null}),
        ),
        (
            "documents_extract_invoice",
            json!({"company":"synthetic","documentId":7,"confirm":true,"actor":"user:forged"}),
        ),
        (
            "documents_extract_invoice",
            json!({"company":"synthetic","documentId":7,"confirm":true,"provider":"http-json-v1"}),
        ),
        (
            "documents_invoice_extraction",
            json!({"company":"synthetic","documentId":7,"confirm":null}),
        ),
        (
            "documents_invoice_extraction",
            json!({"company":"synthetic","documentId":7,"confirm":true}),
        ),
        (
            "documents_invoice_extraction",
            json!({"company":"synthetic","documentId":7,"body":"{}"}),
        ),
        (
            "documents_extract_invoice",
            json!({"company":"synthetic","documentId":0,"confirm":true}),
        ),
        (
            "documents_extract_invoice",
            json!({"company":"synthetic","documentId":9007199254740992u64,"confirm":true}),
        ),
        (
            "documents_invoice_extraction",
            json!({"company":"synthetic","documentId":1.5}),
        ),
        (
            "documents_invoice_extraction",
            json!({"company":"../other","documentId":7}),
        ),
    ] {
        messages.push(call(id, name, args));
        id += 1;
    }
    let rows = parsed(&run("http://127.0.0.1:9", &messages));
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
fn invoice_extraction_stdio_exact_post_and_read_preserve_full_evidence_without_local_interpretation()
 {
    for (name, method, path) in [
        ("documents_extract_invoice", "POST", "extract-invoice"),
        ("documents_invoice_extraction", "GET", "invoice-extraction"),
    ] {
        let value = json!({"ok":true,"extraction":{"attemptId":1,"documentId":7,"status":"needs_resolution","originalHash":"a".repeat(64),"fields":[{"key":"grossAmount","value":12.34,"confidence":0.79,"sourceText":"Original cited gross","page":1}],"resolutions":[{"fieldKey":"extraction_status","value":{"status":"needs_resolution","errors":["buyer conflict"]}}],"conflicts":["buyer conflict"],"exceptionState":{"id":1,"status":"open"}}});
        let reply = value.to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
            reply.len()
        );
        let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
        let mut args = json!({"company":"synthetic","documentId":7});
        if method == "POST" {
            args["confirm"] = json!(true);
        }
        let rows = parsed(&run(
            &origin,
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert_eq!(
            rows[1]["result"]["structuredContent"]["data"],
            json!({"extraction":value["extraction"]})
        );
        let wire = request
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(
            wire.starts_with(
                format!("{method} /api/companies/synthetic/documents/original/7/{path} HTTP/1.1")
                    .as_bytes()
            )
        );
        let split = wire.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        if method == "POST" {
            assert_eq!(&wire[split..], br#"{"confirm":true}"#);
        } else {
            assert!(wire[split..].is_empty());
        }
        task.join().unwrap();
    }
}
#[test]
fn invoice_extraction_disabled_is_canonical_but_other_temporary_write_failures_stay_unknown() {
    for (status, body, code) in [
        (
            503,
            r#"{"ok":false,"errors":["EXTRACTION_PROVIDER_UNAVAILABLE"]}"#,
            None,
        ),
        (
            503,
            r#"{"ok":false,"code":"internal"}"#,
            Some("outcome_unknown"),
        ),
        (200, "not-json", Some("outcome_unknown")),
        (
            403,
            r#"{"ok":true,"extraction":null}"#,
            Some("server_refused"),
        ),
    ] {
        let response = format!(
            "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
        let rows = parsed(&run(
            &origin,
            &[
                init("2025-11-25"),
                ready(),
                call(
                    2,
                    "documents_extract_invoice",
                    json!({"company":"synthetic","documentId":7,"confirm":true}),
                ),
            ],
        ));
        assert_eq!(rows[1]["result"]["isError"], true);
        if let Some(code) = code {
            assert_eq!(rows[1]["result"]["structuredContent"]["code"], code);
        } else {
            assert_eq!(
                rows[1]["result"]["structuredContent"],
                serde_json::from_str::<Value>(body).unwrap()
            );
        }
        assert!(
            request
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .starts_with(
                    b"POST /api/companies/synthetic/documents/original/7/extract-invoice HTTP/1.1"
                )
        );
        task.join().unwrap();
    }
}
#[test]
fn original_extraction_confirmation_envelope_and_rpc_argument_errors() {
    for args in [
        json!({"company":"synthetic","documentId":7}),
        json!({"company":"synthetic","documentId":7,"confirm":false}),
    ] {
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[
                init("2025-11-25"),
                ready(),
                call(2, "documents_extract_invoice", args),
            ],
        ));
        let r = &rows[1]["result"];
        assert_eq!(r["isError"], true);
        assert_eq!(r["structuredContent"]["code"], "CONFIRM_REQUIRED");
        assert!(r["structuredContent"].get("data").is_none());
        assert_eq!(
            serde_json::from_str::<Value>(r["content"][0]["text"].as_str().unwrap()).unwrap(),
            r["structuredContent"]
        );
    }
    for args in [
        json!({"company":" ","documentId":7,"confirm":true}),
        json!({"company":"synthetic","documentId":7,"confirm":null}),
        json!({"company":"synthetic","documentId":7,"confirm":true,"expectedActor":"user:forged"}),
        json!({"company":"synthetic","documentId":7,"confirm":true,"principal":"forged"}),
    ] {
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[
                init("2025-11-25"),
                ready(),
                call(2, "documents_extract_invoice", args),
            ],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602);
    }
}
