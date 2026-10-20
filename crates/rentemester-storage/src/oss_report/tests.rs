use super::*;
use rusqlite::Connection;

fn database() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE companies(id INTEGER PRIMARY KEY,vat_period_type TEXT);
        INSERT INTO companies VALUES(1,'quarter');
        CREATE TABLE account_role_mappings(role TEXT,account_no TEXT,status TEXT);
        CREATE TABLE accounts(id INTEGER PRIMARY KEY,account_no TEXT,type TEXT,normal_balance TEXT);
        INSERT INTO accounts VALUES(1,'S','income','credit'),(2,'B','asset','debit'),(3,'V','vat','credit'),(4,'R','asset','debit');
        CREATE TABLE journal_entries(id INTEGER PRIMARY KEY,transaction_date TEXT,status TEXT,currency TEXT,created_by_program TEXT);
        CREATE TABLE journal_lines(id INTEGER PRIMARY KEY,journal_entry_id INTEGER,account_id INTEGER,vat_code TEXT,debit_amount NUMERIC,credit_amount NUMERIC);
        CREATE TABLE documents(id INTEGER PRIMARY KEY,payload_json TEXT,invoice_date TEXT,currency TEXT);
        CREATE TABLE issued_invoice_postings(invoice_document_id INTEGER,journal_entry_id INTEGER,receivable_account_id INTEGER,booked_gross_dkk NUMERIC);
        INSERT INTO journal_entries VALUES(1,'2026-08-02','reversed','DKK','rentemester'),(2,'2026-08-02','posted','DKK','rentemester'),(3,'2026-09-02','posted','DKK','rentemester');
        INSERT INTO journal_lines VALUES
        (1,1,1,'OSS_EU_CONSUMER',0,400.01),(2,1,1,'OSS_EU_CONSUMER',0,599.99),(3,1,2,NULL,1000,0),
        (4,2,1,'DK_SALE_25',0,100),(5,2,3,NULL,0,25),(6,2,4,NULL,125,0),
        (7,3,1,'OSS_EU_CONSUMER',1000,0),(8,3,2,NULL,0,1000);").unwrap();
    db
}
fn report(tx: &Transaction<'_>, from: &str, to: &str) -> Result<Value, Error> {
    read(tx, Date::parse(from)?, Date::parse(to)?)
}
#[test]
fn dated_oss_reversal_keeps_original_history_and_ordinary_vat() {
    let mut db = database();
    let tx = db.transaction().unwrap();
    let original = report(&tx, "2026-08-01", "2026-08-31").unwrap();
    assert_eq!(original["ok"], true);
    assert_eq!(original["ossConsumerSalesBase"], 1000.0);
    assert_eq!(original["entryCount"], 1);
    assert_eq!(original["vatReport"]["ok"], true);
    assert_eq!(original["vatReport"]["outputVat"], 25.0);
    assert_eq!(original["vatReport"]["rubrikker"]["rubrikB"], 0.0);
    assert_eq!(original["vatReport"]["rubrikker"]["rubrikC"], 0.0);
    assert_eq!(original["submission"], false);
    assert!(
        original["warnings"][0]
            .as_str()
            .unwrap()
            .contains("kun OSS-grundlaget")
    );
    let corrected = report(&tx, "2026-08-01", "2026-09-30").unwrap();
    assert_eq!(corrected["ossConsumerSalesBase"], 0.0);
    assert_eq!(corrected["entryCount"], 2);
    assert_eq!(corrected["vatReport"]["outputVat"], 25.0);
    assert_eq!(corrected["warnings"], json!([]));
    let reversal = report(&tx, "2026-09-01", "2026-09-30").unwrap();
    assert_eq!(reversal["ossConsumerSalesBase"], -1000.0);
    assert_eq!(reversal["entryCount"], 1);
    assert_eq!(reversal["warnings"], json!([]));
    assert_eq!(original, report(&tx, "2026-08-01", "2026-08-31").unwrap());
}
#[test]
fn invalid_underlying_vat_never_publishes_partial_oss_totals() {
    let mut db = database();
    let tx = db.transaction().unwrap();
    tx.execute("UPDATE journal_lines SET credit_amount=26 WHERE id=5", [])
        .unwrap();
    let value = report(&tx, "2026-08-01", "2026-08-31").unwrap();
    assert_eq!(value["ok"], false);
    assert_eq!(value["vatReport"]["ok"], false);
    assert_eq!(value["ossConsumerSalesBase"], 0);
    assert_eq!(value["entryCount"], 0);
    assert_eq!(value["errors"], value["vatReport"]["errors"]);
    assert!(!value["errors"].as_array().unwrap().is_empty());
    assert_eq!(value["warnings"], json!([]));
    assert_eq!(value["submission"], false);
}
#[test]
fn oss_requires_vat_registration_and_bounded_canonical_dates() {
    let mut db = database();
    let tx = db.transaction().unwrap();
    assert!(report(&tx, "2026-09-02", "2026-09-01").is_err());
    assert!(report(&tx, "2010-01-01", "2026-01-01").is_err());
    assert!(report(&tx, "2026-02-30", "2026-03-01").is_err());
    assert!(report(&tx, "2026-8-01", "2026-08-31").is_err());
    assert!(
        report(&tx, "2026-08-02", "2026-08-02").unwrap()["ok"]
            .as_bool()
            .unwrap()
    );
    tx.execute("UPDATE companies SET vat_period_type=NULL", [])
        .unwrap();
    assert!(report(&tx, "2026-08-01", "2026-08-31").is_err());
}
#[test]
fn domestic_only_period_has_no_oss_classification_or_warning() {
    let mut db = database();
    let tx = db.transaction().unwrap();
    tx.execute("DELETE FROM journal_lines WHERE journal_entry_id<>2", [])
        .unwrap();
    let value = report(&tx, "2026-08-01", "2026-08-31").unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["ossConsumerSalesBase"], 0.0);
    assert_eq!(value["entryCount"], 0);
    assert_eq!(value["vatReport"]["outputVat"], 25.0);
    assert_eq!(value["warnings"], json!([]));
}
