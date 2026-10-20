use super::*;
#[test]
fn transfer_discovery_170_and_strict_scope_proof_bounds() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 225);
    for (name, read, company, max) in [
        ("workspace_transfer_status", true, false, 0),
        ("restored_receipts", true, true, 0),
        ("workspace_transfer_freeze", false, false, 16384),
        ("workspace_transfer_cancel", false, false, 16384),
        ("workspace_transfer_export", false, false, 16384),
        ("workspace_transfer_release", false, false, 32768),
        ("workspace_transfer_activate", false, false, 32768),
        ("reconcile_restored_receipt", false, true, 16384),
    ] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(t["annotations"]["readOnlyHint"], read);
        assert_eq!(
            t["inputSchema"]["properties"].get("company").is_some(),
            company
        );
        if !read {
            assert_eq!(t["annotations"]["idempotentHint"], false);
            assert_eq!(t["inputSchema"]["properties"]["body"]["maxLength"], max);
        }
    }
    for (name, args) in [
        ("workspace_transfer_status", json!({"confirm":true})),
        ("workspace_transfer_status", json!({"company":"synthetic"})),
        ("restored_receipts", json!({})),
        ("workspace_transfer_freeze", json!({"body":"{}"})),
        (
            "workspace_transfer_release",
            json!({"body":"{}","confirm":true,"trustedPublicKey":"must-not-echo"}),
        ),
        (
            "workspace_transfer_activate",
            json!({"body":" ".repeat(32769),"confirm":true}),
        ),
        (
            "reconcile_restored_receipt",
            json!({"company":"synthetic","body":"{}","confirm":true,"targetUser":"unauthorized-alias"}),
        ),
    ] {
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602);
        assert!(!rows[1].to_string().contains("must-not-echo"));
        assert!(!rows[1].to_string().contains("unauthorized-alias"));
    }
}
#[test]
fn transfer_mcp_preserves_passive_reads_known_refusals_export_and_mapping() {
    for (name, path, body, status, mime, reply, company) in [
        (
            "workspace_transfer_status",
            "/api/workspace/transfer",
            None,
            "200 OK",
            "application/json",
            r#"{"ok":true,"transfer":{"mode":"staged","future":{"amount":"90071992547409.99"}}}"#,
            false,
        ),
        (
            "workspace_transfer_freeze",
            "/api/workspace/transfer/freeze",
            Some(
                json!({"expectedStateHash":"a".repeat(64),"destination":"https://synthetic-target.example.invalid","reason":" explicit source review ","confirm":true}),
            ),
            "409 Conflict",
            "application/json",
            r#"{"ok":false,"code":"source_already_moved","transfer":{"mode":"moved","release":"retained"}}"#,
            false,
        ),
        (
            "workspace_transfer_export",
            "/api/workspace/transfer/export",
            Some(json!({"expectedStateHash":"a".repeat(64),"confirm":true})),
            "200 OK",
            "application/vnd.rentemester.native-backup+json",
            "signed\r\narchive",
            false,
        ),
        (
            "reconcile_restored_receipt",
            "/api/companies/synthetic/restore/receipts/reconcile",
            Some(
                json!({"receiptId":7,"expectedStateHash":"a".repeat(64),"reason":" exact source receipt review ","confirm":true}),
            ),
            "200 OK",
            "application/json",
            r#"{"ok":true,"restoredReceipts":{"rows":[{"receiptId":7,"mapped":true}]}}"#,
            true,
        ),
    ] {
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type:{mime}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        );
        let (origin, request, t) = http(Box::leak(response.into_boxed_str()));
        let bytes = body.map(|b| format!(" {b}\n"));
        let mut args = json!({});
        if company {
            args["company"] = json!("synthetic");
        }
        if let Some(body) = &bytes {
            args["body"] = json!(body);
            args["confirm"] = json!(true);
        }
        let rows = parsed(&run(
            &origin,
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        let result = &rows[1]["result"];
        if name.ends_with("export") {
            assert_eq!(
                result["structuredContent"]["data"]["backup"]["bytes"],
                json!(reply.as_bytes())
            );
        } else {
            let canonical: Value = serde_json::from_str(reply).unwrap();
            assert_eq!(result["structuredContent"]["data"], canonical);
            assert_eq!(result["isError"], canonical["ok"] == false);
        }
        let wire = String::from_utf8(request.recv().unwrap()).unwrap();
        assert!(wire.starts_with(&format!(
            "{} {path} HTTP/1.1",
            if bytes.is_some() { "POST" } else { "GET" }
        )));
        if let Some(bytes) = bytes {
            assert!(wire.ends_with(&bytes));
        }
        t.join().unwrap();
    }
}
