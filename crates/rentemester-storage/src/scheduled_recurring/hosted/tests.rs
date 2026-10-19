use super::*;
#[test]
fn v3_typed_principal_and_consent_are_backup_validated_without_changing_extension1_or_v1_bytes() {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic hosted proof").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET address='Synthetic1',cvr='87654321',vat_period_type='quarter' WHERE id=1").unwrap();
    let customer = crate::sales::create_customer(
        &tx,
        &serde_json::from_value(
            json!({"name":"Synthetic proof buyer","address":"Synthetic2","vatOrCvr":"DK12345678"}),
        )
        .unwrap(),
        "human",
    )
    .unwrap()["id"]
        .as_i64()
        .unwrap();
    let i=serde_json::from_value(json!({"name":"Synthetic proof","interval":"weekly","intervalCount":1,"firstIssueDate":"2026-01-01","paymentTermsDays":14,"vatRatePercent":25,"currency":"DKK","customerId":customer,"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100}],"confirm":true})).unwrap();
    let id = recurring::create(&tx, &i, "human", "2026-01-01T00:00:00.000Z").unwrap()["templateId"]
        .as_i64()
        .unwrap();
    let p = super::super::plan(&tx, id, "2026-01-01").unwrap();
    super::super::enable(
        &tx,
        &serde_json::from_value(
            json!({"templateId":id,"templateHash":p["templateHash"],"confirm":true}),
        )
        .unwrap(),
        "human",
        "2026-01-01",
        "2026-01-01T00:00:00.000Z",
    )
    .unwrap();
    let original: String = tx
        .query_row(
            "SELECT canonical_payload FROM native_scheduled_recurring_events WHERE id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!original.contains("hosted"));
    let binding = Binding {
        service_account_id: "synthetic-service".into(),
        credential_id: "synthetic-credential".into(),
        grantor_user_id: "human".into(),
        authority_hash: "a".repeat(64),
        policy: "normal_due_only".into(),
    };
    let p = plan(&tx, id, &binding, "Synthetic executor", "2026-01-01").unwrap();
    consent(&tx,&serde_json::from_value(json!({"templateId":id,"serviceAccountId":binding.service_account_id,"credentialId":binding.credential_id,"planHash":p["planHash"],"reason":"Explicit source proof review","confirm":true})).unwrap(),&binding,"Synthetic executor","human","2026-01-01","2026-01-01T00:00:00.000Z").unwrap();
    tx.commit().unwrap();
    super::super::validate_backup(&db).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT canonical_payload FROM native_scheduled_recurring_events WHERE id=1",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        original
    );
    assert!(schema::installed(&db).unwrap());
    let mut event: Payload = serde_json::from_str(
        &db.query_row(
            "SELECT canonical_payload FROM native_scheduled_recurring_events WHERE id=2",
            [],
            |r| r.get::<_, String>(0),
        )
        .unwrap(),
    )
    .unwrap();
    event.hosted.as_mut().unwrap().principal_kind = "service_account".into();
    let canonical = serde_json::to_string(&event).unwrap();
    let previous: String = db
        .query_row(
            "SELECT previous_hash FROM native_scheduled_recurring_events WHERE id=2",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let hash = chain_hash(&previous, &canonical);
    db.execute_batch("DROP TRIGGER native_scheduled_events_no_update")
        .unwrap();
    db.execute("UPDATE native_scheduled_recurring_events SET canonical_payload=?1,event_hash=?2 WHERE id=2",params![canonical,hash]).unwrap();
    db.execute_batch("CREATE TRIGGER native_scheduled_events_no_update BEFORE UPDATE ON native_scheduled_recurring_events BEGIN SELECT RAISE(ABORT,'scheduled evidence is append-only'); END;").unwrap();
    assert!(schema::installed(&db).unwrap());
    assert!(
        super::super::validate_backup(&db).is_err(),
        "even a recomputed valid chain cannot mislabel human consent as a service action"
    );
}
