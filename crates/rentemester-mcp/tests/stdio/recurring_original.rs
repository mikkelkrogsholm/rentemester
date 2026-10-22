use super::*;
fn create() -> Value {
    json!({"company":"synthetic","name":"Synthetic subscription","interval":"monthly","firstIssueDate":"2024-01-31","invoice":{"invoiceType":"simplified","seller":{"name":"Synthetic Seller","address":"S1","vatOrCvr":"87654321"},"lines":[{"description":"Synthetic reduced service"}],"totals":{"grossAmount":125,"vatRate":0.25,"vatComputationBasis":"VAT_20_OF_GROSS"}},"confirm":true})
}
#[test]
fn original_recurring_discovery_source_shapes_confirmation_and_no_forged_authority() {
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
    assert_eq!(names.len(), tools.len(), "tool names must stay unique");
    let new_names = [
        "recurring_invoice_create",
        "recurring_invoice_generate",
        "recurring_invoice_list",
        "recurring_invoice_run_workspace",
    ];
    assert_eq!(
        names
            .iter()
            .filter(|name| !new_names.contains(name))
            .count(),
        288,
        "unchanged source baseline plus exactly four original recurring tools"
    );
    for native in [
        "recurring_invoices",
        "create_recurring_invoice",
        "generate_recurring_invoice",
        "retire_recurring_invoice",
    ] {
        assert!(names.contains(native));
    }
    for (name, read, idem) in [
        ("recurring_invoice_create", false, false),
        ("recurring_invoice_generate", false, true),
        ("recurring_invoice_list", true, true),
    ] {
        let tool = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(
            tool["annotations"],
            json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":idem,"openWorldHint":false})
        );
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert!(tool["outputSchema"]["properties"]["data"].is_object());
    }
    let mut unconfirmed = create();
    unconfirmed.as_object_mut().unwrap().remove("confirm");
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            call(2, "recurring_invoice_create", unconfirmed),
        ],
    ));
    let result = &rows[1]["result"];
    assert_eq!(result["structuredContent"]["code"], "CONFIRM_REQUIRED");
    assert_eq!(result["isError"], true);
    for (name, args) in [
        ("recurring_invoice_create", {
            let mut v = create();
            v["actor"] = json!("forged");
            v
        }),
        (
            "recurring_invoice_generate",
            json!({"company":"synthetic","templateId":1,"asOfDate":"2024-01-31","confirm":true,"periodIndex":0}),
        ),
        (
            "recurring_invoice_list",
            json!({"company":"synthetic","includeInactive":null}),
        ),
        (
            "recurring_invoice_list",
            json!({"company":"synthetic","confirm":true}),
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
fn original_recurring_stdio_preserves_core_data_rules_and_failures() {
    for (name, args, core, envelope, path) in [
        (
            "recurring_invoice_create",
            create(),
            json!({"ok":true,"templateId":7,"appliedRules":["DK-RECURRING-INVOICE-TEMPLATE-001"],"errors":[]}),
            json!({"ok":true,"data":{"templateId":7},"appliedRules":["DK-RECURRING-INVOICE-TEMPLATE-001"],"errors":[]}),
            "POST /api/companies/synthetic/recurring-invoices/original/create ",
        ),
        (
            "recurring_invoice_generate",
            json!({"company":"synthetic","templateId":7,"asOfDate":"2024-02-29","confirm":true}),
            json!({"ok":true,"created":false,"templateId":7,"periodIndex":1,"documentId":11,"invoiceNumber":"2024-0001","issueDate":"2024-02-29","dueDate":"2024-03-30","appliedRules":["DK-RECURRING-INVOICE-GENERATE-001"],"errors":[]}),
            json!({"ok":true,"data":{"created":false,"templateId":7,"periodIndex":1,"documentId":11,"invoiceNumber":"2024-0001","issueDate":"2024-02-29","dueDate":"2024-03-30"},"appliedRules":["DK-RECURRING-INVOICE-GENERATE-001"],"errors":[]}),
            "POST /api/companies/synthetic/recurring-invoices/original/generate ",
        ),
        (
            "recurring_invoice_list",
            json!({"company":"synthetic","includeInactive":true}),
            json!({"ok":true,"count":1,"rows":[{"id":7,"active":false,"deliveryChannel":"digisense"}],"errors":[]}),
            json!({"ok":true,"data":{"count":1,"rows":[{"id":7,"active":false,"deliveryChannel":"digisense"}]},"errors":[]}),
            "GET /api/companies/synthetic/recurring-invoices/original/list?includeInactive=true ",
        ),
        (
            "recurring_invoice_generate",
            json!({"company":"synthetic","templateId":7,"asOfDate":"2024-02-29","confirm":true}),
            json!({"ok":false,"templateId":7,"appliedRules":["DK-RECURRING-INVOICE-GENERATE-001"],"errors":["recurring invoice template 7 does not exist"]}),
            json!({"ok":false,"code":"NOT_FOUND","appliedRules":["DK-RECURRING-INVOICE-GENERATE-001"],"errors":["recurring invoice template 7 does not exist"]}),
            "POST /api/companies/synthetic/recurring-invoices/original/generate ",
        ),
    ] {
        let reply = core.to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        );
        let (origin, req, t) = http(Box::leak(response.into_boxed_str()));
        let rows = parsed(&run(
            &origin,
            &[init("2025-11-25"), ready(), call(2, name, args.clone())],
        ));
        let result = &rows[1]["result"];
        assert_eq!(result["structuredContent"], envelope);
        assert_eq!(
            serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
            envelope
        );
        assert_eq!(result["isError"], envelope["ok"] != true);
        let request = String::from_utf8(req.recv().unwrap()).unwrap();
        assert!(request.starts_with(path));
        if !path.starts_with("GET") {
            let body: Value =
                serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
            let mut expected = args;
            expected.as_object_mut().unwrap().remove("company");
            assert_eq!(body, expected);
        }
        t.join().unwrap();
    }
}

#[test]
fn workspace_recurring_stdio_preserves_source_success_and_partial_failure() {
    let args = json!({"workspace":"/configured/workspace","asOfDate":"2024-07-31","maxGenerations":2,"confirm":true});
    for core in [
        json!({"ok":true,"companies":[{"slug":"next","status":"processed","generated":2,"attempted":1}],"errors":[],"hasMore":true,"remainingGenerations":3,"continuation":{"remainingGenerations":3,"companies":[{"slug":"next","remainingGenerations":3}]}}),
        json!({"ok":false,"companies":[{"slug":"blocked","status":"failed","reason":"run failed"},{"slug":"next","status":"processed","generated":2,"attempted":0}],"errors":["recurring run failed for company blocked"],"hasMore":true,"remainingGenerations":3,"continuation":{"remainingGenerations":3,"companies":[{"slug":"next","remainingGenerations":3}]}}),
    ] {
        let payload = core.to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{payload}",
            payload.len()
        );
        let (origin, req, t) = http(Box::leak(response.into_boxed_str()));
        let rows = parsed(&run(
            &origin,
            &[
                init("2025-11-25"),
                ready(),
                call(2, "recurring_invoice_run_workspace", args.clone()),
            ],
        ));
        let result = &rows[1]["result"];
        let expected = if core["ok"] == true {
            json!({"ok":true,"data":core,"errors":[]})
        } else {
            let mut data = core.as_object().unwrap().clone();
            let errors = data.remove("errors").unwrap();
            data.remove("ok");
            json!({"ok":false,"data":data,"errors":errors})
        };
        assert_eq!(result["structuredContent"], expected);
        assert_eq!(
            serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
            expected
        );
        assert_eq!(result["isError"], core["ok"] != true);
        let request = String::from_utf8(req.recv().unwrap()).unwrap();
        assert!(request.starts_with("POST /api/recurring-invoices/run-workspace "));
        assert_eq!(
            serde_json::from_str::<Value>(request.split("\r\n\r\n").nth(1).unwrap()).unwrap(),
            args
        );
        t.join().unwrap();
    }
    let mut unconfirmed = args.clone();
    unconfirmed.as_object_mut().unwrap().remove("confirm");
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            call(2, "recurring_invoice_run_workspace", unconfirmed),
        ],
    ));
    assert_eq!(
        rows[1]["result"]["structuredContent"]["code"],
        "CONFIRM_REQUIRED"
    );
    for bad in [
        json!({"workspace":"w","asOfDate":"2024-01-01","maxGenerations":5001,"confirm":true}),
        json!({"workspace":"w","asOfDate":"2024-01-01","actor":"forged","confirm":true}),
        json!({"workspace":"w","asOfDate":"2024-01-01","company":"forged","confirm":true}),
    ] {
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[
                init("2025-11-25"),
                ready(),
                call(2, "recurring_invoice_run_workspace", bad),
            ],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602);
    }
}
