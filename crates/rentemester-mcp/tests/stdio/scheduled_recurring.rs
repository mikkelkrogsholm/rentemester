// Uses the existing real stdio/HTTP harness, with no authentication fixture bypass.
use super::*;
#[test]
fn scheduled_discovery_158_tools_and_strict_read_write_arguments() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 222);
    for name in ["scheduled_recurring", "scheduled_recurring_plan"] {
        assert_eq!(
            tools.iter().find(|t| t["name"] == name).unwrap()["annotations"]["readOnlyHint"],
            true
        );
    }
    for name in [
        "enable_scheduled_recurring",
        "pause_scheduled_recurring",
        "resume_scheduled_recurring",
        "cancel_scheduled_recurring",
        "catch_up_scheduled_recurring",
        "skip_scheduled_recurring",
        "dispatch_scheduled_recurring",
        "reconsent_scheduled_recurring",
    ] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(t["annotations"]["readOnlyHint"], false);
        assert_eq!(t["annotations"]["idempotentHint"], false);
        assert_eq!(t["inputSchema"]["properties"]["body"]["maxLength"], 16384);
    }
    for (name, args) in [
        (
            "scheduled_recurring",
            json!({"company":"synthetic","confirm":true}),
        ),
        (
            "scheduled_recurring_plan",
            json!({"company":"synthetic","templateId":0}),
        ),
        (
            "scheduled_recurring_plan",
            json!({"company":"synthetic","templateId":7,"asOf":"2026-01-01"}),
        ),
        (
            "dispatch_scheduled_recurring",
            json!({"company":"synthetic","body":"{}"}),
        ),
        (
            "dispatch_scheduled_recurring",
            json!({"company":"synthetic","body":" ".repeat(16385),"confirm":true}),
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
fn scheduled_dispatch_exact_bytes_and_unknown_outcome() {
    let (origin, request, t) =
        http("HTTP/1.1 503 Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");
    let body = format!(
        " {{\"templateId\":7,\"planHash\":\"{}\",\"periodIndex\":0,\"confirm\":true}}\n",
        "a".repeat(64)
    );
    let rows = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "dispatch_scheduled_recurring",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        rows[1]["result"]["structuredContent"]["error"]["code"],
        "outcome_unknown"
    );
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(
        wire.starts_with("POST /api/companies/synthetic/scheduled-recurring/dispatch HTTP/1.1")
    );
    assert!(wire.ends_with(&body));
    t.join().unwrap();
}

#[test]
fn reconsent_exact_bytes_and_canonical_restored_consent_response() {
    let reply = r#"{"ok":true,"scheduledRecurring":{"state":"paused","restoreConsent":{"required":false,"boundaryHash":"historical","historicalOwnerUserId":"old-user","historicalQueued":[{"periodIndex":2,"issueDate":"2025-01-01"}]}},"futureEvidence":{"preserved":true}}"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
        reply.len()
    );
    let (origin, request, t) = http(Box::leak(response.into_boxed_str()));
    let body = format!(
        " {{\"templateId\":7,\"planHash\":\"{}\",\"reason\":\" explicit synthetic consent \",\"confirm\":true}}\n",
        "a".repeat(64)
    );
    let rows = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "reconsent_scheduled_recurring",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    let result = &rows[1]["result"]["structuredContent"]["data"];
    assert_eq!(result["scheduledRecurring"]["state"], "paused");
    assert_eq!(
        result["scheduledRecurring"]["restoreConsent"]["historicalQueued"][0]["periodIndex"],
        2
    );
    assert_eq!(result["futureEvidence"]["preserved"], true);
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(
        wire.starts_with("POST /api/companies/synthetic/scheduled-recurring/reconsent HTTP/1.1")
    );
    assert!(wire.ends_with(&body));
    t.join().unwrap();
}
