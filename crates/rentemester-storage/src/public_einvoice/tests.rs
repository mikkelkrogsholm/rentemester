use super::*;
const NOW: &str = "2026-10-20T10:00:00.000Z";
fn fixture() -> (Connection, i64) {
    fixture_registration(true)
}
fn fixture_registration(with_registration: bool) -> (Connection, i64) {
    let mut c = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut db, "Synthetic Seller").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    crate::company_setup::update_profile(
        &tx,
        json!({"cvr":"DK12345678"}).as_object().unwrap(),
        "user:synthetic",
    )
    .unwrap();
    let p = json!({"invoiceType":"full","vatTreatment":"standard","status":"issued","invoiceNumber":"SYN-2026-1","issueDate":"2026-10-20","dueDate":"2026-11-03","currency":"DKK","seller":{"name":"Synthetic Seller","address":"Synthetic Street 1","vatOrCvr":"DK12345678"},"buyer":{"name":"Synthetic Buyer","address":"Synthetic Street 2","eanNumber":"5790000000000","publicRecipient":true},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100,"lineTotalExVat":100}],"totals":{"netAmount":100,"vatRate":0.25,"vatAmount":25,"grossAmount":125}});
    let raw = serde_json::to_string_pretty(&p).unwrap();
    let sha = hash(raw.as_bytes());
    let pdf = hash(b"%PDF synthetic");
    let registered = crate::sales::register(
        &tx,
        &p,
        crate::sales::Artifact {
            filename: "synthetic.json",
            path: &format!("invoices/issued/{sha}.json"),
            hash: &sha,
            mime: "application/json",
        },
        crate::sales::Artifact {
            filename: "synthetic.pdf",
            path: &format!("invoices/issued/{pdf}.pdf"),
            hash: &pdf,
            mime: "application/pdf",
        },
        "user:synthetic",
    )
    .unwrap();
    let id = registered["documentId"].as_i64().unwrap();
    let i = identity(&tx).unwrap();
    if with_registration {
        save_key(&tx, &i, "test", "synthetic-key", "user:synthetic", NOW).unwrap();
        save_outbound(
            &tx,
            &i,
            "test",
            "synthetic-key",
            true,
            false,
            "user:synthetic",
            NOW,
        )
        .unwrap();
    }
    tx.commit().unwrap();
    (db, id)
}
#[test]
fn reservation_repeat_and_terminal_rejection_never_redeliver_or_write_journal() {
    let (mut db, id) = fixture();
    let s = load_source(&db, id, "synthetic-key", "test").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let first = reserve(&tx, &s, "user:synthetic", NOW).unwrap();
    assert!(first.should_deliver);
    assert_eq!(first.result["status"], "uncertain");
    assert_eq!(first.result["duplicate"], false);
    let repeat = reserve(&tx, &s, "user:other", NOW).unwrap();
    assert!(!repeat.should_deliver);
    assert_eq!(repeat.id, first.id);
    let mut changed = s.clone();
    changed.company_key = "other-key".into();
    changed.environment = "production".into();
    assert!(
        !reserve(&tx, &changed, "user:other", NOW)
            .unwrap()
            .should_deliver
    );
    let rejected = record_outcome(
        &tx,
        first.id,
        Observed {
            document_id: Some("synthetic-remote".into()),
            status: "document-not-valid".into(),
        },
        "user:synthetic",
        NOW,
    )
    .unwrap();
    assert_eq!(rejected["status"], "failed");
    assert!(
        !reserve(&tx, &s, "user:synthetic", NOW)
            .unwrap()
            .should_deliver
    );
    assert_eq!(
        tx.query_row("SELECT count(*) FROM peppol_submissions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    validate_backup(&tx).unwrap();
    tx.commit().unwrap();
    assert_eq!(get(&db, id).unwrap().unwrap(), rejected);
    assert_eq!(accepted(&db, id).unwrap().unwrap().2, "synthetic-remote");
}
#[test]
fn pending_reopen_status_is_readonly_and_accepted_identity_cannot_change() {
    let (mut db, id) = fixture();
    let s = load_source(&db, id, "synthetic-key", "test").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let r = reserve(&tx, &s, "user:synthetic", NOW).unwrap();
    tx.commit().unwrap();
    let before = crate::test_support::typed_tables(&db);
    assert_eq!(get(&db, id).unwrap().unwrap()["status"], "uncertain");
    assert!(accepted(&db, id).unwrap().is_none());
    assert_eq!(crate::test_support::typed_tables(&db), before);
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    record_outcome(
        &tx,
        r.id,
        Observed {
            document_id: Some("remote-one".into()),
            status: "queued-for-delivery".into(),
        },
        "user:synthetic",
        NOW,
    )
    .unwrap();
    assert!(
        record_outcome(
            &tx,
            r.id,
            Observed {
                document_id: Some("remote-two".into()),
                status: "delivered".into()
            },
            "user:synthetic",
            NOW
        )
        .is_err()
    );
    let delivered = record_outcome(
        &tx,
        r.id,
        Observed {
            document_id: Some("remote-one".into()),
            status: "delivered".into(),
        },
        "user:synthetic",
        NOW,
    )
    .unwrap();
    assert_eq!(delivered["status"], "acknowledged");
    assert!(
        record_outcome(
            &tx,
            r.id,
            Observed {
                document_id: None,
                status: "delivery-uncertain".into()
            },
            "user:synthetic",
            NOW
        )
        .is_err()
    );
    tx.commit().unwrap();
    validate_backup(&db).unwrap();
}
#[test]
fn current_binding_refuses_profile_or_environment_change_historical_proof_survives() {
    let (mut db, id) = fixture();
    assert_eq!(
        load_bound(&db, "test").unwrap().as_deref(),
        Some("synthetic-key")
    );
    assert!(load_bound(&db, "production").is_err());
    let s = load_source(&db, id, "synthetic-key", "test").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let r = reserve(&tx, &s, "user:synthetic", NOW).unwrap();
    tx.commit().unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    crate::company_setup::update_profile(
        &tx,
        json!({"cvr":"DK87654321"}).as_object().unwrap(),
        "user:synthetic",
    )
    .unwrap();
    tx.commit().unwrap();
    assert!(load_bound(&db, "test").is_err());
    assert!(load_source(&db, id, "synthetic-key", "test").is_err());
    validate_backup(&db).unwrap();
    assert_eq!(get(&db, id).unwrap().unwrap()["id"], r.id);
}
#[test]
fn invalid_source_does_not_reserve_and_history_tamper_refuses() {
    let (mut db, id) = fixture();
    let mut s = load_source(&db, id, "synthetic-key", "test").unwrap();
    s.oioubl_xml.push('x');
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    assert!(reserve(&tx, &s, "user:synthetic", NOW).is_err());
    assert_eq!(
        tx.query_row("SELECT count(*) FROM peppol_submissions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    tx.rollback().unwrap();
    let s = load_source(&db, id, "synthetic-key", "test").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    reserve(&tx, &s, "user:synthetic", NOW).unwrap();
    tx.commit().unwrap();
    db.execute_batch("DROP TRIGGER peppol_submission_events_no_update;UPDATE peppol_submission_events SET status='delivered'").unwrap();
    assert!(validate_backup(&db).is_err());
    assert!(get(&db, id).is_err());
}
#[test]
fn registration_is_current_profile_bound_and_idempotent() {
    let (mut db, _) = fixture();
    let i = identity(&db).unwrap();
    let before = crate::test_support::typed_tables(&db);
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    save_key(&tx, &i, "test", "synthetic-key", "user:other", NOW).unwrap();
    save_outbound(
        &tx,
        &i,
        "test",
        "synthetic-key",
        true,
        false,
        "user:other",
        NOW,
    )
    .unwrap();
    tx.commit().unwrap();
    assert_eq!(before, crate::test_support::typed_tables(&db));
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    assert!(save_key(&tx, &i, "test", "another-key", "user:synthetic", NOW).is_err());
    assert!(
        save_outbound(
            &tx,
            &i,
            "test",
            "synthetic-key",
            false,
            false,
            "user:synthetic",
            NOW
        )
        .is_err()
    );
    assert!(
        save_outbound(
            &tx,
            &i,
            "test",
            "synthetic-key",
            true,
            true,
            "user:synthetic",
            NOW
        )
        .is_err()
    );
}
#[test]
fn simultaneous_writers_reserve_exactly_one_delivery() {
    let (db, id) = fixture();
    let source = load_source(&db, id, "synthetic-key", "test").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("company.sqlite");
    db.execute("VACUUM INTO ?", [path.to_str().unwrap()])
        .unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles = (0..2)
        .map(|_| {
            let b = barrier.clone();
            let path = path.clone();
            let source = source.clone();
            std::thread::spawn(move || {
                let mut db = Connection::open(path).unwrap();
                db.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
                b.wait();
                let tx = db
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                    .unwrap();
                let r = reserve(&tx, &source, "user:synthetic", NOW).unwrap();
                tx.commit().unwrap();
                r.should_deliver
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        handles
            .into_iter()
            .map(|h| usize::from(h.join().unwrap()))
            .sum::<usize>(),
        1
    );
    let db = Connection::open(path).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM peppol_submissions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    validate_backup(&db).unwrap();
}
#[test]
fn legacy_unknown_blocks_new_native_submission_without_reinterpretation() {
    let (mut db, id) = fixture();
    let source = load_source(&db, id, "synthetic-key", "test").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let envelope = "<LegacySubmission/>";
    tx.execute("INSERT INTO peppol_submissions(invoice_document_id,invoice_no,idempotency_key,submission_reference,access_point_id,receiver_endpoint_id,oioubl_sha256,envelope_sha256,envelope_xml,status)VALUES(?,?,'legacy-key','legacy-reference','legacy-ap',?,?,?,?,'prepared')",params![id,source.invoice_number,source.receiver_endpoint_id,source.oioubl_sha256,hash(envelope.as_bytes()),envelope]).unwrap();
    let r = reserve(&tx, &source, "user:synthetic", NOW).unwrap();
    assert!(!r.should_deliver);
    assert_eq!(r.result["legacy"], true);
    assert!(accepted(&tx, id).unwrap().is_none());
    assert_eq!(
        tx.query_row("SELECT count(*) FROM peppol_submission_events", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
    validate_backup(&tx).unwrap();
}

#[test]
fn verified_legacy_registration_is_attested_without_rewriting_old_rows() {
    let (mut db, _) = fixture_registration(false);
    let i = identity(&db).unwrap();
    db.execute("INSERT INTO digisense_companies(company_key,company_type,participant_id,company_name,registered_at)VALUES('synthetic-key','DK:CVR','DK12345678','Synthetic Seller','2020-01-01 12:00:00')",[]).unwrap();
    db.execute("INSERT INTO digisense_participants(company_key,network,direction,participant_type,participant_id,webhook_url,registered_on_network,webhook_registered,registered_at)VALUES('synthetic-key','nemhandel','outbound','DK:CVR','DK12345678',NULL,1,0,'2020-01-02 12:00:00')",[]).unwrap();
    assert!(
        matches!(registration::registration(&db,"test").unwrap(),registration::Registration::Unverified{company_key} if company_key=="synthetic-key")
    );
    assert!(load_bound(&db, "test").is_err());
    let original = |db: &Connection| {
        let mut q=db.prepare("SELECT company_key,company_type,participant_id,company_name,registered_at FROM digisense_companies").unwrap();
        q.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    };
    let before = original(&db);
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    save_key(&tx, &i, "test", "synthetic-key", "user:verified-owner", NOW).unwrap();
    save_outbound(
        &tx,
        &i,
        "test",
        "synthetic-key",
        true,
        false,
        "user:verified-owner",
        NOW,
    )
    .unwrap();
    tx.commit().unwrap();
    assert_eq!(before, original(&db));
    assert_eq!(
        db.query_row(
            "SELECT registered_at FROM digisense_participants",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "2020-01-02 12:00:00"
    );
    validate_backup(&db).unwrap();
    assert!(outbound_registered(&db, "test", "synthetic-key").unwrap());
    assert_eq!(db.query_row("SELECT count(*) FROM audit_log WHERE entity_type='native_public_einvoice_registration_v1' AND message LIKE '%\"version\":2%'",[],|r|r.get::<_,i64>(0)).unwrap(),2);
}
#[test]
fn unverified_conflict_and_corrupt_native_proof_never_become_missing() {
    let (mut db, _) = fixture_registration(false);
    let i = identity(&db).unwrap();
    db.execute("INSERT INTO digisense_companies(company_key,company_type,participant_id,company_name)VALUES('old-key','DK:CVR','DK12345678','Synthetic Seller')",[]).unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    assert!(save_key(&tx, &i, "test", "different-key", "user:owner", NOW).is_err());
    save_key(&tx, &i, "test", "old-key", "user:owner", NOW).unwrap();
    tx.commit().unwrap();
    db.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('native_public_einvoice_company','native_public_einvoice_registration_v1','registration','{}','user:corrupt')",[]).unwrap();
    assert!(registration::registration(&db, "test").is_err());
    assert!(load_bound(&db, "test").is_err());
}

fn access_point(name: &str) -> rentemester_engine::peppol_handoff::AccessPoint {
    rentemester_engine::peppol_handoff::AccessPoint {
        access_point_id: name.into(),
        endpoint_url: "metadata:never-fetch?x=1&y=2".into(),
        sender_endpoint_id: " synthetic sender ".into(),
    }
}
fn manual_ack() -> rentemester_engine::peppol_handoff::Acknowledgement {
    rentemester_engine::peppol_handoff::Acknowledgement {
        transmission_id: "manual-only-receipt".into(),
        acknowledged_at: "2026-10-22T00:00:00Z".into(),
    }
}
#[test]
fn original_preparation_without_provider_is_exact_duplicate_and_never_upgrades_ack() {
    let (mut db, id) = fixture_registration(false);
    let tx = db.transaction().unwrap();
    let original = prepare_original(
        &tx,
        id,
        &access_point(" synthetic "),
        None,
        "user:synthetic",
        NOW,
    )
    .unwrap();
    assert_eq!(original["duplicate"], false);
    assert_eq!(original["status"], "prepared");
    assert_eq!(original["redeliveryBlocked"], false);
    let mut changed = access_point("synthetic");
    changed.endpoint_url = "metadata:changed".into();
    let duplicate =
        prepare_original(&tx, id, &changed, Some(&manual_ack()), "user:other", NOW).unwrap();
    assert_eq!(duplicate["duplicate"], true);
    assert_eq!(duplicate["status"], "prepared");
    assert_eq!(duplicate["envelope"], original["envelope"]);
    assert_eq!(duplicate["envelopeSha256"], original["envelopeSha256"]);
    assert!(get(&tx, id).unwrap().is_none());
    assert!(accepted(&tx, id).unwrap().is_none());
    assert_eq!(
        tx.query_row("SELECT count(*) FROM peppol_submissions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        tx.query_row("SELECT count(*) FROM peppol_submission_events", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
    validate_backup(&tx).unwrap();
}
#[test]
fn safe_preparation_then_native_delivery_uses_native_authority_and_key() {
    let (mut db, id) = fixture();
    let source = load_source(&db, id, "synthetic-key", "test").unwrap();
    let tx = db.transaction().unwrap();
    let prepared = prepare_original(
        &tx,
        id,
        &access_point("synthetic"),
        None,
        "user:synthetic",
        NOW,
    )
    .unwrap();
    let reserved = reserve(&tx, &source, "user:synthetic", NOW).unwrap();
    assert!(reserved.should_deliver);
    assert_ne!(
        prepared["idempotencyKey"],
        reserved.result["idempotencyKey"]
    );
    let delivered = record_outcome(
        &tx,
        reserved.id,
        Observed {
            document_id: Some("provider-id".into()),
            status: "delivered".into(),
        },
        "user:synthetic",
        NOW,
    )
    .unwrap();
    assert_eq!(get(&tx, id).unwrap().unwrap(), delivered);
    assert_eq!(accepted(&tx, id).unwrap().unwrap().2, "provider-id");
    assert!(
        !reserve(&tx, &source, "user:synthetic", NOW)
            .unwrap()
            .should_deliver
    );
    assert_eq!(
        rows(&tx).unwrap()["submissions"].as_array().unwrap().len(),
        2
    );
    assert_eq!(rows(&tx).unwrap()["submissions"][0]["id"], reserved.id);
    validate_backup(&tx).unwrap();
}
#[test]
fn manual_ack_blocks_native_even_after_configuration_change_and_is_not_provider_evidence() {
    let (mut db, id) = fixture();
    let mut source = load_source(&db, id, "synthetic-key", "test").unwrap();
    let tx = db.transaction().unwrap();
    let manual = prepare_original(
        &tx,
        id,
        &access_point("synthetic"),
        Some(&manual_ack()),
        "user:synthetic",
        NOW,
    )
    .unwrap();
    assert_eq!(manual["status"], "acknowledged");
    assert_eq!(manual["transmissionId"], "manual-only-receipt");
    assert_eq!(manual["redeliveryBlocked"], true);
    source.company_key = "changed-key".into();
    source.environment = "production".into();
    assert!(
        !reserve(&tx, &source, "user:synthetic", NOW)
            .unwrap()
            .should_deliver
    );
    assert!(accepted(&tx, id).unwrap().is_none());
    assert_eq!(
        tx.query_row("SELECT count(*) FROM peppol_submission_events", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
    validate_backup(&tx).unwrap();
}
#[test]
fn reverse_reservation_race_denies_new_manual_ack_and_backup_detects_provenance_tamper() {
    let (mut db, id) = fixture();
    let source = load_source(&db, id, "synthetic-key", "test").unwrap();
    let tx = db.transaction().unwrap();
    let p = prepare_original(
        &tx,
        id,
        &access_point("synthetic"),
        None,
        "user:synthetic",
        NOW,
    )
    .unwrap();
    reserve(&tx, &source, "user:synthetic", NOW).unwrap();
    assert!(
        prepare_original(
            &tx,
            id,
            &access_point("different-key"),
            Some(&manual_ack()),
            "user:synthetic",
            NOW
        )
        .is_err()
    );
    assert_eq!(get(&tx, id).unwrap().unwrap()["legacy"], false);
    tx.execute_batch(
        "DROP TRIGGER IF EXISTS audit_log_no_update; DROP TRIGGER IF EXISTS audit_log_no_delete;",
    )
    .unwrap();
    let audit: String = tx
        .query_row(
            "SELECT message FROM audit_log WHERE event_type='original_public_einvoice_handoff'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    // A canonical but fabricated actor still cannot match the row-bound provenance.
    let mut e: Value = serde_json::from_str(&audit).unwrap();
    e["rowSha256"] = json!("00");
    let triggers: Vec<String> = {
        let mut q = tx
            .prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name='audit_log'")
            .unwrap();
        q.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    for trigger in triggers {
        tx.execute_batch(&format!(
            "DROP TRIGGER \"{}\"",
            trigger.replace('"', "\"\"")
        ))
        .unwrap();
    }
    tx.execute(
        "UPDATE audit_log SET message=? WHERE event_type='original_public_einvoice_handoff'",
        [crate::workspace_inbox::canonical(&e).unwrap()],
    )
    .unwrap();
    assert!(validate_backup(&tx).is_err());
    assert!(get(&tx, p["invoiceDocumentId"].as_i64().unwrap()).is_err());
}

#[test]
fn restart_retains_manual_ack_block_after_earlier_safe_preparation() {
    let (mut db, id) = fixture();
    let source = load_source(&db, id, "synthetic-key", "test").unwrap();
    let tx = db.transaction().unwrap();
    prepare_original(
        &tx,
        id,
        &access_point("first-artifact"),
        None,
        "user:synthetic",
        NOW,
    )
    .unwrap();
    let ack = prepare_original(
        &tx,
        id,
        &access_point("second-attestation"),
        Some(&manual_ack()),
        "user:synthetic",
        NOW,
    )
    .unwrap();
    tx.commit().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("restored.sqlite");
    db.execute("VACUUM INTO ?", [path.to_str().unwrap()])
        .unwrap();
    drop(db);
    let mut restored = Connection::open(path).unwrap();
    validate_backup(&restored).unwrap();
    assert_eq!(rows(&restored).unwrap()["submissions"][0]["id"], ack["id"]);
    assert_eq!(get(&restored, id).unwrap().unwrap()["id"], ack["id"]);
    assert!(accepted(&restored, id).unwrap().is_none());
    let tx = restored
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    assert!(
        !reserve(&tx, &source, "user:synthetic", NOW)
            .unwrap()
            .should_deliver
    );
}
