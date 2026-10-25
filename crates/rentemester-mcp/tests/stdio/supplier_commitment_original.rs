use super::*;

fn commitment() -> Value {
    json!({"commitmentId":"commitment-synthetic","vendorPartyId":"vendor:1","type":"subscription","description":"Reviewed service","businessPurpose":"Operations","amount":125,"currency":"DKK","frequency":"monthly","nextDate":"2026-01-01","evidenceRefs":["document:1"]})
}
fn source_http(
    status: &str,
    core: Value,
) -> (
    &'static str,
    std::sync::mpsc::Receiver<Vec<u8>>,
    std::thread::JoinHandle<()>,
) {
    // Keep the existing single-request HTTP harness and exact response bytes.
    let body = core.to_string();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
        body.len()
    );
    let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
    (Box::leak(origin.into_boxed_str()), request, task)
}
fn source_result(origin: &str, name: &str, args: Value) -> Value {
    parsed(&run(
        origin,
        &[init("2025-11-25"), ready(), call(2, name, args)],
    ))[1]["result"]
        .clone()
}
fn assert_envelope(result: &Value) {
    assert_eq!(
        serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
        result["structuredContent"]
    );
    assert_eq!(result["isError"], result["structuredContent"]["ok"] != true);
}
#[test]
fn supplier_commitment_original_discovery_has_362_unique_tools_and_explicit_native_alias() {
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
    let names: std::collections::BTreeSet<_> =
        tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), EXPECTED_NATIVE_TOOLS);
    for (name, read) in [
        ("supplier_commitment_plan", true),
        ("supplier_commitment_apply", false),
        ("supplier_commitment_list", true),
        ("supplier_commitment_change", false),
        ("supplier_commitment_match", false),
        ("supplier_commitment_matches", true),
        ("supplier_commitment_alerts", true),
        ("native_supplier_commitment_matches", true),
    ] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(t["annotations"]["readOnlyHint"], read);
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        for authority in ["actor", "principal", "program", "client"] {
            assert!(t["inputSchema"]["properties"].get(authority).is_none());
        }
        if !read {
            assert!(
                !t["inputSchema"]["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("confirm"))
            );
        }
    }
    let original = tools
        .iter()
        .find(|t| t["name"] == "supplier_commitment_matches")
        .unwrap();
    assert!(
        original["inputSchema"]["properties"]
            .get("commitmentId")
            .is_some()
    );
    let native = tools
        .iter()
        .find(|t| t["name"] == "native_supplier_commitment_matches")
        .unwrap();
    assert!(
        native["inputSchema"]["properties"]
            .get("commitmentId")
            .is_none()
    );
}
#[test]
fn supplier_commitment_original_confirmation_is_source_envelope_before_http() {
    for (name, mut args) in [
        (
            "supplier_commitment_apply",
            json!({"company":"synthetic","commitment":commitment(),"payloadHash":"a".repeat(64)}),
        ),
        (
            "supplier_commitment_change",
            json!({"company":"synthetic","commitmentId":"commitment-synthetic","action":"paused","reason":"Reviewed pause"}),
        ),
        (
            "supplier_commitment_match",
            json!({"company":"synthetic","commitmentId":"commitment-synthetic","occurrenceDate":"2026-01-01","evidence":{"kind":"payable","id":"1"}}),
        ),
    ] {
        for confirm in [None, Some(false)] {
            if let Some(confirm) = confirm {
                args["confirm"] = json!(confirm);
            }
            let r = source_result("http://127.0.0.1:9", name, args.clone());
            assert_envelope(&r);
            assert_eq!(r["structuredContent"]["code"], "CONFIRM_REQUIRED");
            assert!(r["structuredContent"].get("data").is_none());
        }
    }
}
#[test]
fn supplier_commitment_original_strict_source_shapes_reject_authority_and_factual_overrides() {
    for (name, args) in [
        (
            "supplier_commitment_list",
            json!({"company":"synthetic","actor":"user:forged"}),
        ),
        (
            "supplier_commitment_plan",
            json!({"company":"synthetic","commitment":commitment(),"confirm":true}),
        ),
        ("supplier_commitment_alerts", json!({"company":"synthetic"})),
        (
            "supplier_commitment_matches",
            json!({"company":"synthetic","body":"{}"}),
        ),
        (
            "native_supplier_commitment_matches",
            json!({"company":"synthetic","commitmentId":"commitment-synthetic"}),
        ),
        (
            "supplier_commitment_change",
            json!({"company":"synthetic","commitmentId":"commitment-synthetic","action":"ended","reason":"Reviewed","confirm":true,"principal":"user:forged"}),
        ),
        (
            "supplier_commitment_match",
            json!({"company":"synthetic","commitmentId":"commitment-synthetic","occurrenceDate":"2026-01-01","confirm":true,"evidence":{"kind":"payable","id":"1","amount":125}}),
        ),
    ] {
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602, "{rows:?}");
    }
}
#[test]
fn supplier_commitment_original_apply_success_without_errors_preserves_receipt() {
    let hash = "a".repeat(64);
    let core = json!({"ok":true,"idempotent":false,"commitmentId":"commitment-synthetic","payloadHash":hash});
    let (origin, request, task) = source_http("200 OK", core.clone());
    let args = json!({"company":"synthetic","commitment":commitment(),"payloadHash":hash,"idempotencyKey":"reviewed-key","confirm":true});
    let r = source_result(origin, "supplier_commitment_apply", args.clone());
    assert_envelope(&r);
    assert_eq!(
        r["structuredContent"],
        json!({"ok":true,"errors":[],"data":{"idempotent":false,"commitmentId":"commitment-synthetic","payloadHash":hash}})
    );
    let wire = request.recv().unwrap();
    assert!(wire.starts_with(
        b"POST /api/companies/synthetic/original/supplier-commitments/apply HTTP/1.1"
    ));
    let body = wire.windows(4).position(|v| v == b"\r\n\r\n").unwrap() + 4;
    let mut expected = args.as_object().unwrap().clone();
    expected.remove("company");
    assert_eq!(
        serde_json::from_slice::<Value>(&wire[body..]).unwrap(),
        Value::Object(expected)
    );
    task.join().unwrap();
}
#[test]
fn supplier_commitment_original_business_false_omits_data_and_retains_errors() {
    for (message, code) in [
        ("reviewed payload hash differs", None),
        ("commitment does not exist", Some("NOT_FOUND")),
        ("precondition not met", Some("PRECONDITION_MISSING")),
    ] {
        let (origin, request, task) = source_http(
            "422 Unprocessable Entity",
            json!({"ok":false,"errors":[message]}),
        );
        let r = source_result(
            origin,
            "supplier_commitment_change",
            json!({"company":"synthetic","commitmentId":"commitment-synthetic","action":"ended","reason":"Reviewed end","confirm":true}),
        );
        assert_envelope(&r);
        assert_eq!(r["structuredContent"]["ok"], false);
        assert_eq!(r["structuredContent"]["errors"], json!([message]));
        assert!(r["structuredContent"].get("data").is_none());
        assert_eq!(
            r["structuredContent"].get("code").and_then(Value::as_str),
            code
        );
        assert!(request.recv().unwrap().starts_with(
            b"POST /api/companies/synthetic/original/supplier-commitments/change HTTP/1.1"
        ));
        task.join().unwrap();
    }
}
#[test]
fn supplier_commitment_original_filtered_matches_alerts_and_curated_failures() {
    for (name, args, path) in [
        (
            "supplier_commitment_list",
            json!({"company":"synthetic"}),
            "/api/companies/synthetic/original/supplier-commitments",
        ),
        (
            "supplier_commitment_matches",
            json!({"company":"synthetic","commitmentId":"commitment-synthetic"}),
            "/api/companies/synthetic/original/supplier-commitments/matches?commitmentId=commitment-synthetic",
        ),
        (
            "supplier_commitment_alerts",
            json!({"company":"synthetic","asOf":"2026-01-01"}),
            "/api/companies/synthetic/original/supplier-commitments/alerts?asOf=2026-01-01",
        ),
    ] {
        let (origin, request, task) =
            source_http("200 OK", json!({"ok":true,"rows":[],"errors":[]}));
        let r = source_result(origin, name, args);
        assert_envelope(&r);
        assert_eq!(
            r["structuredContent"],
            json!({"ok":true,"errors":[],"data":{"rows":[]}})
        );
        assert!(
            request
                .recv()
                .unwrap()
                .starts_with(format!("GET {path} HTTP/1.1").as_bytes())
        );
        task.join().unwrap();
    }
    for (status, core, code) in [
        (
            "403 Forbidden",
            json!({"ok":false,"errors":["private server detail"]}),
            "server_refused",
        ),
        ("503 Unavailable", json!({"ok":true}), "outcome_unknown"),
        ("200 OK", json!({"ok":true}), "outcome_unknown"),
    ] {
        let (origin, request, task) = source_http(status, core);
        let r = source_result(
            origin,
            "supplier_commitment_change",
            json!({"company":"synthetic","commitmentId":"commitment-synthetic","action":"paused","reason":"Reviewed pause","confirm":true}),
        );
        assert_envelope(&r);
        assert_eq!(r["structuredContent"]["code"], code);
        assert!(r["structuredContent"].get("data").is_none());
        request.recv().unwrap();
        task.join().unwrap();
    }
}
