use super::*;
use crate::journal_post::{self, Actor};
use rentemester_engine::{
    fiscal_year::Date, identifiers::JournalEntryId, manual_journal::ManualJournal,
};
use rusqlite::{Connection, TransactionBehavior};

fn fixture() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    let mut l = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic ledger audit").unwrap();
    l.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('2000','Synthetic asset','asset','debit'),('5000','Synthetic equity','equity','credit'),('6000','Synthetic expense','expense','debit')").unwrap();
    l
}
fn request() -> ManualJournal {
    serde_json::from_value(json!({"transactionDate":"2026-01-01","text":"Synthetic capital","currency":"DKK","lines":[{"accountNo":"2000","debitAmount":"10.01"},{"accountNo":"5000","creditAmount":"10.01"}]})).unwrap()
}
fn post(tx: &Transaction<'_>) -> Value {
    journal_post::post(
        tx,
        &request(),
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        Date::parse("2026-02-15").unwrap(),
        Actor {
            subject: "user:synthetic",
            program: "rentemester-rust",
        },
    )
    .unwrap()
}
fn reverse(tx: &Transaction<'_>) -> Value {
    crate::journal_reversal::reverse(
        tx,
        &crate::journal_reversal::Input {
            entry_id: JournalEntryId::from_sqlite(1),
            transaction_date: Date::parse("2026-01-02").unwrap(),
            reason: "Synthetic correction".into(),
        },
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        Date::parse("2026-02-15").unwrap(),
        Actor {
            subject: "user:synthetic",
            program: "rentemester-rust",
        },
    )
    .unwrap()
}
fn mutate(db: &Connection, table: &str, apply: impl FnOnce()) {
    let mut q = db
        .prepare(
            "SELECT name,sql FROM sqlite_schema WHERE type='trigger' AND tbl_name=?1 ORDER BY name",
        )
        .unwrap();
    let triggers = q
        .query_map([table], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    drop(q);
    for (name, _) in &triggers {
        db.execute_batch(&format!("DROP TRIGGER \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
    }
    apply();
    for (_, sql) in triggers {
        db.execute_batch(&sql).unwrap();
    }
}
fn rehash(db: &Connection) {
    mutate(db, "journal_entries", || {
        let ids = db
            .prepare("SELECT id FROM journal_entries ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let mut previous = "GENESIS".to_owned();
        for id in ids {
            let hash = crate::journal_hash::stored_hash(db, id, &previous).unwrap();
            db.execute(
                "UPDATE journal_entries SET previous_hash=?1,entry_hash=?2 WHERE id=?3",
                params![previous, hash, id],
            )
            .unwrap();
            previous = hash;
        }
    });
}
fn has(report: &Value, code: &str) -> bool {
    report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["code"] == code)
}
fn receipt(tx: &Transaction<'_>) {
    let payload = json!({"request":request()});
    let outcome = crate::idempotency::execute(
        tx,
        crate::idempotency::Input {
            key: "synthetic-ledger-audit",
            operation: crate::idempotency::Operation::JournalPost,
            principal_kind: crate::idempotency::PrincipalKind::User,
            subject: "synthetic",
            payload: &payload,
            created_by: "user:synthetic",
            created_by_program: "rentemester-rust",
            now_ms: 1_767_225_600_000,
        },
        |tx| Ok(post(tx)),
    )
    .unwrap();
    assert!(matches!(
        outcome,
        crate::idempotency::Outcome::Accepted { .. }
    ));
}
#[test]
fn populated_native_post_receipt_and_reversal_are_deterministic_and_read_only() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    receipt(&tx);
    assert_eq!(reverse(&tx)["ok"], true);
    let before = crate::test_support::typed_tables(&tx);
    let changes = tx.total_changes();
    let report = verify(&tx).unwrap();
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(report["checked"]["entries"], 2);
    assert_eq!(report["checked"]["lines"], 4);
    assert_eq!(report["checked"]["reversals"], 1);
    assert_eq!(report["checked"]["journalReceipts"], 1);
    assert_eq!(report, verify(&tx).unwrap());
    assert_eq!(before, crate::test_support::typed_tables(&tx));
    assert_eq!(changes, tx.total_changes());
    assert!(crate::periods::audit::verify(&tx).unwrap().is_empty());
}
#[test]
fn native_audit_tail_deletion_and_retained_receipt_outcome_tamper_are_not_hidden() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    receipt(&tx);
    mutate(&tx, "audit_log", || {
        tx.execute(
            "DELETE FROM audit_log WHERE entity_type='journal_entry'",
            [],
        )
        .unwrap();
    });
    let report = verify(&tx).unwrap();
    assert!(
        has(&report, "MISSING_OR_DUPLICATE_NATIVE_JOURNAL_AUDIT"),
        "{report}"
    );
    mutate(&tx, "mutation_idempotency_outcomes", || {
        tx.execute(
            "UPDATE mutation_idempotency_outcomes SET outcome_json='{}'",
            [],
        )
        .unwrap();
    });
    let report = verify(&tx).unwrap();
    assert!(has(&report, "INVALID_NATIVE_RECEIPT_OUTCOME"), "{report}");
    assert!(has(&report, "MISSING_OR_DUPLICATE_NATIVE_JOURNAL_AUDIT"));
}
#[test]
fn historical_document_and_audit_exceptions_preserve_all_other_invariants_and_old_close_semantics()
{
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    post(&tx);
    mutate(&tx, "journal_entries", || {
        tx.execute(
            "UPDATE journal_entries SET created_by_program='rentemester-import-postings'",
            [],
        )
        .unwrap();
    });
    mutate(&tx, "journal_lines", || {
        tx.execute("UPDATE journal_lines SET account_id=(SELECT id FROM accounts WHERE account_no='6000') WHERE debit_amount>0",[]).unwrap();
    });
    mutate(&tx, "audit_log", || {
        tx.execute("DELETE FROM audit_log", []).unwrap();
    });
    rehash(&tx);
    let report = verify(&tx).unwrap();
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(report["checked"]["historicalEntries"], 1);
    let close = crate::periods::audit::verify(&tx).unwrap();
    assert!(
        close
            .iter()
            .any(|v| v["code"] == "UNSUPPORTED_HISTORICAL_PROVENANCE")
    );
    assert!(close.iter().any(|v| v["code"] == "MISSING_JOURNAL_AUDIT"));
    mutate(&tx, "journal_lines", || {
        tx.execute(
            "UPDATE journal_lines SET credit_amount=9 WHERE credit_amount>0",
            [],
        )
        .unwrap();
    });
    rehash(&tx);
    let report = verify(&tx).unwrap();
    assert!(has(&report, "UNBALANCED_OR_EMPTY_JOURNAL"), "{report}");
}
#[test]
fn migration_fixture_can_lack_audit_but_unmarked_expense_cannot_lose_document_evidence() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    post(&tx);
    mutate(&tx, "journal_entries", || {
        tx.execute(
            "UPDATE journal_entries SET created_by_program='synthetic-migration-fixture'",
            [],
        )
        .unwrap();
    });
    mutate(&tx, "audit_log", || {
        tx.execute("DELETE FROM audit_log", []).unwrap();
    });
    rehash(&tx);
    assert_eq!(verify(&tx).unwrap()["ok"], true);
    mutate(&tx, "journal_lines", || {
        tx.execute("UPDATE journal_lines SET account_id=(SELECT id FROM accounts WHERE account_no='6000') WHERE debit_amount>0",[]).unwrap();
    });
    rehash(&tx);
    assert!(has(&verify(&tx).unwrap(), "MISSING_JOURNAL_DOCUMENT"));
}
#[test]
fn balanced_rehashed_reversal_tamper_and_issued_sequence_tail_are_detected() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    post(&tx);
    reverse(&tx);
    mutate(&tx, "journal_lines", || {
        tx.execute("UPDATE journal_lines SET debit_amount=CASE WHEN debit_amount>0 THEN 9 ELSE 0 END,credit_amount=CASE WHEN credit_amount>0 THEN 9 ELSE 0 END WHERE journal_entry_id=2",[]).unwrap();
    });
    rehash(&tx);
    let report = verify(&tx).unwrap();
    assert!(has(&report, "REVERSAL_NOT_EXACT_INVERSE"), "{report}");
    assert!(!has(&report, "UNBALANCED_OR_EMPTY_JOURNAL"));
    tx.execute(
        "INSERT INTO sequences(kind,scope,value)VALUES('journal_entry','synthetic:2025-26',100000)",
        [],
    )
    .unwrap();
    assert!(has(&verify(&tx).unwrap(), "JOURNAL_SEQUENCE_TRUNCATED"));
}
#[test]
fn reversal_metadata_uses_normalized_currency_and_exact_cent_six_decimal_semantics() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    post(&tx);
    reverse(&tx);
    mutate(&tx, "journal_entries", || {
        tx.execute("UPDATE journal_entries SET currency=' dkk ' WHERE id=2", [])
            .unwrap();
    });
    rehash(&tx);
    assert_eq!(verify(&tx).unwrap()["ok"], true);
    assert!(
        crate::periods::audit::verify(&tx)
            .unwrap()
            .iter()
            .any(|v| v["code"] == "INVALID_REVERSAL_TARGET")
    );
    mutate(&tx, "journal_entries", || {
        tx.execute("UPDATE journal_entries SET amount_dkk=10.01 WHERE id=2", [])
            .unwrap();
    });
    rehash(&tx);
    assert!(has(&verify(&tx).unwrap(), "REVERSAL_METADATA_MISMATCH"));
    assert_eq!(
        rate(ValueRef::Real(7.45000001)).unwrap(),
        rate(ValueRef::Real(7.45)).unwrap()
    );
    assert_ne!(
        rate(ValueRef::Real(7.450001)).unwrap(),
        rate(ValueRef::Real(7.45)).unwrap()
    );
}
#[test]
fn audit_id_gaps_native_audit_spoof_and_hash_tamper_are_reported() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    post(&tx);
    tx.execute_batch("INSERT INTO audit_log(event_type,entity_type,message,actor)VALUES('synthetic','synthetic','one','user:synthetic'),('synthetic','synthetic','two','user:synthetic')").unwrap();
    mutate(&tx, "audit_log", || {
        tx.execute("DELETE FROM audit_log WHERE id=2", []).unwrap();
        tx.execute(
            "UPDATE audit_log SET event_type='synthetic' WHERE entity_type='journal_entry'",
            [],
        )
        .unwrap();
    });
    mutate(&tx, "journal_entries", || {
        tx.execute(
            "UPDATE journal_entries SET entry_hash=?1 WHERE id=1",
            ["f".repeat(64)],
        )
        .unwrap();
    });
    let report = verify(&tx).unwrap();
    assert!(has(&report, "MISSING_OR_DUPLICATE_NATIVE_JOURNAL_AUDIT"));
    assert!(report["errors"].as_array().unwrap().iter().any(|v| {
        v["error"]
            .as_str()
            .is_some_and(|s| s.contains("entry_hash mismatch"))
    }));
    assert!(
        report["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["error"] == "audit log id sequence has gaps")
    );
}
#[test]
fn closed_history_and_opening_are_populated_validated_and_tampering_is_not_a_blanket_blocker() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let bytes=serde_json::to_vec(&json!({"format":rentemester_engine::archive_import::PACKAGE,"currency":"DKK","files":[{"path":"2024/Posteringer.csv","contentBase64":STANDARD.encode("Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Moms;Beløb;Saldo\n2000;Synthetic asset;2024-06-01;1;Historical;Synthetic first;;10,01;10,01\n5000;Synthetic equity;2024-06-01;1;Historical;Synthetic counterpart;;-10,01;-10,01\n")},{"path":"2024/SaldoBalance.csv","contentBase64":STANDARD.encode("Konto;Kontonavn;Beløb\n2000;Synthetic asset;10,01\n5000;Synthetic equity;-10,01\n")},{"path":"2025/Posteringer.csv","contentBase64":STANDARD.encode("Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Moms;Beløb;Saldo\n2000;Excluded;2025-01-01;0;;Primobeholdning;;0;0\n")}]})).unwrap();
    let (source, parsed) = crate::archive_import::decode_source(&bytes).unwrap();
    let prepared = crate::archive_import::prepare(&tx, &source, &parsed).unwrap();
    let cmd = crate::archive_import::Apply {
        plan_hash: prepared.review()["planHash"].as_str().unwrap().into(),
        idempotency_key: "synthetic-archive".into(),
        confirm: true,
    };
    crate::archive_import::apply(&tx, &prepared, &cmd, "synthetic", 1_792_497_600_000).unwrap();
    let proofs = crate::archive_import::originals(&tx)
        .unwrap()
        .iter()
        .map(|r| crate::archive_import::verify_original(&tx, r, &bytes).unwrap())
        .collect::<Vec<_>>();
    let input:crate::opening_balance::Input=serde_json::from_value(json!({"cutOverDate":"2025-01-01","lines":[{"accountNo":"2000","debitAmount":"10.01"},{"accountNo":"5000","creditAmount":"10.01"}],"note":"Synthetic approved opening"})).unwrap();
    let latest = Date::parse("2026-10-20").unwrap();
    let prepared = crate::opening_balance::prepare(&tx, &input, &proofs, latest).unwrap();
    let cmd = crate::opening_balance::Apply {
        input,
        plan_hash: prepared.review()["planHash"].as_str().unwrap().into(),
        idempotency_key: "synthetic-opening".into(),
        confirm: true,
    };
    crate::opening_balance::apply(
        &tx,
        &cmd,
        &proofs,
        latest,
        Actor {
            subject: "user:synthetic",
            program: crate::opening_balance::PROGRAM,
        },
        1_792_497_600_000,
    )
    .unwrap();
    let report = verify(&tx).unwrap();
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(report["checked"]["archiveYears"], 1);
    assert_eq!(report["checked"]["archivePostings"], 2);
    assert_eq!(report["checked"]["openingBalances"], 1);
    mutate(&tx, "audit_log", || {
        tx.execute("UPDATE audit_log SET message='{}' WHERE event_type IN ('native_closed_history_archive_import','native_opening_balance')",[]).unwrap();
    });
    let report = verify(&tx).unwrap();
    assert!(has(&report, "INVALID_CLOSED_HISTORY"), "{report}");
    assert!(has(&report, "INVALID_OPENING_HISTORY"), "{report}");
}

#[test]
fn structural_checks_report_zero_lines_foreign_keys_duplicate_reversals_and_invalid_status() {
    let mut db = fixture();
    db.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    post(&tx);
    post(&tx);
    reverse(&tx);
    mutate(&tx, "journal_lines", || {
        tx.execute("INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount)VALUES(1,(SELECT id FROM accounts WHERE account_no='2000'),0,0)",[]).unwrap();
        tx.execute("INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount)VALUES(99999,99999,1,0)",[]).unwrap();
    });
    mutate(&tx, "journal_entries", || {
        tx.execute(
            "UPDATE journal_entries SET status='reversed',reversal_of_entry_id=1 WHERE id=2",
            [],
        )
        .unwrap();
        tx.execute(
            "UPDATE journal_entries SET status='reversed' WHERE id=1",
            [],
        )
        .unwrap();
    });
    rehash(&tx);
    let report = verify(&tx).unwrap();
    for code in [
        "INVALID_JOURNAL_LINE",
        "INVALID_REVERSAL_STATUS",
        "INVALID_REVERSAL_TARGET",
        "MULTIPLE_REVERSALS",
        "REVERSAL_NOT_EXACT_INVERSE",
    ] {
        assert!(has(&report, code), "missing {code}: {report}");
    }
    assert!(
        report["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["table"] == "journal_lines" && v["parent"] == "accounts")
    );
    assert!(
        report["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["table"] == "journal_lines" && v["parent"] == "journal_entries")
    );
}
