use super::*;
const CREATED: &str = "2026-01-01T00:00:00.000Z";
const CHECKED: &str = "2026-01-01T00:10:00.000Z";
fn connection() -> rusqlite::Connection {
    let mut control = rusqlite::Connection::open_in_memory().unwrap();
    let mut company = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic governance")
        .unwrap();
    control
}
fn placement() -> Value {
    json!({"format":"rentemester-native-backup-placement/1","destinationId":"synthetic-drive","operationId":"synthetic-placement","archiveSha256":"a".repeat(64),"archiveSizeBytes":123,"backupCreatedAt":CREATED,"verified":true,"verifyMethod":"sha256-reread","signatureVerified":true,"remoteSyncVerified":false,"destination":{"id":"synthetic-drive"},"placedBy":"user:synthetic","placedAt":"2026-01-01T00:01:00.000Z"})
}
fn remote() -> Receipt {
    Receipt {
        format: backup_remote::FORMAT.into(),
        destination_id: "synthetic-drive".into(),
        operation_id: "synthetic-verification".into(),
        placement_operation_id: "synthetic-placement".into(),
        remote_object_id: "Synthetic_Object".into(),
        archive_sha256: "a".repeat(64),
        archive_size_bytes: 123,
        backup_created_at: CREATED.into(),
        provider: "google-drive".into(),
        remote_file_name: "synthetic-placement.native-backup.json".into(),
        remote_parent_id: "Synthetic_Folder".into(),
        observed_at: "2026-01-01T00:02:00.000Z".into(),
        verified_at: "2026-01-01T00:03:00.000Z".into(),
        verified_by: "user:synthetic".into(),
        remote_sync_verified: true,
        verify_method: "google-drive-metadata-and-sha256-reread".into(),
        signature_verified: true,
        attestation_independently_verified: false,
    }
}
fn append(tx: &Transaction<'_>, event: &str, kind: &str, value: &Value) {
    tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES(?1,?2,?3,'user:synthetic')",rusqlite::params![event,kind,value.to_string()]).unwrap();
}
fn put_placement(tx: &Transaction<'_>, value: &Value) {
    append(tx, "native_backup_placed", "native_backup_placement", value);
}
fn read(tx: &Transaction<'_>, hash: &str) -> Result<Evidence, Error> {
    matching(tx, &["synthetic-drive".into()], hash, 123, CREATED, CHECKED)
}
#[test]
fn same_archive_join_preserves_observation_and_new_backup_cannot_borrow_old_receipt() {
    let mut c = connection();
    let tx = c.transaction().unwrap();
    put_placement(&tx, &placement());
    let receipt = remote();
    backup_remote::append(&tx, &receipt, &receipt.verified_by).unwrap();
    let before = tx.total_changes();
    let evidence = read(&tx, &"a".repeat(64)).unwrap();
    assert_eq!(evidence.placements.len(), 1);
    assert_eq!(evidence.remote_verifications, vec![receipt]);
    assert_eq!(tx.total_changes(), before);
    let newer = read(&tx, &"b".repeat(64)).unwrap();
    assert!(newer.placements.is_empty() && newer.remote_verifications.is_empty());
    tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('native_portable_restore','backup','synthetic','system:restore')",[]).unwrap();
    assert!(
        read(&tx, &"a".repeat(64))
            .unwrap()
            .remote_verifications
            .is_empty()
    );
}
#[test]
fn unknown_destinations_future_facts_and_other_hashes_are_ignored() {
    let mut c = connection();
    let tx = c.transaction().unwrap();
    let mut unknown = placement();
    unknown["destinationId"] = json!("unconfigured-drive");
    unknown["format"] = json!("not-supported");
    put_placement(&tx, &unknown);
    let mut future = placement();
    future["placedAt"] = json!("2026-01-02T00:00:00.000Z");
    put_placement(&tx, &future);
    let mut other = placement();
    other["archiveSha256"] = json!("b".repeat(64));
    put_placement(&tx, &other);
    let evidence = read(&tx, &"a".repeat(64)).unwrap();
    assert!(evidence.placements.is_empty());
    let mut receipt = remote();
    receipt.observed_at = "2026-01-02T00:00:00.000Z".into();
    receipt.verified_at = receipt.observed_at.clone();
    backup_remote::append(&tx, &receipt, &receipt.verified_by).unwrap();
    assert!(
        read(&tx, &"a".repeat(64))
            .unwrap()
            .remote_verifications
            .is_empty()
    );
}
#[test]
fn loose_remote_metadata_malformed_matching_records_and_mismatched_size_fail_closed() {
    let mut c = connection();
    let tx = c.transaction().unwrap();
    let receipt = remote();
    backup_remote::append(&tx, &receipt, &receipt.verified_by).unwrap();
    assert!(read(&tx, &"a".repeat(64)).is_err());
    put_placement(&tx, &placement());
    assert_eq!(
        read(&tx, &"a".repeat(64))
            .unwrap()
            .remote_verifications
            .len(),
        1
    );
    let mut wrong = placement();
    wrong["archiveSizeBytes"] = json!(124);
    put_placement(&tx, &wrong);
    assert!(read(&tx, &"a".repeat(64)).is_err());
    let mut c = connection();
    let tx = c.transaction().unwrap();
    let mut malformed = placement();
    malformed["signatureVerified"] = json!(false);
    put_placement(&tx, &malformed);
    assert!(read(&tx, &"a".repeat(64)).is_err());
}
