use super::*;
#[test]
fn source_discovery_optional_boolean_confirmation_and_disjoint_comparison() {
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
        "budget_set",
        "budget_list",
        "budget_vs_actual",
        "budget_forecast",
        "liquidity_forecast_13_week",
        "budget",
        "set_budget",
    ] {
        assert!(names.contains(name));
    }
    let comparison = tools
        .iter()
        .find(|t| t["name"] == "budget_vs_actual")
        .unwrap();
    assert_eq!(comparison["inputSchema"]["type"], "object");
    assert_eq!(
        comparison["inputSchema"]["oneOf"].as_array().unwrap().len(),
        2
    );
    let set = tools.iter().find(|t| t["name"] == "budget_set").unwrap();
    assert_eq!(
        set["inputSchema"]["properties"]["confirm"],
        json!({"type":"boolean"})
    );
    assert!(
        !set["inputSchema"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("confirm"))
    );
    assert_eq!(set["annotations"]["idempotentHint"], false);
    for args in [
        json!({"company":"synthetic","accountNo":"expense","period":"2026-01","amount":90}),
        json!({"company":"synthetic","accountNo":"expense","period":"2026-01","amount":90,"confirm":false}),
    ] {
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, "budget_set", args)],
        ));
        assert_eq!(
            rows[1]["result"]["structuredContent"]["code"],
            "CONFIRM_REQUIRED"
        );
    }
    for args in [
        json!({"company":"synthetic"}),
        json!({"company":"synthetic","from":"2026-01"}),
        json!({"company":"synthetic","to":"2026-01"}),
        json!({"company":"synthetic","year":"2026","from":"2026-01","to":"2026-01"}),
        json!({"company":"synthetic","year":"2026","to":"2026-01"}),
    ] {
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[
                init("2025-11-25"),
                ready(),
                call(2, "budget_vs_actual", args),
            ],
        ));
        assert!(rows[1].get("error").is_some(), "{rows:?}");
    }
}
#[test]
fn source_compare_and_native_year_dispatch_keep_distinct_envelopes() {
    for (args, reply, path, source) in [
        (
            json!({"company":"synthetic","from":"2026-01","to":"2026-01"}),
            r#"{"ok":true,"appliedRules":["budget:vs-actual"],"periodStart":"2026-01","periodEnd":"2026-01","lines":[],"totalBudget":0,"totalActual":0,"totalVariance":0,"errors":[]}"#,
            "GET /api/companies/synthetic/original/budget/vs-actual?from=2026-01&to=2026-01 ",
            true,
        ),
        (
            json!({"company":"synthetic","year":"2026"}),
            r#"{"ok":true,"selectedYear":"2026","fiscalYears":["2026"],"nativeEvidence":"preserve"}"#,
            "GET /api/companies/synthetic/budget-vs-actual?year=2026 ",
            false,
        ),
    ] {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        );
        let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
        let rows = parsed(&run(
            &origin,
            &[
                init("2025-11-25"),
                ready(),
                call(2, "budget_vs_actual", args),
            ],
        ));
        let got = request
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        assert!(got.starts_with(path.as_bytes()), "{got:?}");
        task.join().unwrap();
        let v = &rows[1]["result"]["structuredContent"];
        assert_eq!(v["ok"], true);
        if source {
            assert_eq!(v["appliedRules"], json!(["budget:vs-actual"]));
            assert_eq!(v["data"]["totalVariance"], 0);
        } else {
            assert!(v.to_string().contains("preserve"));
        }
    }
}
#[test]
fn domain_422_retains_report_data_without_fabricated_code() {
    let reply = r#"{"ok":false,"appliedRules":["liquidity-forecast"],"openingBalance":0,"closingBalance":0,"periods":[],"errors":["Opening cash unavailable"]}"#;
    let response = format!(
        "HTTP/1.1 422 Unprocessable Entity\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
        reply.len()
    );
    let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
    let rows = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "budget_forecast",
                json!({"company":"synthetic","startDate":"2026-01-15","months":1}),
            ),
        ],
    ));
    request
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap();
    task.join().unwrap();
    let v = &rows[1]["result"]["structuredContent"];
    assert_eq!(v["ok"], false);
    assert!(v.get("code").is_none());
    assert_eq!(v["data"]["periods"], json!([]));
    assert_eq!(v["errors"], json!(["Opening cash unavailable"]));
}
