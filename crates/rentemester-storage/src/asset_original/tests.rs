use super::*;
fn setup() -> rusqlite::Connection {
    let mut control = rusqlite::Connection::open_in_memory().unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic source assets").unwrap();
    db
}
fn writeoff(document: i64, cost: f64) -> WriteOff {
    serde_json::from_value(json!({"name":"Synthetic equipment","category":"equipment","acquisitionDate":"2026-01-01","cost":cost,"documentId":document,"expenseAccount":"5820","date":"2026-10-19","thresholdRuleSource":"Synthetic advisor instruction","confirmImmediateWriteOff":true})).unwrap()
}
#[test]
fn thresholds_clamp_known_edges_and_exact_source_slices_include_zero() {
    for (year, expected) in [
        (1900, 33100),
        (2024, 33100),
        (2025, 34400),
        (2026, 36000),
        (2099, 36000),
    ] {
        assert_eq!(threshold(year), expected);
    }
    let cost = Money::parse("0.01").unwrap();
    assert_eq!(
        rentemester_engine::asset::source_amount(&cost, 2, 1).unwrap(),
        Money::default()
    );
    assert_eq!(
        rentemester_engine::asset::source_amount(&cost, 2, 2).unwrap(),
        cost
    );
    assert!(rentemester_engine::asset::amount(&cost, 2, 1).is_err());
    for (months, period) in [(0, 1), (12001, 1), (2, 0), (2, 3)] {
        assert!(rentemester_engine::asset::source_amount(&cost, months, period).is_err());
    }
}
#[test]
fn missing_document_durable_exception_dedup_and_manual_resolution() {
    let mut db = setup();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let latest = Date::parse("2026-10-19").unwrap();
    let mut input = writeoff(987654, 250.0);
    input.confirm_immediate_write_off = false;
    let refused = write_off(
        &tx,
        &input,
        "synthetic",
        PrincipalKind::ServiceAccount,
        latest,
    )
    .unwrap();
    assert_eq!(refused["ok"], false);
    assert_eq!(
        tx.query_row("SELECT count(*) FROM exceptions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    input.confirm_immediate_write_off = true;
    // Metadata refusals precede financial posting-date checks in the source.
    for date in ["2025-12-31", "2027-01-01"] {
        input.date = date.into();
        let result = write_off(
            &tx,
            &input,
            "synthetic",
            PrincipalKind::ServiceAccount,
            latest,
        )
        .unwrap();
        assert_eq!(result.as_object().unwrap().len(), 3);
        assert_eq!(result["ok"], false);
    }
    let (count, document, evidence): (i64, Option<i64>, String) = tx
        .query_row(
            "SELECT count(*),related_document_id,source_evidence FROM exceptions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(document, None);
    assert_eq!(
        serde_json::from_str::<Value>(&evidence).unwrap(),
        json!({"name":"Synthetic equipment","cost":250.0,"purchaseDocumentId":987654})
    );
    tx.execute("UPDATE exceptions SET status='resolved',resolved_by='user:synthetic',resolved_at=CURRENT_TIMESTAMP,resolution_note='Synthetic manual review'",[]).unwrap();
    write_off(&tx, &input, "synthetic", PrincipalKind::User, latest).unwrap();
    assert_eq!(
        tx.query_row("SELECT count(*) FROM exceptions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        tx.query_row("SELECT count(*) FROM asset_writeoffs", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    tx.commit().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM exceptions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn invalid_inputs_never_queue_exception_and_unexpected_errors_remain_errors() {
    let mut db = setup();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let latest = Date::parse("2026-10-19").unwrap();
    for (id, cost, date) in [
        (0, 250.0, "2026-10-19"),
        (9_007_199_254_740_992, 250.0, "2026-10-19"),
        (99, 0.0, "2026-10-19"),
        (99, 250.0, "2026-02-30"),
    ] {
        let mut i = writeoff(id, cost);
        i.date = date.into();
        assert!(write_off(&tx, &i, "synthetic", PrincipalKind::User, latest).is_err());
    }
    assert!(write_off(&tx, &writeoff(99, 250.0), "", PrincipalKind::User, latest).is_err());
    assert_eq!(
        tx.query_row("SELECT count(*) FROM exceptions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    tx.execute_batch("DROP TABLE exceptions").unwrap();
    assert!(
        write_off(
            &tx,
            &writeoff(99, 250.0),
            "synthetic",
            PrincipalKind::User,
            latest
        )
        .is_err()
    );
}

#[test]
fn source_register_selected_periods_receipts_report_and_writeoff() {
    let mut control = rusqlite::Connection::open_in_memory().unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Assets").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic equipment','asset','debit'),('D','Synthetic depreciation','expense','debit'),('AD','Synthetic accumulated depreciation','asset','credit'),('C','Synthetic payable','liability','credit');").unwrap();
    let meta=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-07-01","invoiceNo":"SYN-ASSET-1","deliveryDescription":"Synthetic equipment","amountIncVat":1000,"vatAmount":0,"sender":{"name":"Synthetic Supplier","address":"Synthetic 2","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic Company","address":"Synthetic 1","vatOrCvr":"DK87654321"}})).unwrap();
    let document = crate::documents::register(
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
    let register_input: Register=serde_json::from_value(json!({"name":"Synthetic equipment","category":"equipment","acquisitionDate":"2026-07-01","cost":1000,"usefulLifeMonths":3,"documentId":document,"assetAccount":"A","depreciationAccount":"D","accumulatedAccount":"AD"})).unwrap();
    let today = Date::parse("2026-10-19").unwrap();
    assert!(
        register(
            &tx,
            &register_input,
            "synthetic",
            PrincipalKind::User,
            today
        )
        .is_err()
    );
    let acquisition:ManualJournal=serde_json::from_value(json!({"transactionDate":"2026-07-01","text":"Synthetic equipment purchase","currency":"DKK","documentId":document.to_string(),"lines":[{"accountNo":"A","debitAmount":"1000.00"},{"accountNo":"C","creditAmount":"1000.00"}]})).unwrap();
    let posted = crate::journal_post::post(
        &tx,
        &acquisition,
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        today,
        crate::journal_post::Actor {
            subject: "user:synthetic",
            program: "rentemester-rust",
        },
    )
    .unwrap();
    assert_eq!(posted["ok"], true, "{posted}");
    let mut wrong: Register = serde_json::from_value(json!({"name":"Synthetic wrong account","category":"equipment","acquisitionDate":"2026-07-01","cost":1000,"usefulLifeMonths":3,"documentId":document,"assetAccount":"missing","depreciationAccount":"D","accumulatedAccount":"AD"})).unwrap();
    assert!(register(&tx, &wrong, "synthetic", PrincipalKind::User, today).is_err());
    wrong.asset_account = Some("A".into());
    wrong.note = Some(" ".repeat(4097));
    assert!(register(&tx, &wrong, "synthetic", PrincipalKind::User, today).is_err());
    let registered = register(
        &tx,
        &register_input,
        "synthetic",
        PrincipalKind::User,
        today,
    )
    .unwrap();
    let id = registered["assetId"].as_i64().unwrap();
    assert!(
        register(
            &tx,
            &register_input,
            "synthetic",
            PrincipalKind::User,
            today
        )
        .is_err()
    );
    assert_eq!(registered["periodAmount"], 333.33);
    let mut request: Depreciate =
        serde_json::from_value(json!({"period":1,"date":"2026-09-30"})).unwrap();
    tx.execute_batch("INSERT INTO accounting_periods(period_start,period_end,kind,status) VALUES('2026-07-01','2026-09-30','vat_period','closed');").unwrap();
    assert!(
        depreciate(
            &tx,
            id,
            &request,
            "synthetic",
            PrincipalKind::ServiceAccount,
            today
        )
        .is_err()
    );
    assert_eq!(assets::next(&tx, id).unwrap()["postedPeriods"], 0);
    request.date = "2026-10-19".into();
    let mut entries = vec![];
    for index in 1..=3 {
        request.period = index;
        let result = depreciate(
            &tx,
            id,
            &request,
            "synthetic",
            PrincipalKind::ServiceAccount,
            today,
        )
        .unwrap();
        assert!(result["entryNo"].is_string());
        assert_eq!(result["entryHash"].as_str().unwrap().len(), 64);
        assert_eq!(result["appliedRules"][0], DEPR);
        entries.push(result["entryId"].as_i64().unwrap());
        assert_eq!(
            result["periodAmount"],
            if index == 3 { 333.34 } else { 333.33 }
        );
        assert!(
            depreciate(
                &tx,
                id,
                &request,
                "synthetic",
                PrincipalKind::ServiceAccount,
                today
            )
            .is_err()
        );
    }
    let listed = report(&tx).unwrap();
    assert_eq!(listed["totals"]["netBookValue"], 0.0);
    assert_eq!(listed["totals"]["accumulatedDepreciation"], 1000.0);
    assert_eq!(listed["assets"][0].as_object().unwrap().len(), 9);
    assert_eq!(
        assets::next(&tx, id).unwrap()["nextPeriodIndex"],
        Value::Null
    );
    let mut wo = writeoff(document, 36000.01);
    let refused = write_off(&tx, &wo, "synthetic", PrincipalKind::ServiceAccount, today).unwrap();
    assert_eq!(refused["ok"], false);
    write_off(&tx, &wo, "synthetic", PrincipalKind::ServiceAccount, today).unwrap();
    assert_eq!(
        tx.query_row(
            "SELECT count(*) FROM exceptions WHERE type='ASSET_WRITEOFF_ELIGIBILITY_UNCERTAIN'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    let evidence: String=tx.query_row("SELECT source_evidence FROM exceptions WHERE type='ASSET_WRITEOFF_ELIGIBILITY_UNCERTAIN'",[],|r|r.get(0)).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&evidence).unwrap()["thresholdRuleSource"],
        "Synthetic advisor instruction"
    );
    wo.cost = Number::from(250);
    wo.expense_account = "D".into();
    wo.payment_account = Some("A".into());
    // A valid source exception may use future metadata, but an actual journal
    // remains subject to the existing admitted posting horizon.
    wo.date = "2027-01-01".into();
    let journals_before: i64 = tx
        .query_row("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
        .unwrap();
    assert!(write_off(&tx, &wo, "synthetic", PrincipalKind::ServiceAccount, today).is_err());
    assert_eq!(
        tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        journals_before
    );
    wo.date = "2026-10-19".into();
    let written = write_off(&tx, &wo, "synthetic", PrincipalKind::ServiceAccount, today).unwrap();
    assert_eq!(written["ok"], true);
    assert_eq!(written["thresholdDkk"], 36000);
    assert!(written["entryNo"].is_string());
    assert!(write_off(&tx, &wo, "synthetic", PrincipalKind::User, today).is_err());
    assert_eq!(report(&tx).unwrap(), listed);
    let snapshot = crate::cockpit::read(&tx, "synthetic", "2026-10-19", None, None).unwrap();
    assert_eq!(snapshot.trial_balance["balanced"], true);
    assert_eq!(snapshot.income_statement["totalExpense"], 1250.0);
    entries.push(posted["entryId"].as_i64().unwrap());
    for entry in entries {
        let reverse=serde_json::from_value(json!({"entryId":entry.to_string(),"transactionDate":"2026-10-19","reason":"Synthetic refused correction"})).unwrap();
        assert_eq!(
            crate::journal_reversal::reverse(
                &tx,
                &reverse,
                &crate::journal_preview::manual_journal_rule_version().unwrap(),
                today,
                crate::journal_post::Actor {
                    subject: "user:synthetic",
                    program: "synthetic"
                }
            )
            .unwrap()["ok"],
            false
        );
    }
}

#[test]
fn source_zero_slice_registration_report_and_posting_refusal() {
    let mut db = setup();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('5800','Synthetic equipment','asset','debit'),('5820','Synthetic depreciation','expense','debit'),('5810','Synthetic accumulated','asset','credit'),('C','Synthetic payable','liability','credit');").unwrap();
    let meta=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-07-01","invoiceNo":"SYN-ZERO-1","deliveryDescription":"Synthetic equipment","amountIncVat":0.01,"vatAmount":0,"sender":{"name":"Synthetic Supplier","address":"Synthetic 2","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic Company","address":"Synthetic 1","vatOrCvr":"DK87654321"}})).unwrap();
    let document = crate::documents::register(
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
    let acquisition:ManualJournal=serde_json::from_value(json!({"transactionDate":"2026-07-01","text":"Synthetic equipment purchase","currency":"DKK","documentId":document.to_string(),"lines":[{"accountNo":"5800","debitAmount":"0.01"},{"accountNo":"C","creditAmount":"0.01"}]})).unwrap();
    let latest = Date::parse("2026-10-19").unwrap();
    assert_eq!(
        crate::journal_post::post(
            &tx,
            &acquisition,
            &crate::journal_preview::manual_journal_rule_version().unwrap(),
            latest,
            crate::journal_post::Actor {
                subject: "user:synthetic",
                program: "synthetic"
            }
        )
        .unwrap()["ok"],
        true
    );
    let input:Register=serde_json::from_value(json!({"name":"Synthetic penny","category":"equipment","acquisitionDate":"2026-07-01","cost":0.01,"usefulLifeMonths":2,"documentId":document,"note":"   "})).unwrap();
    let registered = register(
        &tx,
        &input,
        "synthetic",
        PrincipalKind::ServiceAccount,
        latest,
    )
    .unwrap();
    assert_eq!(registered["periodAmount"], 0.0);
    let id = registered["assetId"].as_i64().unwrap();
    assert_eq!(
        tx.query_row("SELECT note FROM assets WHERE id=?1", [id], |r| r
            .get::<_, Option<String>>(0))
            .unwrap(),
        None
    );
    assert_eq!(assets::next(&tx, id).unwrap()["nextPeriodAmount"], 0.0);
    assert_eq!(
        assets::list(&tx, "synthetic").unwrap()["assets"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let first = Depreciate {
        period: 1,
        date: "2026-10-19".into(),
    };
    assert!(
        depreciate(
            &tx,
            id,
            &first,
            "synthetic",
            PrincipalKind::ServiceAccount,
            latest
        )
        .is_err()
    );
    assert_eq!(
        tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    let final_slice = Depreciate {
        period: 2,
        date: "2026-10-19".into(),
    };
    assert_eq!(
        depreciate(
            &tx,
            id,
            &final_slice,
            "synthetic",
            PrincipalKind::ServiceAccount,
            latest
        )
        .unwrap()["periodAmount"],
        0.01
    );
    let report = report(&tx).unwrap();
    assert_eq!(report["totals"]["cost"], 0.01);
    assert_eq!(report["totals"]["netBookValue"], 0.0);
    assert_eq!(report["assets"][0]["postedPeriods"], 1);
}
