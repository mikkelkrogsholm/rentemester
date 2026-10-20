use super::*;

#[test]
fn bank_profile_schema_and_exact_import_passthrough() {
    let listed = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = listed[1]["result"]["tools"].as_array().unwrap();
    let import = tools.iter().find(|t| t["name"] == "import_bank").unwrap();
    assert_eq!(
        import["inputSchema"]["properties"]["body"]["contentSchema"]["properties"]["profile"]["enum"],
        json!([null, "", "danske-bank"])
    );
    let body = "{ \"csvContent\":\"Dato;Tekst;Beløb\\n01-02-2026;Synthetic;-12,34\", \"account\":\"synthetic\", \"profile\":\"danske-bank\", \"confirm\":true }";
    let (origin, request, task) =
        http("HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}");
    let rows = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "import_bank",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(rows[1]["result"]["structuredContent"]["data"]["ok"], true);
    let wire = request
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert!(wire.starts_with(b"POST /api/companies/synthetic/bank/import HTTP/1.1"));
    assert!(wire.ends_with(body.as_bytes()));
    task.join().unwrap();
}

#[test]
fn bank_profile_unknown_and_missing_confirmation_never_reach_network() {
    let body = |profile: &str| {
        json!({"csvContent":"date;text;amount\n2026-01-01;Synthetic;-1","profile":profile,"confirm":true}).to_string()
    };
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "import_bank",
                json!({"company":"synthetic","body":body("lunar"),"confirm":true}),
            ),
            call(
                3,
                "import_bank",
                json!({"company":"synthetic","body":body("danske-bank")}),
            ),
        ],
    ));
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
