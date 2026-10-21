use super::*;
use crate::journal_post::Actor;
use rentemester_engine::{fiscal_year::Date, manual_journal::ManualJournal};
use rusqlite::{Connection, TransactionBehavior};
use sha2::{Digest, Sha256};
const CSV: &str = "transaction_date,text,amount,currency\n2026-04-01,Synthetic source,12.34,DKK\n2026-04-02,Synthetic other,12.34,DKK\n";
fn posted(tx: &Transaction<'_>, counter: &str) -> i64 {
    let request:ManualJournal=serde_json::from_value(json!({"transactionDate":"2026-04-01","text":"Synthetic bank-audit accounting","currency":"DKK","lines":[{"accountNo":"1751","debitAmount":"12.34"},{"accountNo":counter,"creditAmount":"12.34"}]})).unwrap();
    let value = crate::journal_post::post(
        tx,
        &request,
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        Date::parse("2026-05-01").unwrap(),
        Actor {
            subject: "user:synthetic",
            program: "rentemester-rust",
        },
    )
    .unwrap();
    assert_eq!(value["ok"], true);
    value["entryId"].as_i64().unwrap()
}

fn fixture() -> (Connection, Connection) {
    let mut c = Connection::open_in_memory().unwrap();
    let mut l = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic original bank").unwrap();
    l.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('1751','Synthetic bank','asset','debit'),('8751','Synthetic equity one','equity','credit'),('8752','Synthetic equity two','equity','credit');INSERT INTO bank_accounts(slug,name,ledger_account_no,currency)VALUES('synthetic','Synthetic bank','1751','DKK');").unwrap();
    (c, l)
}
fn import(tx: &Transaction<'_>) {
    crate::bank::import(
        tx,
        CSV,
        &rentemester_engine::bank::parse(CSV).unwrap(),
        Some("synthetic"),
        "synthetic",
        "2026-04-01T00:00:00Z",
        &format!("bank/processed/{:x}.txt", Sha256::digest(CSV.as_bytes())),
    )
    .unwrap();
    crate::unmatched_bank::sync(tx).unwrap();
}
fn request(journal: i64) -> Link {
    Link {
        confirm: true,
        bank_transaction_id: 1,
        journal_entry_id: journal,
        match_method: "source-reference".into(),
        source_reference: Some("synthetic:original".into()),
        note: None,
    }
}
#[test]
fn original_link_native_proof_claims_resolution_and_replay() {
    let (_c, mut l) = fixture();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    import(&tx);
    let journal = posted(&tx, "8751");
    let input = request(journal);
    let first = link(
        &tx,
        &input,
        "service_synthetic",
        PrincipalKind::ServiceAccount,
    )
    .unwrap();
    assert_eq!(first["data"]["idempotent"], false);
    assert_eq!(tx.query_row("SELECT count(*) FROM exceptions WHERE status='open' AND type='UNMATCHED_BANK_TRANSACTION'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
    let (note,by):(String,String)=tx.query_row("SELECT resolution_note,resolved_by FROM exceptions WHERE related_bank_transaction_id=1",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert!(note.contains("no new posting"));
    assert_eq!(by, "user:service_synthetic");
    let changes = lifetime_changes(&tx);
    let second = link(
        &tx,
        &input,
        "service_synthetic",
        PrincipalKind::ServiceAccount,
    )
    .unwrap();
    assert_eq!(second["data"]["idempotent"], true);
    assert_eq!(changes, lifetime_changes(&tx));
    verify_journals(&tx).unwrap();
    assert!(
        crate::periods::bank_audit::verify_supported(&tx)
            .unwrap()
            .is_empty()
    );
    let raw: String = tx
        .query_row(
            "SELECT message FROM audit_log WHERE event_type='bank_journal_reconciliation_link'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut audit: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(audit["compatibility"]["note"], Value::Null);
    assert_eq!(
        audit["compatibility"]["sourceReference"],
        "synthetic:original"
    );
    let facts = audit["plan"].clone();
    verify_link_audit(&facts, NO_NOTE, "user:service_synthetic", &audit).unwrap();
    audit["compatibility"]["principalSubjectId"] = json!("forged");
    assert!(verify_link_audit(&facts, NO_NOTE, "user:service_synthetic", &audit).is_err());
}
fn lifetime_changes(tx: &Transaction<'_>) -> i64 {
    tx.query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap()
}
#[test]
fn original_correction_service_proof_raw_receipt_and_native_v1_compatibility() {
    let (_c, mut l) = fixture();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    import(&tx);
    let old = posted(&tx, "8751");
    // Existing native v1 human proof remains accepted beside original service v2.
    let plan = native::journal_plan(&tx, 1, old, true).unwrap();
    native::apply(
        &tx,
        &native::Apply {
            confirm: true,
            bank_transaction_id: 1,
            journal_entry_id: old,
            plan_hash: plan["planHash"].as_str().unwrap().into(),
            note: "Synthetic reviewed native link".into(),
        },
        "synthetic",
    )
    .unwrap();
    assert!(
        crate::periods::bank_audit::verify_supported(&tx)
            .unwrap()
            .is_empty()
    );
    let reversal = crate::journal_reversal::reverse(
        &tx,
        &crate::journal_reversal::Input {
            entry_id: rentemester_engine::identifiers::JournalEntryId::from_sqlite(old),
            transaction_date: Date::parse("2026-04-01").unwrap(),
            reason: "Synthetic explicit reversal".into(),
        },
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        Date::parse("2026-05-01").unwrap(),
        Actor {
            subject: "user:synthetic",
            program: "rentemester-rust",
        },
    )
    .unwrap();
    assert_eq!(reversal["ok"], true);
    assert!(
        !crate::periods::bank_audit::verify_supported(&tx)
            .unwrap()
            .is_empty()
    );
    let replacement = posted(&tx, "8752");
    let plan = native::correction::plan(&tx, 1, replacement).unwrap();
    let mut input = Correct {
        input: native::correction::Apply {
            confirm: true,
            bank_transaction_id: 1,
            replacement_journal_entry_id: replacement,
            expected_reconciliation_id: plan["plan"]["reconciliationId"].as_str().unwrap().into(),
            plan_hash: plan["plan"]["planHash"].as_str().unwrap().into(),
            reason: "  Synthetic raw reason  ".into(),
            idempotency_key: "synthetic-original-correction".into(),
        },
        expected_principal: Some(ExpectedPrincipal {
            kind: "service-account".into(),
            subject_id: "service_synthetic".into(),
        }),
    };
    let first = correct(
        &tx,
        &input,
        "service_synthetic",
        PrincipalKind::ServiceAccount,
        1780000000000,
    )
    .unwrap();
    assert_eq!(first["data"]["receipt"]["replayed"], false);
    assert_eq!(first["data"]["idempotent"], false);
    let repeated = correct(
        &tx,
        &input,
        "service_synthetic",
        PrincipalKind::ServiceAccount,
        1780000000100,
    )
    .unwrap();
    assert_eq!(repeated["data"]["receipt"]["replayed"], true);
    assert_eq!(repeated["data"]["idempotent"], true);
    assert!(
        crate::periods::bank_audit::verify_supported(&tx)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        tx.query_row(
            "SELECT count(*) FROM bank_reconciliation_correction_events",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    input.expected_principal.as_mut().unwrap().subject_id = "forged".into();
    assert_eq!(
        correct(
            &tx,
            &input,
            "service_synthetic",
            PrincipalKind::ServiceAccount,
            1780000000200
        )
        .unwrap_err()
        .code,
        "access_denied"
    );
    input.expected_principal = None;
    input.input.reason = "Synthetic raw reason".into();
    assert!(
        correct(
            &tx,
            &input,
            "service_synthetic",
            PrincipalKind::ServiceAccount,
            1780000000300
        )
        .is_err()
    );
    let raw: String = tx
        .query_row(
            "SELECT message FROM audit_log WHERE event_type='bank_reconciliation_corrected'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut audit: Value = serde_json::from_str(&raw).unwrap();
    let facts = audit["plan"].clone();
    verify_correction_audit(
        &facts,
        "Synthetic raw reason",
        "user:service_synthetic",
        "service-account",
        "service_synthetic",
        &audit,
    )
    .unwrap();
    audit["compatibility"]["principalKind"] = json!("user");
    assert!(
        verify_correction_audit(
            &facts,
            "Synthetic raw reason",
            "user:service_synthetic",
            "service-account",
            "service_synthetic",
            &audit
        )
        .is_err()
    );
}
