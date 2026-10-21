use super::*;
use crate::sales::Artifact;
use rusqlite::Transaction;
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
fn issue(tx: &Transaction<'_>, date: &str, amount: i32, currency: &str) -> i64 {
    let input=serde_json::from_value(json!({"issueDate":date,"currency":currency,"fxRateToDkk":if currency=="DKK"{1.0}else{7.5},"buyer":{"name":"Synthetic Buyer","address":"Synthetic Street 2","vatOrCvr":"DK12345678"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":amount}]})).unwrap();
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
fn complete_metadata_keeps_dated_principal_current_claims_and_actual_overdue_order() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let early = issue(&tx, "2026-09-01", 100, "DKK");
    let later = issue(&tx, "2026-10-15", 200, "DKK");
    let foreign = issue(&tx, "2026-09-01", 200, "EUR");
    let csv = "date;text;amount;currency;reference\n2026-10-20;Synthetic partial receipt;50;DKK;partial\n2026-10-20;Synthetic unmatched;7;DKK;unmatched\n";
    crate::bank::import(
        &tx,
        csv,
        &rentemester_engine::bank::parse(csv).unwrap(),
        Some("synthetic"),
        "synthetic",
        "2026-10-21T12:00:00Z",
        "synthetic",
    )
    .unwrap();
    crate::invoice_settlement::settle(
        &tx,
        &serde_json::from_value(
            json!({"invoiceDocumentId":early,"bankTransactionReference":"partial","confirm":true}),
        )
        .unwrap(),
        "synthetic",
        Date::parse("2026-10-22").unwrap(),
    )
    .unwrap();
    crate::invoice_compensation::register(&tx,&serde_json::from_value(json!({"invoiceDocumentId":early,"asOfDate":"2026-10-21","compensationAmountDkk":10,"confirm":true})).unwrap(),"synthetic").unwrap();
    tx.execute_batch("INSERT INTO exceptions(type,severity,message)VALUES('SYNTHETIC','medium','Synthetic open review');").unwrap();
    let changes = tx.total_changes();
    let d = read(&tx, "synthetic", "2026-10-10", "2026-10-21").unwrap();
    assert_eq!(d["company"].as_object().unwrap().len(), 6);
    assert_eq!(d["invoices"]["count"], 2);
    assert_eq!(d["invoices"]["openTotal"], 2000.0); // 125 DKK + EUR250 at7.5
    assert_eq!(d["overdueInvoices"]["rows"][0]["documentId"], foreign);
    let row = d["invoices"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["documentId"] == early)
        .unwrap();
    assert_eq!(row["invoiceDate"], "2026-09-01");
    assert_eq!(row["customerName"], "Synthetic Buyer");
    assert_eq!(row["customerCvr"], "DK12345678");
    assert_eq!(row["openBalance"], 125.0);
    assert_eq!(row["claimOpenBalance"], 85.0); // Current75principal + registered10claim.
    assert_eq!(row["basis"]["claims"], "current_state");
    assert_eq!(row["status"], "open");
    assert_eq!(row["buyerPublicRecipient"], false);
    assert!(row["buyerEanNumber"].is_null());
    assert!(row["peppolStatus"].is_null());
    assert_eq!(d["unlinkedBank"]["count"], 1);
    assert_eq!(d["exceptions"]["count"], 2); // Automatic unmatched-bank task + explicit review.
    assert!(
        d["exceptions"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["type"] == "SYNTHETIC")
    );
    assert_eq!(d["vat"]["periodStart"], "2026-07-01");
    assert_eq!(d["vat"]["periodStatus"], "open");
    assert_eq!(d["vat"]["netVatPayable"], 400.0); //25DKK + 50EUR*7.5
    assert_eq!(d["audit"]["scope"], "current_journal_hash_chain");
    assert!(!d["recentActivity"].as_array().unwrap().is_empty());
    let now = read(&tx, "synthetic", "2026-10-21", "2026-10-21").unwrap();
    assert_eq!(now["invoices"]["count"], 3);
    assert!(
        now["invoices"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["documentId"] == later)
    );
    assert_eq!(
        read(&tx, "synthetic", "2026-10-10", "2026-10-21").unwrap(),
        d
    );
    assert_eq!(tx.total_changes(), changes);
}
#[test]
fn empty_nonregistered_and_unposted_unknown_due_are_explicit() {
    let mut db = fixture();
    let tx = db.transaction().unwrap();
    tx.execute("UPDATE companies SET vat_period_type=NULL", [])
        .unwrap();
    let empty = read(&tx, "synthetic", "2026-10-21", "2026-10-21").unwrap();
    assert_eq!(empty["invoices"]["count"], 0);
    assert_eq!(empty["vat"]["registered"], false);
    for field in [
        "periodStart",
        "periodEnd",
        "periodLabel",
        "deadline",
        "periodStatus",
        "daysRemaining",
    ] {
        assert!(empty["vat"][field].is_null(), "{field}");
    }
    assert_eq!(empty["vat"]["netVatPayable"], 0);
    tx.execute_batch("INSERT INTO documents(source,document_type,sha256_hash,invoice_no,invoice_date,amount_inc_vat,currency,payload_json)VALUES('synthetic','issued_invoice','synthetic-json-unknown','UNKNOWN','2026-10-01',0.03,'DKK','{\"currency\":\"DKK\",\"buyer\":{\"name\":\"Synthetic unposted\"}}');").unwrap();
    let d = read(&tx, "synthetic", "2026-10-21", "2026-10-21").unwrap();
    let row = &d["invoices"]["rows"][0];
    assert!(row["effectiveDueDate"].is_null());
    assert!(row["claimOpenBalance"].is_null());
    assert_eq!(row["currentClaimsAvailable"], false);
    assert_eq!(row["currentClaimsUnavailableReason"], "ledger_not_posted");
    assert_eq!(row["isOverdue"], false);
    assert_eq!(d["overdueInvoices"]["count"], 0);
    assert!(read(&tx, "synthetic", "2026-02-30", "2026-10-21").is_err());
}
#[test]
fn before_opening_cutover_has_unavailable_native_amounts_not_zeros() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('E','Synthetic equity','equity','credit');").unwrap();
    let input=serde_json::from_value(json!({"cutOverDate":"2026-10-01","lines":[{"accountNo":"R","debitAmount":"100.00"},{"accountNo":"E","creditAmount":"100.00"}],"note":"Synthetic approved opening"})).unwrap();
    let latest = Date::parse("2026-10-22").unwrap();
    let plan = crate::opening_balance::prepare(&tx, &input, &[], latest).unwrap();
    let apply = crate::opening_balance::Apply {
        input,
        plan_hash: plan.review()["planHash"].as_str().unwrap().into(),
        idempotency_key: "synthetic-dashboard-json-opening".into(),
        confirm: true,
    };
    crate::opening_balance::apply(
        &tx,
        &apply,
        &[],
        latest,
        crate::journal_post::Actor {
            subject: "user:synthetic",
            program: crate::opening_balance::PROGRAM,
        },
        1792497600000,
    )
    .unwrap();
    let d = read(&tx, "synthetic", "2026-09-30", "2026-10-21").unwrap();
    assert_eq!(d["invoices"]["available"], false);
    assert!(d["invoices"]["count"].is_null());
    assert!(d["invoices"]["openTotal"].is_null());
    assert!(d["invoices"]["rows"].is_null());
    assert_eq!(d["vat"]["available"], false);
    assert!(d["vat"]["netVatPayable"].is_null());
}
