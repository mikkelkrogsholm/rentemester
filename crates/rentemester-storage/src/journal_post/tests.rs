use super::*;
use rusqlite::TransactionBehavior;
#[test]
fn actual_native_header_lines_hash_audit_and_rollback() {
    let mut db = Connection::open_in_memory().unwrap();
    crate::company_baseline::rehearse(&mut db).unwrap();
    db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('2000','Synthetic bank','asset','debit'),('5000','Synthetic equity','equity','credit')").unwrap();
    let request:ManualJournal=serde_json::from_value(json!({"transactionDate":"2026-01-01","text":"Synthetic capital","currency":"DKK","lines":[{"accountNo":"2000","debitAmount":"10.00"},{"accountNo":"5000","creditAmount":"10.00"}]})).unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let version = journal_preview::manual_journal_rule_version().unwrap();
    let value = post(
        &tx,
        &request,
        &version,
        Date::parse("2026-02-15").unwrap(),
        Actor {
            subject: "user:synthetic",
            program: "rentemester-rust",
        },
    )
    .unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["entryNo"], "2026-00001");
    tx.commit().unwrap();
    assert!(journal_hash::verify(&db).unwrap().ok);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    post(
        &tx,
        &request,
        &version,
        Date::parse("2026-02-15").unwrap(),
        Actor {
            subject: "user:synthetic",
            program: "rentemester-rust",
        },
    )
    .unwrap();
    tx.rollback().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    // Recreate canonical DDL around a synthetic corrupt tail; admission must pass
    // the schema gate and then refuse this stored sentinel without any effects.
    let guard: String = db.query_row("SELECT sql FROM sqlite_schema WHERE type='trigger' AND name='journal_entries_no_update'", [], |r| r.get(0)).unwrap();
    db.execute_batch("DROP TRIGGER journal_entries_no_update; UPDATE journal_entries SET entry_hash='GENESIS' WHERE id=1;").unwrap();
    db.execute_batch(&guard).unwrap();
    let before = db.total_changes();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(
        post(
            &tx,
            &request,
            &version,
            Date::parse("2026-02-15").unwrap(),
            Actor {
                subject: "user:synthetic",
                program: "rentemester-rust"
            }
        )
        .is_err()
    );
    tx.rollback().unwrap();
    assert_eq!(db.total_changes(), before);
}

#[test]
#[ignore = "canonical synthetic storage post crash/fault process helper"]
fn child() {
    let config: Value = serde_json::from_str(&std::env::var("RM_POST_REQUEST").unwrap()).unwrap();
    let mut db = Connection::open(std::env::var("RM_POST_COMPANY").unwrap()).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
        .unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let request: ManualJournal =
        serde_json::from_value(config["input"]["request"].clone()).unwrap();
    let result = post(
        &tx,
        &request,
        config["input"]["expectedRuleVersion"].as_str().unwrap(),
        Date::parse("2026-02-15").unwrap(),
        Actor {
            subject: "user:u",
            program: "rentemester-rust",
        },
    );
    let output = match result {
        Ok(value) if value["ok"] == true => {
            tx.commit().unwrap();
            json!({"outcome":"committed","result":value})
        }
        Ok(value) => {
            tx.rollback().unwrap();
            json!({"outcome":"not_committed","business":value})
        }
        Err(error) => {
            tx.rollback().unwrap();
            json!({"outcome":"not_committed","code":error.code})
        }
    };
    std::fs::write(
        std::env::var("RM_POST_OUTPUT").unwrap(),
        serde_json::to_vec(&output).unwrap(),
    )
    .unwrap();
}

#[test]
fn legacy_numeric_roundtrip_and_writer_boundary() {
    for raw in ["0.01", "-0.01", "1.01", "100000.23"] {
        let m = Money::parse(raw).unwrap();
        assert!(money(&m).is_ok());
    }
    assert!(money(&Money::parse("9007199254740992.00").unwrap()).is_err());
    for raw in ["0.000001", "1.005000", "7.450000", "12.123456"] {
        assert!(rate(&FxRate::parse(raw).unwrap()).is_ok());
    }
    let mut db = Connection::open_in_memory().unwrap();
    crate::company_baseline::rehearse(&mut db).unwrap();
    let tx = db.transaction().unwrap();
    assert_eq!(
        require_current(&tx).unwrap_err().code,
        "transaction_required"
    );
    tx.rollback().unwrap();
}
