use super::*;
#[test]
fn archive_tools_discovery_readonly_and_confirmation_admission() {
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
    for name in ["archive_import_plan", "archives", "archive_import_original"] {
        assert_eq!(
            tools.iter().find(|t| t["name"] == name).unwrap()["annotations"]["readOnlyHint"],
            true
        );
    }
    assert_eq!(
        tools
            .iter()
            .find(|t| t["name"] == "archive_import_apply")
            .unwrap()["annotations"]["readOnlyHint"],
        false
    );
    for (name, args) in [
        (
            "archive_import_plan",
            json!({"company":"synthetic","body":"{}","confirm":true}),
        ),
        (
            "archive_import_apply",
            json!({"company":"synthetic","body":"{}"}),
        ),
        ("archives", json!({"company":"synthetic","confirm":true})),
        (
            "archive_import_original",
            json!({"company":"synthetic","importId":42,"path":"untrusted"}),
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
fn archive_named_large_plan_frame_does_not_change_generic_limits_or_sourcebytes() {
    let body = serde_json::to_string(
        &json!({"fileName":"source.json","contentBase64":"A".repeat(1_100_000)}),
    )
    .unwrap();
    let (origin, request, task) = http(
        "HTTP/1.1 200 OK\r\nContent-Length:21\r\nConnection:close\r\n\r\n{\"ok\":true,\"plan\":{}}",
    );
    let rows = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "archive_import_plan",
                json!({"company":"synthetic","body":body}),
            ),
        ],
    ));
    assert_eq!(rows[1]["result"]["structuredContent"]["data"]["ok"], true);
    let bytes = request.recv().unwrap();
    assert!(bytes.starts_with(b"POST /api/companies/synthetic/archive-import/plan HTTP/1.1"));
    assert!(bytes.ends_with(body.as_bytes()));
    task.join().unwrap();
    let out = run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            call(2, "companies", json!({"padding":"x".repeat(1_100_000)})),
        ],
    );
    let rows: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(rows.iter().any(|v| v["error"].is_object()));
}
