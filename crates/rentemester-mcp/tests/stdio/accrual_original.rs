use super::*;
fn registration() -> Value {
    json!({"company":"synthetic","accrualType":"prepaid_expense","description":"Synthetic prepayment","totalAmount":125.09,"recognitionPeriods":3,"firstRecognitionDate":"2026-01-31","resultAccountNo":"expense"})
}
#[test]
fn three_original_names_strict_source_schema_native_aliases_confirmation_and_discovery() {
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
    let names = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(names.len(), EXPECTED_NATIVE_TOOLS);
    for name in [
        "register_accrual",
        "recognize_accrual",
        "accruals",
        "accrual_schedule",
        "accrual_register",
        "accrual_recognize",
        "accrual_register_report",
    ] {
        assert!(names.contains(name));
    }
    for (name, read) in [
        ("accrual_register", false),
        ("accrual_recognize", false),
        ("accrual_register_report", true),
    ] {
        let tool = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert_eq!(
            tool["annotations"],
            json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read,"openWorldHint":false})
        );
        assert!(
            tool["inputSchema"]["properties"]
                .get("idempotencyKey")
                .is_none()
        );
        if !read {
            assert!(
                !tool["inputSchema"]["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("confirm"))
            );
        }
    }
    for name in ["accrual_register", "accrual_recognize"] {
        let args = if name == "accrual_register" {
            registration()
        } else {
            json!({"company":"synthetic","accrualId":4,"period":1})
        };
        for confirmation in [None, Some(false)] {
            let mut args = args.clone();
            if let Some(c) = confirmation {
                args["confirm"] = json!(c);
            }
            let rows = parsed(&run(
                "http://127.0.0.1:9",
                &[init("2025-11-25"), ready(), call(3, name, args)],
            ));
            let r = &rows[1]["result"];
            assert_eq!(r["structuredContent"]["code"], "CONFIRM_REQUIRED");
            assert_eq!(r["isError"], true);
            assert!(r["structuredContent"].get("data").is_none());
            assert_eq!(
                serde_json::from_str::<Value>(r["content"][0]["text"].as_str().unwrap()).unwrap(),
                r["structuredContent"]
            );
        }
        for (key, value) in [
            ("confirm", Value::Null),
            ("actor", json!("forged")),
            ("principal", json!("forged")),
            ("idempotencyKey", json!("key")),
            ("expectedActor", Value::Null),
            ("expectedActor", json!(" forged ")),
            ("unknown", json!(true)),
        ] {
            let mut args = args.clone();
            args[key] = value;
            let rows = parsed(&run(
                "http://127.0.0.1:9",
                &[init("2025-11-25"), ready(), call(3, name, args)],
            ));
            assert_eq!(rows[1]["error"]["code"], -32602, "{key}");
        }
    }
    for args in [
        json!({"company":"synthetic","confirm":true}),
        json!({"company":"synthetic","expectedActor":"user:synthetic"}),
        json!({"company":"synthetic","unknown":null}),
    ] {
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[
                init("2025-11-25"),
                ready(),
                call(3, "accrual_register_report", args),
            ],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602);
    }
}

#[test]
fn stdio_source_envelope_receipts_and_domain_failure_never_success_data() {
    for (name, mut args, core, target) in [
        (
            "accrual_register",
            registration(),
            json!({"ok":true,"entryId":9,"entryNo":"2026-0009","entryHash":"a".repeat(64),"appliedRules":["DK-BOOKKEEPING-ACCRUAL-001"],"errors":[],"accrualId":4,"accrualType":"prepaid_expense","totalPeriods":3,"periodAmount":41.69}),
            "POST /api/companies/synthetic/accrual/original/register ",
        ),
        (
            "accrual_recognize",
            json!({"company":"synthetic","accrualId":4,"period":2,"date":"2026-01-01"}),
            json!({"ok":true,"entryId":10,"entryNo":"2026-0010","entryHash":"b".repeat(64),"appliedRules":["DK-BOOKKEEPING-ACCRUAL-001"],"errors":[],"accrualId":4,"periodIndex":2,"periodAmount":41.69,"recognizedPeriods":1,"totalPeriods":3,"fullyRecognized":false}),
            "POST /api/companies/synthetic/accrual/original/4/recognize ",
        ),
        (
            "accrual_register_report",
            json!({"company":"synthetic"}),
            json!({"ok":true,"accruals":[],"totals":{"totalAmount":0,"recognizedAmount":0,"remainingAmount":0},"errors":[]}),
            "GET /api/companies/synthetic/accrual/original/register-report ",
        ),
        (
            "accrual_recognize",
            json!({"company":"synthetic","accrualId":4,"period":2}),
            json!({"ok":false,"errors":["period already recognized"],"appliedRules":["DK-BOOKKEEPING-ACCRUAL-001"]}),
            "POST /api/companies/synthetic/accrual/original/4/recognize ",
        ),
    ] {
        if name != "accrual_register_report" {
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
        assert!(!wire.contains("idempotencyKey"));
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
            assert!(envelope["data"].get("ok").is_none());
        } else {
            assert!(envelope.get("data").is_none());
            assert_eq!(envelope["appliedRules"], core["appliedRules"]);
        }
    }
}
