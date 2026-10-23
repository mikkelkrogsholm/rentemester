use super::*;

fn bindings() -> Vec<(
    &'static str,
    rentemester_client::DimensionsOriginalOperation,
    bool,
)> {
    use rentemester_client::DimensionsOriginalOperation::*;
    vec![
        ("dimension_definition_create", Define, false),
        ("dimension_member_create", Member, false),
        ("dimension_definition_lifecycle", DefinitionLifecycle, false),
        ("dimension_member_lifecycle", MemberLifecycle, false),
        ("dimension_definition_list", Definitions, true),
        ("dimension_member_list", Members, true),
        ("dimension_assignment_plan", Plan, true),
        ("dimension_assignment_apply", Apply, false),
        ("dimension_assignment_replace", Replace, false),
        ("dimension_assignment_supersede", Supersede, false),
        ("dimension_assignment_list", List, true),
        ("dimension_budget_plan", BudgetPlan, true),
        ("dimension_budget_apply", BudgetApply, false),
        ("dimension_budget_list", Budgets, true),
    ]
}
fn assignment() -> Value {
    json!({"company":"synthetic","journalLineId":7,"allocations":[{"dimensionId":"project","memberId":"alpha","amountMinor":1250,"currency":"DKK"}],"source":"reviewed","sourceRef":"reviewed synthetic source"})
}
fn budget() -> Value {
    json!({"company":"synthetic","accountNo":"4100","period":"2026-01","allocations":[{"dimensionId":"project","memberId":"alpha","amount":20}],"sourceRef":"reviewed synthetic budget"})
}
fn checked_result(rows: &[Value]) -> &Value {
    let result = &rows[1]["result"];
    assert_eq!(
        serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
        result["structuredContent"]
    );
    assert_eq!(result["isError"], result["structuredContent"]["ok"] != true);
    &result["structuredContent"]
}
#[test]
fn dimensions_original_discovery_362_unique_frozen_sdk_schemas_and_source_annotations() {
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
    let names: std::collections::BTreeSet<_> =
        tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), 370);
    for (name, op, read) in bindings() {
        let tool = tools.iter().find(|t| t["name"] == name).unwrap();
        let mut expected = rentemester_client::original_dimensions_schema(op);
        let properties = expected["properties"].as_object_mut().unwrap();
        // company has the shared transport slug schema; all operation fields come from SDK.
        properties.insert(
            "company".into(),
            tool["inputSchema"]["properties"]["company"].clone(),
        );
        if !read {
            properties.insert("confirm".into(), json!({"type":"boolean"}));
        }
        let required = expected["required"].as_array_mut().unwrap();
        required.retain(|k| k != "confirm");
        required.insert(0, json!("company"));
        assert_eq!(tool["inputSchema"], expected, "{name}");
        assert_eq!(
            tool["annotations"],
            json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read || name=="dimension_budget_apply","openWorldHint":false})
        );
        for forbidden in ["actor", "principal", "body", "input", "command"] {
            assert!(
                tool["inputSchema"]["properties"].get(forbidden).is_none(),
                "{name}: {forbidden}"
            );
        }
        assert!(
            !tool["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("confirm"))
        );
    }
}
#[test]
fn dimensions_original_all_writes_gate_confirmation_before_http() {
    for (name, _, read) in bindings() {
        if read {
            continue;
        }
        for confirm in [None, Some(false)] {
            let mut args = json!({"company":"synthetic"});
            if let Some(confirm) = confirm {
                args["confirm"] = json!(confirm);
            }
            let rows = parsed(&run(
                "http://127.0.0.1:9",
                &[init("2025-11-25"), ready(), call(2, name, args)],
            ));
            let result = checked_result(&rows);
            assert_eq!(result["ok"], false, "{name}");
            assert_eq!(result["code"], "CONFIRM_REQUIRED");
            assert!(result.get("data").is_none());
        }
    }
}
#[test]
fn dimensions_original_no_identity_wrappers_or_cross_operation_selectors() {
    for (name, _, read) in bindings() {
        for key in ["actor", "principal", "body", "command", "input", "unknown"] {
            let mut args = json!({"company":"synthetic"});
            args[key] = json!("forged");
            if !read {
                args["confirm"] = json!(true);
            }
            let rows = parsed(&run(
                "http://127.0.0.1:9",
                &[init("2025-11-25"), ready(), call(2, name, args)],
            ));
            assert_eq!(rows[1]["error"]["code"], -32602, "{name}: {key}: {rows:?}");
        }
    }
    for (name, args) in [
        (
            "dimension_assignment_plan",
            json!({"company":"synthetic","journalLineId":0,"allocations":[]}),
        ),
        (
            "dimension_definition_list",
            json!({"company":"synthetic","confirm":true}),
        ),
        (
            "dimension_member_list",
            json!({"company":"synthetic","journalLineId":7}),
        ),
        (
            "dimension_budget_list",
            json!({"company":"synthetic","period":"2026-01"}),
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
fn dimensions_original_plan_business_false_is_deliberately_outer_false() {
    for (name, args, path) in [
        ("dimension_assignment_plan", assignment(), "plan"),
        ("dimension_budget_plan", budget(), "budget-plan"),
    ] {
        let reply = r#"{"ok":false,"errors":["inactive member alpha"]}"#;
        let response = format!(
            "HTTP/1.1 422 Unprocessable Entity\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        );
        let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
        let rows = parsed(&run(
            &origin,
            &[init("2025-11-25"), ready(), call(2, name, args.clone())],
        ));
        assert_eq!(
            checked_result(&rows),
            &json!({"ok":false,"errors":["inactive member alpha"]}),
            "{rows:?}"
        );
        let wire = request.recv().unwrap();
        assert!(wire.starts_with(
            format!("POST /api/companies/synthetic/original/dimensions/{path} HTTP/1.1").as_bytes()
        ));
        let offset = wire.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let body: Value = serde_json::from_slice(&wire[offset..]).unwrap();
        let mut expected = args;
        expected.as_object_mut().unwrap().remove("company");
        assert_eq!(body, expected);
        task.join().unwrap();
    }
}
#[test]
fn dimensions_original_four_list_envelopes_only_contain_named_collection() {
    for (name, args, path, key) in [
        (
            "dimension_definition_list",
            json!({"company":"synthetic"}),
            "definitions",
            "definitions",
        ),
        (
            "dimension_member_list",
            json!({"company":"synthetic","dimensionId":"project"}),
            "members?dimensionId=project",
            "members",
        ),
        (
            "dimension_assignment_list",
            json!({"company":"synthetic","journalLineId":7}),
            "assignments?journalLineId=7",
            "assignments",
        ),
        (
            "dimension_budget_list",
            json!({"company":"synthetic"}),
            "budgets",
            "budgets",
        ),
    ] {
        let mut reply = json!({"ok":true});
        reply[key] = json!([]);
        let reply = reply.to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        );
        let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
        let rows = parsed(&run(
            &origin,
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        let result = checked_result(&rows);
        assert_eq!(result["ok"], true, "{rows:?}");
        assert_eq!(result["errors"], json!([]));
        let mut expected = json!({});
        expected[key] = json!([]);
        assert_eq!(result["data"], expected);
        assert!(request.recv().unwrap().starts_with(
            format!("GET /api/companies/synthetic/original/dimensions/{path} HTTP/1.1").as_bytes()
        ));
        task.join().unwrap();
    }
}
#[test]
fn dimensions_original_write_receipts_replay_and_unknown_never_retry() {
    for (name, mut args, path, reply) in [
        (
            "dimension_definition_create",
            json!({"company":"synthetic","dimensionId":"project","kind":"project","name":"Synthetic project"}),
            "define",
            json!({"ok":true,"id":1,"idempotent":false}),
        ),
        (
            "dimension_assignment_apply",
            assignment(),
            "apply",
            json!({"ok":true,"id":7,"idempotent":false,"planHash":"a".repeat(64)}),
        ),
        (
            "dimension_assignment_replace",
            assignment(),
            "replace",
            json!({"ok":true,"id":8,"idempotent":true,"planHash":"a".repeat(64)}),
        ),
        (
            "dimension_budget_apply",
            budget(),
            "budget-apply",
            json!({"ok":true,"ids":[9],"idempotent":true,"errors":[]}),
        ),
    ] {
        args["confirm"] = json!(true);
        if name != "dimension_definition_create" {
            args["planHash"] = json!("a".repeat(64));
            args["idempotencyKey"] = json!("synthetic%_key");
        }
        if name == "dimension_assignment_replace" {
            args["expectedAssignmentId"] = json!(7);
            args["reason"] = json!("reviewed correction");
        }
        for unknown in [false, true] {
            let body = if unknown {
                String::new()
            } else {
                reply.to_string()
            };
            let status = if unknown {
                "503 Service Unavailable"
            } else {
                "200 OK"
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
                body.len()
            );
            let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
            let rows = parsed(&run(
                &origin,
                &[init("2025-11-25"), ready(), call(2, name, args.clone())],
            ));
            let result = checked_result(&rows);
            if unknown {
                assert_eq!(result["code"], "outcome_unknown", "{rows:?}");
                assert!(result.get("data").is_none());
            } else {
                let mut expected = reply.clone();
                expected.as_object_mut().unwrap().remove("ok");
                expected.as_object_mut().unwrap().remove("errors");
                assert_eq!(result["data"], expected, "{rows:?}");
                assert_eq!(result["errors"], json!([]));
            }
            let wire = request.recv().unwrap();
            assert!(
                wire.starts_with(
                    format!("POST /api/companies/synthetic/original/dimensions/{path} HTTP/1.1")
                        .as_bytes()
                )
            );
            let offset = wire.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
            let mut expected = args.clone();
            expected.as_object_mut().unwrap().remove("company");
            assert_eq!(
                serde_json::from_slice::<Value>(&wire[offset..]).unwrap(),
                expected
            );
            task.join().unwrap();
        }
    }
}
