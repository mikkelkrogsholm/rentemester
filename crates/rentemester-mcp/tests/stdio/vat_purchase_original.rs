use super::*;
fn payload(rep: bool) -> Value {
    let mut v = json!({"transactionDate":"","text":"","netAmount":-1});
    if rep {
        v["documentId"] = json!(3);
    } else {
        v["expenseAccountNo"] = json!("synthetic");
        v["invoiceNo"] = json!(" INV ");
    }
    v
}
#[test]
fn registry_341_unique_strict_two_writes_and_confirmation_before_http() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    let names = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(names.len(), 341);
    assert_eq!(tools.len(), 341);
    for name in [
        "vat_report",
        "vat_eu_sales_list",
        "vat_oss_report",
        "expense_book",
        "expense_booking_options",
        "match_bank_expense",
    ] {
        assert!(names.contains(name));
    }
    for (name, rep) in [
        ("vat_post_eu_service_purchase", false),
        ("vat_post_representation_purchase", true),
    ] {
        let tool = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(
            tool["annotations"],
            json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false})
        );
        assert_eq!(
            tool["inputSchema"]["required"],
            json!(["company", "payload"])
        );
        assert_eq!(
            tool["inputSchema"]["properties"].as_object().unwrap().len(),
            3
        );
        assert_eq!(
            tool["inputSchema"]["properties"]["payload"]["additionalProperties"],
            false
        );
        for confirm in [None, Some(false)] {
            let mut args = json!({"company":"synthetic","payload":payload(rep)});
            if let Some(c) = confirm {
                args["confirm"] = json!(c);
            }
            let rows = parsed(&run(
                "http://127.0.0.1:9",
                &[init("2025-11-25"), ready(), call(3, name, args)],
            ));
            let result = &rows[1]["result"];
            assert_eq!(
                result["structuredContent"],
                json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":[format!("confirm: true required for write tool {name}")]})
            );
            assert_eq!(result["isError"], true);
        }
        for (key, value) in [
            ("actor", json!("forged")),
            ("program", json!("x")),
            ("documentId", json!(9007199254740992u64)),
            ("sourceBankTransactionId", json!(0)),
            ("purchaseVatLines", json!([])),
        ] {
            let mut input = payload(rep);
            input[key] = value;
            let rows = parsed(&run(
                "http://127.0.0.1:9",
                &[
                    init("2025-11-25"),
                    ready(),
                    call(
                        3,
                        name,
                        json!({"company":"synthetic","payload":input,"confirm":true}),
                    ),
                ],
            ));
            assert_eq!(rows[1]["error"]["code"], -32602);
        }
    }
}
#[test]
fn source_receipt_and_false_without_data_or_invented_code() {
    for (core, code) in [
        (
            json!({"ok":true,"entryId":8,"entryNo":"2026-8","entryHash":"a".repeat(64),"appliedRules":["DK-VAT-REVERSE-CHARGE-001"],"errors":[]}),
            None,
        ),
        (
            json!({"ok":false,"errors":["Could not resolve document for invoiceNo INV"],"appliedRules":[]}),
            None,
        ),
        (
            json!({"ok":false,"errors":["Document does not exist"],"appliedRules":[]}),
            Some("NOT_FOUND"),
        ),
        (
            json!({"ok":false,"errors":["Date falls in closed period"],"appliedRules":["DK-VAT-REVERSE-CHARGE-001"]}),
            Some("PERIOD_CLOSED"),
        ),
    ] {
        let text = core.to_string();
        let response = format!(
            "HTTP/1.1 {} Mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            if core["ok"] == true { 200 } else { 422 },
            text.len()
        );
        let (origin, request, t) = http(Box::leak(response.into_boxed_str()));
        let rows = parsed(&run(
            &origin,
            &[
                init("2025-11-25"),
                ready(),
                call(
                    3,
                    "vat_post_eu_service_purchase",
                    json!({"company":"synthetic","payload":payload(false),"confirm":true}),
                ),
            ],
        ));
        let wire = String::from_utf8(request.recv().unwrap()).unwrap();
        t.join().unwrap();
        assert!(
            wire.starts_with("POST /api/companies/synthetic/original/vat/eu-service-purchase ")
        );
        let body: Value = serde_json::from_str(wire.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body, json!({"payload":payload(false),"confirm":true}));
        let result = &rows[1]["result"];
        let envelope = &result["structuredContent"];
        assert_eq!(
            serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
            *envelope
        );
        assert_eq!(envelope["ok"], core["ok"]);
        assert_eq!(result["isError"], core["ok"] != true);
        if core["ok"] == true {
            assert_eq!(
                envelope["data"],
                json!({"entryId":8,"entryNo":"2026-8","entryHash":"a".repeat(64)})
            );
        } else {
            assert!(envelope.get("data").is_none());
        }
        assert_eq!(envelope.get("code").and_then(Value::as_str), code);
        if core["appliedRules"] == json!([]) {
            assert!(envelope.get("appliedRules").is_none());
        }
    }
}
