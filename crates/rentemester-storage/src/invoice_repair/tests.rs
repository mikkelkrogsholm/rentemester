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
    let unposted = true;
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
fn legacy(tx: &Transaction<'_>, invoice: i64) -> i64 {
    let request=serde_json::from_value(json!({"transactionDate":"2026-10-22","text":"Synthetic incomplete invoice booking","currency":"DKK","documentId":invoice.to_string(),"lines":[{"accountNo":"R","debitAmount":"100.00"},{"accountNo":"S","creditAmount":"100.00"}]})).unwrap();
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
fn body(id: i64) -> Value {
    json!({"legacyJournalEntryId":id,"reason":"Synthetic legacy correction","revenueAccountNo":"S","confirm":true})
}
fn run(tx: &Transaction<'_>, invoice: i64, id: i64) -> Result<Value, Error> {
    repair(
        tx,
        invoice,
        &body(id),
        "synthetic-service",
        PrincipalKind::ServiceAccount,
        Date::parse("2026-10-22").unwrap(),
    )
}
#[test]
fn atomic_repair_uses_original_dates_preserves_guards_and_current_principal() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let invoice = issue_currency(&tx, "2026-10-21", 100, "DKK");
    let old = legacy(&tx, invoice);
    let original: (String, String) = tx
        .query_row(
            "SELECT entry_hash,transaction_date FROM journal_entries WHERE id=?1",
            [old],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(
        crate::sales::post(
            &tx,
            &crate::sales::Post {
                invoice_document_id: invoice,
                revenue_account_no: "S".into(),
                confirm: true
            },
            "synthetic",
            Date::parse("2026-10-22").unwrap()
        )
        .is_err()
    );
    let reversal = crate::journal_reversal::Input {
        entry_id: JournalEntryId::from_sqlite(old),
        transaction_date: Date::parse("2026-10-22").unwrap(),
        reason: "ordinary protection".into(),
    };
    assert_eq!(
        crate::journal_reversal::reverse(
            &tx,
            &reversal,
            &crate::journal_preview::manual_journal_rule_version().unwrap(),
            Date::parse("2026-10-22").unwrap(),
            crate::journal_post::Actor {
                subject: "user:synthetic",
                program: "rentemester-rust"
            }
        )
        .unwrap()["ok"],
        false
    );
    tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R2','Synthetic changed debtor','asset','debit');UPDATE account_role_mappings SET status='superseded' WHERE role='debtors';INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('debtors','R2',2,'synthetic');").unwrap();
    let out = run(&tx, invoice, old).unwrap();
    let new = out["replacementJournalEntryId"].as_i64().unwrap();
    let mirror = out["reversalJournalEntryId"].as_i64().unwrap();
    assert_eq!(out["entryId"], new);
    assert!(out["entryHash"].is_string());
    assert_eq!(
        tx.query_row::<String, _, _>(
            "SELECT transaction_date FROM journal_entries WHERE id=?1",
            [new],
            |r| r.get(0)
        )
        .unwrap(),
        "2026-10-21"
    );
    assert_eq!(
        tx.query_row::<String, _, _>(
            "SELECT transaction_date FROM journal_entries WHERE id=?1",
            [mirror],
            |r| r.get(0)
        )
        .unwrap(),
        "2026-10-22"
    );
    assert_eq!(
        tx.query_row::<(String, String), _, _>(
            "SELECT entry_hash,transaction_date FROM journal_entries WHERE id=?1",
            [old],
            |r| Ok((r.get(0)?, r.get(1)?))
        )
        .unwrap(),
        original
    );
    assert_eq!(tx.query_row::<i64,_,_>("SELECT count(*) FROM journal_lines a JOIN journal_lines b ON a.account_id=b.account_id AND a.debit_amount=b.credit_amount AND a.credit_amount=b.debit_amount WHERE a.journal_entry_id=?1 AND b.journal_entry_id=?2",params![old,mirror],|r|r.get(0)).unwrap(),2);
    assert_eq!(crate::audit_invoice::verify(&tx).unwrap()["ok"], true);
    assert_eq!(
        tx.query_row::<String, _, _>(
            "SELECT actor FROM audit_log WHERE event_type='invoice_booking_repair'",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        "service_account:synthetic-service"
    );
    assert_eq!(tx.query_row::<i64,_,_>("SELECT count(*) FROM journal_entries WHERE created_by='user:synthetic-service' AND created_by_program='service-principal'",[],|r|r.get(0)).unwrap(),2);
    assert!(run(&tx, invoice, old).is_err());
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        3
    );
}
#[test]
fn repair_dependency_period_and_reversal_failure_are_atomic() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let invoice = issue_currency(&tx, "2026-10-21", 100, "DKK");
    let old = legacy(&tx, invoice);
    for date in ["2026-10-21", "2026-10-22"] {
        tx.execute_batch("SAVEPOINT closed_date").unwrap();
        tx.execute("INSERT INTO accounting_periods(period_start,period_end,kind,status)VALUES(?1,?1,'custom','closed')",[date]).unwrap();
        assert!(run(&tx, invoice, old).is_err());
        tx.execute_batch("ROLLBACK TO closed_date;RELEASE closed_date")
            .unwrap();
    }
    tx.execute_batch("SAVEPOINT extra").unwrap();
    legacy(&tx, invoice);
    assert!(run(&tx, invoice, old).is_err());
    tx.execute_batch("ROLLBACK TO extra;RELEASE extra").unwrap();
    tx.execute_batch("SAVEPOINT downstream").unwrap();
    tx.execute("INSERT INTO invoice_reminders(invoice_document_id,reminder_date,fee_amount)VALUES(?1,'2026-10-22',1)",[invoice]).unwrap();
    assert!(run(&tx, invoice, old).is_err());
    tx.execute_batch("ROLLBACK TO downstream;RELEASE downstream")
        .unwrap();
    tx.execute_batch("CREATE TEMP TRIGGER fail_repair_mirror BEFORE INSERT ON main.journal_entries WHEN NEW.reversal_of_entry_id IS NOT NULL BEGIN SELECT RAISE(ABORT,'synthetic mirror publication failure'); END;SAVEPOINT failed_mirror").unwrap();
    assert!(run(&tx, invoice, old).is_err());
    tx.execute_batch(
        "ROLLBACK TO failed_mirror;RELEASE failed_mirror;DROP TRIGGER fail_repair_mirror",
    )
    .unwrap();
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM issued_invoice_postings", [], |r| r
            .get(0))
            .unwrap(),
        0
    );
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        1
    );
    assert_eq!(tx.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE event_type IN ('invoice_post','invoice_booking_repair')",[],|r|r.get(0)).unwrap(),0);
    assert!(run(&tx, invoice, old).is_ok());
}
