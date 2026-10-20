use super::*;
#[test]
fn reports_preserve_source_exact_money_and_export_bytes() {
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
    for name in [
        "journal",
        "trial_balance",
        "income_statement",
        "balance",
        "overview",
        "report_export",
    ] {
        let tool = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    }
    for (name, path) in [("journal", "journal"), ("overview", "overview")] {
        let reply = r#"{"ok":true,"amount":"9007199254740993.01","prior":null,"source":"archive","future":true}"#;
        let wire_reply = Box::leak(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
                reply.len()
            )
            .into_boxed_str(),
        );
        let (origin, req, task) = http(wire_reply);
        let rows = parsed(&run(
            &origin,
            &[
                init("2025-11-25"),
                ready(),
                call(
                    2,
                    name,
                    json!({"company":"synthetic","year":"2024","source":"archive"}),
                ),
            ],
        ));
        assert_eq!(
            rows[1]["result"]["structuredContent"]["data"],
            serde_json::from_str::<Value>(reply).unwrap()
        );
        assert!(
            req.recv().unwrap().starts_with(
                format!("GET /api/companies/synthetic/{path}?year=2024&source=archive HTTP/1.1")
                    .as_bytes()
            )
        );
        task.join().unwrap();
    }
    let (origin, req, task) = http(
        "HTTP/1.1 200 OK\r\nContent-Length:9\r\nContent-Type:application/pdf\r\nConnection:close\r\n\r\n%PDF-1.4\n",
    );
    let rows = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "report_export",
                json!({"company":"synthetic","report":"journal","format":"pdf","year":"2024","source":"archive"}),
            ),
        ],
    ));
    assert_eq!(
        rows[1]["result"]["structuredContent"]["data"]["bytes"],
        json!(b"%PDF-1.4\n")
    );
    assert!(req.recv().unwrap().starts_with(
        b"GET /api/companies/synthetic/journal/export?year=2024&source=archive&format=pdf HTTP/1.1"
    ));
    task.join().unwrap();
    for (name, args) in [
        ("overview", json!({"company":"synthetic","account":"1000"})),
        (
            "report_export",
            json!({"company":"synthetic","report":"balance","format":"pdf","confirm":true}),
        ),
    ] {
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602);
    }
}
