use super::*;
use crate::sales::Artifact;
use rusqlite::{Connection, TransactionBehavior};
fn fixture() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Portfolio").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET address='Synthetic Street',cvr='87654321',vat_period_type='quarter',fiscal_year_start_month=7,fiscal_year_label_strategy='span' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R','Synthetic debtor','asset','debit'),('S','Synthetic sales','income','credit'),('V','Synthetic VAT','vat','credit'),('B','Synthetic bank','asset','debit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('bank','B',1,'synthetic'),('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');INSERT INTO bank_accounts(slug,name,currency,ledger_account_no)VALUES('synthetic','Synthetic bank','DKK','B');").unwrap();
    tx.commit().unwrap();
    db
}
fn issue_currency(tx: &Transaction<'_>, date: &str, amount: i32, currency: &str) -> i64 {
    let input=serde_json::from_value(json!({"issueDate":date,"currency":currency,"fxRateToDkk":if currency=="DKK"{1.0}else{7.5},"buyer":{"name":"Synthetic Buyer","address":"Synthetic Street 2","vatOrCvr":"DK12345678"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":amount}]})).unwrap();
    let mut payload = crate::sales::prepare(tx, &input, "2026-10-22T12:00:00Z", true).unwrap();
    let unposted = date.starts_with("2025");
    if unposted {
        payload.as_object_mut().unwrap().remove("dueDate");
    }
    let hash =
        crate::invoice_email::sha(serde_json::to_string_pretty(&payload).unwrap().as_bytes());
    let pdf = crate::invoice_email::sha(format!("synthetic-pdf-{hash}").as_bytes());
    let id = crate::sales::register(
        tx,
        &payload,
        Artifact {
            filename: "synthetic.json",
            path: &format!("invoices/{hash}.json"),
            hash: &hash,
            mime: "application/json",
        },
        Artifact {
            filename: "synthetic.pdf",
            path: &format!("invoices/{pdf}.pdf"),
            hash: &pdf,
            mime: "application/pdf",
        },
        "synthetic",
    )
    .unwrap()["documentId"]
        .as_i64()
        .unwrap();
    if !unposted {
        crate::sales::post(
            tx,
            &crate::sales::Post {
                invoice_document_id: id,
                revenue_account_no: "S".into(),
                confirm: true,
            },
            "synthetic",
            Date::parse("2026-10-22").unwrap(),
        )
        .unwrap();
    }
    id
}
fn input(id: i64, amount: i32) -> Input {
    serde_json::from_value(
        json!({"invoiceDocumentId":id,"paymentDate":"2026-10-22","amount":amount}),
    )
    .unwrap()
}
fn receipt(tx: &Transaction<'_>, id: i64, amount: i32, bank: &str) -> i64 {
    let request=serde_json::from_value(json!({"transactionDate":"2026-10-22","text":"Synthetic existing receipt","currency":"DKK","documentId":id.to_string(),"lines":[{"accountNo":bank,"debitAmount":format!("{amount}.00")},{"accountNo":"R","creditAmount":format!("{amount}.00")}]})).unwrap();
    crate::journal_post::post(
        tx,
        &request,
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        Date::parse("2026-10-22").unwrap(),
        crate::journal_post::Actor {
            subject: "user:synthetic",
            program: "rentemester-rust",
        },
    )
    .unwrap()["entryId"]
        .as_i64()
        .unwrap()
}
fn count(tx: &Transaction<'_>, table: &str) -> i64 {
    tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
#[test]
fn bankless_post_then_exact_candidate_preserves_original_debtor_and_full_audit() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let id = issue_currency(&tx, "2026-10-20", 100, "DKK");
    tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R2','Synthetic next debtor','asset','debit'),('B2','Synthetic alternate bank','asset','debit');UPDATE account_role_mappings SET status='superseded' WHERE role='debtors';INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('debtors','R2',2,'synthetic')").unwrap();
    tx.execute_batch("SAVEPOINT other_invoice_scope").unwrap();
    let other = issue_currency(&tx, "2026-10-20", 100, "DKK");
    receipt(&tx, other, 1, "B");
    assert_eq!(crate::audit_invoice::verify(&tx).unwrap()["ok"], false);
    let selected = apply(
        &tx,
        &input(id, 1),
        "synthetic",
        PrincipalKind::User,
        Date::parse("2026-10-22").unwrap(),
    )
    .unwrap();
    assert_eq!(selected["openBalance"], 124.0);
    assert_eq!(crate::audit_invoice::verify(&tx).unwrap()["ok"], false);
    tx.execute_batch("ROLLBACK TO other_invoice_scope;RELEASE other_invoice_scope")
        .unwrap();
    let first = apply(
        &tx,
        &input(id, 50),
        "service-synthetic",
        PrincipalKind::ServiceAccount,
        Date::parse("2026-10-22").unwrap(),
    )
    .unwrap();
    assert_eq!(first["openBalance"], 75.0);
    assert_eq!(
        first["appliedRules"],
        json!([
            "DK-INVOICE-PAYMENT-001",
            "DK-INVOICE-CORRECTION-BALANCE-001"
        ])
    );
    assert!(first.get("entryId").is_none());
    assert_eq!(count(&tx, "journal_entries"), 2);
    assert_eq!(count(&tx, "bank_transactions"), 0);
    assert_eq!(crate::audit_invoice::verify(&tx).unwrap()["ok"], true);
    let mut too_much = input(id, 76);
    assert!(
        apply(
            &tx,
            &too_much,
            "synthetic",
            PrincipalKind::User,
            Date::parse("2026-10-22").unwrap()
        )
        .is_err()
    );
    assert_eq!(count(&tx, "journal_entries"), 2);
    let journal = receipt(&tx, id, 75, "B");
    too_much = input(id, 75);
    too_much.journal_entry_id = Some(journal);
    tx.execute_batch("SAVEPOINT closed_candidate;INSERT INTO accounting_periods(period_start,period_end,kind,status)VALUES('2026-10-01','2026-10-31','custom','closed')").unwrap();
    assert_eq!(
        apply(
            &tx,
            &too_much,
            "synthetic",
            PrincipalKind::User,
            Date::parse("2026-10-22").unwrap()
        )
        .unwrap_err()
        .code,
        "invoice_payment_period_locked"
    );
    tx.execute_batch("ROLLBACK TO closed_candidate;RELEASE closed_candidate")
        .unwrap();
    let mut wrong = input(id, 74);
    wrong.journal_entry_id = Some(journal);
    assert!(
        apply(
            &tx,
            &wrong,
            "synthetic",
            PrincipalKind::User,
            Date::parse("2026-10-22").unwrap()
        )
        .is_err()
    );
    assert_eq!(count(&tx, "invoice_payments"), 1);
    wrong = input(id, 75);
    wrong.journal_entry_id = Some(journal);
    wrong.payment_date = "2026-10-21".into();
    assert!(
        apply(
            &tx,
            &wrong,
            "synthetic",
            PrincipalKind::User,
            Date::parse("2026-10-22").unwrap()
        )
        .is_err()
    );
    tx.execute_batch("SAVEPOINT unrelated_candidate").unwrap();
    receipt(&tx, id, 1, "B2");
    assert!(
        apply(
            &tx,
            &too_much,
            "synthetic",
            PrincipalKind::User,
            Date::parse("2026-10-22").unwrap()
        )
        .is_err()
    );
    tx.execute_batch("ROLLBACK TO unrelated_candidate;RELEASE unrelated_candidate")
        .unwrap();
    assert_eq!(count(&tx, "invoice_payments"), 1);
    let second = apply(
        &tx,
        &too_much,
        "service-synthetic",
        PrincipalKind::ServiceAccount,
        Date::parse("2026-10-22").unwrap(),
    )
    .unwrap();
    assert_eq!(second["journalEntryId"], journal);
    assert_eq!(second["openBalance"], 0.0);
    assert_eq!(count(&tx, "journal_entries"), 3);
    assert_eq!(count(&tx, "invoice_payments"), 2);
    assert_eq!(crate::audit_invoice::verify(&tx).unwrap()["ok"], true);
    assert!(
        apply(
            &tx,
            &too_much,
            "synthetic",
            PrincipalKind::User,
            Date::parse("2026-10-22").unwrap()
        )
        .is_err()
    );
    assert_eq!(count(&tx, "invoice_payments"), 2);
    assert_eq!(
        tx.query_row::<i64, _, _>(
            "SELECT count(*) FROM invoice_payments WHERE bank_transaction_id IS NULL",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(tx.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE event_type='invoice_payment_apply' AND actor='service_account:service-synthetic'",[],|r|r.get(0)).unwrap(),2);
}
#[test]
fn foreign_apply_requires_bank_and_reuses_native_frozen_fx_settlement() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('G','Synthetic FX gain','income','credit'),('L','Synthetic FX loss','expense','debit');INSERT INTO bank_accounts(slug,name,ledger_account_no,currency)VALUES('eur','Synthetic EUR bank','B','EUR')").unwrap();
    crate::native_fx_roles::confirm(&tx, "fx_gain", "G", "synthetic").unwrap();
    crate::native_fx_roles::confirm(&tx, "fx_loss", "L", "synthetic").unwrap();
    let id = issue_currency(&tx, "2026-10-20", 100, "EUR");
    assert!(
        apply(
            &tx,
            &input(id, 50),
            "synthetic",
            PrincipalKind::User,
            Date::parse("2026-10-22").unwrap()
        )
        .is_err()
    );
    let mut i = input(id, 50);
    i.journal_entry_id = Some(1);
    assert!(
        apply(
            &tx,
            &i,
            "synthetic",
            PrincipalKind::User,
            Date::parse("2026-10-22").unwrap()
        )
        .is_err()
    );
    assert_eq!(count(&tx, "invoice_payments"), 0);
    let csv = "date;text;amount;currency;reference;amount_dkk;fx_rate_to_dkk\n2026-10-22;Synthetic foreign receipt;50;EUR;foreign-paid;380;7.6\n";
    crate::bank::import(
        &tx,
        csv,
        &rentemester_engine::bank::parse(csv).unwrap(),
        Some("eur"),
        "synthetic",
        "2026-10-22T12:00:00Z",
        "synthetic",
    )
    .unwrap();
    i.journal_entry_id = None;
    i.bank_transaction_id = Some(1);
    let paid = apply(
        &tx,
        &i,
        "synthetic",
        PrincipalKind::User,
        Date::parse("2026-10-22").unwrap(),
    )
    .unwrap();
    assert_eq!(paid["openBalance"], 75.0);
    assert_eq!(
        paid["appliedRules"],
        json!([
            "DK-INVOICE-PAYMENT-001",
            "DK-INVOICE-CORRECTION-BALANCE-001",
            "DK-INVOICE-FX-REALISED-001"
        ])
    );
    assert_eq!(count(&tx, "journal_entries"), 2);
    assert_eq!(count(&tx, "invoice_payments"), 1);
    assert_eq!(crate::audit_invoice::verify(&tx).unwrap()["ok"], true);
}
