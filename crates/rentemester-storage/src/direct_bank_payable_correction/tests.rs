use super::*;
use rentemester_engine::document;
use rusqlite::{Connection, TransactionBehavior};
use std::cell::Cell;
thread_local! {static FAULT:Cell<u8>=const{Cell::new(0)};}
pub(super) fn checkpoint(point: u8) -> Result<(), Error> {
    FAULT.with(|f| {
        if f.get() == point {
            Err(fail("SYNTHETIC_CORRECTION_FAULT"))
        } else if f.get() == 9 && point == 3 {
            std::process::exit(58)
        } else {
            Ok(())
        }
    })
}
fn latest() -> Date {
    Date::parse("2026-12-31").unwrap()
}
const NOW: i64 = 1792368000000;
fn fixture(bank_date: &str, mixed: bool) -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic correction").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET cvr='87654321',vat_period_type='quarter' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('A','Synthetic bank','asset','debit'),('X','Synthetic expense','expense','debit'),('V','Synthetic VAT','vat','debit'),('C','Synthetic creditor','liability','credit'),('R','Synthetic debtor','asset','debit');INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by)VALUES('bank','A','confirmed',1,'synthetic'),('input_vat','V','confirmed',1,'synthetic'),('creditors','C','confirmed',1,'synthetic'),('debtors','R','confirmed',1,'synthetic');INSERT INTO bank_accounts(slug,name,ledger_account_no)VALUES('synthetic','Synthetic bank','A');").unwrap();
    let mut value = json!({"source":"synthetic-correction","documentType":"purchase_sale","issueDate":"2026-06-16","invoiceNo":"SYN-58","deliveryDescription":"Synthetic supplies","amountIncVat":125,"vatAmount":25,"currency":"DKK","sender":{"name":"Synthetic supplier","address":"Synthetic1","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic buyer","address":"Synthetic2","vatOrCvr":"DK87654321"}});
    if mixed {
        value["amountIncVat"] = json!(509);
        value["vatAmount"] = json!(86);
        value["danishSimplifiedPurchaseInvoice"] = json!(true);
        value["purchaseVatLines"] = json!([{"classification":"dk_purchase_25","netAmount":344,"vatAmount":86},{"classification":"exempt","netAmount":79,"vatAmount":0}]);
    }
    let metadata: document::Metadata = serde_json::from_value(value).unwrap();
    crate::documents::register(
        &tx,
        crate::documents::Original {
            filename: "synthetic.txt",
            hash: &"a".repeat(64),
            path: "documents/originals/synthetic.txt",
            mime: "text/plain",
        },
        &metadata,
        "author",
        "2026-10-19",
    )
    .unwrap();
    let csv = format!(
        "date;text;amount;saldo\n{bank_date};Synthetic SYN-58;-{0};-{0}\n",
        if mixed { 509 } else { 125 }
    );
    crate::bank::import(
        &tx,
        &csv,
        &rentemester_engine::bank::parse(&csv).unwrap(),
        Some("synthetic"),
        "author",
        "2026-10-19T12:00:00Z",
        "bank/processed/synthetic.txt",
    )
    .unwrap();
    crate::expense::post(
        &tx,
        &crate::expense::Input {
            document_id: 1,
            bank_transaction_id: 1,
            expense_account_no: "X".into(),
            vat_treatment: Some("standard".into()),
            payment_account_no: None,
            transaction_date: None,
            text: None,
        },
        "author",
        latest(),
    )
    .unwrap();
    tx.commit().unwrap();
    db
}
fn input() -> PlanInput {
    PlanInput {
        document_id: 1,
        bank_transaction_id: 1,
        bill_date: "2026-06-16".into(),
        due_date: "2026-07-16".into(),
        expense_account_no: "X".into(),
        vat_treatment: Some("standard".into()),
        vendor_id: None,
        note: Some("Synthetic source correction".into()),
    }
}
fn reviewed(tx: &Transaction<'_>) -> ApplyInput {
    let p = plan(tx, &input(), latest()).unwrap();
    assert_eq!(p, plan(tx, &input(), latest()).unwrap());
    let mut value = serde_json::to_value(input()).unwrap();
    value["planHash"] = p["planHash"].clone();
    value["reason"] = json!("Synthetic reviewed temporal correction");
    value["idempotencyKey"] = json!("synthetic-correction-58");
    value["confirm"] = json!(true);
    serde_json::from_value(value).unwrap()
}
fn count(db: &Connection, table: &str) -> i64 {
    db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn period_sources(tx: &Transaction<'_>) {
    let before = tx.total_changes();
    let errors = crate::periods::verify_sources(tx).unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        tx.total_changes(),
        before,
        "period assurance must be read-only"
    );
}
fn untouched(db: &Connection) {
    assert_eq!(count(db, "journal_entries"), 1);
    assert_eq!(count(db, "bank_journal_reconciliations"), 1);
    for t in [
        "payables",
        "payable_payments",
        "direct_bank_purchase_payable_corrections",
        "bank_reconciliation_correction_events",
        "mutation_idempotency_tombstones",
        "mutation_idempotency_outcomes",
    ] {
        assert_eq!(count(db, t), 0, "{t}")
    }
}
fn balances(tx: &Transaction<'_>) -> std::collections::BTreeMap<String, String> {
    crate::trial_balance_owned(
        tx,
        rentemester_engine::trial_balance::Period::new("2026-01-01", "2026-12-31").unwrap(),
    )
    .unwrap()
    .accounts
    .into_iter()
    .filter(|a| a.balance != Money::default())
    .map(|a| (a.account_no, a.balance.decimal()))
    .collect()
}
#[test]
fn same_and_cross_period_mixed_vat_preserve_balances_and_exact_replay() {
    for (date, mixed) in [("2026-06-20", false), ("2026-07-02", true)] {
        let mut db = fixture(date, mixed);
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert_eq!(candidates(&tx).unwrap().as_array().unwrap().len(), 1);
        let list = payables::list(&tx, "a", "2026-10-19", "all").unwrap();
        assert_eq!(list["unregisteredDocuments"], json!([]));
        assert_eq!(
            list["directBankCorrectionCandidates"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(payables::register(&tx, &input().bill(), "author", latest()).is_err());
        let before = balances(&tx);
        let vat = crate::vat_report::range(&tx, "2026-01-01", "2026-12-31").unwrap();
        let i = reviewed(&tx);
        let out = apply(&tx, &i, "author", latest(), NOW).unwrap();
        assert_eq!(out["correction"]["idempotent"], false);
        assert_eq!(out["idempotency"]["replayed"], false);
        assert_eq!(count(&tx, "journal_entries"), 4);
        assert_eq!(count(&tx, "bank_journal_reconciliations"), 1);
        assert_eq!(balances(&tx), before);
        assert_eq!(
            crate::vat_report::range(&tx, "2026-01-01", "2026-12-31").unwrap(),
            vat
        );
        assert_eq!(
            payables::list(&tx, "a", "2026-06-19", "all").unwrap()["rows"][0]["openBalance"]
                .as_f64(),
            Some(if mixed { 509.0 } else { 125.0 })
        );
        assert_eq!(
            payables::list(&tx, "a", date, "all").unwrap()["rows"][0]["openBalance"].as_f64(),
            Some(0.0)
        );
        assert!(crate::journal_hash::verify(&tx).unwrap().ok);
        let replay = apply(&tx, &i, "author", latest(), NOW + 1).unwrap();
        assert_eq!(replay["idempotency"]["replayed"], true);
        assert_eq!(replay["correction"], out["correction"]);
        let original_replay =
            apply_original(&tx, &i, "author", latest(), NOW + 1, PrincipalKind::User).unwrap();
        assert_eq!(original_replay["correction"], out["correction"]);
        assert_eq!(original_replay["idempotency"]["replayed"], true);
        let mut conflict: ApplyInput =
            serde_json::from_value(serde_json::to_value(&i).unwrap()).unwrap();
        conflict.note = Some("Changed accepted request".into());
        assert_eq!(
            apply(&tx, &conflict, "author", latest(), NOW + 2)
                .unwrap_err()
                .code,
            "IDEMPOTENCY_CONFLICT"
        );
        let mut logical: ApplyInput =
            serde_json::from_value(serde_json::to_value(&i).unwrap()).unwrap();
        logical.idempotency_key = None;
        let prior = apply(
            &tx,
            &logical,
            "another-current-authorized-user",
            latest(),
            NOW + 3,
        )
        .unwrap();
        assert_eq!(prior["correction"]["idempotent"], true);
        assert_eq!(
            prior["correction"]["sourcePrincipal"]["subjectId"],
            "author"
        );
        assert_eq!(count(&tx, "journal_entries"), 4);
        period_sources(&tx);
        tx.commit().unwrap();
        validate_backup(&db).unwrap();
    }
}
#[test]
fn stale_source_wrong_direction_closed_period_and_changed_hash_refuse() {
    for change in [
        "UPDATE accounts SET name='Changed synthetic review fact' WHERE account_no='X'",
        "INSERT INTO accounting_periods(period_start,period_end,kind,status)VALUES('2026-06-01','2026-06-30','custom','closed')",
    ] {
        let mut db = fixture("2026-07-02", false);
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let i = reviewed(&tx);
        tx.execute(change, []).unwrap();
        assert!(apply(&tx, &i, "author", latest(), NOW).is_err());
        tx.rollback().unwrap();
        untouched(&db);
    }
    let mut db = fixture("2026-07-02", false);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let mut i = reviewed(&tx);
    i.plan_hash = "0".repeat(64);
    assert!(apply(&tx, &i, "author", latest(), NOW).is_err());
    assert!(
        tx.execute("UPDATE bank_transactions SET amount=125 WHERE id=1", [])
            .is_err()
    );
    tx.execute("INSERT INTO bank_transactions(bank_account_id,transaction_date,text,amount,currency)VALUES(1,'2026-07-02','Synthetic incoming refusal',125,'DKK')", []).unwrap();
    let mut incoming = input();
    incoming.bank_transaction_id = 2;
    assert!(plan(&tx, &incoming, latest()).is_err());
    let mut bad = input();
    bad.bill_date = "2026-06-17".into();
    assert!(plan(&tx, &bad, latest()).is_err());
    tx.rollback().unwrap();
    untouched(&db);
}
#[test]
fn every_composite_boundary_error_rolls_back_whole_transaction() {
    for point in 1..=5 {
        let mut db = fixture("2026-07-02", true);
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let i = reviewed(&tx);
        FAULT.with(|f| f.set(point));
        let error = apply(&tx, &i, "author", latest(), NOW).unwrap_err();
        FAULT.with(|f| f.set(0));
        assert_eq!(error.message, "SYNTHETIC_CORRECTION_FAULT");
        tx.rollback().unwrap();
        untouched(&db);
    }
}
#[test]
fn concurrent_real_connections_commit_one_effect_and_known_keyed_replay() {
    let mut db = fixture("2026-07-02", true);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let i = reviewed(&tx);
    let wire = serde_json::to_string(&i).unwrap();
    tx.commit().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("concurrent.sqlite");
    db.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    drop(db);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let threads = (0..2)
        .map(|_| {
            let barrier = barrier.clone();
            let path = path.clone();
            let wire = wire.clone();
            std::thread::spawn(move || {
                let mut db = Connection::open(path).unwrap();
                db.busy_timeout(std::time::Duration::from_secs(20)).unwrap();
                barrier.wait();
                let tx = db
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .unwrap();
                let i = serde_json::from_str(&wire).unwrap();
                let out = apply(&tx, &i, "author", latest(), NOW).unwrap();
                tx.commit().unwrap();
                out["idempotency"]["replayed"].as_bool().unwrap()
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter(|r| !*r)
            .count(),
        1
    );
    let db = Connection::open(path).unwrap();
    assert_eq!(count(&db, "journal_entries"), 4);
    assert_eq!(count(&db, "direct_bank_purchase_payable_corrections"), 1);
    assert_eq!(count(&db, "mutation_idempotency_tombstones"), 1);
    validate_backup(&db).unwrap();
}
#[test]
fn crash_child() {
    let Some(path) = std::env::var_os("RM_CORRECTION58_CRASH") else {
        return;
    };
    let mut db = Connection::open(path).unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let i = reviewed(&tx);
    FAULT.with(|f| f.set(9));
    let _ = apply(&tx, &i, "author", latest(), NOW);
    panic!("child did not reach actual payment-prefix boundary")
}
#[test]
fn actual_process_death_before_commit_preserves_source() {
    let db = fixture("2026-07-02", true);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crash.sqlite");
    db.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    drop(db);
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "direct_bank_payable_correction::tests::crash_child",
            "--nocapture",
        ])
        .env("RM_CORRECTION58_CRASH", &path)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(58),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let db = Connection::open(path).unwrap();
    untouched(&db);
}
#[test]
fn file_snapshot_reopen_keeps_linked_history_and_typed_validation() {
    let mut db = fixture("2026-07-02", true);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let i = reviewed(&tx);
    let expected = apply(&tx, &i, "author", latest(), NOW).unwrap();
    tx.commit().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot.sqlite");
    db.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    drop(db);
    let mut db = Connection::open(path).unwrap();
    validate_backup(&db).unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    period_sources(&tx);
    let out = apply(&tx, &i, "author", latest(), NOW + 1).unwrap();
    assert_eq!(out["correction"], expected["correction"]);
    assert_eq!(out["idempotency"]["replayed"], true);
    tx.commit().unwrap();
    assert_eq!(count(&db, "journal_entries"), 4);
}

#[test]
fn reviewed_period_close_and_reopen_retain_direct_correction_and_journal_history() {
    let mut db = fixture("2026-07-02", true);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let reviewed_input = reviewed(&tx);
    let correction = apply(&tx, &reviewed_input, "author", latest(), NOW).unwrap();
    period_sources(&tx);
    // Physical-original verification belongs to the host. This storage test
    // supplies its successful result and exercises the real accounting controls.
    let packet = crate::periods::compute(&tx, "2026-07-01", "2026-09-30", Ok(vec![])).unwrap();
    assert_eq!(packet["blockers"], 0, "{packet}");
    let journal = rows(&tx, "SELECT * FROM journal_entries ORDER BY id", []).unwrap();
    let lines = rows(&tx, "SELECT * FROM journal_lines ORDER BY id", []).unwrap();
    let retained = rows(
        &tx,
        "SELECT * FROM direct_bank_purchase_payable_corrections ORDER BY id",
        [],
    )
    .unwrap();
    let review = crate::periods::review(&tx, &packet, "author").unwrap();
    let input: crate::periods::Close = serde_json::from_value(json!({
        "periodStart":"2026-07-01", "periodEnd":"2026-09-30", "kind":"vat_period",
        "packetHash":packet["hash"], "reviewId":review["id"], "confirm":true
    }))
    .unwrap();
    assert_eq!(
        crate::periods::close(&tx, &input, &packet, "author", "2026-10-19").unwrap()["status"],
        "closed"
    );
    assert_eq!(
        crate::periods::list(&tx, "synthetic").unwrap()["periods"][0]["effectiveStatus"],
        "closed"
    );
    let locked = crate::periods::compute(&tx, "2026-07-01", "2026-09-30", Ok(vec![])).unwrap();
    let reopen: crate::periods::Reopen = serde_json::from_value(json!({
        "periodStart":"2026-07-01", "periodEnd":"2026-09-30", "kind":"vat_period",
        "reason":"Synthetic additional reviewed evidence", "confirm":true
    }))
    .unwrap();
    crate::periods::reopen(&tx, &reopen, &locked, "author").unwrap();
    assert_eq!(
        crate::periods::list(&tx, "synthetic").unwrap()["periods"][0]["effectiveStatus"],
        "open"
    );
    assert_eq!(
        rows(&tx, "SELECT * FROM journal_entries ORDER BY id", []).unwrap(),
        journal
    );
    assert_eq!(
        rows(&tx, "SELECT * FROM journal_lines ORDER BY id", []).unwrap(),
        lines
    );
    assert_eq!(
        rows(
            &tx,
            "SELECT * FROM direct_bank_purchase_payable_corrections ORDER BY id",
            []
        )
        .unwrap(),
        retained
    );
    assert_eq!(
        apply(&tx, &reviewed_input, "author", latest(), NOW + 1).unwrap()["correction"],
        correction["correction"]
    );
    period_sources(&tx);
    tx.commit().unwrap();
}

#[test]
fn period_close_refuses_tampered_retained_direct_correction_without_financial_effects() {
    let mut db = fixture("2026-07-02", true);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    apply(&tx, &reviewed(&tx), "author", latest(), NOW).unwrap();
    period_sources(&tx);
    let packet = crate::periods::compute(&tx, "2026-07-01", "2026-09-30", Ok(vec![])).unwrap();
    assert_eq!(packet["blockers"], 0, "{packet}");
    let review = crate::periods::review(&tx, &packet, "author").unwrap();
    tx.commit().unwrap();
    // Synthetic corruption only; restore the exact trigger DDL so an absent
    // schema guard cannot substitute for validation of retained correction facts.
    let triggers = db
        .prepare("SELECT name,sql FROM sqlite_schema WHERE type='trigger' ORDER BY name")
        .unwrap()
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for (name, _) in &triggers {
        db.execute_batch(&format!("DROP TRIGGER \"{name}\";"))
            .unwrap();
    }
    db.execute(
        "UPDATE direct_bank_purchase_payable_corrections SET original_journal_hash=?1",
        ["f".repeat(64)],
    )
    .unwrap();
    for (_, ddl) in triggers {
        db.execute_batch(&ddl).unwrap();
    }
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let before = tx.total_changes();
    let result = crate::periods::verify_sources(&tx);
    assert!(
        result.is_err() || !result.unwrap().is_empty(),
        "corrupt correction passed period assurance"
    );
    let current = crate::periods::compute(&tx, "2026-07-01", "2026-09-30", Ok(vec![])).unwrap();
    assert!(
        current["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "LEDGER_AUDIT_CHAIN"
                && (item["status"] == "blocked" || item["status"] == "unavailable"))
    );
    let input: crate::periods::Close = serde_json::from_value(json!({
        "periodStart":"2026-07-01", "periodEnd":"2026-09-30", "kind":"vat_period",
        "packetHash":packet["hash"], "reviewId":review["id"], "confirm":true
    }))
    .unwrap();
    assert!(crate::periods::close(&tx, &input, &current, "author", "2026-10-19").is_err());
    assert_eq!(count(&tx, "journal_entries"), 4);
    assert_eq!(count(&tx, "accounting_periods"), 0);
    assert_eq!(tx.total_changes(), before);
}

#[test]
fn original_service_direct_correction_binds_nested_proof_and_exact_keyed_replay() {
    let mut db = fixture("2026-07-02", false);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let input = reviewed(&tx);
    let first = apply_original(
        &tx,
        &input,
        "service_synthetic",
        latest(),
        NOW,
        PrincipalKind::ServiceAccount,
    )
    .unwrap();
    assert_eq!(
        first["correction"]["sourcePrincipal"],
        json!({"kind":"service-account","subjectId":"service_synthetic"})
    );
    assert_eq!(first["idempotency"]["replayed"], false);
    let nested: (String, String) = tx
        .query_row(
            "SELECT principal_kind,principal_subject_id FROM bank_reconciliation_correction_events",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        nested,
        ("service-account".into(), "service_synthetic".into())
    );
    let message: String = tx
        .query_row(
            "SELECT message FROM audit_log WHERE event_type='bank_reconciliation_corrected'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let claims: Value = serde_json::from_str(&message).unwrap();
    assert_eq!(
        claims["compatibility"],
        json!({"schemaVersion":"rentemester-original-bank-correction/1","principalKind":"service-account","principalSubjectId":"service_synthetic"})
    );
    period_sources(&tx);
    let again = apply_original(
        &tx,
        &input,
        "service_synthetic",
        latest(),
        NOW + 1,
        PrincipalKind::ServiceAccount,
    )
    .unwrap();
    assert_eq!(first["correction"], again["correction"]);
    assert_eq!(again["idempotency"]["replayed"], true);
    assert_eq!(count(&tx, "journal_entries"), 4);
    assert_eq!(count(&tx, "payables"), 1);
    assert_eq!(count(&tx, "payable_payments"), 1);
    assert_eq!(count(&tx, "bank_reconciliation_correction_events"), 1);
    let mut changed = reviewed_input(&input);
    changed.reason = format!(" {}", input.reason);
    assert!(
        apply_original(
            &tx,
            &changed,
            "service_synthetic",
            latest(),
            NOW + 2,
            PrincipalKind::ServiceAccount
        )
        .is_err()
    );
}
fn reviewed_input(i: &ApplyInput) -> ApplyInput {
    serde_json::from_value(serde_json::to_value(i).unwrap()).unwrap()
}
