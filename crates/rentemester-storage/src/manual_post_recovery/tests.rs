use super::*;
use crate::idempotency::{self, Input, Operation, Outcome, PrincipalKind};
use rusqlite::TransactionBehavior;
fn fixture() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::company_baseline::rehearse(&mut db).unwrap();
    db
}
fn receipt(tx: &Transaction<'_>, subject: &str, key: &str) -> i64 {
    let input = Input {
        key,
        operation: Operation::JournalPost,
        principal_kind: PrincipalKind::User,
        subject,
        payload: &json!({"synthetic":true}),
        created_by: subject,
        created_by_program: "synthetic",
        now_ms: 1_767_225_600_000,
    };
    match idempotency::execute(tx, input, |_| {
        Ok(json!({"ok":true,"entryId":1,"entryNo":"S1","entryHash":"synthetic"}))
    })
    .unwrap()
    {
        Outcome::Accepted { receipt, .. } => receipt.receipt_id,
        _ => panic!(),
    }
}
#[test]
fn accepted_receipt_is_durable_and_ack_never_posts_or_unreserves_key() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let old = receipt(&tx, "synthetic-owner", "old-client");
    let id = receipt(&tx, "synthetic-owner", "new-browser");
    mark(&tx, "synthetic-owner", id).unwrap();
    tx.commit().unwrap();
    assert_eq!(
        inventory(&db, "synthetic-owner").unwrap()["receipts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(require_clear(&db, "synthetic-owner").is_err());
    assert!(
        inventory(&db, "synthetic-other").unwrap()["receipts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(acknowledge(&tx, "synthetic-other", id).is_err());
    assert_eq!(
        acknowledge(&tx, "synthetic-owner", id).unwrap()["acknowledgedReceiptId"],
        id
    );
    acknowledge(&tx, "synthetic-owner", id).unwrap();
    tx.commit().unwrap();
    assert!(require_clear(&db, "synthetic-owner").is_ok());
    assert_ne!(old, id);
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM mutation_idempotency_tombstones",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM mutation_idempotency_outcomes",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM audit_log WHERE event_type=?1",
            [ACK],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
#[test]
fn rollback_and_expired_result_do_not_allow_a_blind_repost() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let id = receipt(&tx, "owner", "key");
    mark(&tx, "owner", id).unwrap();
    tx.rollback().unwrap();
    assert!(require_clear(&db, "owner").is_ok());
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let id = receipt(&tx, "owner", "key");
    mark(&tx, "owner", id).unwrap();
    idempotency::prune(&tx, 1_800_000_000_000).unwrap();
    tx.commit().unwrap();
    assert_eq!(
        inventory(&db, "owner").unwrap()["receipts"][0]["resultAvailable"],
        false
    );
    assert!(require_clear(&db, "owner").is_err());
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    acknowledge(&tx, "owner", id).unwrap();
    tx.rollback().unwrap();
    assert!(require_clear(&db, "owner").is_err());
}
#[test]
fn malformed_recovery_history_is_not_treated_as_empty() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let id = receipt(&tx, "owner", "key");
    mark(&tx, "owner", id).unwrap();
    tx.commit().unwrap();
    db.execute_batch("DROP TRIGGER IF EXISTS audit_log_no_update;")
        .unwrap();
    // Insert a malformed fact instead of editing the append-only source.
    db.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'idempotency_receipt',?2,'{}','user:owner')",params![ACK,id.to_string()]).unwrap();
    assert!(inventory(&db, "owner").is_err());
}
#[test]
fn an_orphan_or_noncanonical_pending_fact_cannot_make_recovery_clear() {
    for id in ["missing", "999999", "01"] {
        let mut db = fixture();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        receipt(&tx, "owner", "old");
        tx.commit().unwrap();
        db.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'idempotency_receipt',?2,'{}','user:owner')",params![PENDING,id]).unwrap();
        assert!(require_clear(&db, "owner").is_err());
    }
}
#[test]
fn pending_recovery_allows_only_reserved_original_key_replay_not_a_new_intent() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let id = receipt(&tx, "owner", "original");
    mark(&tx, "owner", id).unwrap();
    tx.commit().unwrap();
    assert!(require_clear_for_key(&db, "owner", "original").is_ok());
    assert!(require_clear_for_key(&db, "owner", "fresh").is_err());
    assert!(require_clear_for_key(&db, "owner", " original").is_err());
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let replay = idempotency::execute(
        &tx,
        Input {
            key: "original",
            operation: Operation::JournalPost,
            principal_kind: PrincipalKind::User,
            subject: "owner",
            payload: &json!({"synthetic":true}),
            created_by: "owner",
            created_by_program: "synthetic",
            now_ms: 1_767_225_600_000,
        },
        |_| panic!("existing key cannot apply another effect"),
    )
    .unwrap();
    match replay {
        Outcome::Accepted { receipt, .. } => {
            assert!(receipt.replayed);
            assert_eq!(receipt.receipt_id, id)
        }
        _ => panic!(),
    };
    tx.commit().unwrap();
    assert!(require_clear(&db, "owner").is_err());
}
