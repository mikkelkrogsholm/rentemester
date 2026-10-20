use super::*;
use crate::documents::{Original, register, register_scanned};
use rentemester_engine::document::Metadata;
use rusqlite::{TransactionBehavior, params};
use serde_json::json;
fn fixture() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic scan").unwrap();
    db
}
fn metadata() -> Metadata {
    serde_json::from_value(json!({"source":"synthetic","documentType":"cash_register_receipt","issueDate":"2026-10-18","amountIncVat":125,"vatAmount":25})).unwrap()
}
fn original(hash: &str) -> Original<'_> {
    Original {
        filename: "synthetic.txt",
        hash,
        path: "documents/originals/synthetic.txt",
        mime: "text/plain",
    }
}
fn clean(db: &mut Connection) {
    let hash = "a".repeat(64);
    let scan = ScanEvidence::new(
        &hash,
        "Synthetic scanner",
        Some("1"),
        Some("synthetic-evidence"),
    )
    .unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    register_scanned(
        &tx,
        original(&hash),
        &metadata(),
        "synthetic-owner",
        "2026-10-18",
        Some(&scan),
    )
    .unwrap();
    tx.commit().unwrap();
}
#[test]
fn clean_and_off_registration_preserve_exact_atomic_evidence_and_duplicate_semantics() {
    let mut db = fixture();
    clean(&mut db);
    validate_backup(&db).unwrap();
    let hash = "a".repeat(64);
    let row: (String, String, String) = db
        .query_row(
            "SELECT sha256_hash,scanner_id,result FROM document_scan_evidence",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        row,
        (hash.clone(), "Synthetic scanner".into(), "clean".into())
    );
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let before = tx.total_changes();
    assert!(
        register_scanned(
            &tx,
            original(&hash),
            &metadata(),
            "synthetic-owner",
            "2026-10-18",
            Some(&ScanEvidence::new(&hash, "scanner", None, None).unwrap())
        )
        .unwrap_err()
        .message
        .contains("duplicate")
    );
    assert_eq!(before, tx.total_changes());
    register(
        &tx,
        original(&"b".repeat(64)),
        &metadata(),
        "synthetic-owner",
        "2026-10-18",
    )
    .unwrap();
    tx.commit().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM document_scan_evidence", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    // Signed company snapshots retain the table without a scanner credential or rescan.
    crate::portable_backup::company(&db).unwrap();
}
#[test]
fn invalid_or_mismatched_evidence_has_no_document_or_number_effect_and_rollback_is_atomic() {
    let mut db = fixture();
    let hash = "a".repeat(64);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let before = tx.total_changes();
    let mut scan = ScanEvidence::new(&"b".repeat(64), "scanner", None, None).unwrap();
    assert!(
        register_scanned(
            &tx,
            original(&hash),
            &metadata(),
            "owner",
            "2026-10-18",
            Some(&scan)
        )
        .is_err()
    );
    scan.sha256 = hash.clone();
    scan.scanner_id = " scanner ".into();
    assert!(
        register_scanned(
            &tx,
            original(&hash),
            &metadata(),
            "owner",
            "2026-10-18",
            Some(&scan)
        )
        .is_err()
    );
    assert_eq!(before, tx.total_changes());
    scan.scanner_id = "scanner".into();
    register_scanned(
        &tx,
        original(&hash),
        &metadata(),
        "owner",
        "2026-10-18",
        Some(&scan),
    )
    .unwrap();
    tx.rollback().unwrap();
    for table in ["documents", "document_scan_evidence", "audit_log"] {
        assert_eq!(
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
#[test]
fn snapshot_restore_validation_refuses_corrupt_relationship_fields_and_time() {
    for (column, value) in [
        ("sha256_hash", "b".repeat(64)),
        ("scanner_id", "scanner\u{200b}".into()),
        ("scanner_version", " e\u{301} ".into()),
        ("evidence_ref", "x\u{0000}".into()),
        ("scanned_at", "2026-02-30 12:00:00".into()),
    ] {
        let mut db = fixture();
        clean(&mut db);
        db.execute_batch("DROP TRIGGER document_scan_evidence_no_update")
            .unwrap();
        db.execute(
            &format!("UPDATE document_scan_evidence SET {column}=?1"),
            params![value],
        )
        .unwrap();
        assert!(validate_backup(&db).is_err(), "{column}");
        assert!(
            crate::portable_backup::company(&db).is_err(),
            "snapshot {column}"
        );
    }
    let mut db = fixture();
    clean(&mut db);
    db.execute_batch("PRAGMA foreign_keys=OFF; DROP TRIGGER document_scan_evidence_no_update; UPDATE document_scan_evidence SET document_id=999").unwrap();
    assert!(validate_backup(&db).is_err());
}
