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

#[test]
fn document_party_original_all18_roles_and_reviewed_legacy_scope() {
    let (mut c, mut l) = fixture();
    let ct = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    for role_name in policy::ROLES {
        crate::parties::role(
            &ct,
            "party-synthetic-supplier",
            "synthetic",
            &crate::parties::Role {
                role: (*role_name).into(),
                defaults: None,
            },
            "user:owner",
            "2026-10-19T00:00:00.000Z",
        )
        .unwrap();
        let mut i = input(1);
        i.role = (*role_name).into();
        let snapshot = capture(&ct, &i, "synthetic").unwrap();
        assert_eq!(
            plan(&tx, &i, &snapshot).unwrap()["plan"]["role"],
            *role_name
        );
    }
    ct.execute("INSERT INTO rm_party_legacy_links(company_slug,legacy_kind,legacy_id,party_id,actor,created_at) VALUES('synthetic','vendor','old-1','party-synthetic-supplier','user:owner','2026-10-19T00:00:00.000Z')",[]).unwrap();
    let mut i: PlanInput = serde_json::from_value(
        json!({"documentId":1,"role":"vendor","legacyKind":"vendor","legacyId":"old-1"}),
    )
    .unwrap();
    assert_eq!(
        capture(&ct, &i, "synthetic").unwrap_err().code,
        "LEGACY_REFERENCE_UNREVIEWED"
    );
    i.reviewed_legacy_reference = Some("Synthetic explicit mapping review".into());
    let snapshot = capture(&ct, &i, "synthetic").unwrap();
    let p = plan(&tx, &i, &snapshot).unwrap();
    assert_eq!(p["plan"]["evidence"]["kind"], "reviewed_legacy_reference");
    assert!(capture(&ct, &i, "another-company").is_err());
    apply(
        &tx,
        &apply_input(&i, &p["plan"]["planHash"], "legacy-positive"),
        &snapshot,
        "user:owner",
        "user:owner",
        "2026-10-19T01:00:00.000Z",
    )
    .unwrap();
    validate_backup(&tx).unwrap();
    assert_eq!(
        history(&tx, 1).unwrap()["links"][0]["idempotency_key"],
        "legacy-positive"
    );
}
#[test]
fn document_party_original_internal_current_replay_conflict_supersede_backup() {
    let (_, mut l) = fixture();
    // Synthetic fixture preparation only; product decisions never alter metadata.
    l.execute(
        "UPDATE documents SET document_type='internal_voucher' WHERE id=1",
        [],
    )
    .unwrap();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let i = Internal {
        document_id: 1,
        reason: "Synthetic internal bank evidence".into(),
        confirm: true,
        idempotency_key: Some("decision-1".into()),
    };
    let now = "2026-10-19T01:00:00.000Z";
    let first = internal(
        &tx,
        &i,
        "service_account:synthetic",
        "service_account:synthetic",
        now,
    )
    .unwrap();
    assert_eq!(
        internal(&tx, &i, "user:owner", "user:owner", now).unwrap()["idempotent"],
        true
    );
    let conflict = Internal {
        reason: "Changed decision".into(),
        ..i
    };
    assert_eq!(
        internal(&tx, &conflict, "user:owner", "user:owner", now)
            .unwrap_err()
            .code,
        "IDEMPOTENCY_CONFLICT"
    );
    validate_backup(&tx).unwrap();
    assert_eq!(
        list(&tx, Some("internal_no_external_party"), 100).unwrap()["links"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let s = InternalSupersede {
        document_id: 1,
        decision_hash: first["decisionHash"].as_str().unwrap().into(),
        reason: "Explicit correction".into(),
        confirm: true,
    };
    internal_supersede(&tx, &s, "user:owner", "user:owner", now).unwrap();
    assert_eq!(
        internal_supersede(&tx, &s, "user:owner", "user:owner", now).unwrap()["idempotent"],
        true
    );
    let replay = Internal {
        document_id: 1,
        reason: "Synthetic internal bank evidence".into(),
        confirm: true,
        idempotency_key: Some("decision-1".into()),
    };
    assert_eq!(
        internal(&tx, &replay, "user:owner", "user:owner", now)
            .unwrap_err()
            .code,
        "CURRENT_STATE_CONFLICT"
    );
    validate_backup(&tx).unwrap();
    crate::portable_backup::company(&tx).unwrap();
    assert_eq!(
        tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn document_party_original_retains_historical_actor_and_distinct_principal() {
    let (mut control, mut ledger) = fixture();
    let ct = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let tx = ledger
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let input = input(1);
    let snapshot = capture(&ct, &input, "synthetic").unwrap();
    let planned = plan(&tx, &input, &snapshot).unwrap();
    let applied = apply(
        &tx,
        &apply_input(&input, &planned["plan"]["planHash"], "historical-source"),
        &snapshot,
        "agent:synthetic-source",
        "reviewed-source-principal",
        "2026-10-19T01:00:00.000Z",
    )
    .unwrap();
    assert!(applied["id"].as_i64().unwrap() > 0);
    validate_backup(&tx).unwrap();
    crate::portable_backup::company(&tx).unwrap();
    let row = &history(&tx, 1).unwrap()["links"][0];
    assert_eq!(row["actor"], "agent:synthetic-source");
    assert_eq!(row["principal"], "reviewed-source-principal");
    assert_eq!(row["idempotency_key"], "historical-source");
    assert_eq!(
        tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn document_party_original_valid_legacy_plan_and_internal_decision_mutually_exclude() {
    for link_first in [true, false] {
        let (mut control, mut ledger) = fixture();
        let ct = control
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let tx = ledger
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        // Low-level modeled eligibility only: actual admission still needs the real
        // bank/source/scanner fixture. Reviewed legacy evidence makes the plan
        // valid without manufacturing an identifier on an internal voucher.
        tx.execute(
            "UPDATE documents SET document_type='internal_voucher' WHERE id=1",
            [],
        )
        .unwrap();
        ct.execute("INSERT INTO rm_party_legacy_links(company_slug,legacy_kind,legacy_id,party_id,actor,created_at) VALUES('synthetic','vendor','old-internal','party-synthetic-supplier','user:owner','2026-10-19T00:00:00.000Z')", []).unwrap();
        let i: PlanInput = serde_json::from_value(json!({
            "documentId": 1,
            "role": "vendor",
            "legacyKind": "vendor",
            "legacyId": "old-internal",
            "reviewedLegacyReference": "Synthetic explicit internal voucher mapping review"
        }))
        .unwrap();
        let snapshot = capture(&ct, &i, "synthetic").unwrap();
        let planned = plan(&tx, &i, &snapshot).unwrap();
        assert_eq!(planned["ok"], true);
        assert_eq!(planned["plan"]["partyId"], "party-synthetic-supplier");
        assert_eq!(
            planned["plan"]["evidence"]["kind"],
            "reviewed_legacy_reference"
        );
        let command = apply_input(&i, &planned["plan"]["planHash"], "exclusive-link");
        let decision = Internal {
            document_id: 1,
            reason: "Synthetic reviewed internal voucher".into(),
            confirm: true,
            idempotency_key: Some("exclusive-internal".into()),
        };
        let original: (String, String) = tx
            .query_row(
                "SELECT sha256_hash,payload_json FROM documents WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let now = "2026-10-19T01:00:00.000Z";
        let first = if link_first {
            apply(&tx, &command, &snapshot, "user:owner", "user:owner", now)
        } else {
            internal(&tx, &decision, "user:owner", "user:owner", now)
        }
        .unwrap();
        assert_eq!(first["ok"], true);
        assert_eq!(first["idempotent"], false);
        let history_before = history(&tx, 1).unwrap();
        let changes_before = tx.total_changes();
        // In particular, internal-first must reach the current-state branch
        // with a still-valid plan, not fail earlier for invalid evidence/hash.
        assert_eq!(plan(&tx, &i, &snapshot).unwrap(), planned);
        let refused = if link_first {
            internal(&tx, &decision, "user:owner", "user:owner", now)
        } else {
            apply(&tx, &command, &snapshot, "user:owner", "user:owner", now)
        }
        .unwrap_err();
        assert_eq!(refused.code, "CURRENT_STATE_CONFLICT");
        assert_eq!(tx.total_changes(), changes_before);
        assert_eq!(history(&tx, 1).unwrap(), history_before);
        let counts: (i64, i64) = tx
            .query_row(
                "SELECT (SELECT count(*) FROM document_party_link_events), (SELECT count(*) FROM document_party_resolution_events)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(counts, if link_first { (1, 0) } else { (0, 1) });
        let after: (String, String) = tx
            .query_row(
                "SELECT sha256_hash,payload_json FROM documents WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(after, original);
        assert_eq!(
            tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        validate_backup(&tx).unwrap();
        crate::portable_backup::company(&tx).unwrap();
        tx.commit().unwrap();
        validate_backup(&ledger).unwrap();
    }
}
