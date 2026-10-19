use super::*;
#[test]
fn discovery_158_and_exact_read_write_scope_and_bounds() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 170);
    for (name, readonly, max) in [
        ("service_principals", true, 0),
        ("create_service_principal", false, 8192),
        ("rotate_service_principal", false, 8192),
        ("revoke_service_principal", false, 8192),
        ("recover_service_principal", false, 8192),
        ("hosted_recurring_plan", true, 0),
        ("consent_hosted_recurring", false, 16384),
        ("revoke_hosted_recurring", false, 16384),
    ] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(t["annotations"]["readOnlyHint"], readonly);
        if !readonly {
            assert_eq!(t["annotations"]["idempotentHint"], false);
            assert_eq!(t["inputSchema"]["properties"]["body"]["maxLength"], max);
        }
    }
    for (name, args) in [
        ("service_principals", json!({"company":"synthetic"})),
        ("service_principals", json!({"confirm":true})),
        (
            "hosted_recurring_plan",
            json!({"company":"synthetic","templateId":0}),
        ),
        (
            "hosted_recurring_plan",
            json!({"company":"synthetic","templateId":7,"query":"secret"}),
        ),
        ("create_service_principal", json!({"body":"{}"})),
        (
            "recover_service_principal",
            json!({"body":"{}","confirm":true,"company":"synthetic"}),
        ),
        (
            "consent_hosted_recurring",
            json!({"company":"synthetic","body":"{}","confirm":true,"serviceKey":"must-not-leak"}),
        ),
        (
            "revoke_hosted_recurring",
            json!({"company":"synthetic","body":" ".repeat(16385),"confirm":true}),
        ),
    ] {
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602);
        assert!(!rows[1].to_string().contains("must-not-leak"));
    }
}
#[test]
fn one_time_lifecycle_receipt_and_hosted_unknown_are_preserved_without_dispatch_or_replay() {
    for (name, path, body, response, success) in [
        (
            "create_service_principal",
            "/api/workspace/service-principals",
            json!({"displayName":"Synthetic executor","operationId":"12345678-1234-4123-8123-123456789abc","confirm":true}),
            "HTTP/1.1 201 Created\r\nContent-Length:48\r\nConnection:close\r\n\r\n{\"ok\":true,\"secret\":\"synthetic-one-time-output\"}",
            true,
        ),
        (
            "consent_hosted_recurring",
            "/api/companies/synthetic/scheduled-recurring/hosted-consent",
            json!({"templateId":7,"serviceAccountId":"service-synthetic","credentialId":"credential-synthetic","planHash":"a".repeat(64),"reason":" explicit reviewed source ","confirm":true}),
            "HTTP/1.1 503 Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n",
            false,
        ),
    ] {
        let (origin, request, t) = http(response);
        let bytes = format!(" {body}\n");
        let mut args = json!({"body":bytes,"confirm":true});
        if name == "consent_hosted_recurring" {
            args["company"] = json!("synthetic");
        }
        let rows = parsed(&run(
            &origin,
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        if success {
            assert_eq!(
                rows[1]["result"]["structuredContent"]["data"]["secret"],
                "synthetic-one-time-output"
            );
        } else {
            assert_eq!(
                rows[1]["result"]["structuredContent"]["error"]["code"],
                "outcome_unknown"
            );
        }
        let wire = String::from_utf8(request.recv().unwrap()).unwrap();
        assert!(wire.starts_with(&format!("POST {path} HTTP/1.1")));
        assert!(wire.ends_with(&bytes));
        t.join().unwrap();
    }
}

#[test]
fn supplied_secret_in_lifecycle_or_hosted_body_is_refused_without_echo_or_http() {
    for (name, body, company) in [
        (
            "create_service_principal",
            json!({"displayName":"Synthetic","secret":"supplied-secret-must-not-echo","confirm":true}),
            false,
        ),
        (
            "consent_hosted_recurring",
            json!({"templateId":7,"serviceAccountId":"synthetic","credentialId":"credential","planHash":"a".repeat(64),"reason":"reviewed","serviceKey":"supplied-secret-must-not-echo","confirm":true}),
            true,
        ),
    ] {
        let mut args = json!({"body":body.to_string(),"confirm":true});
        if company {
            args["company"] = json!("synthetic");
        }
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert_eq!(
            rows[1]["result"]["structuredContent"]["error"]["code"],
            "invalid_input"
        );
        assert!(
            !rows[1]
                .to_string()
                .contains("supplied-secret-must-not-echo")
        );
    }
}
