use super::*;
use crate::documents::{Original, register};
const AT: &str = "2026-10-20T12:00:00.000Z";
const PDF: &[u8] = b"%PDF-1.4\nsynthetic cited invoice\n%%EOF";
fn identity() -> ProviderIdentity {
    ProviderIdentity {
        id: "http-json-v1".into(),
        version: "1".into(),
    }
}
fn fixture() -> (Connection, i64) {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Buyer").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let hash = bytes_hash(PDF);
    let metadata=serde_json::from_value(json!({"source":"synthetic","documentType":"cash_register_receipt","issueDate":"2026-10-18","amountIncVat":125,"vatAmount":25})).unwrap();
    let id = register(
        &tx,
        Original {
            filename: "invoice.pdf",
            hash: &hash,
            path: &format!("documents/originals/{hash}.pdf"),
            mime: "application/pdf",
        },
        &metadata,
        "synthetic",
        "2026-10-20",
    )
    .unwrap()["document"]["id"]
        .as_i64()
        .unwrap();
    tx.commit().unwrap();
    (db, id)
}
fn output() -> ProviderOutput {
    ProviderOutput{fields:[("invoiceNumber",json!("INV-1")),("supplierName",json!("Synthetic Supplier")),("supplierCountry",json!("DK")),("supplierLegalId",json!("12345678")),("supplierLegalIdKind",json!("dk_cvr")),("buyerName",json!("Synthetic Buyer")),("buyerCountry",json!("DK")),("buyerLegalId",json!("87654321")),("buyerLegalIdKind",json!("dk_cvr")),("invoiceDate",json!("2026-10-18")),("currency",json!("DKK")),("netAmount",json!(100)),("vatAmount",json!(25)),("grossAmount",json!(125))].into_iter().map(|(key,value)|json!({"key":key,"sourceText":format!("Invoice cited {value}"),"value":value,"confidence":0.99,"page":1})).collect()}
}
fn completed() -> (Connection, i64, Value) {
    let (mut db, id) = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let p = prepare(&tx, id, PDF, &identity(), "user:synthetic", AT).unwrap();
    tx.commit().unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let result = finish(&tx, &p, &Ok(output()), "user:synthetic", AT).unwrap();
    tx.commit().unwrap();
    (db, id, result)
}
#[test]
fn completed_context_cache_readonly_retries_and_manual_guard() {
    let (mut db, id, result) = completed();
    assert_eq!(result["status"], "completed");
    require_completed_or_absent(&db, id).unwrap();
    let before = db.total_changes();
    let p = prepare(&db, id, PDF, &identity(), "user:synthetic", AT).unwrap();
    assert_eq!(p.cached(), Some(&result));
    recheck(&db, &p).unwrap();
    assert_eq!(
        finish(
            &db,
            &p,
            &Err(ProviderFailure::Unavailable),
            "user:synthetic",
            AT
        )
        .unwrap(),
        result
    );
    assert_eq!(db.total_changes(), before);
    db.execute(
        "UPDATE companies SET name='New reviewed buyer' WHERE id=1",
        [],
    )
    .unwrap();
    assert!(recheck(&db, &p).is_err());
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let p = prepare(&tx, id, PDF, &identity(), "user:synthetic", AT).unwrap();
    assert!(p.cached().is_none());
    tx.commit().unwrap();
    assert!(require_completed_or_absent(&db, id).is_err());
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let failed = finish(
        &tx,
        &p,
        &Err(ProviderFailure::Unavailable),
        "user:synthetic",
        AT,
    )
    .unwrap();
    assert_eq!(
        failed["conflicts"],
        json!(["EXTRACTION_PROVIDER_UNAVAILABLE"])
    );
    assert!(failed["exceptionState"]["id"].as_i64().is_some());
    tx.commit().unwrap();
    validate_history(&db).unwrap();
    crate::portable_backup::company(&db).unwrap();
}
#[test]
fn frozen_context_conflict_invalid_output_and_atomic_rollback() {
    let (mut db, id) = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let p = prepare(&tx, id, PDF, &identity(), "user:synthetic", AT).unwrap();
    tx.commit().unwrap();
    let mut fields = output();
    fields.fields[0]["confidence"] = json!(0.2);
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let result = finish(&tx, &p, &Ok(fields), "user:synthetic", AT).unwrap();
    assert_eq!(result["status"], "needs_resolution");
    assert!(
        result["conflicts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().contains("confidence"))
    );
    tx.rollback().unwrap();
    assert_eq!(inspect(&db, id).unwrap().unwrap()["status"], "started");
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("CREATE TEMP TRIGGER fail_extraction BEFORE INSERT ON invoice_extraction_resolutions BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
    assert!(finish(&tx, &p, &Ok(output()), "user:synthetic", AT).is_err());
    tx.rollback().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM invoice_extraction_results", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let result = finish(
        &tx,
        &p,
        &Ok(ProviderOutput {
            fields: vec![json!({"key":"invented"})],
        }),
        "user:synthetic",
        AT,
    )
    .unwrap();
    assert_eq!(result["status"], "needs_resolution");
    tx.commit().unwrap();
    validate_history(&db).unwrap();
}
#[test]
fn corrupt_fields_blob_header_and_native_terminal_refuse() {
    for sql in [
        "DROP TRIGGER invoice_extraction_fields_no_update; UPDATE invoice_extraction_fields SET value_json='999' WHERE field_key='grossAmount'",
        "DROP TRIGGER invoice_extraction_documents_no_update; UPDATE invoice_extraction_documents SET pdf_bytes=x'00'",
        "DROP TRIGGER invoice_extraction_results_no_update; UPDATE invoice_extraction_results SET result_json='{\"version\":\"unknown\"}'",
        "DROP TRIGGER invoice_extraction_resolutions_no_update; UPDATE invoice_extraction_resolutions SET resolution_json='{\"status\":\"completed\",\"errors\":[\"changed\"]}' WHERE field_key='extraction_status'",
    ] {
        let (db, id, _) = completed();
        db.execute_batch(sql).unwrap();
        assert!(validate_history(&db).is_err(), "{sql}");
        assert!(inspect(&db, id).is_err());
        assert!(crate::portable_backup::company(&db).is_err());
    }
}
#[test]
fn historical_legacy_exact_bytes_no_context_reinterpretation_and_version_collision() {
    let (mut db, id, result) = completed();
    // Build the exact supported historical TS header and original attempt/result audit grammar.
    db.execute_batch(
        "DROP TRIGGER invoice_extraction_results_no_update; DROP TRIGGER invoice_extraction_attempts_no_update; DROP TRIGGER audit_log_no_update; UPDATE invoice_extraction_attempts SET initiated_by=NULL; UPDATE invoice_extraction_results SET initiated_by=NULL;",
    )
    .unwrap();
    db.execute(
        "UPDATE invoice_extraction_results SET result_json=?1",
        ["{\"extractor\":{\"id\":\"http-json-v1\",\"version\":\"1\"}}"],
    )
    .unwrap();
    db.execute("UPDATE audit_log SET message='Invoice extraction attempt started' WHERE event_type='invoice_extraction_attempt'",[]).unwrap();
    db.execute("UPDATE audit_log SET message='Invoice extraction completed' WHERE event_type='invoice_extraction_result'",[]).unwrap();
    db.execute("UPDATE companies SET name='Later buyer profile'", [])
        .unwrap();
    validate_history(&db).unwrap();
    assert_eq!(inspect(&db, id).unwrap().unwrap(), result);
    require_completed_or_absent(&db, id).unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let p = prepare(&tx, id, PDF, &identity(), "user:synthetic", AT).unwrap();
    assert!(p.cached().is_none());
    assert_eq!(
        tx.query_row(
            "SELECT extractor_version FROM invoice_extraction_attempts ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "1#retry-1"
    );
    tx.rollback().unwrap();
    assert_eq!(engine::retry_number("10", "1"), None);
    assert_eq!(engine::retry_number("1#retry-01", "1"), None);
    assert_eq!(engine::retry_number("1#retry-1", "1"), Some(1));
}
