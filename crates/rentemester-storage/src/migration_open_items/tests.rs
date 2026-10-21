use super::*;
use rusqlite::TransactionBehavior;
fn fixture() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut db, "Synthetic migration settlement").unwrap();
    db.execute_batch("INSERT INTO accounts(id,account_no,name,type,normal_balance) VALUES(1,'5510','Synthetic bank','asset','debit'),(2,'5520','Synthetic receivables','asset','debit'),(3,'6800','Synthetic payables','liability','credit'); INSERT INTO bank_accounts(id,slug,name,ledger_account_no) VALUES(1,'synthetic','Synthetic bank','5510');").unwrap();
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert_eq!(post(&tx, "recognize_payable", "10.00", None), 1);
        tx.commit().unwrap();
    }
    let h = "a".repeat(64);
    db.execute("INSERT INTO dinero_import_sources(id,raw_sha256,raw_size_bytes,canonical_listing_sha256,canonical_listing_count) VALUES(1,?1,0,?1,0)",[&h]).unwrap();
    db.execute("INSERT INTO dinero_import_inventories(id,source_id,source_raw_sha256,canonical_listing_sha256,canonical_listing_count,entry_count,total_size_bytes) VALUES(1,1,?1,?1,0,0,0)",[&h]).unwrap();
    db.execute("INSERT INTO dinero_import_attempts(id,inventory_id,source_id,source_raw_sha256,parser_contract,actor,cutover_date,outcome,result_sha256) VALUES(1,1,1,?1,'synthetic-fixture','agent:synthetic','2026-01-01','accepted',?1)",[&h]).unwrap();
    db.execute_batch("INSERT INTO migration_open_item_batches(id,dinero_import_attempt_id,control_account_no,kind,source_control_amount,opening_journal_entry_id) VALUES(1,1,'5520','receivable',10,NULL),(2,1,'6800','payable',10,1); INSERT INTO migration_open_items(id,batch_id,external_ref,original_amount,open_amount_at_import,source_kind,resolution_status) VALUES(1,1,'UNALLOCATED:5520',12,10,'control_balance','unallocated'); INSERT INTO migration_open_items(id,batch_id,external_ref,counterparty_name,issue_date,due_date,original_amount,open_amount_at_import,source_kind,resolution_status) VALUES(2,2,'SYN-PAY','Synthetic supplier','2025-12-10','2026-01-10',15,10,'opening','resolved');").unwrap();
    db
}
fn post(
    tx: &Transaction<'_>,
    kind: &str,
    amount: &str,
    bank: Option<(i64, &str, Option<&str>)>,
) -> i64 {
    let bank_id = bank.map(|(id, _, _)| id);
    if let Some((id, amount, dkk)) = bank {
        tx.execute("INSERT INTO bank_transactions(id,transaction_date,text,amount,amount_dkk,source_file_hash,transaction_hash,bank_account_id) VALUES(?1,'2026-04-01','Synthetic settlement',?2,?3,?4,?5,1)",params![id,amount.parse::<f64>().unwrap(),dkk.map(|v|v.parse::<f64>().unwrap()),"b".repeat(64),format!("synthetic-{id}")]).unwrap();
    }
    post_linked(tx, kind, amount, bank_id)
}
fn post_linked(tx: &Transaction<'_>, kind: &str, amount: &str, bank_id: Option<i64>) -> i64 {
    let receivable = kind == "receivable" || kind == "recognize_payable";
    let input=serde_json::from_value(json!({"transactionDate":"2026-04-01","text":"Synthetic settlement","currency":"DKK","sourceBankTransactionId":bank_id,"lines":[{"accountNo":"5510","debitAmount":if receivable {amount}else{"0.00"},"creditAmount":if receivable {"0.00"}else{amount}},{"accountNo":if kind=="payable" || kind=="recognize_payable"{"6800"}else{"5520"},"debitAmount":if receivable{"0.00"}else{amount},"creditAmount":if receivable{amount}else{"0.00"}}]})).unwrap();
    let result = journal_post::post(
        tx,
        &input,
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        Date::parse("2026-10-21").unwrap(),
        journal_post::Actor {
            subject: "synthetic",
            program: "migration-test",
        },
    )
    .unwrap();
    assert_eq!(result["ok"], true, "{result}");
    result["entryId"].as_i64().unwrap()
}
fn input(item: i64, journal: i64, amount: &str, bank: Option<i64>) -> Apply {
    Apply {
        item_id: item,
        amount: Money::parse_canonical(amount).unwrap(),
        application_date: "2026-04-01".into(),
        source_journal_entry_id: journal,
        bank_transaction_id: bank,
        note: Some("  Synthetic evidence review  ".into()),
    }
}
fn snapshot(db: &Connection) -> Value {
    crate::test_support::typed_tables(db)
}
#[test]
fn derived_readback_settlement_and_immutable_provenance() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let j = post(&tx, "receivable", "3.33", Some((1, "3.33", None)));
    let p = post(&tx, "payable", "10.00", Some((2, "-10.00", None)));
    let before = snapshot(&tx);
    apply(&tx, &input(1, j, "3.33", Some(1))).unwrap();
    apply(&tx, &input(2, p, "10.00", Some(2))).unwrap();
    let after = snapshot(&tx);
    for (name, value) in before.as_object().unwrap() {
        if name != "migration_open_item_applications" {
            assert_eq!(value, &after[name], "{name}");
        }
    }
    assert!(crate::journal_hash::verify(&tx).unwrap().ok);
    let changes = tx.total_changes();
    let r = read(&tx, None).unwrap();
    assert_eq!(tx.total_changes(), changes);
    assert_eq!(r["rows"][0]["originalAmount"], "12.00");
    assert_eq!(r["rows"][0]["openAmountAtImport"], "10.00");
    assert_eq!(r["rows"][0]["appliedAmount"], "3.33");
    assert_eq!(r["rows"][0]["openBalance"], "6.67");
    assert_eq!(r["rows"][0]["resolutionStatus"], "unallocated");
    assert_eq!(r["rows"][0]["sourceKind"], "control_balance");
    assert_eq!(
        r["rows"][0]["applications"][0]["note"],
        "Synthetic evidence review"
    );
    assert_eq!(r["rows"][0]["applications"][0]["sourceJournalEntryId"], j);
    assert_eq!(r["rows"][1]["status"], "settled");
    assert_eq!(r["rows"][1]["sourceKind"], "opening");
    assert_eq!(r["rows"][1]["counterpartyName"], "Synthetic supplier");
    assert_eq!(
        read(&tx, Some(2)).unwrap()["rows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(read(&tx, Some(999)).unwrap()["rows"], json!([]));
    assert!(
        tx.execute("UPDATE migration_open_items SET original_amount=20", [])
            .is_err()
    );
    assert!(
        tx.execute("DELETE FROM migration_open_item_applications", [])
            .is_err()
    );
    tx.commit().unwrap();
    db.execute_batch("PRAGMA query_only=ON").unwrap();
    assert_eq!(read(&db, None).unwrap(), r);
}
#[test]
fn invalid_direction_overapplication_reused_evidence_and_dates_have_no_effect() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let j = post(&tx, "receivable", "6.00", Some((1, "6.00", None)));
    let excess = post(&tx, "receivable", "5.00", None);
    let payable = post(&tx, "reverse_receivable", "6.00", None);
    let linked = post(&tx, "receivable", "6.00", Some((2, "6.00", None)));
    let mismatch = post(&tx, "receivable", "6.00", Some((3, "6.00", Some("5.00"))));
    let same_bank = post_linked(&tx, "receivable", "6.00", Some(1));
    for bad in [
        input(1, payable, "6.00", None),
        input(1, 999, "6.00", None),
        input(999, j, "6.00", None),
        input(1, j, "0.00", None),
        input(1, linked, "6.00", Some(1)),
        input(1, mismatch, "6.00", Some(3)),
        input(1, j, "6.00", Some(999)),
    ] {
        let before = snapshot(&tx);
        assert!(apply(&tx, &bad).is_err());
        assert_eq!(before, snapshot(&tx));
    }
    let mut bad = input(1, j, "6.00", None);
    bad.application_date = "2026-02-30".into();
    assert!(apply(&tx, &bad).is_err());
    apply(&tx, &input(1, j, "6.00", Some(1))).unwrap();
    for bad in [
        input(1, j, "6.00", None),
        input(1, excess, "5.00", None),
        input(1, same_bank, "6.00", Some(1)),
    ] {
        let before = snapshot(&tx);
        let err = apply(&tx, &bad).unwrap_err();
        assert!(
            err.message.contains("already used") || err.message.contains("over-apply"),
            "{}",
            err.message
        );
        assert_eq!(before, snapshot(&tx));
    }
}
#[test]
fn optional_bank_is_optional_and_caller_rollback_restores_all_effects() {
    let mut db = fixture();
    let before = snapshot(&db);
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let j = post(&tx, "receivable", "10.00", None);
        apply(&tx, &input(1, j, "10.00", None)).unwrap();
        assert_eq!(read(&tx, None).unwrap()["rows"][0]["status"], "settled");
        tx.rollback().unwrap();
    }
    assert_eq!(before, snapshot(&db));
    let tx = db.transaction().unwrap();
    assert!(apply(&tx, &input(1, 1, "1.00", None)).is_err());
}
