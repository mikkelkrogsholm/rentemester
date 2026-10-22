use super::*;

const PREFIX: &str = "/api/companies/synthetic/documents/original/party-links";
const ROLES: [&str; 18] = [
    "issuer",
    "supplier",
    "customer",
    "recipient",
    "payer",
    "payee",
    "processor",
    "acquirer",
    "related_company",
    "establishment",
    "location",
    "payment_descriptor",
    "vendor",
    "owner",
    "adviser",
    "employee",
    "authority",
    "bank",
];
fn plan_args() -> Value {
    json!({"company":"synthetic","documentId":7,"role":"supplier","partyId":"party-synthetic","jurisdiction":"DK","identifierKind":"dk_cvr","identifier":"12345678"})
}
fn invoke(name: &str, args: Value, core: Value, status: &str) -> (Value, String) {
    let body = core.to_string();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
        body.len()
    );
    let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
    let rows = parsed(&run(
        &origin,
        &[init("2025-11-25"), ready(), call(2, name, args)],
    ));
    let wire = String::from_utf8(
        request
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap(),
    )
    .unwrap();
    task.join().unwrap();
    let result = &rows[1]["result"];
    let envelope = result["structuredContent"].clone();
    assert_eq!(
        serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
        envelope
    );
    assert_eq!(result["isError"], envelope["ok"] != true);
    (envelope, wire)
}
#[test]
fn document_party_original_registry_all_roles_and_retained_aliases() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), EXPECTED_NATIVE_TOOLS);
    for (name, read) in [
        ("documents_party_link_plan", true),
        ("documents_party_link_list", true),
        ("documents_party_link_inspect", true),
        ("documents_party_link_apply", false),
        ("documents_party_link_supersede", false),
        ("documents_internal_no_external_party", false),
        ("documents_internal_no_external_party_supersede", false),
    ] {
        let tool = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(
            tool["annotations"],
            json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false})
        );
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert_eq!(tool["outputSchema"]["additionalProperties"], false);
        for field in [
            "actor",
            "principal",
            "expectedActor",
            "expectedPrincipal",
            "workspace",
        ] {
            assert!(tool["inputSchema"]["properties"].get(field).is_none());
        }
        if !read {
            assert_eq!(
                tool["inputSchema"]["properties"]["confirm"],
                json!({"type":"boolean"})
            );
            assert!(
                !tool["inputSchema"]["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("confirm"))
            );
        }
    }
    let plan = tools
        .iter()
        .find(|t| t["name"] == "documents_party_link_plan")
        .unwrap();
    assert_eq!(
        plan["inputSchema"]["properties"]["role"]["enum"],
        json!(ROLES)
    );
    for name in [
        "documents_ingest",
        "documents_list",
        "ingest_document",
        "parse_document",
        "workspace_parties",
    ] {
        assert!(
            tools.iter().any(|t| t["name"] == name),
            "missing alias {name}"
        );
    }
}
#[test]
fn document_party_original_missing_or_false_confirmation_before_http() {
    for name in [
        "documents_party_link_apply",
        "documents_party_link_supersede",
        "documents_internal_no_external_party",
        "documents_internal_no_external_party_supersede",
    ] {
        let mut args = match name {
            "documents_party_link_apply" => {
                let mut a = plan_args();
                a["planHash"] = json!("a".repeat(64));
                a
            }
            "documents_party_link_supersede" => {
                json!({"company":"synthetic","documentId":7,"role":"supplier","planHash":"a".repeat(64),"reason":"Reviewed correction"})
            }
            "documents_internal_no_external_party" => {
                json!({"company":"synthetic","documentId":7,"reason":"Internal voucher"})
            }
            _ => {
                json!({"company":"synthetic","documentId":7,"decisionHash":"a".repeat(64),"reason":"Reviewed correction"})
            }
        };
        for confirm in [None, Some(false)] {
            if let Some(value) = confirm {
                args["confirm"] = json!(value);
            }
            let rows = parsed(&run(
                "http://127.0.0.1:9",
                &[init("2025-11-25"), ready(), call(2, name, args.clone())],
            ));
            assert_eq!(
                rows[1]["result"]["structuredContent"]["code"],
                "CONFIRM_REQUIRED"
            );
            assert_eq!(rows[1]["result"]["isError"], true);
        }
    }
}
#[test]
fn document_party_original_strict_arguments_no_authority_and_hash_validation() {
    for (field, value) in [
        ("company", json!("")),
        ("documentId", json!(0)),
        ("documentId", json!(1.5)),
        ("documentId", json!(9007199254740992u64)),
        ("partyId", json!("ab")),
        ("partyId", Value::Null),
        ("jurisdiction", json!("DNK")),
        ("identifierKind", json!("name")),
        ("identifier", json!("x".repeat(161))),
        ("role", json!("unknown")),
        ("actor", json!("user:forged")),
        ("principal", Value::Null),
        ("expectedActor", json!("user:forged")),
        ("workspace", json!("/private/workspace")),
        ("confirm", json!(true)),
    ] {
        let mut args = plan_args();
        args[field] = value;
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[
                init("2025-11-25"),
                ready(),
                call(2, "documents_party_link_plan", args),
            ],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602, "field {field}: {rows:?}");
    }
    for (field, value) in [
        ("planHash", json!("z".repeat(64))),
        ("confirm", Value::Null),
        ("idempotencyKey", json!("x".repeat(129))),
        ("actor", json!("user:forged")),
    ] {
        let mut args = plan_args();
        args["planHash"] = json!("a".repeat(64));
        args["confirm"] = json!(true);
        args[field] = value;
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[
                init("2025-11-25"),
                ready(),
                call(2, "documents_party_link_apply", args),
            ],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602);
    }
}
#[test]
fn document_party_original_plan_domain_failure_is_outer_failure_and_read_transport() {
    let (envelope, wire) = invoke(
        "documents_party_link_plan",
        plan_args(),
        json!({"ok":false,"errors":["NO_IDENTIFIER_EVIDENCE"]}),
        "200 OK",
    );
    assert_eq!(
        envelope,
        json!({"ok":false,"errors":["NO_IDENTIFIER_EVIDENCE"]})
    );
    assert!(wire.starts_with(&format!("POST {PREFIX}/plan HTTP/1.1")));
    let body: Value = serde_json::from_str(wire.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert!(body.get("company").is_none());
    assert!(body.get("confirm").is_none());
    let (envelope, _) = invoke(
        "documents_party_link_plan",
        plan_args(),
        json!({"ok":true}),
        "503 Unavailable",
    );
    assert_ne!(envelope["code"], "outcome_unknown");
    assert_eq!(envelope["ok"], false);
}
#[test]
fn document_party_original_apply_exact_body_optional_key_and_unknown_outcome() {
    for key in [None, Some("synthetic-receipt")] {
        let mut args = plan_args();
        args["planHash"] = json!("a".repeat(64));
        args["confirm"] = json!(true);
        if let Some(k) = key {
            args["idempotencyKey"] = json!(k);
        }
        let (envelope, wire) = invoke(
            "documents_party_link_apply",
            args.clone(),
            json!({"ok":true,"id":9,"idempotent":false,"planHash":"a".repeat(64)}),
            "200 OK",
        );
        assert_eq!(
            envelope["data"],
            json!({"id":9,"idempotent":false,"planHash":"a".repeat(64)})
        );
        args.as_object_mut().unwrap().remove("company");
        assert_eq!(
            serde_json::from_str::<Value>(wire.split("\r\n\r\n").nth(1).unwrap()).unwrap(),
            args
        );
        assert!(wire.starts_with(&format!("POST {PREFIX}/apply HTTP/1.1")));
    }
    let mut args = plan_args();
    args["planHash"] = json!("a".repeat(64));
    args["confirm"] = json!(true);
    for status in [
        "503 Unavailable",
        "200 OK",
        "403 Forbidden",
        "401 Unauthorized",
    ] {
        let (envelope, _) = invoke(
            "documents_party_link_apply",
            args.clone(),
            json!({"ok":true}),
            status,
        );
        assert_eq!(envelope["ok"], false);
        assert_eq!(
            envelope["code"],
            if status.starts_with("401") || status.starts_with("403") {
                "server_refused"
            } else {
                "outcome_unknown"
            }
        );
        assert!(envelope.get("data").is_none());
    }
}
#[test]
fn document_party_original_list_snake_rows_no_pagination_and_inspect_strings() {
    let row = json!({"id":7,"document_no":"SYN-7","sha256_hash":"a".repeat(64),"linked":1,"resolution_state":"resolved"});
    let (envelope, wire) = invoke(
        "documents_party_link_list",
        json!({"company":"synthetic","status":"linked"}),
        json!({"ok":true,"links":[row.clone()]}),
        "200 OK",
    );
    assert_eq!(envelope["data"], json!({"links":[row]}));
    assert!(wire.starts_with(&format!("GET {PREFIX}?status=linked HTTP/1.1")));
    assert!(envelope["data"].get("hasMore").is_none());
    let row = json!({"id":9,"party_id":"party-synthetic","party_role":"supplier","event_type":"linked","evidence_kind":"exact_identifier","evidence_json":"{}","document_sha256":"a".repeat(64),"document_payload_sha256":"b".repeat(64),"party_snapshot_json":"{}","plan_hash":"c".repeat(64),"reason":null,"actor":"user:synthetic","principal":"user:synthetic","created_at":"2026-01-01T00:00:00Z"});
    let (envelope, wire) = invoke(
        "documents_party_link_inspect",
        json!({"company":"synthetic","documentId":7}),
        json!({"ok":true,"links":[row.clone()]}),
        "200 OK",
    );
    assert_eq!(envelope["data"], json!({"links":[row]}));
    assert!(wire.starts_with(&format!("GET {PREFIX}/7 HTTP/1.1")));
}
#[test]
fn document_party_original_internal_and_supersession_flat_projections() {
    for (name, args, core, expected, path) in [
        (
            "documents_internal_no_external_party",
            json!({"company":"synthetic","documentId":7,"reason":"Internal evidence","confirm":true}),
            json!({"ok":true,"id":8,"idempotent":true,"decisionHash":"b".repeat(64)}),
            json!({"id":8,"idempotent":true,"decisionHash":"b".repeat(64)}),
            "internal",
        ),
        (
            "documents_internal_no_external_party_supersede",
            json!({"company":"synthetic","documentId":7,"reason":"Reviewed correction","decisionHash":"b".repeat(64),"confirm":true}),
            json!({"ok":true,"id":null,"idempotent":true}),
            json!({"id":null,"idempotent":true}),
            "internal-supersede",
        ),
        (
            "documents_party_link_supersede",
            json!({"company":"synthetic","documentId":7,"role":"supplier","reason":"Reviewed correction","planHash":"a".repeat(64),"confirm":true}),
            json!({"ok":true,"id":10,"idempotent":false}),
            json!({"id":10,"idempotent":false}),
            "supersede",
        ),
    ] {
        let (envelope, wire) = invoke(name, args, core, "200 OK");
        assert_eq!(envelope["data"], expected);
        assert!(wire.starts_with(&format!("POST {PREFIX}/{path} HTTP/1.1")));
    }
}
