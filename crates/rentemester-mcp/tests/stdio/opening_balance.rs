use super::*;
fn body(apply: bool) -> String {
    let input = json!({"cutOverDate":"2024-07-01","lines":[{"accountNo":"A","debitAmount":"100.00"},{"accountNo":"E","creditAmount":"100.00"}],"sourceArchiveImportIds":[42]});
    serde_json::to_string(&if apply {
        json!({"input":input,"planHash":"a".repeat(64),"idempotencyKey":"exact-key","confirm":true})
    } else {
        input
    })
    .unwrap()
}
#[test]
fn opening_discovery_strict_selectors_and_separate_confirmation() {
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
    for (name, readonly) in [
        ("opening_balance", true),
        ("opening_balance_plan", true),
        ("opening_balance_apply", false),
    ] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(t["annotations"]["readOnlyHint"], readonly);
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        if name != "opening_balance" {
            assert_eq!(t["inputSchema"]["properties"]["body"]["maxLength"], 65536);
            assert!(t["inputSchema"]["properties"]["body"]["contentSchema"].is_object());
        }
    }
    for (name, args) in [
        (
            "opening_balance",
            json!({"company":"synthetic","year":"2024"}),
        ),
        (
            "opening_balance_plan",
            json!({"company":"synthetic","body":body(false),"confirm":true}),
        ),
        (
            "opening_balance_apply",
            json!({"company":"synthetic","body":body(true)}),
        ),
        (
            "opening_balance_apply",
            json!({"company":"synthetic","body":body(true),"confirm":false}),
        ),
    ] {
        let r = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert_eq!(r[1]["error"]["code"], -32602);
    }
}
#[test]
fn opening_plan_and_apply_send_exact_body_and_preserve_receipt() {
    for (name, apply) in [
        ("opening_balance_plan", false),
        ("opening_balance_apply", true),
    ] {
        let reply = r#"{"ok":true,"openingBalance":{"actor":"user:original"},"idempotency":{"replayed":true}}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        );
        let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
        let body = body(apply);
        let mut args = json!({"company":"synthetic","body":body});
        if apply {
            args["confirm"] = json!(true);
        }
        let r = parsed(&run(
            &origin,
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert_eq!(
            r[1]["result"]["structuredContent"]["data"],
            serde_json::from_str::<Value>(reply).unwrap()
        );
        let wire = request.recv().unwrap();
        assert!(
            wire.starts_with(
                format!(
                    "POST /api/companies/synthetic/opening-balance/{} HTTP/1.1",
                    if apply { "apply" } else { "plan" }
                )
                .as_bytes()
            )
        );
        assert!(wire.ends_with(body.as_bytes()));
        task.join().unwrap();
    }
}
#[test]
fn opening_uncertain_apply_is_not_readonly_plan_or_retry() {
    for (name, apply, code) in [
        ("opening_balance_plan", false, "transport"),
        ("opening_balance_apply", true, "outcome_unknown"),
    ] {
        let (origin, request, task) = http(
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length:2\r\nConnection:close\r\n\r\n{}",
        );
        let mut args = json!({"company":"synthetic","body":body(apply)});
        if apply {
            args["confirm"] = json!(true);
        }
        let r = parsed(&run(
            &origin,
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert_eq!(r[1]["result"]["structuredContent"]["error"]["code"], code);
        request.recv().unwrap();
        task.join().unwrap();
    }
}
