use super::*;
#[test]
fn three_original_names_native_aliases_strict_five_treatments_and_confirmation() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 322);
    for name in [
        "expense_vat_preflight",
        "expense_vat_preflight_apply",
        "expense_book",
        "expense_booking_options",
        "match_bank_expense",
    ] {
        assert_eq!(tools.iter().filter(|t| t["name"] == name).count(), 1);
    }
    let book = tools.iter().find(|t| t["name"] == "expense_book").unwrap();
    let treatments = book["inputSchema"]["properties"]["vatTreatment"]["enum"]
        .as_array()
        .unwrap();
    assert_eq!(treatments.len(), 5);
    assert!(!treatments.contains(&json!("eu_goods_acquisition")));
    assert_eq!(book["inputSchema"]["additionalProperties"], false);
    assert!(
        book["inputSchema"]["properties"]
            .get("idempotencyKey")
            .is_some()
    );
    for name in ["expense_vat_preflight_apply", "expense_book"] {
        let mut args = json!({"company":"synthetic","documentId":3});
        if name == "expense_book" {
            args["bankTransactionId"] = json!(7);
            args["expenseAccount"] = json!("synthetic-expense");
        }
        for confirm in [None, Some(false)] {
            let mut args = args.clone();
            if let Some(v) = confirm {
                args["confirm"] = json!(v);
            }
            let rows = parsed(&run(
                "http://127.0.0.1:9",
                &[
                    init("2025-11-25"),
                    ready(),
                    json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":name,"arguments":args}}),
                ],
            ));
            assert_eq!(
                rows[1]["result"]["structuredContent"]["code"],
                "CONFIRM_REQUIRED"
            );
        }
        for (key, value) in [
            ("actor", json!("forged")),
            ("principal", Value::Null),
            ("confirm", Value::Null),
            ("expectedActor", json!("forged")),
            ("documentId", json!("3")),
        ] {
            let mut args = args.clone();
            args[key] = value;
            let rows = parsed(&run(
                "http://127.0.0.1:9",
                &[
                    init("2025-11-25"),
                    ready(),
                    json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":name,"arguments":args}}),
                ],
            ));
            assert_eq!(rows[1]["error"]["code"], -32602);
        }
    }
}
