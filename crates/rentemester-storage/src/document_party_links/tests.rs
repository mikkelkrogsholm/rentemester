use super::*;
use rusqlite::{Connection, TransactionBehavior};
fn fixture() -> (Connection, Connection) {
    let mut c = Connection::open_in_memory().unwrap();
    let mut l = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic document links").unwrap();
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let create:crate::parties::Create=serde_json::from_value(json!({"partyId":"party-synthetic-supplier","kind":"organization","name":"Synthetic supplier","identifiers":[{"country":"DK","identifierKind":"dk_cvr","identifier":"12345678"}],"source":"synthetic-source","observedAt":"2026-10-19T00:00:00.000Z","reviewAssertion":"Synthetic explicit supplier identity review","role":"vendor"})).unwrap();
    let id = crate::parties::create(
        &tx,
        &create,
        "unused",
        "user:owner",
        "2026-10-19T00:00:00.000Z",
    )
    .unwrap();
    crate::parties::role(
        &tx,
        &id,
        "synthetic",
        &crate::parties::Role {
            role: "vendor".into(),
            defaults: None,
        },
        "user:owner",
        "2026-10-19T00:00:00.000Z",
    )
    .unwrap();
    tx.commit().unwrap();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    for n in 1..=2 {
        let metadata=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-10-18","deliveryDescription":"Synthetic supplies","invoiceNo":format!("SYN-{n}"),"amountIncVat":125,"vatAmount":25,"sender":{"name":"Synthetic supplier","address":"Synthetic street 1","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic buyer","address":"Synthetic street 2","vatOrCvr":"DK87654321"}})).unwrap();
        crate::documents::register(
            &tx,
            crate::documents::Original {
                filename: "synthetic.txt",
                hash: &format!("{n:064x}"),
                path: "synthetic-source",
                mime: "text/plain",
            },
            &metadata,
            "owner",
            "2026-10-19",
        )
        .unwrap();
    }
    tx.commit().unwrap();
    (c, l)
}
fn input(id: i64) -> PlanInput {
    serde_json::from_value(json!({"documentId":id,"role":"vendor","jurisdiction":"DK","identifierKind":"dk_cvr","identifier":"12345678"})).unwrap()
}
fn apply_input(i: &PlanInput, hash: &Value, key: &str) -> Apply {
    let mut v = serde_json::to_value(i).unwrap();
    v["planHash"] = hash.clone();
    v["confirm"] = json!(true);
    v["idempotencyKey"] = json!(key);
    serde_json::from_value(v).unwrap()
}
#[test]
fn exact_identifier_apply_duplicate_history_supersede_and_no_accounting_effect() {
    let (mut c, mut l) = fixture();
    let ct = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let lt = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let i = input(1);
    let snapshot = capture(&ct, &i, "synthetic").unwrap();
    assert_eq!(snapshot["roles"].as_array().unwrap().len(), 1);
    let p = plan(&lt, &i, &snapshot).unwrap();
    assert_eq!(p["plan"]["evidence"]["identifier"], "DK12345678");
    assert_eq!(plan(&lt, &i, &snapshot).unwrap(), p);
    let command = apply_input(&i, &p["plan"]["planHash"], "synthetic-key");
    let first = apply(
        &lt,
        &command,
        &snapshot,
        "user:first",
        "user:first",
        "2026-10-19T01:00:00.000Z",
    )
    .unwrap();
    let replay = apply(
        &lt,
        &command,
        &snapshot,
        "user:second",
        "user:second",
        "2026-10-19T02:00:00.000Z",
    )
    .unwrap();
    assert_eq!(first["id"], replay["id"]);
    assert_eq!(replay["idempotent"], true);
    let h = history(&lt, 1).unwrap();
    assert_eq!(h["links"][0]["actor"], "user:first");
    assert_eq!(h["links"][0]["principal"], "user:first");
    assert_eq!(
        list(&lt, Some("linked"), 100).unwrap()["links"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let supersede = Supersede {
        document_id: 1,
        role: "vendor".into(),
        plan_hash: p["plan"]["planHash"].as_str().unwrap().into(),
        reason: "Explicit synthetic unlink review".into(),
        confirm: true,
    };
    supersede_link(&lt, &supersede);
    assert_eq!(
        history(&lt, 1).unwrap()["links"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        list(&lt, Some("unresolved"), 100).unwrap()["links"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        apply(
            &lt,
            &command,
            &snapshot,
            "user:first",
            "user:first",
            "2026-10-19T03:00:00.000Z"
        )
        .is_err()
    );
    assert_eq!(
        lt.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    lt.commit().unwrap();
    ct.rollback().unwrap();
    let lt = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert_eq!(
        history(&lt, 1).unwrap()["links"][0]["created_at"],
        "2026-10-19T01:00:00.000Z"
    );
}
fn supersede_link(tx: &Transaction<'_>, i: &Supersede) {
    let first = supersede(
        tx,
        i,
        "user:reviewer",
        "user:reviewer",
        "2026-10-19T03:00:00.000Z",
    )
    .unwrap();
    let duplicate = supersede(
        tx,
        i,
        "user:reviewer",
        "user:reviewer",
        "2026-10-19T04:00:00.000Z",
    )
    .unwrap();
    assert_eq!(first["id"], duplicate["id"]);
    assert_eq!(duplicate["idempotent"], true);
}
#[test]
fn current_snapshot_source_and_exact_key_conflicts_refuse_and_outer_rollback_has_no_link() {
    let (mut c, mut l) = fixture();
    let ct = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let lt = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let i = input(1);
    let snapshot = capture(&ct, &i, "synthetic").unwrap();
    let p = plan(&lt, &i, &snapshot).unwrap();
    let command = apply_input(&i, &p["plan"]["planHash"], "one-key");
    apply(
        &lt,
        &command,
        &snapshot,
        "user:first",
        "user:first",
        "2026-10-19T01:00:00.000Z",
    )
    .unwrap();
    let j = input(2);
    let other = plan(&lt, &j, &snapshot).unwrap();
    assert!(
        apply(
            &lt,
            &apply_input(&j, &other["plan"]["planHash"], "one-key"),
            &snapshot,
            "user:first",
            "user:first",
            "2026-10-19T01:00:00.000Z"
        )
        .is_err()
    );
    let mut changed = snapshot.clone();
    changed["name"] = json!("Changed canonical party evidence");
    assert!(
        apply(
            &lt,
            &command,
            &changed,
            "user:first",
            "user:first",
            "2026-10-19T01:00:00.000Z"
        )
        .is_err()
    );
    lt.rollback().unwrap();
    ct.rollback().unwrap();
    assert_eq!(
        l.query_row("SELECT count(*) FROM document_party_link_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn hidden_party_wrong_role_conflicting_identifier_and_legacy_inference_refuse() {
    let (mut c, mut l) = fixture();
    let ct = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let lt = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let i = input(1);
    assert!(capture(&ct, &i, "other-company").is_err());
    let mut wrong = i.clone();
    wrong.role = "supplier".into();
    assert_eq!(
        capture(&ct, &wrong, "synthetic").unwrap_err().code,
        "SCOPE_MISMATCH"
    );
    wrong = i.clone();
    wrong.party_id = Some("party-other".into());
    assert_eq!(
        capture(&ct, &wrong, "synthetic").unwrap_err().code,
        "IDENTIFIER_CONFLICT"
    );
    wrong = i.clone();
    wrong.identifier = Some("87654321".into());
    assert!(capture(&ct, &wrong, "synthetic").is_err());
    wrong = i.clone();
    wrong.legacy_kind = Some("vendor".into());
    assert_eq!(
        capture(&ct, &wrong, "synthetic").unwrap_err().code,
        "UNSUPPORTED_EVIDENCE_KIND"
    );
    let p = capture(&ct, &i, "synthetic").unwrap();
    wrong = i;
    wrong.identifier = Some("87654321".into());
    assert_eq!(
        plan(&lt, &wrong, &p).unwrap_err().code,
        "NO_IDENTIFIER_EVIDENCE"
    );
}

#[test]
fn backup_validates_linked_and_superseded_history_without_live_registry() {
    let (mut control, mut ledger) = fixture();
    let ct = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let tx = ledger
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let i = input(1);
    let snapshot = capture(&ct, &i, "synthetic").unwrap();
    let p = plan(&tx, &i, &snapshot).unwrap();
    apply(
        &tx,
        &apply_input(&i, &p["plan"]["planHash"], "backup-link"),
        &snapshot,
        "user:original",
        "user:original",
        "2026-10-19T01:00:00.000Z",
    )
    .unwrap();
    supersede_link(
        &tx,
        &Supersede {
            document_id: 1,
            role: "vendor".into(),
            plan_hash: p["plan"]["planHash"].as_str().unwrap().into(),
            reason: "Historical correction".into(),
            confirm: true,
        },
    );
    tx.commit().unwrap();
    ct.rollback().unwrap();
    drop(control); // No restored/live party registry is consulted.
    let before = ledger.total_changes();
    validate_backup(&ledger).unwrap();
    assert_eq!(ledger.total_changes(), before);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("historical.sqlite");
    ledger
        .execute("VACUUM INTO ?1", [path.to_str().unwrap()])
        .unwrap();
    drop(ledger);
    let ledger = Connection::open(path).unwrap();
    validate_backup(&ledger).unwrap();
    let triggers = ledger.prepare("SELECT name,sql FROM sqlite_schema WHERE type='trigger' AND tbl_name='document_party_link_events'").unwrap().query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
    for (name, _) in &triggers {
        ledger
            .execute_batch(&format!("DROP TRIGGER \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
    }
    ledger
        .execute(
            "UPDATE document_party_link_events SET party_snapshot_json='{}' WHERE id=1",
            [],
        )
        .unwrap();
    for (_, ddl) in &triggers {
        ledger.execute_batch(ddl).unwrap();
    }
    assert!(validate_backup(&ledger).is_err());
    assert!(crate::portable_backup::company(&ledger).is_err());
}
