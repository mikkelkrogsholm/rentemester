use super::*;
#[test]
fn disposition_discovery_keeps_global_status_and_source_actions_explicit() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 225);
    for (name, read) in [
        ("group_disposition_status", true),
        ("plan_group_disposition", true),
        ("propose_group_disposition", false),
        ("approve_group_disposition", false),
        ("link_group_disposition", false),
        ("settle_group_disposition", false),
        ("supersede_group_disposition", false),
        ("reopen_group_disposition", false),
    ] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(t["annotations"]["readOnlyHint"], read);
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        if name != "group_disposition_status" {
            assert_eq!(t["inputSchema"]["properties"]["body"]["maxLength"], 32768);
        }
    }
    for (name, args) in [
        (
            "group_disposition_status",
            json!({"id":"synthetic","company":"hidden"}),
        ),
        (
            "plan_group_disposition",
            json!({"company":"synthetic","body":"{}","confirm":true}),
        ),
        (
            "approve_group_disposition",
            json!({"company":"synthetic","body":"{}","confirm":true,"principal":"forged"}),
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
fn disposition_link_unknown_is_sent_once_without_new_heads_or_keys() {
    let body = format!(
        r#" {{"dispositionId":"synthetic-explicit-id","payloadHash":"{}","side":"left","journalEntryId":3,"ledgerHeadHash":"{}","confirm":true}} "#,
        "a".repeat(64),
        "b".repeat(64)
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
                "link_group_disposition",
                json!({"company":"synthetic-left","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        rows[1]["result"]["structuredContent"]["error"]["code"],
        "outcome_unknown"
    );
    let wire = request.recv().unwrap();
    assert!(
        wire.starts_with(b"POST /api/companies/synthetic-left/group-dispositions/link HTTP/1.1")
    );
    assert!(wire.ends_with(body.as_bytes()));
    task.join().unwrap();
}
