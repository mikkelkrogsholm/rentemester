use super::*;
fn connection() -> rusqlite::Connection {
    let mut c = rusqlite::Connection::open_in_memory().unwrap();
    let mut l = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic registrations").unwrap();
    c
}
fn add(kind: &str, index: usize) -> Request {
    serde_json::from_value(json!({"action":"add","operationId":format!("synthetic-add-{index}"),"destinationId":format!("synthetic-target-{index}"),"label":" Synthetic declared target ","kind":kind,"location":"inert://owner-description/never-opened","attestedBy":"Synthetic operator","inEeaOrEu":false,"nonRelatedParty":true,"regionNote":"","confirm":true})).unwrap()
}
const AT: &str = "2026-01-01T00:10:00.000Z";
#[test]
fn all_five_kinds_canonical_replay_scope_conflict_and_inert_location() {
    let mut c = connection();
    let tx = c.transaction().unwrap();
    for (index, kind) in ["local-folder", "dropbox", "google-drive", "ssh", "other"]
        .iter()
        .enumerate()
    {
        let request = add(kind, index);
        let value = apply(&tx, "a", &request, "user:u", AT, None).unwrap();
        assert_eq!(value.result["kind"], *kind);
        assert_eq!(value.result["label"], "Synthetic declared target");
        let changes = tx.total_changes();
        assert_eq!(
            apply(&tx, "a", &request, "user:u", AT, None).unwrap(),
            value
        );
        assert_eq!(tx.total_changes(), changes);
        assert!(apply(&tx, "b", &request, "user:u", AT, None).is_err());
    }
    assert_eq!(list(&tx, "a").unwrap().0.len(), 5);
    assert!(list(&tx, "b").unwrap().0.is_empty());
}
#[test]
fn tombstones_keep_declarations_and_receipts_through_restore_without_becoming_verified() {
    let mut c = connection();
    let tx = c.transaction().unwrap();
    let a = add("ssh", 1);
    apply(&tx, "a", &a, "user:u", AT, None).unwrap();
    let d = Request::Declare {
        operation_id: "synthetic-declaration".into(),
        destination_id: a.destination_id().into(),
        backup_id: "a".repeat(64),
        note: Some("Synthetic manual transfer assertion".into()),
        confirm: true,
    };
    let backup = ProtectedBackup {
        backup_id: "a".repeat(64),
        archive_sha256: "a".repeat(64),
        archive_size_bytes: 123,
        backup_created_at: "2026-01-01T00:00:00.000Z".into(),
    };
    let declared = apply(&tx, "a", &d, "user:u", AT, Some(&backup)).unwrap();
    assert_eq!(declared.result["verified"], false);
    assert_eq!(declared.result["remoteSyncVerified"], false);
    assert_eq!(declared.result["verifyMethod"], "declared");
    let remove = Request::Remove {
        operation_id: "synthetic-removal".into(),
        destination_id: a.destination_id().into(),
        confirm: true,
    };
    let removed = apply(&tx, "a", &remove, "user:u", AT, None).unwrap();
    assert!(list(&tx, "a").unwrap().0.is_empty());
    assert_eq!(list(&tx, "a").unwrap().1.len(), 3);
    assert_eq!(
        apply(&tx, "a", &remove, "user:u", AT, None).unwrap(),
        removed
    );
    assert_eq!(apply(&tx, "a", &d, "user:u", AT, None).unwrap(), declared);
    let newremove = Request::Remove {
        operation_id: "synthetic-another-removal".into(),
        destination_id: remove.destination_id().into(),
        confirm: true,
    };
    assert!(apply(&tx, "a", &newremove, "user:u", AT, None).is_err());
    let mut reused = serde_json::to_value(a).unwrap();
    reused["operationId"] = json!("synthetic-reuse");
    assert!(
        apply(
            &tx,
            "a",
            &serde_json::from_value(reused).unwrap(),
            "user:u",
            AT,
            None
        )
        .is_err()
    );
    tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('native_portable_restore','backup','synthetic','system:restore')",[]).unwrap();
    assert_eq!(inspect(&tx, "a", d.operation_id()).unwrap(), Some(declared));
    assert_eq!(list(&tx, "a").unwrap().1.len(), 3);
    assert!(
        crate::backup_governance::matching(
            &tx,
            &["synthetic-target-1".into()],
            &"a".repeat(64),
            123,
            "2026-01-01T00:00:00.000Z",
            AT
        )
        .unwrap()
        .remote_verifications
        .is_empty()
    );
}
#[test]
fn unknown_fields_control_characters_missing_explicit_flags_and_forged_facts_refuse() {
    let mut value = serde_json::to_value(add("ssh", 1)).unwrap();
    value["actor"] = json!("user:forged");
    assert!(serde_json::from_value::<Request>(value).is_err());
    let mut value = serde_json::to_value(add("ssh", 1)).unwrap();
    value.as_object_mut().unwrap().remove("nonRelatedParty");
    assert!(serde_json::from_value::<Request>(value).is_err());
    for (field, value) in [
        ("label", json!("\nunsafe")),
        ("operatorTargetId", json!("synthetic-operator")),
        ("confirm", json!(false)),
    ] {
        let mut input = serde_json::to_value(add("ssh", 1)).unwrap();
        input[field] = value;
        assert!(
            serde_json::from_value::<Request>(input)
                .unwrap()
                .canonical()
                .is_err()
        );
    }
    let mut input = serde_json::to_value(add("other", 1)).unwrap();
    input["kind"] = json!("sftp-execute");
    assert!(
        serde_json::from_value::<Request>(input)
            .unwrap()
            .canonical()
            .is_err()
    );
}

fn portable_history_source() -> (rusqlite::Connection, Vec<Receipt>) {
    let mut source = connection();
    source.execute("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt)VALUES('old-owner','Synthetic historical','old@example.invalid',1,'2026-01-01','2026-01-01')",[]).unwrap();
    let tx = source.transaction().unwrap();
    let registered = add("ssh", 17);
    let first = apply(&tx, "a", &registered, "user:old-owner", AT, None).unwrap();
    let request = Request::Declare {
        operation_id: "synthetic-portable-declare".into(),
        destination_id: registered.destination_id().into(),
        backup_id: "a".repeat(64),
        note: Some("Historical unverified transfer declaration".into()),
        confirm: true,
    };
    let backup = ProtectedBackup {
        backup_id: "a".repeat(64),
        archive_sha256: "a".repeat(64),
        archive_size_bytes: 123,
        backup_created_at: "2026-01-01T00:00:00.000Z".into(),
    };
    let second = apply(&tx, "a", &request, "user:old-owner", AT, Some(&backup)).unwrap();
    let removed = Request::Remove {
        operation_id: "synthetic-portable-remove".into(),
        destination_id: registered.destination_id().into(),
        confirm: true,
    };
    let third = apply(&tx, "a", &removed, "user:old-owner", AT, None).unwrap();
    tx.commit().unwrap();
    (source, vec![first, second, third])
}
#[test]
fn portable_codec_restores_exact_add_declare_remove_receipts_without_authentication_authority() {
    let (mut source, expected) = portable_history_source();
    let tx = source.transaction().unwrap();
    let exported = crate::portable_backup::export_control(&tx).unwrap();
    tx.rollback().unwrap();
    let mut restored = rusqlite::Connection::open_in_memory().unwrap();
    crate::portable_backup::initialize_control(&mut restored, &exported.bytes, &"b".repeat(64))
        .unwrap();
    for table in [
        "user",
        "session",
        "account",
        "twoFactor",
        "rm_workspace_user_access_events",
        "rm_company_membership_events",
        "apikey",
    ] {
        assert_eq!(
            restored
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    let tx = restored.transaction().unwrap();
    validate_history(&tx).unwrap();
    let (destinations, history) = list(&tx, "a").unwrap();
    assert!(destinations.is_empty());
    assert_eq!(history, expected);
    for original in &expected {
        assert_eq!(
            inspect(&tx, "a", &original.operation_id).unwrap().as_ref(),
            Some(original)
        );
    }
    assert_eq!(history[1].result["verified"], false);
    assert_eq!(history[1].result["remoteSyncVerified"], false);
    assert_eq!(
        tx.query_row(
            "SELECT count(*) FROM workspace_audit WHERE event_type='native_portable_restore'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
#[test]
fn portable_export_refuses_a_malformed_known_registration_event() {
    let mut source = connection();
    source.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('native_backup_registration','backup_registration_operation','{}','user:synthetic')",[]).unwrap();
    let tx = source.transaction().unwrap();
    assert!(crate::portable_backup::export_control(&tx).is_err());
}
#[test]
fn forged_registration_archive_rolls_back_all_imported_audit_and_restore_boundary() {
    let (mut source, _) = portable_history_source();
    let tx = source.transaction().unwrap();
    let exported = crate::portable_backup::export_control(&tx).unwrap();
    tx.rollback().unwrap();
    let mut codec: Value = serde_json::from_slice(&exported.bytes).unwrap();
    let audit = &mut codec["tables"]["workspace_audit"];
    let column = audit["columns"]
        .as_array()
        .unwrap()
        .iter()
        .position(|v| v == "entity_id")
        .unwrap();
    let row = &mut audit["rows"][0][column];
    let mut event: Value = serde_json::from_str(row["value"].as_str().unwrap()).unwrap();
    // Preserve a valid transport codec and tamper the registration's immutable receipt.
    event["receipt"]["result"]["location"] = json!("forged inert destination");
    row["value"] = json!(event.to_string());
    let mut restored = rusqlite::Connection::open_in_memory().unwrap();
    assert!(
        crate::portable_backup::initialize_control(
            &mut restored,
            &serde_json::to_vec(&codec).unwrap(),
            &"c".repeat(64)
        )
        .is_err()
    );
    assert!(restored.is_autocommit());
    assert_eq!(
        restored
            .query_row("SELECT count(*) FROM workspace_audit", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn legacy_portable_audit_without_registrations_still_restores() {
    let mut source = connection();
    source.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('synthetic_legacy_event','company','a','user:historical')",[]).unwrap();
    let tx = source.transaction().unwrap();
    let exported = crate::portable_backup::export_control(&tx).unwrap();
    tx.rollback().unwrap();
    let mut restored = rusqlite::Connection::open_in_memory().unwrap();
    crate::portable_backup::initialize_control(&mut restored, &exported.bytes, &"d".repeat(64))
        .unwrap();
    let tx = restored.transaction().unwrap();
    validate_history(&tx).unwrap();
    assert!(list(&tx, "a").unwrap().1.is_empty());
    assert_eq!(
        tx.query_row(
            "SELECT count(*) FROM workspace_audit WHERE event_type='synthetic_legacy_event'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
