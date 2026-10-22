use super::*;
fn registration() -> Value {
    json!({"company":"synthetic","name":"Synthetic","category":"Equipment","acquisitionDate":"2026-01-01","cost":125,"usefulLifeMonths":3,"documentId":7})
}
fn writeoff() -> Value {
    json!({"company":"synthetic","name":"Synthetic","category":"Equipment","acquisitionDate":"2026-01-01","cost":250,"documentId":8,"expenseAccount":"expense","date":"2026-01-02","thresholdRuleSource":"Advisor assessment","confirmImmediateWriteOff":false})
}
#[test]
fn discovery_native_aliases_strict_schemas_and_outer_confirmation() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 329);
    for name in [
        "register_asset",
        "depreciate_asset",
        "assets",
        "next_asset_depreciation",
        "asset_register",
        "asset_depreciate",
        "asset_write_off",
        "asset_register_report",
    ] {
        assert!(tools.iter().any(|t| t["name"] == name), "{name}");
    }
    for name in [
        "asset_register",
        "asset_depreciate",
        "asset_write_off",
        "asset_register_report",
    ] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        let read = name == "asset_register_report";
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        assert_eq!(
            t["annotations"],
            json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read,"openWorldHint":false})
        );
        if !read {
            assert!(
                !t["inputSchema"]["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("confirm"))
            );
        }
        assert!(
            t["inputSchema"]["properties"]
                .get("idempotencyKey")
                .is_none()
        );
    }
    let schema = &tools
        .iter()
        .find(|t| t["name"] == "asset_write_off")
        .unwrap()["inputSchema"];
    assert_eq!(
        schema["properties"]["confirmImmediateWriteOff"]["type"],
        "boolean"
    );
    assert!(
        schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("confirmImmediateWriteOff"))
    );
    for (name, args) in [
        ("asset_register", registration()),
        (
            "asset_depreciate",
            json!({"company":"synthetic","assetId":4,"period":1,"date":"2026-01-01"}),
        ),
        ("asset_write_off", writeoff()),
    ] {
        for confirmation in [None, Some(false)] {
            let mut args = args.clone();
            if let Some(c) = confirmation {
                args["confirm"] = json!(c);
            }
            let rows = parsed(&run(
                "http://127.0.0.1:9",
                &[init("2025-11-25"), ready(), call(3, name, args)],
            ));
            assert_eq!(
                rows[1]["result"]["structuredContent"]["code"],
                "CONFIRM_REQUIRED"
            );
        }
    }
    for (key, value) in [
        ("note", Value::Null),
        ("principal", json!("forged")),
        ("idempotencyKey", json!("key")),
        ("confirmImmediateWriteOff", json!("false")),
    ] {
        let mut args = writeoff();
        args["confirm"] = json!(true);
        args[key] = value;
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[
                init("2025-11-25"),
                ready(),
                call(3, "asset_write_off", args),
            ],
        ));
        assert!(rows[1]["error"].is_object(), "{rows:?}");
    }
}
#[test]
fn source_routes_envelopes_and_false_inner_is_business_refusal() {
    for (name, mut args, core, target) in [
        (
            "asset_register",
            registration(),
            json!({"ok":true,"assetId":4,"totalPeriods":3,"periodAmount":41.66,"appliedRules":["DK-ASSET-DEPR-001"],"errors":[]}),
            "POST /api/companies/synthetic/asset/original/register ",
        ),
        (
            "asset_depreciate",
            json!({"company":"synthetic","assetId":4,"period":1,"date":"2026-01-01"}),
            json!({"ok":true,"entryId":1,"entryNo":"2026-0001","entryHash":"a".repeat(64),"assetId":4,"periodIndex":1,"periodAmount":41.66,"appliedRules":["DK-ASSET-DEPR-001"],"errors":[]}),
            "POST /api/companies/synthetic/asset/original/4/depreciate ",
        ),
        (
            "asset_write_off",
            writeoff(),
            json!({"ok":false,"appliedRules":["DK-ASSET-WRITEOFF-001"],"errors":["Immediate write-off requires explicit confirmation"]}),
            "POST /api/companies/synthetic/asset/original/write-off ",
        ),
        (
            "asset_register_report",
            json!({"company":"synthetic"}),
            json!({"ok":true,"assets":[],"totals":{"cost":0,"accumulatedDepreciation":0,"netBookValue":0},"errors":[]}),
            "GET /api/companies/synthetic/asset/original/register-report ",
        ),
    ] {
        if name != "asset_register_report" {
            args["confirm"] = json!(true);
        }
        let body = core.to_string();
        let response = format!(
            "HTTP/1.1 {} Mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            if core["ok"] == true { 200 } else { 422 },
            body.len(),
            body
        );
        let (origin, request, t) = http(Box::leak(response.into_boxed_str()));
        let rows = parsed(&run(
            &origin,
            &[init("2025-11-25"), ready(), call(3, name, args)],
        ));
        let wire = String::from_utf8(request.recv().unwrap()).unwrap();
        t.join().unwrap();
        assert!(wire.starts_with(target));
        if name == "asset_write_off" {
            let input: Value =
                serde_json::from_str(wire.split_once("\r\n\r\n").unwrap().1).unwrap();
            assert_eq!(input["confirmImmediateWriteOff"], false);
            assert_eq!(input["confirm"], true);
        }
        let result = &rows[1]["result"];
        let envelope = &result["structuredContent"];
        assert_eq!(
            serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
            *envelope
        );
        assert_eq!(envelope["ok"], core["ok"]);
        assert_eq!(result["isError"], core["ok"] != true);
        if core["ok"] == true {
            assert!(envelope["data"].is_object());
        } else {
            assert!(envelope.get("data").is_none());
            assert_eq!(envelope, &core);
        }
    }
}
