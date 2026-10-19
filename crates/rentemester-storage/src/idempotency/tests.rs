use super::*;
use rusqlite::TransactionBehavior;
use serde_json::json;
fn fixture() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::company_baseline::rehearse(&mut db).unwrap();
    db.execute_batch("CREATE TABLE receipt_test_effects(n INTEGER PRIMARY KEY)")
        .unwrap();
    db
}
fn input<'a>(payload: &'a Value, key: &'a str) -> Input<'a> {
    Input {
        key,
        operation: Operation::JournalPost,
        principal_kind: PrincipalKind::ServiceAccount,
        subject: "synthetic-service",
        payload,
        created_by: "agent:synthetic",
        created_by_program: "test",
        now_ms: 1_767_225_600_000,
    }
}
fn effect(tx: &Transaction<'_>) -> Result<Value, Error> {
    let n = crate::sequences::next(tx, "journal", "synthetic", 0)?;
    tx.execute("INSERT INTO receipt_test_effects VALUES(?1)", [n])
        .map_err(storage)?;
    Ok(json!({"ok":true,"effect":n}))
}
fn accepted(outcome: Outcome) -> (Value, Receipt) {
    match outcome {
        Outcome::Accepted { result, receipt } => (result, receipt),
        Outcome::Refused { code } => panic!("unexpected refusal: {code}"),
    }
}
#[test]
fn stable_receipt_replay_conflict_retention_and_business_rollback() {
    let mut db = fixture();
    let payload = json!({"a":1,"b":[null,true,"ø"]});
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let (result, first) = accepted(execute(&tx, input(&payload, "key"), effect).unwrap());
    assert!(!first.replayed);
    tx.commit().unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let mut rotated = input(&payload, "key");
    rotated.created_by = "agent:rotated-credential";
    rotated.created_by_program = "moved-client";
    rotated.now_ms += RETENTION_MS + 1;
    let (again, replay) =
        accepted(execute(&tx, rotated, |_| panic!("replay must not execute")).unwrap());
    assert_eq!(again, result);
    assert!(replay.replayed);
    assert_eq!(first.receipt_id, replay.receipt_id);
    tx.commit().unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(matches!(
        execute(&tx, input(&json!({"a":2}), "key"), |_| panic!("conflict")).unwrap(),
        Outcome::Refused {
            code: "IDEMPOTENCY_CONFLICT"
        }
    ));
    tx.commit().unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert_eq!(prune(&tx, 1_767_225_600_000 + RETENTION_MS).unwrap(), 1);
    tx.commit().unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(matches!(
        execute(&tx, input(&payload, "key"), |_| panic!("expired")).unwrap(),
        Outcome::Refused {
            code: "IDEMPOTENCY_OUTCOME_EXPIRED"
        }
    ));
    tx.commit().unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let failed = execute(&tx, input(&payload, "rejection"), |tx| {
        effect(tx)?;
        Ok(json!({"ok":false,"errors":["synthetic business rejection"]}))
    })
    .err()
    .unwrap();
    assert_eq!(
        failed.rejection,
        Some(json!({"ok":false,"errors":["synthetic business rejection"]}))
    );
    tx.rollback().unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let (_, r) = accepted(execute(&tx, input(&payload, "rejection"), effect).unwrap());
    assert!(!r.replayed);
    tx.commit().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM receipt_test_effects", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    let events: Vec<String> = db
        .prepare(
            "SELECT event_type FROM audit_log WHERE event_type LIKE 'idempotency_%' ORDER BY id",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        events,
        [
            "idempotency_original",
            "idempotency_replay",
            "idempotency_conflict",
            "idempotency_outcome_expired",
            "idempotency_original"
        ]
    );
    assert!(
        db.execute("DELETE FROM mutation_idempotency_tombstones", [])
            .is_err()
    );
    assert!(
        db.execute(
            "UPDATE ledger_identity SET ledger_uuid='00000000000000000000000000000000'",
            []
        )
        .is_err()
    );
}
#[test]
fn canonical_bounds_utf16_keys_and_native_precision() {
    for key in ["", " \t", "\u{feff}", &"😀".repeat(65)] {
        assert!(validate_key(key).is_err());
    }
    for key in ["\u{85}", " x ", &"😀".repeat(64), &"x".repeat(128)] {
        assert!(validate_key(key).is_ok());
    }
    assert_eq!(
        canonical_payload_hash(&json!({"b":1.0,"a":-0.0})).unwrap(),
        hash(b"{\"a\":0,\"b\":1}")
    );
    for (raw, canonical) in [
        (r#"{"n":333333333.33333329}"#, r#"{"n":333333333.3333333}"#),
        (r#"{"n":0.9999999999999999}"#, r#"{"n":0.9999999999999999}"#),
    ] {
        let parsed: Value = serde_json::from_str(raw).unwrap();
        assert_eq!(
            canonical_payload_hash(&parsed).unwrap(),
            hash(canonical.as_bytes())
        );
    }
    let payload = json!({"\u{e000}":1,"😀":2,"a":[1e-7,1e-6,"\n\u{0}ø"]});
    assert_eq!(
        canonical_payload_hash(&payload).unwrap(),
        hash("{\"a\":[1e-7,0.000001,\"\\n\\u0000ø\"],\"😀\":2,\"\u{e000}\":1}".as_bytes())
    );
    assert!(canonical_payload_hash(&json!({"n":9_007_199_254_740_992u64})).is_err());
    assert!(canonical_payload_hash(&json!({"x":"a".repeat(MAX_BYTES)})).is_err());
    let mut deep = Value::Null;
    for _ in 0..65 {
        deep = json!([deep]);
    }
    assert!(canonical_payload_hash(&json!({"x":deep})).is_err());
}
#[test]
fn principal_operation_scope_and_late_audit_failure() {
    let mut db = fixture();
    let payload = json!({});
    for i in 0..3 {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let mut v = input(&payload, "shared");
        if i == 1 {
            v.subject = "other-service";
        }
        if i == 2 {
            v.operation = Operation::JournalReverse;
        }
        let (_, r) = accepted(execute(&tx, v, effect).unwrap());
        assert!(!r.replayed);
        tx.commit().unwrap();
    }
    db.execute_batch("CREATE TRIGGER synthetic_late_audit BEFORE INSERT ON audit_log WHEN NEW.event_type='idempotency_original' BEGIN SELECT RAISE(ABORT,'late failure'); END").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(execute(&tx, input(&payload, "late"), effect).is_err());
    tx.rollback().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM receipt_test_effects", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM mutation_idempotency_tombstones",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        3
    );
}
#[test]
fn malformed_stored_outcome_and_id_exhaustion_never_reexecute() {
    let mut db = fixture();
    let payload = json!({});
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    execute(&tx, input(&payload, "key"), effect).unwrap();
    tx.commit().unwrap();
    db.execute(
        "UPDATE mutation_idempotency_outcomes SET outcome_json='{\"ok\":false}'",
        [],
    )
    .unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(
        execute(&tx, input(&payload, "key"), |_| panic!(
            "malformed must not execute"
        ))
        .is_err()
    );
    tx.rollback().unwrap();
    db.execute(
        "INSERT INTO audit_log(id,event_type,entity_type,message) VALUES(?1,'test','test','test')",
        [i64::MAX],
    )
    .unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(execute(&tx, input(&payload, "overflow"), effect).is_err());
    tx.rollback().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM receipt_test_effects", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn deferred_read_transaction_and_invalid_inputs_refuse_before_callback() {
    let mut db = fixture();
    let payload = json!({});
    let tx = db.transaction().unwrap();
    assert_eq!(
        execute(&tx, input(&payload, "key"), |_| panic!("read only"))
            .err()
            .unwrap()
            .error
            .code,
        "transaction_required"
    );
    tx.rollback().unwrap();
    for kind in ["key", "subject", "actor", "clock", "payload"] {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let invalid_payload = json!([]);
        let mut v = input(&payload, "valid");
        match kind {
            "key" => v.key = "",
            "subject" => v.subject = "",
            "actor" => v.created_by = "",
            "clock" => v.now_ms = i64::MAX,
            _ => v.payload = &invalid_payload,
        }
        assert!(execute(&tx, v, |_| panic!("invalid input")).is_err());
        tx.rollback().unwrap();
    }
}
/// Probe used only by the differential/process harness on synthetic canonical fixtures.
#[test]
#[ignore = "native idempotency process helper"]
fn child() {
    let request: Value =
        serde_json::from_str(&std::env::var("RM_RECEIPT_REQUEST").unwrap()).unwrap();
    let report = std::env::var("RM_RECEIPT_REPORT").unwrap();
    if request["mode"] == "hash" {
        let result = canonical_payload_hash(&request["payload"])
            .map(|v| json!({"ok":true,"hash":v}))
            .unwrap_or_else(|e| json!({"ok":false,"code":e.code}));
        std::fs::write(report, serde_json::to_vec(&result).unwrap()).unwrap();
        return;
    }
    let mut db = Connection::open(std::env::var("RM_RECEIPT_DB").unwrap()).unwrap();
    db.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
        .unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    if request["mode"] == "prune" {
        let count = prune(&tx, request["now"].as_i64().unwrap()).unwrap();
        tx.commit().unwrap();
        std::fs::write(
            report,
            serde_json::to_vec(&json!({"ok":true,"pruned":count})).unwrap(),
        )
        .unwrap();
        return;
    }
    let operation = match request["operation"].as_str().unwrap_or("journal_post") {
        "closed_history_archive_import" => Operation::ClosedHistoryArchiveImport,
        "journal_post" => Operation::JournalPost,
        "journal_reverse" => Operation::JournalReverse,
        "expense_book" => Operation::ExpenseBook,
        "payable_register" => Operation::PayableRegister,
        "payable_pay" => Operation::PayablePay,
        "bank_reconciliation_correction_apply" => Operation::BankReconciliationCorrectionApply,
        "direct_bank_purchase_payable_correction_apply" => {
            Operation::DirectBankPurchasePayableCorrectionApply
        }
        _ => panic!("invalid fixture operation"),
    };
    let v = Input {
        key: request["key"].as_str().unwrap(),
        operation,
        principal_kind: if request["kind"] == "user" {
            PrincipalKind::User
        } else {
            PrincipalKind::ServiceAccount
        },
        subject: request["subject"].as_str().unwrap_or("synthetic-service"),
        payload: &request["payload"],
        created_by: request["actor"].as_str().unwrap_or("agent:synthetic"),
        created_by_program: request["program"].as_str().unwrap_or("test"),
        now_ms: request["now"].as_i64().unwrap_or(1_767_225_600_000),
    };
    let result = execute(&tx, v, |tx| {
        let value = effect(tx)?;
        if request["reject"] == true {
            Ok(json!({"ok":false,"errors":["synthetic business rejection"]}))
        } else if request["fail"] == true {
            Err(storage("synthetic callback failure"))
        } else if request["large"] == true {
            Ok(json!({"ok":true,"large":"x".repeat(MAX_BYTES)}))
        } else {
            Ok(value)
        }
    });
    let output = match result {
        Ok(Outcome::Accepted { result, receipt }) => {
            json!({"ok":true,"result":result,"receipt":receipt})
        }
        Ok(Outcome::Refused { code }) => json!({"ok":false,"code":code}),
        Err(f) => {
            tx.rollback().unwrap();
            std::fs::write(
                report,
                serde_json::to_vec(
                    &json!({"ok":false,"code":f.error.code,"rejection":f.rejection}),
                )
                .unwrap(),
            )
            .unwrap();
            return;
        }
    };
    if let Ok(marker) = std::env::var("RM_RECEIPT_MARKER") {
        std::fs::write(marker, "before-commit").unwrap();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
    }
    if let Some(ms) = request["commitTimeoutMs"].as_u64() {
        tx.busy_timeout(std::time::Duration::from_millis(ms))
            .unwrap();
    }
    match tx.commit() {
        Ok(()) => std::fs::write(report, serde_json::to_vec(&output).unwrap()).unwrap(),
        Err(_) => std::fs::write(
            report,
            b"{\"ok\":false,\"code\":\"commit_outcome_unknown\"}",
        )
        .unwrap(),
    }
}
#[test]
fn restored_accepted_key_never_becomes_new_intent_under_fresh_principal() {
    let mut source = fixture();
    let payload = json!({"synthetic":"accepted response lost"});
    let tx = source
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let (_, original) = accepted(execute(&tx, input(&payload, "lost-original"), effect).unwrap());
    tx.commit().unwrap();
    let uuid: String = source
        .query_row("SELECT ledger_uuid FROM ledger_identity", [], |r| r.get(0))
        .unwrap();
    // Restore keeps this exact ledger/receipt data and appends the immutable imported boundary.
    let high: i64 = source
        .query_row(
            "SELECT MAX(id) FROM mutation_idempotency_tombstones",
            [],
            |r| r.get(0),
        )
        .unwrap();
    source.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('native_restore_receipt_boundary','mutation_idempotency_tombstones',?1,'Imported receipt high-water mark; archive aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa; original keys require manual reconciliation under fresh identities','system:portable-restore')",[high.to_string()]).unwrap();
    let tx = source
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let mut restored = input(&payload, "lost-original");
    restored.subject = "fresh-restored-owner";
    restored.principal_kind = PrincipalKind::User;
    let refusal = execute(&tx, restored, |_| {
        panic!("accepted source operation must not execute again")
    })
    .err()
    .unwrap();
    assert_eq!(refusal.error.code, "RESTORED_KEY_RECONCILIATION_REQUIRED");
    tx.rollback().unwrap();
    let tx = source
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let new_payload = json!({"synthetic":"new intended operation"});
    let mut next = input(&new_payload, "fresh-key");
    next.subject = "fresh-restored-owner";
    next.principal_kind = PrincipalKind::User;
    let (_, fresh) = accepted(execute(&tx, next, effect).unwrap());
    tx.commit().unwrap();
    assert_ne!(fresh.receipt_id, original.receipt_id);
    assert_eq!(
        source
            .query_row("SELECT ledger_uuid FROM ledger_identity", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        uuid
    );
    assert_eq!(
        source
            .query_row("SELECT COUNT(*) FROM receipt_test_effects", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        source
            .query_row(
                "SELECT COUNT(*) FROM mutation_idempotency_outcomes WHERE tombstone_id=?1",
                [original.receipt_id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}
