use super::*;
use crate::workspace_inbox::{self as inbox, Provenance};
const AT: &str = "2026-01-01T00:00:00.000Z";
const ASSIGNED: &str = "2026-01-01T00:01:00.000Z";
const CLAIMED: &str = "2026-01-01T00:02:00.000Z";
const EXPIRES: &str = "2026-01-01T00:04:00.000Z";
const FINISHED: &str = "2026-01-01T00:03:00.000Z";
const ACTOR: &str = "user:historical-inbox-owner";
fn visible() -> BTreeSet<String> {
    ["a".into(), "b".into()].into_iter().collect()
}
fn input(id: &str) -> inbox::Ingest {
    serde_json::from_value(json!({"sourceId":id,"idempotencyKey":format!("key-{id}"),"bytesBase64":"typed-api-original","filename":format!("{id}.txt"),"mimeType":"text/plain","transport":"upload","receivedAt":AT,"metadata":{"documentType":"cash_register_receipt","deliveryDescription":"Synthetic inbox original"},"candidates":[]})).unwrap()
}
fn body(id: &str) -> Vec<u8> {
    format!("Synthetic inbox source {id}\n\0\u{00ff}").into_bytes()
}
fn register(company: &mut Connection, id: &str) -> Value {
    let metadata=serde_json::from_value(json!({"source":format!("workspace-inbox:{id}"),"documentType":"cash_register_receipt","deliveryDescription":"Synthetic inbox original"})).unwrap();
    let hash = format!("{:x}", Sha256::digest(body(id)));
    let path = format!("documents/originals/{hash}.txt");
    let tx = company
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let doc = crate::documents::register(
        &tx,
        crate::documents::Original {
            filename: &format!("{id}.txt"),
            hash: &hash,
            path: &path,
            mime: "text/plain",
        },
        &metadata,
        "historical-inbox-owner",
        "2026-01-01",
    )
    .unwrap();
    tx.commit().unwrap();
    doc["document"].clone()
}
fn fixture() -> (tempfile::TempDir, Connection, Connection, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let mut control = Connection::open_in_memory().unwrap();
    let mut a = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut a, "Synthetic A").unwrap();
    let mut b = Connection::open(dir.path().join("b.sqlite")).unwrap();
    crate::fresh::initialize_company(&mut b, "Synthetic B").unwrap();
    for (id, state) in [
        ("inbox-unassigned", 0),
        ("inbox-approved", 1),
        ("inbox-claimed", 2),
        ("inbox-split", 3),
        ("inbox-completed", 4),
    ] {
        let tx = control
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let i = input(id);
        inbox::ingest(
            &tx,
            "a",
            &i,
            &body(id),
            "unused",
            &visible(),
            Provenance {
                actor: ACTOR,
                now: AT,
            },
        )
        .unwrap();
        if state >= 1 {
            inbox::assign(&tx, "a", id, "b", &visible(), ACTOR, ASSIGNED).unwrap();
        }
        if state >= 2 {
            inbox::claim(&tx, id, "b", &format!("claim-{id}"), CLAIMED, EXPIRES).unwrap();
        }
        tx.commit().unwrap();
        if state >= 3 {
            let doc = register(&mut b, id);
            if state == 4 {
                let tx = control
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                    .unwrap();
                inbox::complete(
                    &tx,
                    id,
                    "b",
                    &format!("claim-{id}"),
                    doc["id"].as_i64().unwrap(),
                    doc["documentNo"].as_str().unwrap(),
                    Provenance {
                        actor: ACTOR,
                        now: FINISHED,
                    },
                )
                .unwrap();
                tx.commit().unwrap();
            }
        }
    }
    (dir, control, a, b)
}
fn exported(c: &mut Connection) -> Control {
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let e = export_control(&tx).unwrap();
    validate_companies(&tx, &["a".into(), "b".into()]).unwrap();
    tx.rollback().unwrap();
    e
}
fn empty_target(e: &Control) -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    initialize_control(&mut db, &e.bytes, &"a".repeat(64)).unwrap();
    db
}
#[test]
fn inbox_codec_roundtrip_all_states_full_original_history_and_credential_free_new_principal() {
    let (dir, mut control, a, b) = fixture();
    validate_company(&control, &a, "a").unwrap();
    validate_company(&control, &b, "b").unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('historical-inbox-owner','Synthetic source','old@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('source-session','historical-inbox-owner','SYNTHETIC-EXCLUDED-TOKEN','2026-01-01','2026-01-01','2027-01-01');INSERT INTO twoFactor(id,userId,secret,backupCodes,verified)VALUES('source-factor','historical-inbox-owner','SYNTHETIC-EXCLUDED-SECRET','[]',1);INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('historical-inbox-owner','activate','workspace_owner','system:fixture');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('historical-inbox-owner','a','grant','owner','system:fixture');").unwrap();
    let e = exported(&mut control);
    assert!(!e.unsupported.iter().any(|s| s.0 == "workspace-inbox"));
    assert_eq!(e.unsupported.len(), 2);
    assert!(
        !String::from_utf8(e.inbox_bytes.clone())
            .unwrap()
            .contains("SYNTHETIC-EXCLUDED")
    );
    let mut restored = empty_target(&e);
    initialize(&mut restored, &e.inbox_bytes, &["a".into(), "b".into()]).unwrap();
    assert_eq!(dump(&restored).unwrap(), e.inbox_bytes);
    assert!(initialize(&mut restored, &e.inbox_bytes, &["a".into(), "b".into()]).is_err());
    for name in [
        "user",
        "session",
        "twoFactor",
        "account",
        "apikey",
        "rm_workspace_user_access_events",
        "rm_company_membership_events",
        "rm_workspace_invitation_events",
    ] {
        assert_eq!(
            restored
                .query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        )
    }
    let tx = control
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let target = restored
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    for id in [
        "inbox-unassigned",
        "inbox-approved",
        "inbox-claimed",
        "inbox-split",
        "inbox-completed",
    ] {
        assert_eq!(
            inbox::inspect(&tx, "a", id, &visible()).unwrap(),
            inbox::inspect(&target, "a", id, &visible()).unwrap()
        );
        assert_eq!(inbox::original(&target, id).unwrap(), body(id));
    }
    assert_eq!(
        inbox::inspect(&target, "a", "inbox-split", &visible()).unwrap()["handoffs"][0]["leaseExpiresAt"],
        EXPIRES
    );
    assert!(
        inbox::ingest(
            &target,
            "a",
            &input("inbox-unassigned"),
            &body("inbox-unassigned"),
            "unused",
            &visible(),
            Provenance {
                actor: "user:restored-new-owner",
                now: FINISHED
            }
        )
        .is_err()
    );
    let fresh = input("inbox-fresh-restored");
    assert!(
        inbox::ingest(
            &target,
            "a",
            &fresh,
            &body("inbox-fresh-restored"),
            "unused",
            &visible(),
            Provenance {
                actor: "user:restored-new-owner",
                now: FINISHED
            }
        )
        .is_ok()
    );
    target.rollback().unwrap();
    tx.rollback().unwrap();
    // Reopen an independent actual company SQLite copy; stable IDs/metadata/UUID
    // remain bound to the restored control, without reopening a live source inode.
    b.close().unwrap();
    std::fs::copy(
        dir.path().join("b.sqlite"),
        dir.path().join("restored-b.sqlite"),
    )
    .unwrap();
    let b = Connection::open(dir.path().join("restored-b.sqlite")).unwrap();
    validate_company(&restored, &a, "a").unwrap();
    validate_company(&restored, &b, "b").unwrap();
    assert_eq!(
        b.query_row("SELECT COUNT(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn inbox_codec_corrupt_unknown_blob_proof_claim_crossref_and_types_refuse_atomically() {
    let (_dir, mut source, _, _) = fixture();
    let e = exported(&mut source);
    let original: Value = serde_json::from_slice(&e.inbox_bytes).unwrap();
    for case in 0..17 {
        let mut bad = original.clone();
        match case {
            0 => bad["format"] = json!("future-inbox-codec"),
            1 => bad["tables"][TABLES[0]]["columns"][0] = json!("future_column"),
            2 => bad["tables"]["user"] = json!({"columns":[],"rows":[]}),
            3 => {
                bad["tables"].as_object_mut().unwrap().remove(TABLES[3]);
            }
            4 => {
                bad["tables"][TABLES[0]]["rows"][0][3] = json!({"type":"Text","value":"not-a-blob"})
            }
            5 => bad["tables"][TABLES[0]]["rows"][0][3]["value"] = json!("YQ=="),
            6 => bad["tables"][TABLES[0]]["rows"][0][4]["value"] = json!("f".repeat(64)),
            7 => bad["tables"][TABLES[1]]["rows"][0][3]["value"] = json!("f".repeat(64)),
            8 => bad["tables"][TABLES[1]]["rows"][0][1]["value"] = json!("missing-source"),
            9 => bad["tables"][TABLES[2]]["rows"][0][1]["value"] = json!("missing-company"),
            10 => bad["tables"][TABLES[4]]["rows"][0][2]["value"] = json!("f".repeat(64)),
            11 => bad["tables"][TABLES[4]]["rows"][0][3]["value"] = json!("completed"),
            12 => bad["tables"][TABLES[4]]["rows"][0][6] = json!({"type":"Integer","value":42}),
            13 => {
                bad["tables"][TABLES[1]]["rows"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            14 => bad["tables"][TABLES[4]]["rows"][0][1]["value"] = json!("a"),
            15 => bad["tables"][TABLES[1]]["rows"][0][0] = json!({"type":"Text","value":"1"}),
            _ => bad["tables"][TABLES[0]]["rows"]
                .as_array_mut()
                .unwrap()
                .reverse(),
        }
        let mut restored = empty_target(&e);
        assert!(
            initialize(
                &mut restored,
                &serde_json::to_vec(&bad).unwrap(),
                &["a".into(), "b".into()]
            )
            .is_err(),
            "case{case}"
        );
        for name in TABLES {
            assert_eq!(
                restored
                    .query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0,
                "case{case}"
            );
        }
    }
    let mut restored = empty_target(&e);
    assert!(initialize(&mut restored, &e.inbox_bytes, &["a".into()]).is_err());
    source
        .execute_batch("ALTER TABLE rm_workspace_inbox_sources ADD COLUMN future_data TEXT")
        .unwrap();
    let tx = source
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    assert!(export_control(&tx).is_err());
}
#[test]
fn inbox_completed_and_split_document_references_must_be_same_company_marker_hash_metadata_and_inventory()
 {
    let (_dir, mut control, a, mut b) = fixture();
    let e = exported(&mut control);
    let mut restored = empty_target(&e);
    initialize(&mut restored, &e.inbox_bytes, &["a".into(), "b".into()]).unwrap();
    validate_company(&restored, &a, "a").unwrap();
    validate_company(&restored, &b, "b").unwrap();
    assert!(validate_company(&restored, &b, "a").is_err());
    // Immutable canonical document triggers prevent ordinary mutation; synthetic
    // corruption removes/restores exact guards solely to test data validation.
    let triggers = b
        .prepare("SELECT name,sql FROM sqlite_schema WHERE type='trigger'")
        .unwrap()
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for sql in [
        "UPDATE documents SET source='workspace-inbox:missing-source' WHERE id=1",
        "UPDATE documents SET sha256_hash='ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff' WHERE id=1",
        "UPDATE documents SET payload_json='{}' WHERE id=1",
        "UPDATE documents SET original_filename='different-source.txt' WHERE id=1",
        "UPDATE documents SET mime_type='application/pdf' WHERE id=1",
        "DELETE FROM documents WHERE id=2",
    ] {
        let tx = b.transaction().unwrap();
        for (name, _) in &triggers {
            tx.execute_batch(&format!("DROP TRIGGER \"{name}\";"))
                .unwrap();
        }
        tx.execute_batch(sql).unwrap();
        for (_, ddl) in &triggers {
            tx.execute_batch(ddl).unwrap();
        }
        assert!(validate_company(&restored, &tx, "b").is_err(), "{sql}");
        tx.rollback().unwrap();
    }
}
