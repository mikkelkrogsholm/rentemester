use super::*;
#[test]
fn five_batch_tools_have_explicit_read_scope_and_confirmed_exact_bodies() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 193);
    for (name, read) in [
        ("bookkeeping_batch_plan", true),
        ("bookkeeping_batch_status", true),
        ("bookkeeping_batch_persist", false),
        ("bookkeeping_batch_approve", false),
        ("bookkeeping_batch_apply", false),
    ] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(t["annotations"]["readOnlyHint"], read);
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        if !read {
            assert_eq!(t["inputSchema"]["properties"]["body"]["maxLength"], 32768);
            assert_eq!(t["inputSchema"]["properties"]["confirm"]["const"], true);
        }
    }
    for (name, args) in [
        (
            "bookkeeping_batch_plan",
            json!({"company":"synthetic","scope":{},"confirm":true}),
        ),
        (
            "bookkeeping_batch_status",
            json!({"company":"synthetic","runId":1,"runKey":"both"}),
        ),
        ("bookkeeping_batch_status", json!({"company":"synthetic"})),
        (
            "bookkeeping_batch_apply",
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
fn batch_confirmed_apply_unknown_is_sent_once_without_rewriting() {
    let body = format!(
        r#" {{ "runId":7,"planHash":"{}","confirm":true }} "#,
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
                "bookkeeping_batch_apply",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        rows[1]["result"]["structuredContent"]["error"]["code"],
        "outcome_unknown"
    );
    let wire = request.recv().unwrap();
    assert!(wire.starts_with(b"POST /api/companies/synthetic/bookkeeping-batch/apply HTTP/1.1"));
    assert!(wire.ends_with(body.as_bytes()));
    task.join().unwrap();
}
