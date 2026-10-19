use super::*;
use crate::idempotency::{self, Input, Operation, Outcome, PrincipalKind};
use rusqlite::TransactionBehavior;
fn input<'a>(payload: &'a Value, key: &'a str, subject: &'a str) -> Input<'a> {
    Input {
        key,
        operation: Operation::JournalPost,
        principal_kind: PrincipalKind::User,
        subject,
        payload,
        created_by: "user:synthetic",
        created_by_program: "test",
        now_ms: 1_767_225_600_000,
    }
}
fn fixture() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::company_baseline::rehearse(&mut db).unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('2000','Synthetic asset','asset','debit'),('5000','Synthetic equity','equity','credit')").unwrap();
    let request=serde_json::from_value(json!({"transactionDate":"2026-01-01","text":"Synthetic receipt source","currency":"DKK","lines":[{"accountNo":"2000","debitAmount":"10.00"},{"accountNo":"5000","creditAmount":"10.00"}]})).unwrap();
    let version = crate::journal_preview::manual_journal_rule_version().unwrap();
    idempotency::execute(
        &tx,
        input(
            &json!({"synthetic":1}),
            "accepted-lost-response",
            "old-owner",
        ),
        |tx| {
            crate::journal_post::post(
                tx,
                &request,
                &version,
                rentemester_engine::fiscal_year::Date::parse("2026-02-15").unwrap(),
                crate::journal_post::Actor {
                    subject: "user:old-owner",
                    program: "rentemester-rust",
                },
            )
        },
    )
    .unwrap();
    tx.commit().unwrap();
    boundary(&db, "1");
    db
}
fn boundary(db: &Connection, h: &str) {
    db.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('native_restore_receipt_boundary','mutation_idempotency_tombstones','1',?1,'system:portable-restore')",[format!("Imported receipt high-water mark; archive {}; original keys require manual reconciliation under fresh identities",h.repeat(64))]).unwrap();
}
fn reconcile_one(db: &mut Connection, subject: &str) {
    let row = inventory(db).unwrap()["rows"][0].clone();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let result = reconcile(
        &tx,
        &Reconcile {
            receipt_id: 1,
            expected_state_hash: row["stateHash"].as_str().unwrap().into(),
            reason: "Reviewed immutable source accepted result".into(),
            confirm: true,
        },
        subject,
        "2026-01-01T00:10:00.000Z",
    )
    .unwrap();
    assert_eq!(result["rows"][0]["mapping"]["targetUserId"], subject);
    tx.commit().unwrap();
}
#[test]
fn exact_mapping_replays_only_retained_outcome_and_repeated_restore_requires_new_review() {
    let mut db = fixture();
    let payload = json!({"synthetic":1});
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(
            idempotency::execute(
                &tx,
                input(&payload, "accepted-lost-response", "new-owner"),
                |_| panic!("must never execute")
            )
            .is_err()
        );
        tx.rollback().unwrap();
    }
    reconcile_one(&mut db, "new-owner");
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let outcome = idempotency::execute(
            &tx,
            input(&payload, "accepted-lost-response", "new-owner"),
            |_| panic!("must never execute"),
        )
        .unwrap();
        match outcome {
            Outcome::Accepted { result, receipt } => {
                assert_eq!(result["entryId"], 1);
                assert!(receipt.replayed);
                assert_eq!(
                    receipt.reconciliation.unwrap()["sourcePrincipal"]["subject"],
                    "old-owner"
                )
            }
            _ => panic!("replay"),
        };
        tx.commit().unwrap();
    }
    assert_eq!(
        db.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(
            idempotency::execute(
                &tx,
                input(
                    &json!({"synthetic":2}),
                    "accepted-lost-response",
                    "new-owner"
                ),
                |_| panic!("changed payload")
            )
            .is_err()
        );
        tx.rollback().unwrap();
    }
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(
            idempotency::execute(
                &tx,
                input(&payload, "accepted-lost-response", "other-owner"),
                |_| panic!("other owner")
            )
            .is_err()
        );
        tx.rollback().unwrap();
    }
    boundary(&db, "2");
    assert!(inventory(&db).unwrap()["rows"][0]["mapping"].is_null());
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(
            idempotency::execute(
                &tx,
                input(&payload, "accepted-lost-response", "new-owner"),
                |_| panic!("imported old map")
            )
            .is_err()
        );
        tx.rollback().unwrap();
    }
    reconcile_one(&mut db, "third-owner");
    assert_eq!(
        inventory(&db).unwrap()["rows"][0]["mapping"]["targetUserId"],
        "third-owner"
    );
}
#[test]
fn stale_review_pruned_outcome_and_corrupt_journal_or_boundary_refuse() {
    let mut db = fixture();
    let row = inventory(&db).unwrap()["rows"][0].clone();
    reconcile_one(&mut db, "new-owner");
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(
            reconcile(
                &tx,
                &Reconcile {
                    receipt_id: 1,
                    expected_state_hash: row["stateHash"].as_str().unwrap().into(),
                    reason: "Reviewed stale source outcome".into(),
                    confirm: true
                },
                "new-owner",
                "2026-01-01T00:10:00.000Z"
            )
            .is_err()
        );
        tx.rollback().unwrap();
    }
    db.execute("DELETE FROM mutation_idempotency_outcomes", [])
        .unwrap();
    let row = inventory(&db).unwrap()["rows"][0].clone();
    assert_eq!(row["resultAvailable"], false);
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(
            reconcile(
                &tx,
                &Reconcile {
                    receipt_id: 1,
                    expected_state_hash: row["stateHash"].as_str().unwrap().into(),
                    reason: "Cannot adopt pruned source result".into(),
                    confirm: true
                },
                "new-owner",
                "2026-01-01T00:10:00.000Z"
            )
            .is_err()
        );
        tx.rollback().unwrap();
    }
    let db = fixture();
    db.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('native_restore_receipt_boundary','mutation_idempotency_tombstones','1','malformed','user:synthetic')",[]).unwrap();
    assert!(inventory(&db).is_err());
    let db = fixture();
    db.execute_batch("DROP TRIGGER IF EXISTS journal_entries_no_update")
        .unwrap();
    // A valid source outcome must resolve to the exact current immutable journal identity.
    db.execute(
        "UPDATE journal_entries SET entry_no='CORRUPT' WHERE id=1",
        [],
    )
    .unwrap();
    assert!(inventory(&db).is_err());
}

#[test]
fn reversal_receipt_maps_exact_child_and_rollback_does_not_publish_mapping() {
    let mut db = fixture();
    let reverse: crate::journal_reversal::Input = serde_json::from_value(
        json!({"entryId":"1","transactionDate":"2026-01-02","reason":"Synthetic source reversal"}),
    )
    .unwrap();
    let payload = json!({"reversal":1});
    let version = crate::journal_preview::manual_journal_rule_version().unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let mut intent = input(&payload, "accepted-reversal-lost-response", "old-owner");
    intent.operation = Operation::JournalReverse;
    let result = idempotency::execute(&tx, intent, |tx| {
        crate::journal_reversal::reverse(
            tx,
            &reverse,
            &version,
            rentemester_engine::fiscal_year::Date::parse("2026-02-15").unwrap(),
            crate::journal_post::Actor {
                subject: "user:old-owner",
                program: "rentemester-rust",
            },
        )
    })
    .unwrap();
    assert!(matches!(result, Outcome::Accepted { .. }));
    tx.commit().unwrap();
    db.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('native_restore_receipt_boundary','mutation_idempotency_tombstones','2',?1,'system:portable-restore')",[format!("Imported receipt high-water mark; archive {}; original keys require manual reconciliation under fresh identities","3".repeat(64))]).unwrap();
    let row = inventory(&db).unwrap()["rows"][1].clone();
    assert_eq!(row["result"]["originalEntryId"], 1);
    let reviewed = Reconcile {
        receipt_id: 2,
        expected_state_hash: row["stateHash"].as_str().unwrap().into(),
        reason: "Reviewed exact reversal child evidence".into(),
        confirm: true,
    };
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    reconcile(&tx, &reviewed, "new-owner", "2026-01-01T00:10:00.000Z").unwrap();
    tx.rollback().unwrap();
    assert!(inventory(&db).unwrap()["rows"][1]["mapping"].is_null());
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    reconcile(&tx, &reviewed, "new-owner", "2026-01-01T00:10:00.000Z").unwrap();
    tx.commit().unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let mut intent = input(&payload, "accepted-reversal-lost-response", "new-owner");
    intent.operation = Operation::JournalReverse;
    match idempotency::execute(&tx, intent, |_| panic!("never reverse twice")).unwrap() {
        Outcome::Accepted { result, receipt } => {
            assert_eq!(result["entryId"], 2);
            assert!(receipt.replayed)
        }
        _ => panic!("expected retained child"),
    };
    tx.commit().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
