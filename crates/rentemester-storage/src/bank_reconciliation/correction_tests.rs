use super::*;
use rusqlite::{Connection, TransactionBehavior};
const NOW: i64 = 1_780_000_000_000;
const CSV: &str =
    "transaction_date,text,amount,currency\n2026-04-01,Synthetic correction receipt,12.34,DKK\n";
fn fixture() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let mut c = Connection::open_in_memory().unwrap();
    let mut l = Connection::open(dir.path().join("company.sqlite")).unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Bank Correction").unwrap();
    l.execute_batch("INSERT INTO accounts(id,account_no,name,type,normal_balance)VALUES(1,'B','Synthetic bank','asset','debit'),(2,'E','Synthetic equity one','equity','credit'),(3,'F','Synthetic equity two','equity','credit');INSERT INTO bank_accounts(id,slug,name,ledger_account_no,currency)VALUES(1,'correction','Synthetic correction bank','B','DKK');INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash)VALUES(1,'SYN-ORIGINAL','2026-04-01','Original','fixture','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount)VALUES(1,1,12.34,0),(1,2,0,12.34);").unwrap();
    {
        let tx = l
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        crate::bank::import(
            &tx,
            CSV,
            &rentemester_engine::bank::parse(CSV).unwrap(),
            Some("correction"),
            "synthetic",
            "2026-04-01T00:00:00Z",
            "bank/processed/synthetic.txt",
        )
        .unwrap();
        super::super::verify_source(&tx, 1, CSV.as_bytes()).unwrap();
        let plans = super::super::candidates(&tx, 1).unwrap();
        let proof = plans["plans"][0]["planHash"].as_str().unwrap().to_owned();
        super::super::apply(
            &tx,
            &super::super::Apply {
                bank_transaction_id: 1,
                journal_entry_id: 1,
                plan_hash: proof,
                note: "Synthetic initial reviewed link".into(),
                confirm: true,
            },
            "synthetic",
        )
        .unwrap();
        tx.commit().unwrap();
    }
    (dir, l)
}
fn prepare(tx: &Transaction<'_>, old: i64, child: i64, new: i64) {
    tx.execute("INSERT INTO journal_entries(id,entry_no,transaction_date,text,status,reversal_of_entry_id,rule_version,entry_hash)VALUES(?1,?2,'2026-04-01','Explicit reversal','reversed',?3,'fixture',?4)",params![child,format!("SYN-REV-{child}"),old,format!("{:064x}",child)]).unwrap();
    tx.execute("INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount,vat_code,text,currency)SELECT ?1,account_id,credit_amount,debit_amount,vat_code,CASE WHEN text IS NOT NULL AND text<>'' THEN 'REVERSAL: '||text ELSE 'REVERSAL of '||(SELECT entry_no FROM journal_entries WHERE id=?2) END,currency FROM journal_lines WHERE journal_entry_id=?2 ORDER BY id",params![child,old]).unwrap();
    tx.execute("INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash)VALUES(?1,?2,'2026-04-01','Independent replacement','fixture',?3)",params![new,format!("SYN-NEW-{new}"),format!("{:064x}",new)]).unwrap();
    tx.execute("INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount)VALUES(?1,1,12.34,0),(?1,3,0,12.34)",[new]).unwrap();
}
fn input(tx: &Transaction<'_>, replacement: i64, key: &str) -> Apply {
    let p = plan(tx, 1, replacement).unwrap();
    Apply {
        bank_transaction_id: 1,
        replacement_journal_entry_id: replacement,
        expected_reconciliation_id: p["plan"]["reconciliationId"].as_str().unwrap().into(),
        plan_hash: p["plan"]["planHash"].as_str().unwrap().into(),
        reason: "Synthetic review of original, actual reversal and independent replacement".into(),
        idempotency_key: key.into(),
        confirm: true,
    }
}
fn journal_bytes(tx: &Transaction<'_>) -> String {
    crate::cockpit::read(tx, "synthetic", "2026-04-05", Some("2026"), None)
        .unwrap()
        .journal
        .to_string()
}
#[test]
fn correction_needs_real_reversal_then_preserves_accounting_and_replays_known_receipt_after_reopen()
{
    let (dir, mut l) = fixture();
    let request;
    let first;
    {
        let tx = l
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(plan(&tx, 1, 3).is_err());
        prepare(&tx, 1, 2, 3);
        let proposed = plan(&tx, 1, 3).unwrap();
        assert_eq!(proposed["plan"]["reversalJournalEntryId"], 2);
        assert_eq!(proposed["plan"]["history"].as_array().unwrap().len(), 1);
        let before = journal_bytes(&tx);
        request = input(&tx, 3, "synthetic-correction-1");
        first = apply(&tx, &request, "synthetic", NOW).unwrap();
        assert_eq!(first["reconciliationId"], "correction:1");
        assert_eq!(first["receipt"]["replayed"], false);
        assert_eq!(journal_bytes(&tx), before);
        let view = crate::bank::read(&tx, "synthetic", "2026-04-05", Some("2026")).unwrap();
        assert_eq!(view["transactions"][0]["journalEntryNo"], "SYN-NEW-3");
        assert_eq!(
            view["transactions"][0]["reconciliationNote"],
            request.reason
        );
        assert_eq!(
            view["transactions"][0]["reconciliationHistory"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(
            tx.execute(
                "UPDATE bank_reconciliation_correction_events SET reason='rewrite'",
                []
            )
            .is_err()
        );
        assert!(
            tx.execute("DELETE FROM bank_reconciliation_correction_events", [])
                .is_err()
        );
        tx.commit().unwrap();
    }
    drop(l);
    let mut l = Connection::open(dir.path().join("company.sqlite")).unwrap();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let again = apply(&tx, &request, "synthetic", NOW + 1).unwrap();
    assert_eq!(again["id"], first["id"]);
    assert_eq!(again["planHash"], first["planHash"]);
    assert_eq!(again["receipt"]["replayed"], true);
    assert_eq!(
        tx.query_row(
            "SELECT COUNT(*) FROM bank_reconciliation_correction_events",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
#[test]
fn stale_proof_wrong_reason_and_competing_supersession_do_not_create_another_event() {
    let (_dir, mut l) = fixture();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    prepare(&tx, 1, 2, 3);
    let mut bad = input(&tx, 3, "bad");
    bad.plan_hash = "0".repeat(64);
    assert!(apply(&tx, &bad, "synthetic", NOW).is_err());
    bad = input(&tx, 3, "bad");
    bad.reason = " ".into();
    assert!(apply(&tx, &bad, "synthetic", NOW).is_err());
    bad = input(&tx, 3, "bad");
    bad.confirm = false;
    assert!(apply(&tx, &bad, "synthetic", NOW).is_err());
    let first = input(&tx, 3, "first");
    let mut competing = input(&tx, 3, "competing");
    apply(&tx, &first, "synthetic", NOW).unwrap();
    let after = journal_bytes(&tx);
    assert!(apply(&tx, &competing, "synthetic", NOW).is_err());
    assert_eq!(journal_bytes(&tx), after);
    prepare(&tx, 3, 4, 5);
    let next = input(&tx, 5, "second");
    assert_eq!(next.expected_reconciliation_id, "correction:1");
    assert_eq!(
        plan(&tx, 1, 5).unwrap()["plan"]["history"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let before = journal_bytes(&tx);
    apply(&tx, &next, "synthetic", NOW).unwrap();
    assert_eq!(journal_bytes(&tx), before);
    competing.idempotency_key = "stale-after-chain".into();
    assert!(apply(&tx, &competing, "synthetic", NOW).is_err());
    assert_eq!(
        tx.query_row(
            "SELECT COUNT(*) FROM bank_reconciliation_correction_events",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        tx.query_row(
            "SELECT COUNT(*) FROM bank_journal_reconciliation_links",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
#[test]
fn reversal_identity_without_exact_opposite_lines_is_not_sufficient() {
    let (_dir, mut l) = fixture();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO journal_entries(id,entry_no,transaction_date,text,status,reversal_of_entry_id,rule_version,entry_hash)VALUES(2,'BAD-CHILD','2026-04-01','Wrong equity opposite','reversed',1,'fixture','bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount)VALUES(2,1,0,12.34),(2,3,12.34,0);").unwrap();
    assert!(
        plan(&tx, 1, 3)
            .unwrap_err()
            .message
            .contains("mirror every")
    );
    assert_eq!(
        tx.query_row(
            "SELECT COUNT(*) FROM bank_reconciliation_correction_events",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
