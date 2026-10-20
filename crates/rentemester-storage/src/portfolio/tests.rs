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
    tx.execute_batch("UPDATE companies SET address='Synthetic Street',cvr='87654321',vat_period_type='quarter',fiscal_year_start_month=7,fiscal_year_label_strategy='span' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R','Synthetic debtor','asset','debit'),('S','Synthetic sales','income','credit'),('V','Synthetic VAT','vat','credit'),('B','Synthetic bank','asset','debit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');INSERT INTO bank_accounts(slug,name,currency,ledger_account_no)VALUES('synthetic','Synthetic bank','DKK','B');").unwrap();
    tx.commit().unwrap();
    db
}
fn issue(tx: &Transaction<'_>, date: &str, amount: i32) -> i64 {
    let input=serde_json::from_value(json!({"issueDate":date,"buyer":{"name":"Synthetic Buyer","address":"Synthetic Street 2","vatOrCvr":"DK12345678"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":amount}]})).unwrap();
    let payload = crate::sales::prepare(tx, &input, "2026-10-22T12:00:00Z", true).unwrap();
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
    id
}
#[test]
fn shifted_year_true_cutoff_ignores_later_sale_and_credit() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let id = issue(&tx, "2026-07-02", 100);
    issue(&tx, "2026-10-21", 200);
    let request=serde_json::from_value(json!({"invoiceDocumentId":id,"issueDate":"2026-10-21","reason":"Synthetic full correction","confirm":true})).unwrap();
    let credit = crate::credit_note::prepare(&tx, &request, "2026-10-21T12:00:00Z").unwrap();
    let raw = serde_json::to_vec(&credit.payload).unwrap();
    let hash = crate::invoice_email::sha(&raw);
    crate::credit_note::register_and_post(
        &tx,
        credit,
        Artifact {
            filename: "synthetic-credit.json",
            path: &format!("invoices/{hash}.json"),
            hash: &hash,
            mime: "application/json",
        },
        "synthetic",
        Date::parse("2026-10-22").unwrap(),
    )
    .unwrap();
    let before = company(&tx, "synthetic", "2026-10-20").unwrap();
    assert_eq!(before["fiscalPeriodStart"], "2026-07-01");
    assert_eq!(before["fiscalPeriodEnd"], "2027-06-30");
    assert_eq!(before["periodEnd"], "2026-10-20");
    assert_eq!(before["resultat"], 100.0);
    assert_eq!(before["omsaetning"], 100.0);
    assert_eq!(before["openInvoiceTotal"], 125.0);
    assert_eq!(before["vat"]["payable"], 0.0);
    assert_eq!(before["vat"]["periodStart"], "2026-10-01");
    let after = company(&tx, "synthetic", "2026-10-21").unwrap();
    assert_eq!(after["resultat"], 200.0);
    assert_eq!(after["openInvoiceTotal"], 250.0);
    assert_eq!(after["vat"]["payable"], 25.0);
}
#[test]
fn absent_statement_is_unknown_but_documented_zero_is_known() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let csv = "date;text;amount;currency;reference;balance_after\n2026-10-19;Synthetic inflow;100;DKK;in;100\n2026-10-20;Synthetic outflow;-100;DKK;out;0\n";
    let rows = rentemester_engine::bank::parse(csv).unwrap();
    crate::bank::import(
        &tx,
        csv,
        &rows,
        Some("synthetic"),
        "synthetic",
        "2026-10-20T12:00:00Z",
        "synthetic-source",
    )
    .unwrap();
    let missing = company(&tx, "synthetic", "2026-10-18").unwrap();
    assert!(missing["actualBankBalance"].is_null());
    assert_eq!(missing["bankStatementStatus"], "none");
    assert_eq!(missing["unlinkedBankCount"], 0);
    let known = company(&tx, "synthetic", "2026-10-19").unwrap();
    assert_eq!(known["actualBankBalance"], 100.0);
    assert_eq!(known["unlinkedBankCount"], 1);
    let zero = company(&tx, "synthetic", "2026-10-20").unwrap();
    assert_eq!(zero["actualBankBalance"], 0.0);
    assert_eq!(zero["bankStatementStatus"], "known");
    assert_eq!(zero["unlinkedBankCount"], 2);
}
#[test]
fn tasks_are_current_status_of_dated_sources_not_invented_historical_health() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO exceptions(type,severity,message,created_at)VALUES('SYNTHETIC','medium','Synthetic task','2026-10-19 12:00:00'),('SYNTHETIC','medium','Synthetic future task','2026-10-21 12:00:00');").unwrap();
    let before: i64 = tx
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    let card = company(&tx, "synthetic", "2026-10-20").unwrap();
    assert_eq!(card["openTaskCount"], 1);
    assert_eq!(card["availability"]["tasks"]["status"], "current_state");
    assert_eq!(
        card["availability"]["tasks"]["reason"],
        "current_open_state_for_sources_through_cutoff"
    );
    let after: i64 = tx
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(card["audit"]["scope"], "current_journal_hash_chain");
}
#[test]
fn invalid_date_and_non_dkk_functional_currency_refuse() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(company(&tx, "synthetic", "2026-02-30").is_err());
    tx.execute("UPDATE companies SET currency='EUR' WHERE id=1", [])
        .unwrap();
    assert!(
        company(&tx, "synthetic", "2026-10-20")
            .unwrap_err()
            .message
            .contains("DKK")
    );
}
