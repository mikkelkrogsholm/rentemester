use super::*;
#[test]
fn correction_plan_is_readonly_and_apply_requires_exact_confirmed_body() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 209);
    for (name, read) in [
        ("direct_bank_payable_correction_plan", true),
        ("direct_bank_payable_correction", false),
    ] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(t["annotations"]["readOnlyHint"], read);
        assert_eq!(t["inputSchema"]["properties"]["body"]["maxLength"], 16384);
    }
    for (name, args) in [
        (
            "direct_bank_payable_correction_plan",
            json!({"company":"synthetic","body":"{}","confirm":true}),
        ),
        (
            "direct_bank_payable_correction",
            json!({"company":"synthetic","body":"{}","confirm":true,"actor":"forged"}),
        ),
    ] {
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602);
    }
}
#[test]
fn correction_unknown_is_sent_once_without_rewriting() {
    let body = format!(
        r#" {{"documentId":1,"bankTransactionId":2,"billDate":"2026-01-10","dueDate":"2026-02-10","expenseAccountNo":"3000","vatTreatment":"standard","planHash":"{}","reason":"synthetic correction","idempotencyKey":"original chosen key","confirm":true}} "#,
        "a".repeat(64)
    );
    let (origin, request, task) =
        http("HTTP/1.1 503 Service Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");
    let rows = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "direct_bank_payable_correction",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        rows[1]["result"]["structuredContent"]["error"]["code"],
        "outcome_unknown"
    );
    let wire = request.recv().unwrap();
    assert!(wire.starts_with(
        b"POST /api/companies/synthetic/payables/direct-bank-correction/apply HTTP/1.1"
    ));
    assert!(wire.ends_with(body.as_bytes()));
    task.join().unwrap();
}
