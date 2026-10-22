use super::*;
use rusqlite::{Connection, TransactionBehavior};
fn fixture() -> (tempfile::TempDir, Connection, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let mut control = Connection::open(dir.path().join("control.sqlite")).unwrap();
    let mut company = Connection::open(dir.path().join("company.sqlite")).unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic original evidence")
        .unwrap();
    (dir, control, company)
}
fn metadata(simplified: bool) -> Metadata {
    let mut value = json!({"source":"synthetic-original","documentType":"purchase_sale","issueDate":"2026-10-18","invoiceNo":"SYN-1","deliveryDescription":"Synthetic supplies","amountIncVat":125,"vatAmount":25,"sender":{"name":"Synthetic supplier","address":"Synthetic street 1","vatOrCvr":"DK12345678"}});
    if simplified {
        value["danishSimplifiedPurchaseInvoice"] = json!(true);
    } else {
        value["recipient"] = json!({"name":"Synthetic buyer","address":"Synthetic street 2","vatOrCvr":"DK87654321"});
    }
    serde_json::from_value(value).unwrap()
}
fn register(tx: &Transaction<'_>, m: &Metadata) -> i64 {
    let result = crate::documents::register(
        tx,
        crate::documents::Original {
            filename: "synthetic.pdf",
            hash: &"a".repeat(64),
            path: "documents/originals/synthetic.pdf",
            mime: "application/pdf",
        },
        m,
        "synthetic",
        "2026-10-18",
    )
    .unwrap();
    result["document"]["id"].as_i64().unwrap()
}
#[test]
fn documents_original_enrichment_positive_noop_conflict_history_and_late_failure() {
    let (_dir, _control, mut db) = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let initial:Metadata=serde_json::from_value(json!({"source":"synthetic-original","documentType":"cash_register_receipt","issueDate":"2026-10-18"})).unwrap();
    let id = register(&tx, &initial);
    tx.commit().unwrap();
    let incoming:Metadata=serde_json::from_value(json!({"source":"synthetic-original","documentType":"cash_register_receipt","issueDate":"2026-10-18","amountIncVat":125,"vatAmount":25,"sender":{"name":"Synthetic supplier"}})).unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert_eq!(
        enrich(&tx, id, &incoming, "service_account:synthetic").unwrap()["enriched"],
        true
    );
    assert_eq!(
        enrich(&tx, id, &incoming, "service_account:synthetic").unwrap()["enriched"],
        false
    );
    let other:Metadata=serde_json::from_value(json!({"source":"synthetic-original","documentType":"cash_register_receipt","issueDate":"2026-10-18","amountIncVat":250})).unwrap();
    assert!(enrich(&tx, id, &other, "user:synthetic").is_err());
    validate_history(&tx).unwrap();
    let (original, actor): (String, String) = tx
        .query_row(
            "SELECT original_payload_json,actor FROM document_metadata_enrichments",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(original, serde_json::to_string(&initial).unwrap());
    assert_eq!(actor, "service_account:synthetic");
    tx.commit().unwrap();
    db.execute("UPDATE documents SET amount_inc_vat=999 WHERE id=?1", [id])
        .unwrap();
    assert!(validate_history(&db).is_err());
    // Rollback a late audit failure: projection/history cannot survive alone.
    let (_dir, _control, mut late) = fixture();
    let tx = late
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let id = register(&tx, &initial);
    tx.commit().unwrap();
    late.execute_batch("CREATE TRIGGER synthetic_audit_failure BEFORE INSERT ON audit_log BEGIN SELECT RAISE(ABORT,'synthetic late failure');END;").unwrap();
    let tx = late
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(enrich(&tx, id, &incoming, "user:synthetic").is_err());
    tx.rollback().unwrap();
    assert_eq!(
        late.query_row(
            "SELECT count(*) FROM document_metadata_enrichments",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        late.query_row("SELECT payload_json FROM documents", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        serde_json::to_string(&initial).unwrap()
    );
}
#[test]
fn documents_original_context_positive_noop_conflict_stale_and_valid_nonempty_backup() {
    let (_dir, _control, mut db) = fixture();
    db.execute_batch("UPDATE companies SET cvr='DK87654321',address='Synthetic street 2',postal_code='1000',city='Synthetic city',vat_period_type='quarter' WHERE id=1;").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let m = metadata(true);
    let id = register(&tx, &m);
    assert_eq!(
        company_context(
            &tx,
            id,
            "Synthetic receipt",
            "Synthetic business use",
            "user:synthetic"
        )
        .unwrap()["applied"],
        true
    );
    assert_eq!(
        company_context(
            &tx,
            id,
            "Synthetic receipt",
            "Synthetic business use",
            "user:synthetic"
        )
        .unwrap()["applied"],
        false
    );
    assert!(
        company_context(
            &tx,
            id,
            "Different receipt",
            "Synthetic business use",
            "user:synthetic"
        )
        .is_err()
    );
    assert!(valid_context_for_document(&tx, id, &m).unwrap());
    validate_history(&tx).unwrap();
    tx.commit().unwrap();
    crate::portable_backup::company(&db).unwrap();
    db.execute("UPDATE companies SET city='Changed city' WHERE id=1", [])
        .unwrap();
    validate_history(&db).unwrap();
    crate::portable_backup::company(&db).unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(
        company_context(
            &tx,
            id,
            "Synthetic receipt",
            "Synthetic business use",
            "user:synthetic"
        )
        .is_err()
    );
}
#[test]
fn documents_original_context_tampered_snapshot_and_enrichment_hash_refused() {
    let (_dir, _control, mut db) = fixture();
    db.execute_batch("UPDATE companies SET cvr='DK87654321',address='Synthetic street 2',postal_code='1000',city='Synthetic city',vat_period_type='quarter' WHERE id=1;").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let id = register(&tx, &metadata(true));
    company_context(&tx, id, "Receipt", "Business use", "user:synthetic").unwrap();
    tx.commit().unwrap();
    db.execute_batch("DROP TRIGGER document_company_contexts_no_update;UPDATE document_company_contexts SET company_snapshot_sha256=printf('%064d',0);").unwrap();
    assert!(validate_history(&db).is_err());
}
#[test]
fn documents_original_vendor_fill_precedence_and_same_hash_force_cannot_bypass() {
    let (_dir, _control, mut db) = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute("INSERT INTO vendors(name,address,vat_or_cvr,country_code,identifier_kind) VALUES(' Vendor name ',' Vendor address ',' DK12345678 ','DK','dk_cvr')",[]).unwrap();
    let mut m:Metadata=serde_json::from_value(json!({"source":"synthetic","documentType":"cash_register_receipt","sender":{"name":"Invoice name","address":" "}})).unwrap();
    let observation = vendor_fill(&tx, &mut m, Some(1)).unwrap();
    assert!(observation.is_some());
    let sender = m.sender.as_ref().unwrap();
    assert_eq!(sender.name.as_deref(), Some("Invoice name"));
    assert_eq!(sender.address.as_deref(), Some("Vendor address"));
    assert_eq!(sender.vat_or_cvr.as_deref(), Some("DK12345678"));
    register(&tx, &m);
    assert!(
        crate::documents::register_original(
            &tx,
            crate::documents::Original {
                filename: "synthetic.pdf",
                hash: &"a".repeat(64),
                path: "documents/originals/synthetic.pdf",
                mime: "application/pdf"
            },
            &m,
            "synthetic",
            "2026-10-18",
            None,
            crate::idempotency::PrincipalKind::ServiceAccount,
            true
        )
        .is_err()
    );
    let listed = list(&tx).unwrap();
    assert_eq!(listed["documents"].as_array().unwrap().len(), 1);
    assert_eq!(listed["documents"][0]["storedPath"], Value::Null);
    assert!(listed["documents"][0].get("payload_json").is_none());
}
#[test]
fn documents_original_verified_context_is_shared_expense_alternative_and_enrichment_backup() {
    let (_dir, _control, mut db) = fixture();
    db.execute_batch("UPDATE companies SET cvr='DK87654321',address='Synthetic street 2',postal_code='1000',city='Synthetic city',vat_period_type='quarter' WHERE id=1; INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('X','Synthetic expense','expense','debit'),('V','Synthetic input VAT','vat','debit'); INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by) VALUES('input_vat','V','confirmed',1,'synthetic');").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let m = metadata(true);
    let id = register(&tx, &m);
    let gross = rentemester_engine::money::Money::parse("125").unwrap();
    let rate = rentemester_engine::fx::FxRate::parse("1").unwrap();
    assert!(crate::expense::purchase(&tx, id, &m, "X", Some("standard"), &gross, &rate).is_err());
    company_context(
        &tx,
        id,
        "Synthetic receipt",
        "Synthetic business use",
        "user:synthetic",
    )
    .unwrap();
    let allocation =
        crate::expense::purchase(&tx, id, &m, "X", Some("standard"), &gross, &rate).unwrap();
    assert_eq!(
        allocation.deductible,
        rentemester_engine::money::Money::parse("25").unwrap()
    );
    tx.commit().unwrap();
    crate::portable_backup::company(&db).unwrap();
    let (_dir, _control, mut enriched) = fixture();
    let tx = enriched
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let initial: Metadata = serde_json::from_value(
        json!({"source":"synthetic-original","documentType":"cash_register_receipt"}),
    )
    .unwrap();
    let id = register(&tx, &initial);
    let incoming:Metadata=serde_json::from_value(json!({"source":"synthetic-original","documentType":"cash_register_receipt","amountIncVat":125})).unwrap();
    enrich(&tx, id, &incoming, "user:synthetic").unwrap();
    tx.commit().unwrap();
    crate::portable_backup::company(&enriched).unwrap();
    enriched.execute_batch("DROP TRIGGER document_metadata_enrichments_no_update; UPDATE document_metadata_enrichments SET original_payload_sha256=printf('%064d',0);").unwrap();
    assert!(validate_history(&enriched).is_err());
    assert!(crate::portable_backup::company(&enriched).is_err());
}

#[test]
fn documents_original_context_is_exact_document_even_with_identical_or_null_normalized_metadata() {
    let (_dir, _control, mut db) = fixture();
    db.execute_batch("UPDATE companies SET cvr='DK87654321',address='Synthetic street 2',postal_code='1000',city='Synthetic city',vat_period_type='quarter' WHERE id=1;").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let m = metadata(true);
    let a = register(&tx, &m);
    let forced = |hash: &str, path: &str| {
        crate::documents::register_original(
            &tx,
            crate::documents::Original {
                filename: "other.pdf",
                hash,
                path,
                mime: "application/pdf",
            },
            &m,
            "synthetic",
            "2026-10-18",
            None,
            crate::idempotency::PrincipalKind::ServiceAccount,
            true,
        )
        .unwrap()["document"]["id"]
            .as_i64()
            .unwrap()
    };
    let b = forced(&"b".repeat(64), "documents/originals/other.pdf");
    let c = forced(&"c".repeat(64), "documents/originals/null.pdf");
    // A physically distinct original may retain explicit null optional metadata.
    let mut nulls = serde_json::to_value(&m).unwrap();
    nulls["paymentDetails"] = Value::Null;
    tx.execute(
        "UPDATE documents SET payload_json=?1 WHERE id=?2",
        params![nulls.to_string(), c],
    )
    .unwrap();
    company_context(&tx, a, "A receipt", "Business use", "user:synthetic").unwrap();
    assert!(valid_context_for_document(&tx, a, &m).unwrap());
    assert!(!valid_context_for_document(&tx, b, &m).unwrap());
    assert!(!valid_context_for_document(&tx, c, &m).unwrap());
    company_context(&tx, b, "B receipt", "Business use", "user:synthetic").unwrap();
    assert!(valid_context_for_document(&tx, a, &m).unwrap());
    assert!(valid_context_for_document(&tx, b, &m).unwrap());
    assert!(!valid_context_for_document(&tx, c, &m).unwrap());
    company_context(&tx, c, "C receipt", "Business use", "user:synthetic").unwrap();
    assert!(valid_context_for_document(&tx, c, &m).unwrap());
}

#[test]
fn documents_original_enrichment_defaults_are_canonical_and_original_bytes_retained() {
    let (_dir, _control, mut db) = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let initial: Metadata =
        serde_json::from_value(json!({"source":"synthetic-original","currency":" dkk "})).unwrap();
    let id = register(&tx, &initial);
    let incoming: Metadata = serde_json::from_value(json!({"source":"synthetic-original","documentType":"purchase_sale","currency":"DKK","deliveryDescription":"Supplies"})).unwrap();
    enrich(&tx, id, &incoming, "user:synthetic").unwrap();
    let (raw, current): (String,String) = tx.query_row("SELECT original_payload_json,enriched_metadata_json FROM document_metadata_enrichments WHERE document_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!(raw, serde_json::to_string(&initial).unwrap());
    let value: Value = serde_json::from_str(&current).unwrap();
    assert_eq!(value["documentType"], "purchase_sale");
    assert_eq!(value["currency"], "DKK");
    validate_history(&tx).unwrap();
}

#[test]
fn documents_original_blocked_dedup_only_open_exceptions() {
    let (_dir, _control, mut db) = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let m = metadata(true);
    blocked(&tx, "synthetic.pdf", &"a".repeat(64), &m, "Refused").unwrap();
    blocked(&tx, "synthetic.pdf", &"a".repeat(64), &m, "Refused").unwrap();
    assert_eq!(
        tx.query_row("SELECT count(*) FROM exceptions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    tx.execute("UPDATE exceptions SET status='resolved',resolved_by='user:synthetic',resolved_at=CURRENT_TIMESTAMP,resolution_note='Reviewed'",[]).unwrap();
    blocked(&tx, "synthetic.pdf", &"a".repeat(64), &m, "Refused again").unwrap();
    assert_eq!(
        tx.query_row("SELECT count(*) FROM exceptions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
