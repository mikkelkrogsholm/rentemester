use super::*;
use serde_json::json;
fn fixture() -> (Manifest, Profile) {
    let manifest:Manifest=serde_json::from_value(json!({"version":1,"groups":[{"id":"synthetic","name":"Synthetic","memberships":[{"id":"a","companySlug":"a","validFrom":"2020-01-01"},{"id":"b","companySlug":"b","validFrom":"2020-01-01"}],"ownership":[{"id":"owns-b","parentCompanySlug":"a","childCompanySlug":"b","basisPoints":10000,"evidenceRefs":["synthetic"],"validFrom":"2020-01-01"}]}]})).unwrap();
    let profile=serde_json::from_value::<Profile>(json!({"version":1,"id":"synthetic-profile","groupId":"synthetic","currency":"dkk","reportingLines":[{"id":"assets","label":"Assets","section":"asset","displayOrder":1},{"id":"debt","label":"Debt","section":"liability","displayOrder":2},{"id":"equity","label":"Equity","section":"equity","displayOrder":3},{"id":"result","label":"Result","section":"equity","displayOrder":4,"role":"current-result"},{"id":"sales","label":"Sales","section":"income","displayOrder":5},{"id":"costs","label":"Costs","section":"expense","displayOrder":6}],"accountMappings":[{"id":"a-bank","companySlug":"a","accountNo":"1000","reportingLineId":"assets","validFrom":"2020-01-01"},{"id":"a-sales","companySlug":"a","accountNo":"3000","reportingLineId":"sales","validFrom":"2020-01-01"},{"id":"b-bank","companySlug":"b","accountNo":"1000","reportingLineId":"assets","validFrom":"2020-01-01"},{"id":"b-sales","companySlug":"b","accountNo":"3000","reportingLineId":"sales","validFrom":"2020-01-01"}],"evidenceRefs":["synthetic"],"validFrom":"2020-01-01"})).unwrap().normalize(&manifest).unwrap();
    (manifest, profile)
}
fn ledger(slug: &str) -> Ledger {
    let v = |no: &str, kind: &str, amount: &str| AccountValue {
        account_no: no.into(),
        account_type: kind.into(),
        amount: Money::parse_canonical(amount).unwrap(),
        source_hash: format!("synthetic-{slug}-{no}"),
    };
    Ledger {
        company_slug: slug.into(),
        currency: "DKK".into(),
        ledger_head_hash: Some(format!("synthetic-{slug}")),
        entry_count: 1,
        accounts: vec![
            Account {
                account_no: "1000".into(),
                account_type: "asset".into(),
                active: true,
            },
            Account {
                account_no: "3000".into(),
                account_type: "income".into(),
                active: true,
            },
        ],
        pnl: vec![v("3000", "income", "-12.34")],
        cumulative_result: vec![v("3000", "income", "-12.34")],
        balance: vec![v("1000", "asset", "12.34")],
    }
}
#[test]
fn explicit_chart_canonical_order_intervals_and_unique_account_mapping() {
    let (m, p) = fixture();
    let c = p.canonical().unwrap();
    assert_eq!(c, include_str!("profile-source-canonical.json").trim_end());
    assert!(c.starts_with("{\"version\":1,\"id\":\"synthetic-profile\",\"groupId\":\"synthetic\",\"currency\":\"DKK\",\"reportingLines\":"));
    assert!(c.ends_with("\"evidenceRefs\":[\"synthetic\"],\"validFrom\":\"2020-01-01\"}"));
    let mut bad = p.clone();
    let mut duplicate = bad.account_mappings[0].clone();
    duplicate.id = "other".into();
    bad.account_mappings.push(duplicate);
    assert!(bad.normalize(&m).is_err());
    let mut bad = p;
    bad.account_mappings[0].interval.valid_from = "2019-12-31".into();
    assert!(bad.normalize(&m).is_err());
}
#[test]
fn nonempty_two_company_exact_ready_without_invented_eliminations() {
    let (_, p) = fixture();
    let r = calculate(
        &p,
        "hash",
        "2026-01-01",
        "2026-12-31",
        &[ledger("a"), ledger("b")],
        &[],
        &[],
        vec![],
        |s| s.into(),
    )
    .unwrap();
    assert_eq!(r["status"], "ready");
    let f = r["consolidatedFigures"].as_array().unwrap();
    for id in ["assets", "result", "sales"] {
        assert_eq!(
            f.iter().find(|v| v["lineId"] == id).unwrap()["consolidatedAmount"],
            "24.68"
        );
    }
    assert_eq!(r["sourceSnapshots"].as_array().unwrap().len(), 2);
    assert_eq!(r["appliedEliminations"], json!([]));
}
#[test]
fn unmapped_cumulative_result_and_unbalanced_sources_block_figures() {
    let (_, p) = fixture();
    let mut l = ledger("a");
    l.pnl.clear();
    l.cumulative_result[0].account_no = "9999".into();
    let r = calculate(
        &p,
        "hash",
        "2026-01-01",
        "2026-12-31",
        &[l, ledger("b")],
        &[],
        &[],
        vec![],
        |s| s.into(),
    )
    .unwrap();
    assert_eq!(r["status"], "blocked");
    assert!(r["consolidatedFigures"].is_null());
    assert!(r["blockers"].to_string().contains("cumulative result"));
    let mut l = ledger("a");
    l.balance[0].amount = Money::parse_canonical("12.35").unwrap();
    let r = calculate(
        &p,
        "hash",
        "2026-01-01",
        "2026-12-31",
        &[l, ledger("b")],
        &[],
        &[],
        vec![],
        |s| s.into(),
    )
    .unwrap();
    assert!(
        r["blockers"]
            .to_string()
            .contains("assets equals liabilities")
    );
}
#[test]
fn actual_v2_both_orientations_adjust_the_correct_sections_and_stale_sources_refuse() {
    let (_, mut p) = fixture();
    let interval = p.interval.clone();
    for (id, company, no, line) in [
        ("a-equity", "a", "8000", "equity"),
        ("b-equity", "b", "8000", "equity"),
        ("b-payable", "b", "2000", "debt"),
    ] {
        p.account_mappings.push(Mapping {
            id: id.into(),
            company_slug: company.into(),
            account_no: no.into(),
            reporting_line_id: line.into(),
            interval: interval.clone(),
        });
    }
    let value = |no: &str, kind: &str, n: &str| AccountValue {
        account_no: no.into(),
        account_type: kind.into(),
        amount: Money::parse_canonical(n).unwrap(),
        source_hash: format!("source-{no}"),
    };
    let mut a = ledger("a");
    a.pnl.clear();
    a.cumulative_result.clear();
    a.accounts.push(Account {
        account_no: "8000".into(),
        account_type: "equity".into(),
        active: true,
    });
    a.balance = vec![
        value("1000", "asset", "12.34"),
        value("8000", "equity", "-12.34"),
    ];
    let mut b = ledger("b");
    b.pnl.clear();
    b.cumulative_result.clear();
    b.accounts.extend([
        Account {
            account_no: "8000".into(),
            account_type: "equity".into(),
            active: true,
        },
        Account {
            account_no: "2000".into(),
            account_type: "liability".into(),
            active: true,
        },
    ]);
    b.balance = vec![
        value("2000", "liability", "-12.34"),
        value("8000", "equity", "12.34"),
    ];
    let side = |company: &str, no: &str, position: &str| json!({"companySlug":company,"accountNos":[no],"selectionHash":"a".repeat(64),"ledgerHeadHash":"b".repeat(64),"entryCount":1,"creditOre":if position=="receivable"{"1234"}else{"0"},"debitOre":if position=="payable"{"1234"}else{"0"},"position":position});
    for reverse in [false, true] {
        let (left, right) = if reverse {
            (
                side("b", "2000", "payable"),
                side("a", "1000", "receivable"),
            )
        } else {
            (
                side("a", "1000", "receivable"),
                side("b", "2000", "payable"),
            )
        };
        let payload:crate::eliminations::BalancePayload=serde_json::from_value(json!({"version":2,"id":"synthetic-elimination","kind":"intercompany-balance","groupId":"synthetic","mappingId":"synthetic-map","mappingHash":"c".repeat(64),"asOf":"2026-12-31","currency":"DKK","amountOre":"1234","left":left,"right":right,"evidenceRefs":["synthetic-source"]})).unwrap();
        let fresh_side = |s: &Value| json!({"companySlug":s["companySlug"],"position":s["position"],"balance":"12.34","currency":"DKK","sourceSnapshot":{"selectionHash":s["selectionHash"],"ledgerHeadHash":s["ledgerHeadHash"],"entryCount":1}});
        let fresh = json!({"mappingId":"synthetic-map","mappingHash":"c".repeat(64),"status":"matched","left":fresh_side(&left),"right":fresh_side(&right)});
        if reverse {
            let mut legacy = serde_json::to_value(&payload).unwrap();
            legacy["version"] = json!(1);
            legacy["left"].as_object_mut().unwrap().remove("debitOre");
            legacy["left"].as_object_mut().unwrap().remove("position");
            legacy["left"]["creditOre"] = json!("1234");
            legacy["right"].as_object_mut().unwrap().remove("creditOre");
            legacy["right"].as_object_mut().unwrap().remove("position");
            legacy["right"]["debitOre"] = json!("1234");
            let historical = Applied {
                elimination_id: "historical".into(),
                payload_hash: "f".repeat(64),
                payload: serde_json::from_value(legacy).unwrap(),
            };
            let blocked = calculate(
                &p,
                "hash",
                "2026-01-01",
                "2026-12-31",
                &[a.clone(), b.clone()],
                &[historical],
                std::slice::from_ref(&fresh),
                vec![],
                |s| s.into(),
            )
            .unwrap();
            assert_eq!(blocked["status"], "blocked");
            assert!(blocked["blockers"].to_string().contains("reverse/replan"));
        }
        let e = Applied {
            elimination_id: "synthetic-elimination".into(),
            payload_hash: "d".repeat(64),
            payload,
        };
        let r = calculate(
            &p,
            "hash",
            "2026-01-01",
            "2026-12-31",
            &[a.clone(), b.clone()],
            std::slice::from_ref(&e),
            std::slice::from_ref(&fresh),
            vec![],
            |s| s.into(),
        )
        .unwrap();
        assert_eq!(r["status"], "ready");
        for id in ["assets", "debt"] {
            let l = r["consolidatedFigures"]
                .as_array()
                .unwrap()
                .iter()
                .find(|l| l["lineId"] == id)
                .unwrap();
            assert_eq!(l["eliminationAdjustment"], "-12.34");
            assert_eq!(l["consolidatedAmount"], "0.00");
        }
        let mut stale = fresh.clone();
        stale["left"]["sourceSnapshot"]["ledgerHeadHash"] = json!("e".repeat(64));
        let r = calculate(
            &p,
            "hash",
            "2026-01-01",
            "2026-12-31",
            &[a.clone(), b.clone()],
            &[e],
            &[stale],
            vec![],
            |s| s.into(),
        )
        .unwrap();
        assert_eq!(r["status"], "blocked");
        assert!(r["consolidatedFigures"].is_null());
    }
}
