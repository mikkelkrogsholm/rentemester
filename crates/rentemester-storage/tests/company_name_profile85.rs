//! Synthetic domain proof only: admission/authentication remain the unchanged owner pipeline.
use rentemester_storage::{company_creation, company_setup};
use rusqlite::{Connection, TransactionBehavior};
use serde_json::{Value, json};

fn company() -> Connection {
    let input = serde_json::from_value(json!({"name":"Synthetic Original","slug":"synthetic-fixed","fiscalYearStartMonth":"4","fiscalYearLabelStrategy":"span","vatPeriodType":"quarter"})).unwrap();
    let settings = company_creation::settings(&input).unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    company_creation::initialize(&mut db, &settings, "synthetic-subject").unwrap();
    db
}
fn read(db: &mut Connection) -> Value {
    let tx = db.transaction().unwrap();
    company_setup::profile(&tx).unwrap()
}
fn count(db: &Connection, table: &str) -> i64 {
    db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn ledger(db: &Connection) -> String {
    db.query_row(
        "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
        [],
        |r| r.get(0),
    )
    .unwrap()
}
fn patch(db: &mut Connection, input: Value) -> Value {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let result =
        company_setup::update_profile(&tx, input.as_object().unwrap(), "synthetic-subject")
            .unwrap();
    tx.commit().unwrap();
    result
}
fn refused(db: &mut Connection, input: Value) {
    let before = read(db);
    let audit = count(db, "audit_log");
    let bank = count(db, "bank_accounts");
    let identity = ledger(db);
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(
        company_setup::update_profile(&tx, input.as_object().unwrap(), "synthetic-subject")
            .is_err()
    );
    // Exactly the caller's refusal contract: never commit an unsuccessful mutation.
    tx.rollback().unwrap();
    assert_eq!(read(db), before);
    assert_eq!(count(db, "audit_log"), audit);
    assert_eq!(count(db, "bank_accounts"), bank);
    assert_eq!(ledger(db), identity);
}

#[test]
fn name_and_partial_address_are_canonical_audited_and_not_a_new_company() {
    let mut db = company();
    let original = read(&mut db);
    let identity = ledger(&db);
    let events = count(&db, "audit_log");
    let updated = patch(
        &mut db,
        json!({"name":"  Synthetic Æ Renamed  ","address":"  Synthetic Street  "}),
    );
    let mut expected = original.clone();
    expected["name"] = json!("Synthetic Æ Renamed");
    expected["address"] = json!("Synthetic Street");
    assert_eq!(updated, expected);
    assert_eq!(read(&mut db), expected);
    assert_eq!(count(&db, "audit_log"), events + 1);
    let event: (String, String, String, String) = db
        .query_row(
            "SELECT event_type,entity_type,entity_id,actor FROM audit_log ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        event,
        (
            "company_profile_update".into(),
            "company_setup".into(),
            "1".into(),
            "user:synthetic-subject".into()
        )
    );
    assert_eq!(ledger(&db), identity);
    for table in [
        "accounts",
        "journal_entries",
        "journal_lines",
        "bank_accounts",
    ] {
        assert_eq!(count(&db, table), 0);
    }
    let tx = db.transaction().unwrap();
    let projected = company_setup::accounts(&tx, "synthetic-fixed").unwrap();
    assert_eq!(projected["slug"], "synthetic-fixed");
    assert_eq!(projected["company"], expected);
    drop(tx);
    // Old patches still change only what is supplied; no name defaulting.
    let updated = patch(
        &mut db,
        json!({"city":"Synthetic City","paymentTermsDays":14}),
    );
    assert_eq!(updated["name"], expected["name"]);
    assert_eq!(updated["address"], expected["address"]);
    for field in [
        "id",
        "currency",
        "country",
        "fiscalYearStartMonth",
        "fiscalYearLabelStrategy",
        "createdAt",
    ] {
        assert_eq!(updated[field], original[field]);
    }
}

#[test]
fn invalid_names_and_late_refusals_rollback_profile_bank_and_audit() {
    let mut db = company();
    for name in [
        Value::Null,
        json!(false),
        json!(12),
        json!(""),
        json!("  \u{2003} "),
        json!("bad\nname"),
        json!("\tSynthetic"),
        json!("bad\u{007f}name"),
        json!("x".repeat(1025)),
        json!("ø".repeat(513)),
    ] {
        refused(
            &mut db,
            json!({"address":"Must rollback","name":name,"payment":{"bankName":"Synthetic Bank","registrationNo":"1234","accountNo":"1234567890"}}),
        );
    }
    // Payment sorts after name: exercise rollback of a valid rename and bank insertion
    // when a later unknown field refuses the transaction.
    refused(
        &mut db,
        json!({"name":"Must rollback","payment":{"bankName":"Synthetic Bank","registrationNo":"1234","accountNo":"1234567890"},"zzzUnknown":true}),
    );
    for extra in [
        json!({"name":"Must rollback","actor":"invented"}),
        json!({"name":"Must rollback","slug":"other"}),
        json!({"name":"Must rollback","companyId":2}),
        json!({"name":"Must rollback","paymentTermsDays":366}),
        json!({"name":"Must rollback","vatPeriodType":"annual"}),
        json!({"name":"Must rollback","payment":{"unknown":"value"}}),
    ] {
        refused(&mut db, extra);
    }
    assert_eq!(
        patch(&mut db, json!({"name":"ø".repeat(512)}))["name"],
        "ø".repeat(512)
    );
    assert_eq!(
        patch(&mut db, json!({"name":"x".repeat(1024)}))["name"],
        "x".repeat(1024)
    );
}

#[test]
fn rename_does_not_bypass_payment_append_only_or_vat_history_guards() {
    let mut db = company();
    let with_bank = patch(
        &mut db,
        json!({"payment":{"bankName":"Synthetic Bank","registrationNo":"1234","accountNo":"1234567890"}}),
    );
    refused(
        &mut db,
        json!({"name":"Must rollback","payment":{"bankName":"Changed bank"}}),
    );
    let renamed = patch(&mut db, json!({"name":"Synthetic New Label"}));
    assert_eq!(renamed["payment"], with_bank["payment"]);
    assert_eq!(count(&db, "bank_accounts"), 1);
    // Existing synthetic VAT fixture on the canonical fresh schema, not a substitute schema.
    db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('V','Synthetic VAT','vat','debit'); INSERT INTO journal_entries(id,entry_no,transaction_date,text,entry_hash,rule_version) VALUES(1,'SYN-1','2026-01-10','Synthetic','synthetic','synthetic'); INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(1,1,25,0);").unwrap();
    refused(
        &mut db,
        json!({"name":"Must rollback","vatPeriodType":null}),
    );
    refused(
        &mut db,
        json!({"name":"Must rollback","vatPeriodType":"month"}),
    );
    assert_eq!(count(&db, "journal_entries"), 1);
    assert_eq!(count(&db, "journal_lines"), 1);
    assert_eq!(read(&mut db)["vatPeriodType"], "quarter");
}
