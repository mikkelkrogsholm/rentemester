use super::*;
const TODAY: &str = "2026-10-23";
const NOW: i64 = 1_792_713_600_000;
fn day() -> Date {
    Date::parse(TODAY).unwrap()
}
fn fixture() -> (Connection, i64, i64) {
    let mut c = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut db, "Synthetic disposal").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic equipment','asset','debit'),('D','Synthetic depreciation','expense','debit'),('AD','Synthetic accumulated','asset','credit'),('L','Synthetic loss','expense','debit'),('C','Synthetic payable','liability','credit');").unwrap();
    let meta=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-07-01","invoiceNo":"SYN-DISPOSAL-1","deliveryDescription":"Synthetic equipment","amountIncVat":1000,"vatAmount":0,"sender":{"name":"Synthetic supplier","address":"Synthetic2","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic company","address":"Synthetic1","vatOrCvr":"DK87654321"}})).unwrap();
    let doc = crate::documents::register(
        &tx,
        crate::documents::Original {
            filename: "synthetic.txt",
            path: "documents/synthetic.txt",
            hash: &"a".repeat(64),
            mime: "text/plain",
        },
        &meta,
        "synthetic",
        TODAY,
    )
    .unwrap()["document"]["id"]
        .as_i64()
        .unwrap();
    let acquisition:ManualJournal=serde_json::from_value(json!({"transactionDate":"2026-07-01","text":"Synthetic capitalized purchase","currency":"DKK","documentId":doc.to_string(),"lines":[{"accountNo":"A","debitAmount":"1000.00"},{"accountNo":"C","creditAmount":"1000.00"}]})).unwrap();
    let receipt = crate::journal_post::post(
        &tx,
        &acquisition,
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        day(),
        crate::journal_post::Actor {
            subject: "user:synthetic",
            program: "synthetic",
        },
    )
    .unwrap();
    assert_eq!(receipt["ok"], true, "{receipt}");
    let input=serde_json::from_value(json!({"name":"Synthetic equipment","category":"equipment","acquisitionDate":"2026-07-01","cost":1000,"usefulLifeMonths":3,"purchaseDocumentId":doc,"assetAccountNo":"A","depreciationExpenseAccountNo":"D","accumulatedDepreciationAccountNo":"AD","confirm":true})).unwrap();
    let id = assets::register(&tx, &input, "synthetic", day()).unwrap()["assetId"]
        .as_i64()
        .unwrap();
    tx.commit().unwrap();
    (db, id, doc)
}
fn input(doc: i64) -> Preview {
    Preview {
        transaction_date: TODAY.into(),
        document_id: doc,
        reason: "Synthetic retirement".into(),
        loss_account_no: Some("L".into()),
    }
}
fn request(p: &Preview, hash: &Value, key: &str) -> Dispose {
    Dispose {
        transaction_date: p.transaction_date.clone(),
        document_id: p.document_id,
        reason: p.reason.clone(),
        loss_account_no: p.loss_account_no.clone(),
        expected_plan_hash: hash.as_str().unwrap().into(),
        idempotency_key: key.into(),
        confirm: true,
    }
}
fn depreciate(tx: &Transaction<'_>, id: i64, index: u32) {
    let i = assets::Depreciate {
        transaction_date: Some("2026-10-20".into()),
        period_index: Some(index),
        confirm: true,
    };
    assert_eq!(
        assets::depreciate(tx, id, &i, "synthetic", TODAY, day()).unwrap()["ok"],
        true
    );
}
fn counts(db: &Connection) -> (i64, i64, i64, i64) {
    db.query_row("SELECT (SELECT count(*) FROM journal_entries),(SELECT count(*) FROM native_asset_disposals),(SELECT count(*) FROM mutation_idempotency_tombstones),(SELECT count(*) FROM audit_log)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap()
}
#[test]
fn asset_disposal_zero_partial_full_exact_journal_history_and_replay() {
    for periods in [0, 1, 3] {
        let (mut db, id, doc) = fixture();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        for index in 1..=periods {
            depreciate(&tx, id, index);
        }
        let p = input(doc);
        let before = counts(&tx);
        let planned = preview(&tx, id, &p, day()).unwrap()["disposalPreview"].clone();
        assert_eq!(counts(&tx), before, "preview must be inert");
        assert_eq!(
            planned["lines"].as_array().unwrap().len(),
            if periods == 1 { 3 } else { 2 }
        );
        if periods == 3 {
            assert_eq!(planned["lossAccountNo"], Value::Null);
            assert_eq!(planned["carryingValue"], 0.0);
        }
        let long_key = "😀".repeat(80);
        let i = request(&p, &planned["planHash"], &long_key);
        let accepted = dispose(&tx, id, &i, "synthetic", PrincipalKind::User, NOW, day()).unwrap();
        assert_eq!(accepted["disposal"]["replayed"], false);
        assert_eq!(accepted["disposal"]["remainingCarryingValue"], 0);
        assert!(accepted["disposal"]["journalEntryNo"].is_string());
        let after = counts(&tx);
        assert_eq!(after.0, before.0 + 1);
        assert_eq!(after.1, 1);
        assert_eq!(after.2, 1);
        let entry = accepted["disposal"]["journalEntryId"].as_i64().unwrap();
        let actual = journal_basis(&tx, entry, doc, TODAY).unwrap();
        for (a, p) in actual["lines"]
            .as_array()
            .unwrap()
            .iter()
            .zip(planned["lines"].as_array().unwrap())
        {
            assert_eq!(a["accountNo"], p["accountNo"]);
            assert_eq!(a["debit"], p["debit"]);
            assert_eq!(a["credit"], p["credit"]);
        }
        assert_eq!(
            dispose(&tx, id, &i, "synthetic", PrincipalKind::User, NOW, day()).unwrap()["disposal"]
                ["replayed"],
            true
        );
        assert_eq!(counts(&tx).0, after.0);
        assert_eq!(
            receipt(&tx, id, &long_key, "synthetic", PrincipalKind::User).unwrap()["disposal"]["disposalId"],
            accepted["disposal"]["disposalId"]
        );
        assert!(receipt(&tx, id, &long_key, "other", PrincipalKind::User).is_err());
        assert_eq!(
            receipt(&tx, id, "unknown-key", "synthetic", PrincipalKind::User)
                .unwrap_err()
                .code,
            "not_found"
        );
        let mut changed = request(&p, &planned["planHash"], &long_key);
        changed.reason = "Other intent".into();
        assert!(
            dispose(
                &tx,
                id,
                &changed,
                "synthetic",
                PrincipalKind::User,
                NOW,
                day()
            )
            .is_err()
        );
        changed.idempotency_key = "another-key".into();
        assert!(
            dispose(
                &tx,
                id,
                &changed,
                "synthetic",
                PrincipalKind::User,
                NOW,
                day()
            )
            .is_err()
        );
        assert!(
            assets::depreciate(
                &tx,
                id,
                &assets::Depreciate {
                    transaction_date: Some(TODAY.into()),
                    period_index: Some(2),
                    confirm: true
                },
                "synthetic",
                TODAY,
                day()
            )
            .is_err()
        );
        assert_eq!(
            assets::next(&tx, id).unwrap()["nextPeriodIndex"],
            Value::Null
        );
        for report in [
            assets::list(&tx, "synthetic").unwrap(),
            crate::asset_original::report(&tx).unwrap(),
        ] {
            assert_eq!(report["assets"][0]["status"], "disposed");
            assert_eq!(report["totals"]["netBookValue"], 0.0);
            assert_eq!(report["totals"]["activeCount"], 0);
            assert_eq!(report["totals"]["fullyDepreciatedCount"], 0);
        }
        let reverse=serde_json::from_value(json!({"entryId":entry.to_string(),"transactionDate":TODAY,"reason":"Synthetic forbidden correction"})).unwrap();
        assert_eq!(
            crate::journal_reversal::reverse(
                &tx,
                &reverse,
                &crate::journal_preview::manual_journal_rule_version().unwrap(),
                day(),
                crate::journal_post::Actor {
                    subject: "user:synthetic",
                    program: "synthetic"
                }
            )
            .unwrap()["ok"],
            false
        );
        tx.commit().unwrap();
        validate_backup(&db).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("restored.sqlite");
        db.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
            .unwrap();
        drop(db);
        let mut restored = Connection::open(&path).unwrap();
        restored.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        crate::portable_backup::restore_boundary(&mut restored, &"b".repeat(64)).unwrap();
        validate_backup(&restored).unwrap();
        let tx = restored
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert_eq!(
            assets::list(&tx, "synthetic").unwrap()["assets"][0]["status"],
            "disposed"
        );
        assert!(receipt(&tx, id, &long_key, "fresh-owner", PrincipalKind::User).is_err());
        let inventory = crate::receipt_reconciliation::inventory(&tx).unwrap();
        assert_eq!(inventory["rows"][0]["operation"], "asset_dispose");
        let row = &inventory["rows"][0];
        let mapping = crate::receipt_reconciliation::Reconcile {
            receipt_id: row["receiptId"].as_i64().unwrap(),
            expected_state_hash: row["stateHash"].as_str().unwrap().into(),
            reason: "Synthetic fresh owner review of retained disposal".into(),
            confirm: true,
        };
        crate::receipt_reconciliation::reconcile(
            &tx,
            &mapping,
            "fresh-owner",
            "2026-10-23T00:00:00.000Z",
        )
        .unwrap();
        let recovered = receipt(&tx, id, &long_key, "fresh-owner", PrincipalKind::User).unwrap();
        assert_eq!(
            recovered["disposal"]["journalEntryHash"],
            accepted["disposal"]["journalEntryHash"]
        );
        let prior = counts(&tx).0;
        assert_eq!(
            dispose(&tx, id, &i, "fresh-owner", PrincipalKind::User, NOW, day()).unwrap()["disposal"]
                ["replayed"],
            true
        );
        assert_eq!(counts(&tx).0, prior);
    }
}
#[test]
fn asset_disposal_refuses_stale_dates_closed_mapping_and_tampered_evidence() {
    let (mut db, id, doc) = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let p = input(doc);
    let planned = preview(&tx, id, &p, day()).unwrap();
    depreciate(&tx, id, 1);
    let before = counts(&tx);
    let i = request(&p, &planned["disposalPreview"]["planHash"], "stale-key");
    assert!(dispose(&tx, id, &i, "synthetic", PrincipalKind::User, NOW, day()).is_err());
    assert_eq!(counts(&tx), before);
    for date in ["2026-10-19", "2026-10-24", "2026-06-30", "2026-02-30"] {
        let mut p = input(doc);
        p.transaction_date = date.into();
        assert!(preview(&tx, id, &p, day()).is_err(), "{date}");
    }
    tx.execute_batch("INSERT INTO accounting_periods(period_start,period_end,kind,status) VALUES('2026-10-21','2026-10-23','custom','closed')").unwrap();
    assert!(preview(&tx, id, &p, day()).is_err());
    tx.rollback().unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute("UPDATE accounts SET active=0 WHERE account_no='L'", [])
        .unwrap();
    assert!(preview(&tx, id, &p, day()).is_err());
    tx.rollback().unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    depreciate(&tx, id, 1);
    tx.execute_batch("DROP TRIGGER journal_lines_no_update;")
        .unwrap_or(());
    let triggers = tx
        .prepare("SELECT name FROM sqlite_schema WHERE type='trigger' AND tbl_name='journal_lines'")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for t in triggers {
        tx.execute_batch(&format!("DROP TRIGGER \"{t}\";")).unwrap();
    }
    tx.execute("UPDATE journal_lines SET account_id=(SELECT id FROM accounts WHERE account_no='L') WHERE journal_entry_id=(SELECT journal_entry_id FROM asset_depreciation_entries WHERE asset_id=?1) AND debit_amount>0",[id]).unwrap();
    assert!(preview(&tx, id, &p, day()).is_err());
}
#[test]
fn asset_disposal_atomic_receipt_failure_rolls_back_every_effect() {
    let (mut db, id, doc) = fixture();
    let before = counts(&db);
    {
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let p = input(doc);
        let planned = preview(&tx, id, &p, day()).unwrap();
        tx.execute_batch("CREATE TEMP TRIGGER synthetic_receipt_failure BEFORE INSERT ON main.mutation_idempotency_outcomes BEGIN SELECT RAISE(ABORT,'synthetic receipt failure');END;").unwrap();
        let i = request(&p, &planned["disposalPreview"]["planHash"], "rollback-key");
        assert!(dispose(&tx, id, &i, "synthetic", PrincipalKind::User, NOW, day()).is_err());
        tx.rollback().unwrap();
    }
    assert_eq!(counts(&db), before);
    assert!(record(&db, id).unwrap().is_none());
}
#[test]
fn asset_disposal_key_utf16_and_schema_migration_qualification() {
    for key in [
        "1234567".into(),
        "😀".repeat(81),
        " ".repeat(8),
        "\u{feff}".repeat(8),
    ] {
        assert!(validate_key(&key).is_err());
    }
    assert!(validate_key(&"😀".repeat(80)).is_ok());
    let (mut db, _, _) = fixture();
    db.execute_batch("DROP TRIGGER native_asset_disposals_no_update;DROP TRIGGER native_asset_disposals_no_delete;DROP TABLE native_asset_disposals;DROP TRIGGER native_asset_disposal_schema_no_update;DROP TRIGGER native_asset_disposal_schema_no_delete;DROP TABLE native_asset_disposal_schema;").unwrap();
    assert!(!crate::asset_disposal_schema::installed(&db).unwrap());
    crate::asset_disposal_schema::upgrade(&mut db).unwrap();
    assert!(crate::asset_disposal_schema::installed(&db).unwrap());
    db.execute_batch("DROP TRIGGER native_asset_disposals_no_update")
        .unwrap();
    assert!(crate::asset_disposal_schema::upgrade(&mut db).is_err());
}

#[test]
fn asset_disposal_rejects_valid_hashed_depreciation_with_wrong_actual_lines() {
    let (mut db, id, doc) = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let wrong:ManualJournal=serde_json::from_value(json!({"transactionDate":"2026-10-20","text":"Synthetic wrong depreciation evidence","currency":"DKK","documentId":doc.to_string(),"lines":[{"accountNo":"L","debitAmount":"333.33"},{"accountNo":"AD","creditAmount":"333.33"}]})).unwrap();
    let journal = crate::journal_post::post(
        &tx,
        &wrong,
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        day(),
        crate::journal_post::Actor {
            subject: "user:synthetic",
            program: "synthetic",
        },
    )
    .unwrap();
    assert_eq!(journal["ok"], true);
    tx.execute("INSERT INTO asset_depreciation_entries(asset_id,period_index,transaction_date,amount,journal_entry_id) VALUES(?1,1,'2026-10-20',333.33,?2)",params![id,journal["entryId"].as_i64()]).unwrap();
    assert_eq!(
        assets::posted(&tx, &assets::load(&tx, id).unwrap())
            .unwrap()
            .1,
        Money::parse("333.33").unwrap()
    );
    let error = preview(&tx, id, &input(doc), day()).unwrap_err();
    assert!(
        error.message.contains("accounts/amounts mismatch"),
        "{}",
        error.message
    );
}
