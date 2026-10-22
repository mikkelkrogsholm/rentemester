use super::*;

fn input(kind: &str, result: &str, amount: Value, periods: u32) -> Register {
    serde_json::from_value(json!({"accrualType":kind,"description":"Synthetic service", "totalAmount":amount,"recognitionPeriods":periods,"firstRecognitionDate":"2026-01-31","resultAccountNo":result})).unwrap()
}
fn count(tx: &Transaction<'_>) -> i64 {
    tx.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r.get(0))
        .unwrap()
}
#[test]
fn original_defaults_nullable_evidence_zero_and_source_date_policy() {
    let mut control = rusqlite::Connection::open_in_memory().unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic originals").unwrap();
    let tx = db.transaction().unwrap();
    tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('1300','Prepaid','asset','debit'),('7300','Accrued','liability','credit'),('7310','Deferred','liability','credit'),('E','Expense','expense','debit'),('I','Income','income','credit'),('B','Bank','asset','debit'); INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by) VALUES('bank','B','confirmed',1,'synthetic');").unwrap();
    let latest = Date::parse("2026-10-19").unwrap();
    let prepaid = register(
        &tx,
        &input("prepaid_expense", "E", json!(125.09), 3),
        "owner",
        PrincipalKind::User,
        latest,
    )
    .unwrap();
    assert_eq!(prepaid["ok"], true);
    assert_eq!(prepaid["periodAmount"], 41.69);
    assert!(prepaid.get("schedule").is_none());
    assert!(prepaid["entryNo"].is_string());
    assert_eq!(prepaid["entryHash"].as_str().unwrap().len(), 64);
    let id = prepaid["accrualId"].as_i64().unwrap();
    assert_eq!(document_id(&tx, id).unwrap(), None);
    assert!(document_id(&tx, 999).is_err());
    assert!(crate::accruals::preview(&tx, id).unwrap()["documentId"].is_null());
    assert_eq!(
        crate::accruals::list(&tx, "synthetic").unwrap()["report"],
        report(&tx).unwrap()
    );
    let request = Recognize {
        period: 1,
        date: None,
        settlement_account_no: None,
    };
    let before = count(&tx);
    assert!(recognize(&tx, id, &request, "owner", PrincipalKind::User, latest).is_err());
    assert_eq!(count(&tx), before); // expense recognition never invents evidence
    let mut deferred = input("deferred_revenue", "I", json!(90), 2);
    // Original permits registration after first schedule date.
    deferred.registration_date = Some("2026-02-01".into());
    let d = register(&tx, &deferred, "svc", PrincipalKind::ServiceAccount, latest).unwrap();
    let created: String = tx
        .query_row(
            "SELECT created_by FROM journal_entries WHERE id=?1",
            [d["entryId"].as_i64().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(created, "service_account:svc");
    assert!(
        recognize(
            &tx,
            d["accrualId"].as_i64().unwrap(),
            &request,
            "svc",
            PrincipalKind::ServiceAccount,
            latest
        )
        .is_err()
    );
    let before = count(&tx);
    assert!(
        register(
            &tx,
            &input("accrued_expense", "E", json!(100), 2),
            "owner",
            PrincipalKind::User,
            latest
        )
        .is_err()
    );
    assert_eq!(count(&tx), before); // expense at registration requires original
    let z = register(
        &tx,
        &input("prepaid_expense", "E", json!(0.01), 2),
        "owner",
        PrincipalKind::User,
        latest,
    )
    .unwrap();
    assert_eq!(z["periodAmount"], 0.0);
    let before = count(&tx);
    assert!(
        recognize(
            &tx,
            z["accrualId"].as_i64().unwrap(),
            &request,
            "owner",
            PrincipalKind::User,
            latest
        )
        .is_err()
    );
    assert_eq!(count(&tx), before);
    let r = report(&tx).unwrap();
    assert_eq!(r["totals"]["totalAmount"], 215.10);
    assert_eq!(r["totals"]["recognizedAmount"], 0.0);
    assert_eq!(r["accruals"][0]["balanceAccountNo"], "1300");
    assert_eq!(r["accruals"][1]["balanceAccountNo"], "7310");
}

#[test]
fn all_three_types_exact_receipts_explicit_early_date_duplicates_and_exposure() {
    let mut control = rusqlite::Connection::open_in_memory().unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic evidenced originals")
        .unwrap();
    let tx = db.transaction().unwrap();
    tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('1300','Prepaid','asset','debit'),('7300','Accrued','liability','credit'),('7310','Deferred','liability','credit'),('E','Expense','expense','debit'),('I','Income','income','credit'),('B','Bank','asset','debit'); INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by) VALUES('bank','B','confirmed',1,'synthetic');").unwrap();
    let meta = serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-01-01","invoiceNo":"SYN-ORIGINAL","deliveryDescription":"Synthetic service","amountIncVat":1000,"vatAmount":0,"sender":{"name":"Synthetic Supplier","address":"Synthetic2","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic Company","address":"Synthetic1","vatOrCvr":"DK87654321"}})).unwrap();
    let doc = crate::documents::register(
        &tx,
        crate::documents::Original {
            filename: "synthetic.txt",
            path: "synthetic.txt",
            hash: &"a".repeat(64),
            mime: "text/plain",
        },
        &meta,
        "synthetic",
        "2026-10-19",
    )
    .unwrap()["document"]["id"]
        .as_i64()
        .unwrap();
    let latest = Date::parse("2026-10-19").unwrap();
    for (kind, result, amount, periods, recognize_count) in [
        ("prepaid_expense", "E", json!(125.09), 3, 3),
        ("accrued_expense", "E", json!(100), 2, 1),
        ("deferred_revenue", "I", json!(90), 2, 1),
    ] {
        let mut i = input(kind, result, amount, periods);
        i.document_id = Some(doc);
        i.note = Some("   test note   ".into());
        let r = register(&tx, &i, "owner", PrincipalKind::User, latest).unwrap();
        let id = r["accrualId"].as_i64().unwrap();
        assert_eq!(r["ok"], true);
        assert!(
            r["appliedRules"]
                .as_array()
                .unwrap()
                .contains(&json!("DK-BOOKKEEPING-ACCRUAL-001"))
        );
        for period in 1..=recognize_count {
            // Explicit date may precede selected schedule date; current/horizon still shared.
            let request = Recognize {
                period,
                date: Some("2026-01-31".into()),
                settlement_account_no: None,
            };
            let r = recognize(
                &tx,
                id,
                &request,
                "svc",
                PrincipalKind::ServiceAccount,
                latest,
            )
            .unwrap();
            assert_eq!(r["recognizedPeriods"], period);
            assert_eq!(r["totalPeriods"], periods);
            if kind == "prepaid_expense" && period == 3 {
                assert_eq!(r["periodAmount"], 41.71);
            }
            let before = count(&tx);
            assert!(
                recognize(
                    &tx,
                    id,
                    &request,
                    "svc",
                    PrincipalKind::ServiceAccount,
                    latest
                )
                .is_err()
            );
            assert_eq!(count(&tx), before);
        }
        let before = count(&tx);
        let outside = Recognize {
            period: periods + 1,
            date: None,
            settlement_account_no: None,
        };
        assert!(recognize(&tx, id, &outside, "owner", PrincipalKind::User, latest).is_err());
        assert_eq!(count(&tx), before);
    }
    assert_eq!(count(&tx), 8);
    let r = report(&tx).unwrap();
    assert_eq!(
        r["totals"],
        json!({"totalAmount":315.09,"recognizedAmount":220.09,"remainingAmount":95.0})
    );
    assert_eq!(r["accruals"][0]["fullyRecognized"], true);
    assert_eq!(r["accruals"][1]["balanceAccountNo"], "7300");
    // With valid original evidence, a zero slice still fails shared positive-line validation.
    let mut tiny = input("prepaid_expense", "E", json!(0.01), 2);
    tiny.document_id = Some(doc);
    let z = register(&tx, &tiny, "owner", PrincipalKind::User, latest).unwrap();
    let before = count(&tx);
    let zero = Recognize {
        period: 1,
        date: None,
        settlement_account_no: None,
    };
    assert!(
        recognize(
            &tx,
            z["accrualId"].as_i64().unwrap(),
            &zero,
            "owner",
            PrincipalKind::User,
            latest
        )
        .is_err()
    );
    assert_eq!(count(&tx), before);
    tiny.balance_account_no = Some("E".into());
    assert!(register(&tx, &tiny, "owner", PrincipalKind::User, latest).is_err());
    assert_eq!(count(&tx), before);
}
