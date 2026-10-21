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
