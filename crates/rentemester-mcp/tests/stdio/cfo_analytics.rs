use super::*;
#[test]
fn cfo_readonly_discovery_exact_get_and_full_native_evidence() {
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
    let t = tools.iter().find(|t| t["name"] == "cfo_analytics").unwrap();
    assert_eq!(t["annotations"]["readOnlyHint"], true);
    assert_eq!(t["inputSchema"]["additionalProperties"], false);
    for extra in ["confirm", "body", "actor", "endpoint"] {
        let mut args = json!({"scope":"portfolio","from":"2025-01-01","to":"2026-01-02"});
        args[extra] = json!(true);
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, "cfo_analytics", args)],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602);
    }
    let reply = r#"{"ok":true,"status":"partial","totals":null,"rows":[{"amount":"9007199254740993.01","sourceHash":"retained"}],"future":true}"#;
    let wire_reply = format!(
        "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{}",
        reply.len(),
        reply
    );
    let (origin, request, task) = http(Box::leak(wire_reply.into_boxed_str()));
    let rows = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "cfo_analytics",
                json!({"scope":"portfolio","from":"2025-01-01","to":"2026-01-02","companySlugs":["left","right"],"limit":1}),
            ),
        ],
    ));
    assert_eq!(
        rows[1]["result"]["structuredContent"]["data"],
        serde_json::from_str::<Value>(reply).unwrap()
    );
    assert!(request.recv().unwrap().starts_with(b"GET /api/cfo-analytics?scope=portfolio&from=2025-01-01&to=2026-01-02&companySlug=left&companySlug=right&limit=1 HTTP/1.1"));
    task.join().unwrap();
}
