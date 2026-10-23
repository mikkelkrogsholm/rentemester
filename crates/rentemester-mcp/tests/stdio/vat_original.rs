use super::*;
fn report() -> Value {
    let mut v = json!({"ok":true,"errors":[],"warnings":[],"appliedRules":["DK-VAT-REPORT-001"],"periodStart":"2026-01-01","periodEnd":"2026-03-31","vatPeriodType":"quarter","filingDeadline":"2026-06-01","outputVat":300,"inputVat":50,"netVatPayable":250,"ossConsumerSalesEntryCount":1,"journalEntryCount":2,"reversedJournalEntryCount":1,"reversalJournalEntryCount":1,"totalJournalEntryCount":4,"linesConsidered":6,"reversedLinesConsidered":2,"reversalLinesConsidered":2,"totalLinesConsidered":10,"rubrikker":{"salgsmoms":250,"momsAfVarekobUdland":25,"momsAfYdelseskobUdland":25,"kobsmoms":50,"momstilsvar":250,"rubrikA":200,"rubrikB":200,"rubrikC":90}});
    for (k, n) in [
        ("purchaseBase25", 200),
        ("salesBase25", 1000),
        ("reverseChargeSalesBase", 250),
        ("foreignReverseChargeSalesBase", 200),
        ("domesticReverseChargeSalesBase", 50),
        ("reverseChargePurchaseBase", 100),
        ("euGoodsAcquisitionPurchaseBase", 100),
        ("euGoodsAcquisitionOutputVat", 25),
        ("nonEuServiceReverseChargePurchaseBase", 0),
        ("reverseChargePurchaseOutputVat", 50),
        ("representationPurchaseBase", 40),
        ("badDebtReliefBase25", 0),
        ("exemptSalesBase", 40),
        ("ossConsumerSalesBase", 120),
    ] {
        v[k] = json!(n);
    }
    v
}

#[test]
fn exact_three_readonly_contracts_and_only_two_new_names() {
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
    assert_eq!(names.len(), 339);
    assert!(!names.contains("vat_filing"));
    for name in ["vat_report", "vat_eu_sales_list", "vat_oss_report"] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(
            t["inputSchema"]["required"],
            json!(["company", "from", "to"])
        );
        assert_eq!(t["inputSchema"]["properties"].as_object().unwrap().len(), 3);
        for k in ["company", "from", "to"] {
            assert_eq!(
                t["inputSchema"]["properties"][k],
                json!({"type":"string","minLength":1})
            );
        }
        assert_eq!(
            t["annotations"],
            json!({"readOnlyHint":true,"idempotentHint":true,"destructiveHint":false,"openWorldHint":false})
        );
        for a in [
            json!({"company":"synthetic","from":"","to":"x"}),
            json!({"company":"synthetic","from":"x"}),
            json!({"company":"synthetic","from":"x","to":"y","confirm":true}),
        ] {
            let rows = parsed(&run(
                "http://127.0.0.1:9",
                &[init("2025-11-25"), ready(), call(2, name, a)],
            ));
            assert_eq!(rows[1]["error"]["code"], -32602);
        }
    }
}
#[test]
fn nonempty_report_and_all_false_results_omit_data() {
    for (name, status, v) in [
        ("vat_report", 200, report()),
        (
            "vat_eu_sales_list",
            422,
            json!({"ok":false,"errors":["selskabet er ikke momsregistreret"]}),
        ),
        (
            "vat_oss_report",
            422,
            json!({"ok":false,"errors":["invalid date"],"appliedRules":["DK-VAT-OSS-001"],"ossConsumerSalesBase":0,"entryCount":0,"vatReport":{},"warnings":[]}),
        ),
    ] {
        let text = v.to_string();
        let response = format!(
            "HTTP/1.1 {status} Mock\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{text}",
            text.len()
        );
        let (origin, request, task) = http(Box::leak(response.into_boxed_str()));
        let rows = parsed(&run(
            &origin,
            &[
                init("2025-11-25"),
                ready(),
                call(
                    2,
                    name,
                    json!({"company":"synthetic","from":"2026-01-01","to":"2026-03-31"}),
                ),
            ],
        ));
        let req = request
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        let op = match name {
            "vat_report" => "report",
            "vat_eu_sales_list" => "eu-sales-list",
            _ => "oss-report",
        };
        assert!(
            req.starts_with(format!("GET /api/companies/synthetic/original/vat/{op}?").as_bytes())
        );
        task.join().unwrap();
        let result = &rows[1]["result"];
        let envelope = &result["structuredContent"];
        assert_eq!(
            serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
            *envelope
        );
        assert_eq!(result["isError"], v["ok"] != true);
        assert_eq!(envelope["errors"], v["errors"]);
        if status == 200 {
            assert_eq!(envelope["data"]["outputVat"], 300);
            assert_eq!(envelope["data"]["totalLinesConsidered"], 10);
        } else {
            assert!(envelope.get("data").is_none());
            if name == "vat_eu_sales_list" {
                assert_eq!(envelope["code"], "NOT_VAT_REGISTERED");
            } else {
                assert!(envelope.get("code").is_none());
                assert_eq!(envelope["appliedRules"], v["appliedRules"]);
            }
        }
    }
}
