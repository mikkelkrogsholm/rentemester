use super::*;
#[test]
fn posting_rules_original_three_only_discovery_365_and_offline_gate() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 370);
    for (name, op) in [
        (
            "posting_rule_propose",
            rentemester_client::PostingRulesOriginalOperation::Propose,
        ),
        (
            "posting_rule_approve",
            rentemester_client::PostingRulesOriginalOperation::Approve,
        ),
        (
            "posting_rule_explain",
            rentemester_client::PostingRulesOriginalOperation::Explain,
        ),
    ] {
        let tool = tools.iter().find(|t| t["name"] == name).unwrap();
        let mut expected = rentemester_client::original_posting_rules_schema(op);
        expected["properties"]["company"] = json!({"type":"string","minLength":1});
        if op.write() {
            expected["properties"]["confirm"] = json!({"type":"boolean"});
        }
        let required = expected["required"].as_array_mut().unwrap();
        required.retain(|k| k != "confirm");
        required.insert(0, json!("company"));
        assert_eq!(tool["inputSchema"], expected);
    }
    for name in [
        "posting_rule_disable",
        "posting_rule_supersede",
        "posting_rule_test",
    ] {
        assert!(!tools.iter().any(|t| t["name"] == name));
    }
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "posting_rule_approve",
                json!({"company":"synthetic","ruleId":"synthetic","version":1,"expectedPayloadHash":"a".repeat(64),"rationale":"reviewed","provenance":"review"}),
            ),
        ],
    ));
    assert_eq!(
        rows[1]["result"]["structuredContent"]["code"],
        "CONFIRM_REQUIRED"
    );
}
#[test]
fn posting_rules_original_explain_flat_read_exact_source_envelope() {
    let core = json!({"decision":"human_decision","matched":[],"explanations":[],"reasons":["no approved rule matched"],"dryRun":true});
    let body = core.to_string();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
        body.len()
    );
    let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
    let rows = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "posting_rule_explain",
                json!({"company":"synthetic","context":{"amount":125,"currency":"DKK"}}),
            ),
        ],
    ));
    let wire = String::from_utf8(
        request
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap(),
    )
    .unwrap();
    task.join().unwrap();
    assert!(wire.starts_with("POST /api/companies/synthetic/original/posting-rules/explain "));
    let result = &rows[1]["result"];
    assert_eq!(
        result["structuredContent"],
        json!({"ok":true,"errors":[],"data":core})
    );
    assert_eq!(result["isError"], false);
    assert_eq!(
        serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
        result["structuredContent"]
    );
}
