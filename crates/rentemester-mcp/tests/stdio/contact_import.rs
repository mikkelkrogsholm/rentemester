use super::*;
#[test]
fn contact_import_discovery_and_confirmed_large_frame_keep_ordinary_limits() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 219);
    let t = tools
        .iter()
        .find(|t| t["name"] == "import_contacts")
        .unwrap();
    assert_eq!(t["annotations"]["readOnlyHint"], false);
    assert_eq!(
        t["inputSchema"]["properties"]["body"]["maxLength"],
        12 * 1024 * 1024
    );
    assert_eq!(t["inputSchema"]["additionalProperties"], false);
    for args in [
        json!({"company":"synthetic","body":"{}"}),
        json!({"company":"synthetic","body":"{}","confirm":true,"endpoint":"https://untrusted.invalid"}),
    ] {
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[
                init("2025-11-25"),
                ready(),
                call(2, "import_contacts", args),
            ],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602);
    }
    let body=serde_json::to_string(&json!({"fileName":"Kontakter.csv","content":"x".repeat(1_100_000),"enrichCvr":true,"confirm":true})).unwrap();
    let (origin, wire, task) = http(
        "HTTP/1.1 200 OK\r\nContent-Length:35\r\nConnection:close\r\n\r\n{\"ok\":true,\"import\":{\"summary\":{}}}",
    );
    let rows = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "import_contacts",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(rows[1]["result"]["structuredContent"]["data"]["ok"], true);
    let bytes = wire.recv().unwrap();
    assert!(bytes.starts_with(b"POST /api/companies/synthetic/import HTTP/1.1"));
    assert!(bytes.ends_with(body.as_bytes()));
    task.join().unwrap();
    let output = run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            call(2, "companies", json!({"padding":"x".repeat(1_100_000)})),
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("message limit exceeded"));
    let rows: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.last().unwrap()["error"]["code"], -32700);
}
#[test]
fn contact_import_unknown_is_one_request_without_provider_or_new_key() {
    let body = r#" {"fileName":"Kontakter.csv","content":"Kontaktnavn;CVR-nummer;Kontakttype\nSynthetic;;","enrichCvr":false,"confirm":true} "#;
    let (origin, wire, task) =
        http("HTTP/1.1 503 Service Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");
    let rows = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "import_contacts",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        rows[1]["result"]["structuredContent"]["error"]["code"],
        "outcome_unknown"
    );
    assert!(wire.recv().unwrap().ends_with(body.as_bytes()));
    task.join().unwrap();
}
