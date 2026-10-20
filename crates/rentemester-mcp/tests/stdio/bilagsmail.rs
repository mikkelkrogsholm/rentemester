use super::*;
#[test]
fn mail_tools_publish_safe_confirmed_source_shapes_and_original_bytes_policy() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    for (name, read) in [
        ("bilagsmail", true),
        ("configure_bilagsmail", false),
        ("delete_bilagsmail_config", false),
        ("set_bilagsmail_alias", false),
        ("poll_bilagsmail", false),
        ("review_workspace_inbox", false),
        ("workspace_inbox_original", true),
    ] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(t["annotations"]["readOnlyHint"], read);
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
    }
    let config = tools
        .iter()
        .find(|t| t["name"] == "configure_bilagsmail")
        .unwrap();
    assert_eq!(
        config["inputSchema"]["properties"]["body"]["maxLength"],
        16384
    );
    assert_eq!(
        config["inputSchema"]["properties"]["body"]["contentSchema"]["properties"]["password"]["writeOnly"],
        true
    );
    let delete = tools
        .iter()
        .find(|t| t["name"] == "delete_bilagsmail_config")
        .unwrap();
    assert!(delete["inputSchema"]["properties"].get("body").is_none());
}
#[test]
fn mail_mcp_confirmations_and_secret_authority_fields_refuse_before_network() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "bilagsmail",
                json!({"company":"synthetic","password":"forbidden"}),
            ),
            call(
                3,
                "configure_bilagsmail",
                json!({"company":"synthetic","body":"{}"}),
            ),
            call(
                4,
                "poll_bilagsmail",
                json!({"company":"synthetic","body":"{\"confirm\":false}","confirm":true}),
            ),
            call(
                5,
                "delete_bilagsmail_config",
                json!({"company":"synthetic","body":"{}","confirm":true}),
            ),
            call(
                6,
                "workspace_inbox_original",
                json!({"company":"synthetic","sourceId":"source","outputPath":"/tmp/forbidden"}),
            ),
            call(
                7,
                "review_workspace_inbox",
                json!({"company":"synthetic","sourceId":"source","body":"{}","confirm":true}),
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
#[test]
fn mail_mcp_exact_config_body_and_verified_original_are_preserved() {
    let response = "HTTP/1.1 200 OK\r\nContent-Length:11\r\nConnection:close\r\n\r\n{\"ok\":true}";
    let (origin, request, task) = http(response);
    let body = r#"{ "host":"imap.example.invalid","port":993,"username":"synthetic","password":"private-test-password" }"#;
    let rows = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "configure_bilagsmail",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(rows[1]["result"]["structuredContent"]["data"]["ok"], true);
    assert!(!rows[1].to_string().contains("private-test-password"));
    let wire = request.recv().unwrap();
    assert!(wire.starts_with(b"POST /api/companies/synthetic/bilagsmail/imap-config HTTP/1.1"));
    assert!(wire.ends_with(body.as_bytes()));
    task.join().unwrap();
    let (origin, request, task) = http(
        "HTTP/1.1 200 OK\r\nContent-Type:message/rfc822\r\nx-content-sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\r\nContent-Length:3\r\nConnection:close\r\n\r\nabc",
    );
    let rows = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "workspace_inbox_original",
                json!({"company":"synthetic","sourceId":"source+id"}),
            ),
        ],
    ));
    assert_eq!(
        rows[1]["result"]["structuredContent"]["data"]["original"]["bytes"],
        json!([97, 98, 99])
    );
    assert_eq!(
        rows[1]["result"]["structuredContent"]["data"]["original"]["sha256"],
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert!(request.recv().unwrap().starts_with(
        b"GET /api/companies/synthetic/workspace-inbox/source%2Bid/original HTTP/1.1"
    ));
    task.join().unwrap();
}
