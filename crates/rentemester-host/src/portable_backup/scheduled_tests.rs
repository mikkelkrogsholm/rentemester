//! Synthetic signed SQLite fixtures: scheduler rows stay in the original ledger image.
use super::*;
use rentemester_storage::{recurring_invoices as recurring, scheduled_recurring as scheduled};
use serde_json::json;
const TODAY: &str = "2026-01-01";
const AT: &str = "2026-01-01T00:00:00.000Z";
fn queued(db: &mut Connection) -> i64 {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET address='Synthetic1',cvr='87654321',vat_period_type='quarter' WHERE id=1").unwrap();
    let customer = rentemester_storage::sales::create_customer(
        &tx,
        &serde_json::from_value(
            json!({"name":"Synthetic backup buyer","address":"Synthetic2","vatOrCvr":"DK12345678"}),
        )
        .unwrap(),
        "historical-owner",
    )
    .unwrap()["id"]
        .as_i64()
        .unwrap();
    let input = serde_json::from_value(json!({"name":"Synthetic backup schedule","interval":"weekly","intervalCount":1,"deliveryChannel":"manual","firstIssueDate":"2025-12-18","paymentTermsDays":14,"vatRatePercent":25,"currency":"DKK","customerId":customer,"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100}],"confirm":true})).unwrap();
    let id = recurring::create(&tx, &input, "historical-owner", AT).unwrap()["templateId"]
        .as_i64()
        .unwrap();
    let p = scheduled::plan(&tx, id, TODAY).unwrap();
    scheduled::enable(
        &tx,
        &serde_json::from_value(
            json!({"templateId":id,"templateHash":p["templateHash"],"confirm":true}),
        )
        .unwrap(),
        "historical-owner",
        TODAY,
        AT,
    )
    .unwrap();
    let p = scheduled::plan(&tx, id, TODAY).unwrap();
    scheduled::select(
        &tx,
        id,
        p["planHash"].as_str().unwrap(),
        &[0],
        None,
        true,
        "historical-owner",
        TODAY,
        AT,
    )
    .unwrap();
    let p = scheduled::plan(&tx, id, TODAY).unwrap();
    scheduled::select(
        &tx,
        id,
        p["planHash"].as_str().unwrap(),
        &[1],
        Some("Reviewed synthetic skip"),
        true,
        "historical-owner",
        TODAY,
        AT,
    )
    .unwrap();
    tx.commit().unwrap();
    id
}
fn ledger(archive: &artifact::VerifiedArtifact) -> Connection {
    let bytes = archive
        .entries()
        .iter()
        .find(|e| matches!(e.kind(), artifact::EntryKind::CompanyLedger { .. }))
        .unwrap()
        .bytes();
    let mut db = Connection::open_in_memory().unwrap();
    db.deserialize_read_exact(MAIN_DB, bytes, bytes.len(), false)
        .unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA trusted_schema=OFF")
        .unwrap();
    db
}
fn signed_ledger(old: &artifact::VerifiedArtifact, db: &Connection) -> artifact::VerifiedArtifact {
    let entries = old
        .entries()
        .iter()
        .map(|e| artifact::Snapshot {
            kind: e.kind().clone(),
            path: e.path().into(),
            bytes: if matches!(e.kind(), artifact::EntryKind::CompanyLedger { .. }) {
                db.serialize(MAIN_DB).unwrap().to_vec()
            } else {
                e.bytes().to_vec()
            },
        })
        .collect();
    // Published RFC8032 test vector; never operator signing material.
    let hex = concat!(
        "3051020101300506032b657004220420",
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        "812100",
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
    );
    let der = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let key = artifact::SigningKey::from_pkcs8(&der).unwrap();
    let trust = artifact::TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap();
    let bytes = artifact::create(old.manifest().context.clone(), entries, &key).unwrap();
    artifact::verify(&bytes, &trust).unwrap()
}
fn history(db: &Connection) -> Vec<Vec<String>> {
    let mut q=db.prepare("SELECT id,template_id,kind,template_hash,actor,canonical_payload,previous_hash,event_hash,created_at FROM native_scheduled_recurring_events ORDER BY id").unwrap();
    q.query_map([], |r| {
        Ok((0..9)
            .map(|i| match r.get_ref(i).unwrap() {
                rusqlite::types::ValueRef::Integer(n) => n.to_string(),
                rusqlite::types::ValueRef::Text(t) => String::from_utf8(t.to_vec()).unwrap(),
                _ => panic!("unexpected scheduler cell"),
            })
            .collect())
    })
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}
fn boundaries(db: &Connection) -> Vec<(String, String, String)> {
    let mut q=db.prepare("SELECT event_type,entity_id,message FROM audit_log WHERE event_type LIKE 'native_restore_%' ORDER BY id").unwrap();
    q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}
#[test]
fn signed_scheduler_history_survives_snapshot_restore_boundary_rebackup_and_file_reopen() {
    let old = tests::signed(false, false, false, false, false, None);
    let mut source = ledger(&old);
    let id = queued(&mut source);
    let raw = history(&source);
    assert_eq!(raw.len(), 3);
    let uuid = store::company(&source).unwrap().uuid;
    let archive = signed_ledger(&old, &source);
    drop(source);
    validate(&archive).unwrap();
    let first_archive_hash = rentemester_runtime::backup_keys::digest(archive.manifest_bytes());
    let mut restored = ledger(&archive);
    assert!(rentemester_storage::scheduled_recurring_schema::installed(&restored).unwrap());
    store::restore_boundary(&mut restored, &first_archive_hash).unwrap();
    assert_eq!(history(&restored), raw);
    let first = boundaries(&restored);
    assert_eq!(first.len(), 2);
    let tx = restored
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let current = scheduled::current(&tx, id, TODAY).unwrap();
    assert_ne!(current["ownerUserId"], json!("historical-owner"));
    assert_eq!(current["restoreConsent"]["required"], true);
    assert!(current["queued"].as_array().unwrap().is_empty());
    assert_eq!(
        current["restoreConsent"]["historicalQueued"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let p = scheduled::plan(&tx, id, TODAY).unwrap();
    scheduled::reconsent(&tx, &serde_json::from_value(json!({"templateId":id,"planHash":p["planHash"],"reason":"Fresh synthetic owner review","confirm":true})).unwrap(), "new-owner", TODAY, AT).unwrap();
    let first_plan = scheduled::plan(&tx, id, TODAY).unwrap();
    tx.commit().unwrap();
    let v2 = history(&restored);
    assert_eq!(&v2[..raw.len()], raw);
    assert_eq!(v2.len(), raw.len() + 1);
    assert!(v2.last().unwrap()[5].contains("\"version\":2"));
    // Rebackup preserves the original signed history AND the earlier typed markers.
    let second = signed_ledger(&archive, &restored);
    validate(&second).unwrap();
    drop(restored);
    let second_archive_hash = rentemester_runtime::backup_keys::digest(second.manifest_bytes());
    let mut reopened = ledger(&second);
    store::restore_boundary(&mut reopened, &second_archive_hash).unwrap();
    assert_eq!(history(&reopened), v2);
    assert_eq!(&boundaries(&reopened)[..2], first);
    assert_eq!(boundaries(&reopened).len(), 4);
    assert_eq!(store::company(&reopened).unwrap().uuid, uuid);
    assert!(rentemester_storage::scheduled_recurring_schema::installed(&reopened).unwrap());
    let tx = reopened
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let p = scheduled::plan(&tx, id, TODAY).unwrap();
    assert_ne!(p["planHash"], first_plan["planHash"]);
    assert_eq!(p["restoreConsent"]["required"], true);
    assert_eq!(
        scheduled::current(&tx, id, TODAY).unwrap()["ownerUserId"],
        Value::Null
    );
    tx.rollback().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("synthetic.sqlite");
    std::fs::write(&file, reopened.serialize(MAIN_DB).unwrap().to_vec()).unwrap();
    drop(reopened);
    let disk = Connection::open(file).unwrap();
    assert_eq!(history(&disk), v2);
    store::company(&disk).unwrap();
    let mut control = Connection::open_in_memory().unwrap();
    restore_business(&mut control, &second, &second_archive_hash).unwrap();
    for table in [
        "user",
        "session",
        "twoFactor",
        "rm_workspace_user_access_events",
        "rm_company_membership_events",
    ] {
        assert_eq!(
            control
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
#[test]
fn signed_corrupt_scheduler_is_refused_before_restore_and_boundary_rolls_back() {
    let old = tests::signed(false, false, false, false, false, None);
    for corrupt in [
        "DROP TRIGGER native_scheduled_events_no_update",
        "DROP TRIGGER native_scheduled_events_no_update; UPDATE native_scheduled_recurring_events SET canonical_payload='{}' WHERE id=1; CREATE TRIGGER native_scheduled_events_no_update BEFORE UPDATE ON native_scheduled_recurring_events BEGIN SELECT RAISE(ABORT,'scheduled evidence is append-only'); END;",
        "INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('native_restore_scheduled_boundary','native_scheduled_recurring_events','0','{}','system:portable-restore')",
    ] {
        let mut db = ledger(&old);
        queued(&mut db);
        db.execute_batch(corrupt).unwrap();
        let archive = signed_ledger(&old, &db);
        assert!(
            validate(&archive).is_err(),
            "a valid signature cannot admit corrupt scheduler evidence"
        );
        let before = boundaries(&db);
        assert!(store::restore_boundary(&mut db, &"a".repeat(64)).is_err());
        assert_eq!(
            boundaries(&db),
            before,
            "failed scheduler boundary cannot leave the receipt marker committed"
        );
    }
}
#[test]
fn signed_unresolved_attempt_survives_restore_and_blocks_reconsent_without_new_effects() {
    let old = tests::signed(false, false, false, false, false, None);
    let mut db = ledger(&old);
    let id = queued(&mut db);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let p = scheduled::plan(&tx, id, TODAY).unwrap();
    scheduled::begin(
        &tx,
        &serde_json::from_value(
            json!({"templateId":id,"planHash":p["planHash"],"periodIndex":0,"confirm":true}),
        )
        .unwrap(),
        "historical-owner",
        TODAY,
        AT,
    )
    .unwrap();
    tx.commit().unwrap();
    let raw = history(&db);
    let archive = signed_ledger(&old, &db);
    drop(db);
    validate(&archive).unwrap();
    let mut db = ledger(&archive);
    store::restore_boundary(&mut db, &"a".repeat(64)).unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let p = scheduled::plan(&tx, id, TODAY).unwrap();
    assert!(scheduled::reconsent(&tx,&serde_json::from_value(json!({"templateId":id,"planHash":p["planHash"],"reason":"Cannot resolve unknown historical attempt by consent","confirm":true})).unwrap(),"new-owner",TODAY,AT).is_err());
    tx.rollback().unwrap();
    assert_eq!(history(&db), raw);
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM recurring_invoice_generations",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM documents", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}
