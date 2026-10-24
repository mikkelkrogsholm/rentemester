use super::*;
use rusqlite::{Connection, TransactionBehavior};
fn fixture() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    let mut l = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic CSV owner").unwrap();
    l.execute_batch("UPDATE companies SET fiscal_year_start_month=7,fiscal_year_label_strategy='span' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('A','Old asset','asset','debit'),('E','Old equity','equity','credit');").unwrap();
    l
}
fn source() -> Input {
    Input { source_system: "synthetic-csv".into(), content: "\u{feff}# cutOverDate: 2024-07-01\n# note: Synthetic review\nsection,accountNo,name,debit,credit\naccount,A,Source asset,,\naccount,E,Source equity,,\nopening,A,,150000,\nopening,E,,,150000\n".into() }
}
fn latest() -> Date {
    Date::parse("2026-10-21").unwrap()
}
fn command(tx: &Transaction<'_>, source: Input) -> Apply {
    let p = prepare(tx, &source, &[], latest()).unwrap();
    assert_eq!(p["plan"]["status"], "ready");
    Apply {
        input: source,
        plan_hash: p["plan"]["planHash"].as_str().unwrap().into(),
        idempotency_key: "synthetic-source-key".into(),
        confirm: true,
    }
}
fn post(tx: &Transaction<'_>, input: &Apply) -> Result<Value, Error> {
    apply(
        tx,
        input,
        &[],
        latest(),
        Actor {
            subject: "user:synthetic",
            program: opening_balance::PROGRAM,
        },
        1792573200000,
    )
}
#[test]
fn exact_csv_readonly_review_atomic_reconciliation_common_opening_and_reopen() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let before = tx.total_changes();
    let cmd = command(&tx, source());
    assert_eq!(before, tx.total_changes());
    let result = post(&tx, &cmd).unwrap();
    assert_eq!(result["ok"], true);
    crate::receipt_reconciliation::verify_result(&tx, "opening_balance_post", &result)
        .expect("source opening uses the exact shared audit/restore receipt contract");
    assert_eq!(result["openingBalance"]["journalEntryNo"], "2024-25-00001");
    assert_eq!(
        result["openingBalance"]["sourceBasis"]["kind"],
        "synthetic_csv"
    );
    assert_eq!(
        tx.query_row("SELECT name FROM accounts WHERE account_no='A'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "Source asset"
    );
    let body: String = tx
        .query_row(
            "SELECT message FROM audit_log WHERE event_type='native_opening_balance'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let proof: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        proof["binding"]["syntheticSource"]["content"],
        source().content
    );
    assert!(opening_balance::validate_history(&tx).unwrap().is_some());
    assert!(crate::journal_hash::verify(&tx).unwrap().ok);
    assert!(post(&tx, &cmd).is_err());
    tx.commit().unwrap();
    assert!(opening_balance::read(&db).unwrap()["openingBalance"].is_object());
    // A changed retained interpretation cannot silently become operator-supplied opening.
    let changed = body.replace("150000", "150001");
    let trigger: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='trigger' AND name='audit_log_no_update'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    db.execute_batch("DROP TRIGGER audit_log_no_update")
        .unwrap();
    db.execute(
        "UPDATE audit_log SET message=? WHERE event_type='native_opening_balance'",
        [&changed],
    )
    .unwrap();
    db.execute_batch(&trigger).unwrap();
    assert!(opening_balance::read(&db).is_err());
}
#[test]
fn source_change_stale_chart_unclassified_missing_and_unbalanced_inputs_refuse() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let mut cmd = command(&tx, source());
    cmd.input.content = cmd.input.content.replace("150000", "150100");
    assert!(post(&tx, &cmd).is_err());
    let cmd = command(&tx, source());
    tx.execute(
        "UPDATE accounts SET name='Changed after review' WHERE account_no='A'",
        [],
    )
    .unwrap();
    assert!(post(&tx, &cmd).is_err());
    let missing = Input {
        content: source()
            .content
            .replace("account,A,Source asset", "account,NEW,Source asset"),
        ..source()
    };
    assert!(prepare(&tx, &missing, &[], latest()).is_err());
    let bad = Input {
        content: source()
            .content
            .replace("opening,E,,,150000", "opening,E,,,149999"),
        ..source()
    };
    assert_eq!(
        prepare(&tx, &bad, &[], latest()).unwrap()["plan"]["status"],
        "blocked"
    );
    assert_eq!(
        tx.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        tx.query_row("SELECT COUNT(*) FROM opening_balances", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn failed_posting_rolls_back_source_chart_and_all_opening_effects() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let cmd = command(&tx, source());
    tx.execute_batch("CREATE TEMP TRIGGER refuse_csv_marker BEFORE INSERT ON opening_balances BEGIN SELECT RAISE(ABORT,'synthetic injected failure'); END;").unwrap();
    assert!(post(&tx, &cmd).is_err());
    tx.rollback().unwrap();
    assert_eq!(
        db.query_row("SELECT name FROM accounts WHERE account_no='A'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "Old asset"
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT COUNT(*) FROM audit_log WHERE event_type='native_opening_balance'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn original_import94_complete_unkeyed_source_history_rollback_and_refusal() {
    let mut db = fixture();
    let before: i64 = db.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
    {
        let tx = db.transaction().unwrap();
        let result = run_original(&tx, &source(), &[], latest(), Actor { subject: "user:synthetic", program: opening_balance::PROGRAM }, 1792573200000).unwrap();
        assert_eq!(result["sourceSystem"], "synthetic-csv");
        assert_eq!(result["openingBalanceLineCount"], 2);
        assert!(result["entryId"].as_i64().unwrap() > 0);
        assert!(opening_balance::validate_history(&tx).unwrap().is_some());
        assert!(run_original(&tx, &source(), &[], latest(), Actor { subject: "user:synthetic", program: opening_balance::PROGRAM }, 1792573200000).is_err());
        tx.rollback().unwrap();
    }
    assert_eq!(db.query_row("SELECT count(*) FROM journal_entries", [], |r| r.get::<_,i64>(0)).unwrap(), 0);
    assert_eq!(db.query_row("SELECT count(*) FROM audit_log", [], |r| r.get::<_,i64>(0)).unwrap(), before);
    let tx = db.transaction().unwrap();
    run_original(&tx, &source(), &[], latest(), Actor { subject: "user:synthetic", program: opening_balance::PROGRAM }, 1792573200000).unwrap();
    tx.commit().unwrap();
    assert!(opening_balance::validate_history(&db).unwrap().is_some());
}
