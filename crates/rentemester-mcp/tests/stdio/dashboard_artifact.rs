use super::*;
#[test]
fn dashboard_is_exact_remote_read_and_forbids_local_controls() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tool = rows[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "dashboard_export")
        .unwrap();
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    let (origin, req, task) = http(
        "HTTP/1.1 200 OK\r\nContent-Length:9\r\nContent-Type:text/html; charset=utf-8\r\nContent-Disposition:attachment; filename=\"dashboard-synthetic-2024-02-29.html\"\r\nx-rentemester-as-of:2024-02-29\r\nx-rentemester-generated-at:2024-02-29T12:00:00Z\r\nx-rentemester-commit-sha:unknown\r\nx-rentemester-rule-bundle-version:synthetic1\r\nx-rentemester-render-ms:0.25\r\nConnection:close\r\n\r\n<p>ok</p>",
    );
    let rows = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "dashboard_export",
                json!({"company":"synthetic","asOf":"2024-02-29"}),
            ),
        ],
    ));
    assert_eq!(
        rows[1]["result"]["structuredContent"]["data"]["bytes"],
        json!(b"<p>ok</p>")
    );
    assert!(req.recv().unwrap().starts_with(
        b"GET /api/companies/synthetic/dashboard/export?format=html&asOf=2024-02-29 HTTP/1.1"
    ));
    task.join().unwrap();
    for args in [
        json!({"company":"synthetic","outPath":"x.html"}),
        json!({"company":"synthetic","open":true}),
        json!({"company":"synthetic","confirm":true}),
    ] {
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[
                init("2025-11-25"),
                ready(),
                call(2, "dashboard_export", args),
            ],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602);
    }
}
