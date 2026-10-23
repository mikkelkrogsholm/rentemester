use super::*;
use rusqlite::Connection;

fn database() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE companies(id INTEGER PRIMARY KEY,vat_period_type TEXT);
        INSERT INTO companies VALUES(1,'quarter');
        CREATE TABLE account_role_mappings(role TEXT,account_no TEXT,status TEXT);
        CREATE TABLE accounts(id INTEGER PRIMARY KEY,account_no TEXT,type TEXT,normal_balance TEXT);
        INSERT INTO accounts VALUES(1,'income','income','credit'),(2,'bank','asset','debit'),(3,'vat','vat','credit');
        CREATE TABLE journal_entries(id INTEGER PRIMARY KEY,transaction_date TEXT,status TEXT,currency TEXT,created_by_program TEXT,reversal_of_entry_id INTEGER);
        CREATE TABLE journal_lines(id INTEGER PRIMARY KEY,journal_entry_id INTEGER,account_id INTEGER,vat_code TEXT,debit_amount NUMERIC,credit_amount NUMERIC);
        CREATE TABLE documents(id INTEGER PRIMARY KEY,payload_json TEXT,invoice_date TEXT,currency TEXT);
        CREATE TABLE issued_invoice_postings(invoice_document_id INTEGER,journal_entry_id INTEGER,receivable_account_id INTEGER,booked_gross_dkk NUMERIC);
        CREATE TABLE accounting_periods(id INTEGER PRIMARY KEY,period_start TEXT,period_end TEXT,kind TEXT,status TEXT,reference TEXT);
        CREATE TABLE audit_log(id INTEGER PRIMARY KEY,entity_type TEXT,entity_id TEXT,event_type TEXT);
        INSERT INTO journal_entries VALUES(1,'2026-07-02','reversed','DKK','native',NULL),(2,'2026-08-02','posted','DKK','native',1);
        INSERT INTO journal_lines VALUES(1,1,1,'DK_SALE_25',0,100),(2,1,3,NULL,0,25),(3,1,2,NULL,125,0),
        (4,2,1,'DK_SALE_25',100,0),(5,2,3,NULL,25,0),(6,2,2,NULL,0,125);").unwrap();
    db
}
#[test]
fn range_reversal_counts_are_disjoint_and_do_not_rewrite_earlier_windows() {
    let mut db = database();
    let tx = db.transaction().unwrap();
    let july = report(&tx, "2026-07-01", "2026-07-31").unwrap();
    assert_eq!(july["ok"], true);
    assert_eq!(july["journalEntryCount"], 1);
    assert_eq!(july["reversedJournalEntryCount"], 0);
    assert_eq!(july["outputVat"], 25.0);
    assert_eq!(july["salesBase25"], 100.0);
    assert_eq!(july["filingDeadline"], Value::Null);
    let full = report(&tx, "2026-07-01", "2026-09-30").unwrap();
    assert_eq!(full["journalEntryCount"], 0);
    assert_eq!(full["reversedJournalEntryCount"], 1);
    assert_eq!(full["reversalJournalEntryCount"], 1);
    assert_eq!(full["totalJournalEntryCount"], 2);
    assert_eq!(full["reversedLinesConsidered"], 3);
    assert_eq!(full["reversalLinesConsidered"], 3);
    assert_eq!(full["totalLinesConsidered"], 6);
    assert_eq!(full["netVatPayable"], 0.0);
    assert_eq!(full["filingDeadline"], "2026-12-01");
    let august = report(&tx, "2026-08-01", "2026-08-31").unwrap();
    assert_eq!(august["reversalJournalEntryCount"], 1);
    assert_eq!(august["outputVat"], -25.0);
}
#[test]
fn registration_gate_precedes_dates_and_invalid_reports_are_complete_zeroes() {
    let mut db = database();
    let tx = db.transaction().unwrap();
    let bad = report(&tx, "bad", "also bad").unwrap();
    assert_eq!(
        bad["errors"],
        json!([
            "periodStart must be YYYY-MM-DD",
            "periodEnd must be YYYY-MM-DD"
        ])
    );
    assert_eq!(bad["salesBase25"], 0.0);
    assert_eq!(bad["rubrikker"]["momstilsvar"], 0.0);
    assert_eq!(bad["totalLinesConsidered"], 0);
    tx.execute("UPDATE companies SET vat_period_type=NULL", [])
        .unwrap();
    for reader in [report, eu_list, oss, filing] {
        assert_eq!(reader(&tx, "bad", "").unwrap(), nonregistered());
    }
}
#[test]
fn filing_uses_effective_lifecycle_preferred_kind_reference_and_exact_nested_rubrics() {
    let mut db = database();
    let tx = db.transaction().unwrap();
    assert_eq!(
        filing(&tx, "2026-07-01", "2026-09-30").unwrap()["ok"],
        false
    );
    tx.execute_batch("INSERT INTO accounting_periods VALUES(1,'2026-07-01','2026-09-30','vat_period','open','SYN-REF'),(2,'2026-07-01','2026-09-30','vat_quarter','open',NULL);
        INSERT INTO audit_log VALUES(1,'accounting_period','1','period_close');").unwrap();
    let ready = filing(&tx, "2026-07-01", "2026-09-30").unwrap();
    assert_eq!(ready["ok"], true);
    assert_eq!(ready["periodStatus"], "closed");
    assert_eq!(ready["periodReference"], "SYN-REF");
    assert_eq!(ready["rubrikker"], ready["vatReport"]["rubrikker"]);
    tx.execute(
        "UPDATE journal_entries SET created_by_program='rentemester-import-postings' WHERE id=1",
        [],
    )
    .unwrap();
    let damaged = filing(&tx, "2026-07-01", "2026-09-30").unwrap();
    assert_eq!(damaged["ok"], false);
    assert_eq!(damaged["periodStatus"], "closed");
    assert_eq!(damaged["errors"], damaged["vatReport"]["errors"]);
    assert!(
        damaged["errors"][0]
            .as_str()
            .unwrap()
            .contains("endnu ikke porterede")
    );
    assert_eq!(damaged["warnings"], json!([]));
}
#[test]
fn bad_debt_is_net_output_and_oss_embeds_the_same_full_report() {
    let mut db = database();
    let tx = db.transaction().unwrap();
    tx.execute_batch("INSERT INTO journal_entries VALUES(3,'2026-07-03','posted','DKK','native',NULL),(4,'2026-07-04','posted','DKK','native',NULL);
        INSERT INTO journal_lines VALUES(7,3,1,'DK_BAD_DEBT_25',20,0),(8,3,3,NULL,5,0),(9,3,2,NULL,0,25),
        (10,4,1,'OSS_EU_CONSUMER',0,70),(11,4,2,NULL,70,0);").unwrap();
    let vat = report(&tx, "2026-07-01", "2026-07-31").unwrap();
    assert_eq!(vat["ok"], true);
    assert_eq!(vat["outputVat"], 20.0);
    assert_eq!(vat["netVatPayable"], 20.0);
    assert_eq!(vat["badDebtReliefBase25"], 20.0);
    assert_eq!(vat["rubrikker"]["salgsmoms"], 20.0);
    let preview = oss(&tx, "2026-07-01", "2026-07-31").unwrap();
    assert_eq!(preview["vatReport"], vat);
    assert_eq!(preview["submission"], false);
    assert_eq!(preview["ossConsumerSalesBase"], 70.0);
    assert_eq!(preview["entryCount"], 1);
    tx.execute("UPDATE journal_lines SET credit_amount=26 WHERE id=2", [])
        .unwrap();
    let refused = oss(&tx, "2026-07-01", "2026-07-31").unwrap();
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["ossConsumerSalesBase"], 0);
    assert_eq!(refused["entryCount"], 0);
    assert_eq!(refused["warnings"], json!([]));
    assert_eq!(refused["errors"], refused["vatReport"]["errors"]);
}
