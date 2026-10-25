use super::*;
use rusqlite::{Connection, TransactionBehavior};
fn database() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Agent Review").unwrap();
    db
}
fn financial(tx: &Transaction<'_>) -> (i64, i64, i64) {
    tx.query_row("SELECT (SELECT count(*) FROM journal_entries),(SELECT count(*) FROM journal_lines),(SELECT count(*) FROM payable_payments)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap()
}
#[test]
fn agent_review_deadline_cadences_null_and_calendar_horizon() {
    for cadence in [None, Some("month"), Some("quarter"), Some("half-year")] {
        let mut db = database();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        tx.execute("UPDATE companies SET vat_period_type=?1", [cadence])
            .unwrap();
        let before = financial(&tx);
        let snapshot = review_deadlines(&tx, "2026-10-25", "service-account:actual").unwrap();
        let expected = if let Some(c) = cadence {
            let current = vat_period::window("2026-10-25", c).unwrap();
            let prev = vat_period::window(&previous(&current.start).unwrap(), c).unwrap();
            2 + usize::from(prev.deadline.as_str() >= "2026-10-25")
        } else {
            1
        };
        assert_eq!(snapshot.upcoming_deadlines.len(), expected);
        assert_eq!(snapshot.accrual_recognitions_due, 0);
        for n in &snapshot.upcoming_deadlines {
            assert_eq!(n.days_remaining, days("2026-10-25", &n.due_date).unwrap());
            assert!(!n.ready);
        }
        if let Some(cadence) = cadence {
            let w = vat_period::window("2026-10-25", cadence).unwrap();
            assert_eq!(
                snapshot.upcoming_deadlines[snapshot.upcoming_deadlines.len() - 2].period_start,
                w.start
            );
        }
        assert_eq!(financial(&tx), before);
    }
}
#[test]
fn agent_review_vat_blocked_guard_diagnostic_and_all_open_id_order() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET vat_period_type='quarter';INSERT INTO accounting_periods(period_start,period_end,kind,status) VALUES('2026-07-01','2026-09-30','vat_period','closed');INSERT INTO accounts(id,account_no,name,type,normal_balance) VALUES(1,'SYN','Synthetic revenue','income','credit');INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash,created_by_program) VALUES(1,'SYN-1','2026-08-01','Synthetic unclassified VAT','fixture','hash','rentemester-import-postings');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(1,1,0,100);INSERT INTO exceptions(type,severity,message) VALUES('OTHER','low','Synthetic unrelated open task');").unwrap();
    let before = financial(&tx);
    let review = review_deadlines(&tx, "2026-11-01", "user:actual").unwrap();
    assert!(!review.upcoming_deadlines[0].ready);
    assert!(review.upcoming_deadlines[0].note.contains("blokeret"));
    let open = open_report_exceptions(&tx).unwrap();
    assert_eq!(open[0].kind, "OTHER");
    assert!(
        open.windows(2)
            .all(|p| p[0].exception_id < p[1].exception_id)
    );
    let blocked = open
        .iter()
        .find(|p| p.kind == "AGENT_VAT_FILING_BLOCKED")
        .unwrap();
    let ev: String = tx
        .query_row(
            "SELECT source_evidence FROM exceptions WHERE id=?1",
            [blocked.exception_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        !serde_json::from_str::<Value>(&ev).unwrap()["filingErrors"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(financial(&tx), before);
}
#[test]
fn agent_review_overdue_stable_dedup_actual_actor_and_stale_resolution() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO documents(id,source,sha256_hash,document_type,status) VALUES(1,'synthetic','synthetic-hash','purchase_sale','ingested');INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES(1,'SYN-1','2026-01-01','Synthetic creditor','fixture','hash');INSERT INTO payables(id,document_id,supplier_name,bill_no,bill_date,due_date,gross_amount,net_amount,vat_amount,currency,journal_entry_id) VALUES(1,1,'Synthetic Supplier','SYN-BILL','2026-01-01','2026-01-15',100,100,0,'DKK',1);").unwrap();
    let before = financial(&tx);
    sync_overdue_payables(&tx, "2026-01-20", "service-account:actual").unwrap();
    sync_overdue_payables(&tx, "2026-03-20", "user:other").unwrap();
    let open = open_report_exceptions(&tx).unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(
        open[0].message,
        "kreditorpost SYN-BILL til Synthetic Supplier på 100,00 kr. med forfald 2026-01-15 er overforfalden og endnu ikke betalt."
    );
    let actor: String = tx
        .query_row(
            "SELECT actor FROM audit_log WHERE event_type='exception_record'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(actor, "service-account:actual");
    // Earlier as-of is no longer overdue, without inventing a payment.
    sync_overdue_payables(&tx, "2026-01-15", "user:current").unwrap();
    assert!(open_report_exceptions(&tx).unwrap().is_empty());
    let resolved: String = tx
        .query_row("SELECT resolved_by FROM exceptions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(resolved, "user:current");
    assert_eq!(financial(&tx), before);
}
#[test]
fn agent_review_accrual_due_and_closed_previous_tax_only_no_posting() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET vat_period_type=NULL,cvr='87654321',company_form='ApS';INSERT INTO accounts(id,account_no,name,type,normal_balance) VALUES(1,'B','Synthetic bank','asset','debit'),(2,'E','Synthetic cost','expense','debit');INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash) VALUES(1,'SYN-1','2025-08-01','Synthetic expense','fixture','hash');INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount,vat_code) VALUES(1,2,100,0,'DK_PURCHASE_EXEMPT'),(1,1,0,100,NULL);INSERT INTO accruals(id,accrual_type,description,total_amount,recognition_periods,first_recognition_date,period_step_months,balance_account_no,result_account_no,registration_journal_entry_id) VALUES(1,'prepaid_expense','Synthetic service',90,3,'2026-01-31',1,'B','E',1);INSERT INTO accounting_periods(period_start,period_end,kind,status) VALUES('2025-01-01','2025-12-31','fiscal_year','closed');").unwrap();
    let before = financial(&tx);
    let review = review_deadlines(&tx, "2026-02-01", "user:actual").unwrap();
    assert_eq!(review.accrual_recognitions_due, 1);
    let open = open_report_exceptions(&tx).unwrap();
    assert!(
        open.iter()
            .any(|p| p.kind == "AGENT_ACCRUAL_RECOGNITION_DUE")
    );
    assert!(
        open.iter()
            .any(|p| p.kind == "AGENT_TAX_RETURN_NEEDS_REVIEW")
    );
    review_deadlines(&tx, "2026-02-02", "user:actual").unwrap();
    assert_eq!(open_report_exceptions(&tx).unwrap().len(), open.len());
    assert_eq!(financial(&tx), before);
}
