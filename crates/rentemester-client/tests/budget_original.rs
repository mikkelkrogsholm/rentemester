use rentemester_client::{ApiClient, Error, validate_original_budget_result};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};
fn server(status: u16, value: Value) -> (ApiClient, thread::JoinHandle<(String, Value)>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = ApiClient::new(
        &format!("http://{}", listener.local_addr().unwrap()),
        true,
        None,
    )
    .unwrap();
    let t = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(4)))
            .unwrap();
        let mut bytes = vec![];
        let end = loop {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let header = String::from_utf8_lossy(&bytes[..end]).to_string();
        let len: usize = header
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse().unwrap())
            })
            .unwrap_or(0);
        while bytes.len() < end + len {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
        }
        let body = if len == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[end..end + len]).unwrap()
        };
        let text = value.to_string();
        write!(
            s,
            "HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        )
        .unwrap();
        (header, body)
    });
    (api, t)
}

#[tokio::test]
async fn five_fixed_routes_source_422_and_one_confirmed_append() {
    let list = json!({"ok":true,"count":0,"rows":[],"errors":[]});
    let (api, t) = server(200, list.clone());
    assert_eq!(
        api.original_budget_list("synthetic", Some("2026-01"), Some("expense"))
            .await
            .unwrap(),
        list
    );
    let (h, b) = t.join().unwrap();
    assert!(h.starts_with(
        "GET /api/companies/synthetic/original/budget/list?period=2026-01&accountNo=expense "
    ));
    assert!(b.is_null());
    let compare = json!({"ok":true,"appliedRules":["budget:vs-actual"],"periodStart":"2026-01","periodEnd":"2026-01","lines":[{"accountNo":"expense","accountName":null,"accountType":"expense","period":"2026-01","budget":90,"actual":60,"variance":30}],"totalBudget":90,"totalActual":60,"totalVariance":-30,"errors":[]});
    let (api, t) = server(200, compare.clone());
    assert_eq!(
        api.original_budget_vs_actual("synthetic", "2026-01", "2026-01")
            .await
            .unwrap(),
        compare
    );
    assert!(
        t.join()
            .unwrap()
            .0
            .contains("/original/budget/vs-actual?from=2026-01&to=2026-01")
    );
    let monthly = json!({"ok":true,"appliedRules":["liquidity-forecast"],"startDate":"2026-01-15","months":1,"openingBalance":1000,"closingBalance":1085,"periods":[{"period":"2026-01","openingBalance":1000,"invoiceInflow":125,"recurringInflow":50,"budgetedCostOutflow":90,"netChange":85,"closingBalance":1085}],"errors":[]});
    let (api, t) = server(200, monthly.clone());
    assert_eq!(
        api.original_budget_forecast("synthetic", "2026-01-15", "1e0")
            .await
            .unwrap(),
        monthly
    );
    assert!(t.join().unwrap().0.contains("months=1e0"));
    let invalid = json!({"ok":false,"openingCash":0,"lowestPoint":0,"periods":[],"completeness":{"included":[],"excluded":[]},"errors":["startDate and weeks (1-13) are required"],"appliedRules":["liquidity-forecast-13-week-v1"]});
    let (api, t) = server(422, invalid.clone());
    assert_eq!(
        api.original_budget_forecast_13_week("synthetic", "bad", Some("invalid"))
            .await
            .unwrap(),
        invalid
    );
    assert!(t.join().unwrap().0.contains("weeks=invalid"));
    let set = json!({"ok":true,"budgetLineId":2,"accountNo":"expense","period":"2026-01","amount":90,"appliedRules":["budget:set"],"errors":[]});
    let (api, t) = server(200, set.clone());
    assert_eq!(
        api.original_budget_set(
            "synthetic",
            br#"{"accountNo":"expense","period":"2026-01","amount":90}"#,
            true
        )
        .await
        .unwrap(),
        set
    );
    let (h, b) = t.join().unwrap();
    assert!(h.starts_with("POST /api/companies/synthetic/original/budget/set "));
    assert_eq!(b["confirm"], true);
    assert!(b.get("actor").is_none());
}
#[test]
fn strict_lowercase_rules_shape_arithmetic_and_source_buckets() {
    let weekly = json!({"ok":true,"startDate":"2026-01-01","openingCash":1000,"lowestPoint":1000,"completeness":{"included":["issued receivables"],"excluded":["undated budgets"]},"periods":[{"weekStart":"2026-01-01","openingCash":1000,"receivables":125,"payables":40,"commitments":30,"budgets":0,"undatedBudgetAssumptions":-90,"scenarios":0,"obligations":0,"intercompany":75,"excluded":[{"source":"foreign","amount":3,"currency":"EUR","reason":"dated_fx_required"}],"closingCash":1055,"scenarioClosingCash":1130,"sources":[{"source":"approved_intercompany_disposition","amount":75,"reference":"intercompany-disposition:synthetic:hash","assumption":true},{"source":"vat_obligation","amount":0,"reference":"vat:synthetic","settlementStatus":"unknown"}]}],"errors":[],"appliedRules":["liquidity-forecast-13-week-v3"]});
    assert!(validate_original_budget_result(
        "forecast-13-week",
        &weekly,
        true
    ));
    for (field, value) in [
        ("appliedRules", json!(["DK-FORECAST"])),
        ("lowestPoint", json!(1055)),
        ("company", json!("synthetic")),
    ] {
        let mut bad = weekly.clone();
        bad[field] = value;
        assert!(!validate_original_budget_result(
            "forecast-13-week",
            &bad,
            true
        ));
    }
    let mut bad = weekly.clone();
    bad["periods"][0]["closingCash"] = json!(965);
    assert!(!validate_original_budget_result(
        "forecast-13-week",
        &bad,
        true
    ));
    let mut bad = weekly.clone();
    bad["periods"][0]["sources"][1]["settlementStatus"] = json!("paid");
    assert!(!validate_original_budget_result(
        "forecast-13-week",
        &bad,
        true
    ));
    let list = json!({"ok":true,"count":0,"rows":[],"errors":[],"appliedRules":[]});
    assert!(!validate_original_budget_result("list", &list, true));
}
#[tokio::test]
async fn confirmation_structural_and_unknown_ack_never_retry() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    assert!(matches!(
        api.original_budget_set("synthetic", b"{}", false).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.original_budget_set(
            "synthetic",
            br#"{"accountNo":"x","period":"","amount":null}"#,
            true
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    for status in [200, 408, 500] {
        let (api, t) = server(status, json!({"ok":true}));
        assert_eq!(
            api.original_budget_set(
                "synthetic",
                br#"{"accountNo":"x","period":"","amount":1}"#,
                true
            )
            .await,
            Err(Error::OutcomeUnknown)
        );
        t.join().unwrap();
    }
    let refused = json!({"ok":false,"appliedRules":["budget:set"],"errors":["account missing","amount missing"]});
    let (api, t) = server(422, refused.clone());
    assert_eq!(
        api.original_budget_set("synthetic", br#"{"accountNo":"","period":""}"#, true)
            .await
            .unwrap(),
        refused
    );
    assert!(t.join().unwrap().1.get("amount").is_none());
    let (api, t) = server(403, json!({"ok":false,"errors":["denied"]}));
    assert_eq!(
        api.original_budget_list("synthetic", None, None).await,
        Err(Error::Refused { status: 403 })
    );
    t.join().unwrap();
}

#[test]
fn impossible_successful_horizons_are_rejected_without_weakening_refusals() {
    let mut monthly = json!({"ok":true,"appliedRules":["liquidity-forecast"],"startDate":"2026-01-15","months":1,"openingBalance":0,"closingBalance":0,"periods":[{"period":"2026-01","openingBalance":0,"invoiceInflow":0,"recurringInflow":0,"budgetedCostOutflow":0,"netChange":0,"closingBalance":0}],"errors":[]});
    assert!(validate_original_budget_result("forecast", &monthly, true));
    for (field, value) in [
        ("startDate", json!("")),
        ("startDate", json!("2026-02-30")),
        ("months", json!(0)),
        ("months", json!(19)),
        ("periods", json!([])),
    ] {
        let mut bad = monthly.clone();
        bad[field] = value;
        assert!(!validate_original_budget_result("forecast", &bad, true));
    }
    monthly["periods"][0]["period"] = json!("2026-02");
    assert!(!validate_original_budget_result("forecast", &monthly, true));
    let weekly = json!({"ok":true,"startDate":"2026-01-01","openingCash":0,"lowestPoint":0,"completeness":{"included":[],"excluded":[]},"periods":[],"errors":[],"appliedRules":["liquidity-forecast-13-week-v3"]});
    assert!(!validate_original_budget_result(
        "forecast-13-week",
        &weekly,
        true
    ));
    let refusal = json!({"ok":false,"openingBalance":0,"closingBalance":0,"periods":[],"errors":["opening unavailable"],"appliedRules":["liquidity-forecast"]});
    assert!(validate_original_budget_result("forecast", &refusal, false));
}
