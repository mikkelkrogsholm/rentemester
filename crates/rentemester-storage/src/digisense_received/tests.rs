use super::*;
use rusqlite::{Connection, TransactionBehavior};
use serde_json::json;
fn fixture() -> (tempfile::TempDir, Connection, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let mut control = Connection::open(dir.path().join("control.sqlite")).unwrap();
    let mut company = Connection::open(dir.path().join("company.sqlite")).unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic receive evidence")
        .unwrap();
    (dir, control, company)
}
#[test]
fn terminal_null_receipt_deduplicates_after_reopen_and_remains_append_only() {
    let (dir, _control, mut company) = fixture();
    let record = json!({"internalId":"synthetic-terminal","documentId":"INV-7"});
    let tx = company
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(!known(&tx, "synthetic-terminal").unwrap());
    append(
        &tx,
        "synthetic-key",
        &record,
        None,
        Some(TerminalReason::InvalidMetadata),
        "user:synthetic",
    )
    .unwrap();
    assert!(known(&tx, "synthetic-terminal").unwrap());
    tx.commit().unwrap();
    drop(company);
    let mut company = Connection::open(dir.path().join("company.sqlite")).unwrap();
    let tx = company
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(known(&tx, "synthetic-terminal").unwrap());
    assert!(
        append(
            &tx,
            "synthetic-key",
            &record,
            None,
            Some(TerminalReason::InvalidMetadata),
            "user:synthetic"
        )
        .is_err()
    );
    assert!(
        tx.execute("DELETE FROM digisense_received_documents", [])
            .is_err()
    );
    assert!(
        tx.execute(
            "UPDATE digisense_received_documents SET company_key='other'",
            []
        )
        .is_err()
    );
    let counts:(i64,i64,i64)=tx.query_row("SELECT (SELECT count(*) FROM digisense_received_documents WHERE document_id IS NULL),(SELECT count(*) FROM audit_log WHERE event_type='digisense_document_quarantined'),(SELECT count(*) FROM journal_entries)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(counts, (1, 1, 0));
}
#[test]
fn caller_transaction_rollback_retains_neither_quarantine_nor_audit() {
    let (_dir, _control, mut company) = fixture();
    let tx = company
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    append(
        &tx,
        "synthetic-key",
        &json!({"internalId":"synthetic-rollback"}),
        None,
        Some(TerminalReason::InvalidXml),
        "user:synthetic",
    )
    .unwrap();
    tx.rollback().unwrap();
    let count: i64 = company
        .query_row(
            "SELECT count(*) FROM digisense_received_documents",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    let count: i64 = company
        .query_row(
            "SELECT count(*) FROM audit_log WHERE event_type='digisense_document_quarantined'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}
