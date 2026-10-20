use super::*;
use rusqlite::{Connection, params};
fn database() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE documents(id INTEGER PRIMARY KEY,document_type TEXT,invoice_no TEXT,invoice_date TEXT,currency TEXT,payload_json TEXT,sha256_hash TEXT);CREATE TABLE issued_invoice_postings(invoice_document_id INTEGER,journal_entry_id INTEGER,booked_gross_dkk NUMERIC);CREATE TABLE credit_note_postings(credit_note_document_id INTEGER,original_invoice_document_id INTEGER,journal_entry_id INTEGER,booked_gross_dkk NUMERIC);CREATE TABLE journal_entries(id INTEGER PRIMARY KEY,status TEXT,document_id INTEGER,transaction_date TEXT,currency TEXT,source_bank_transaction_id INTEGER,reversal_of_entry_id INTEGER);CREATE TABLE accounts(id INTEGER PRIMARY KEY,type TEXT);CREATE TABLE journal_lines(id INTEGER PRIMARY KEY,journal_entry_id INTEGER,account_id INTEGER,vat_code TEXT,debit_amount NUMERIC,credit_amount NUMERIC);INSERT INTO accounts VALUES(1,'income'),(2,'asset');").unwrap();
    db
}
fn save(tx: &Transaction<'_>, id: i64, kind: &str, no: &str, date: &str, payload: &Value) {
    let text = serde_json::to_string_pretty(payload).unwrap();
    let hash = crate::invoice_email::sha(text.as_bytes());
    tx.execute(
        "INSERT INTO documents VALUES(?1,?2,?3,?4,'EUR',?5,?6)",
        params![id, kind, no, date, text, hash],
    )
    .unwrap();
}
fn fixture(tx: &Transaction<'_>) {
    let observation = json!({"countryCode":"DE","vatNumber":"123456789","normalized":"DE123456789","valid":true,"name":"Synthetic EU Buyer","address":null,"rawResponse":"{}"});
    let validation = vies::record(&observation, "2026-08-01T12:00:00Z").unwrap();
    let invoice = json!({"invoiceType":"full","invoiceNumber":"2026-0001","issueDate":"2026-08-02","issuedAt":"2026-08-02T12:00:00Z","currency":"EUR","vatTreatment":"foreign_reverse_charge","reverseChargeBasis":"EU_B2B_SERVICES","viesValidation":validation,"fxValuationVersion":1,"buyer":{"name":"Synthetic EU Buyer","vatOrCvr":"DE123456789"},"totals":{"netAmount":100,"vatAmount":0,"grossAmount":100,"fxRateToDkk":7.5,"netAmountDkk":750,"vatAmountDkk":0,"grossAmountDkk":750}});
    save(tx, 1, "issued_invoice", "2026-0001", "2026-08-02", &invoice);
    let credit = json!({"type":"credit_note","creditNoteNumber":"CN-2026-0001","originalInvoiceNumber":"2026-0001","originalInvoiceDocumentId":1,"issueDate":"2026-09-02","currency":"EUR","netAmount":20,"vatAmount":0,"grossAmount":20,"netAmountDkk":150,"vatAmountDkk":0,"grossAmountDkk":150,"buyer":invoice["buyer"]});
    save(tx, 2, "credit_note", "CN-2026-0001", "2026-09-02", &credit);
    tx.execute_batch("INSERT INTO issued_invoice_postings VALUES(1,1,750);INSERT INTO credit_note_postings VALUES(2,1,2,150);INSERT INTO journal_entries VALUES(1,'posted',1,'2026-08-02','DKK',NULL,NULL),(2,'posted',2,'2026-09-02','DKK',NULL,NULL);INSERT INTO journal_lines VALUES(1,1,1,'REVERSE_CHARGE_EXEMPT',0,750),(2,1,2,NULL,750,0),(3,2,1,'REVERSE_CHARGE_EXEMPT',150,0),(4,2,2,NULL,0,150);").unwrap();
}
fn report(tx: &Transaction<'_>, from: &str, to: &str) -> Result<Value, Error> {
    read(tx, Date::parse(from).unwrap(), Date::parse(to).unwrap())
}
#[test]
fn frozen_dkk_bases_and_dated_signed_credits_survive_cache_expiry() {
    let mut db = database();
    let tx = db.transaction().unwrap();
    fixture(&tx);
    let august = report(&tx, "2026-08-01", "2026-08-31").unwrap();
    assert_eq!(august["totalValue"], 750);
    assert_eq!(august["invoiceCount"], 1);
    assert_eq!(august["creditNoteCount"], 0);
    let september = report(&tx, "2026-09-01", "2026-09-30").unwrap();
    assert_eq!(september["totalValue"], -150);
    assert_eq!(september["invoiceCount"], 0);
    assert_eq!(september["sources"][0]["netAmountDkk"], -150);
    let year = report(&tx, "2026-01-01", "2026-12-31").unwrap();
    assert_eq!(year["totalValue"], 600);
    assert_eq!(year["customers"][0]["totalValue"], 600);
    assert_eq!(year["customers"][0]["vatNumber"], "DE123456789");
    assert!(report(&tx, "2026-09-01", "2026-08-01").is_err());
}
#[test]
fn missing_foreign_valuation_never_becomes_zero() {
    let mut db = database();
    let tx = db.transaction().unwrap();
    fixture(&tx);
    let mut payload: Value = serde_json::from_str(
        &tx.query_row("SELECT payload_json FROM documents WHERE id=1", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
    )
    .unwrap();
    payload["totals"]
        .as_object_mut()
        .unwrap()
        .remove("netAmountDkk");
    tx.execute("DELETE FROM documents WHERE id=1", []).unwrap();
    save(
        &tx,
        1,
        "issued_invoice",
        "2026-0001",
        "2026-08-02",
        &payload,
    );
    assert!(report(&tx, "2026-01-01", "2026-12-31").is_err());
}
#[test]
fn unposted_invoices_are_excluded_but_reversed_or_inconsistent_links_refuse() {
    let mut db = database();
    let tx = db.transaction().unwrap();
    fixture(&tx);
    tx.execute("DELETE FROM issued_invoice_postings", [])
        .unwrap();
    assert_eq!(
        report(&tx, "2026-08-01", "2026-08-31").unwrap()["totalValue"],
        0
    );
    tx.execute("INSERT INTO issued_invoice_postings VALUES(1,1,750)", [])
        .unwrap();
    tx.execute(
        "UPDATE journal_entries SET status='reversed' WHERE id=1",
        [],
    )
    .unwrap();
    assert!(report(&tx, "2026-08-01", "2026-08-31").is_err());
    tx.execute("UPDATE journal_entries SET status='posted' WHERE id=1", [])
        .unwrap();
    tx.execute("UPDATE journal_lines SET credit_amount=749 WHERE id=1", [])
        .unwrap();
    assert!(report(&tx, "2026-08-01", "2026-08-31").is_err());
}
#[test]
fn frozen_identity_or_snapshot_tampering_refuses() {
    let mut db = database();
    let tx = db.transaction().unwrap();
    fixture(&tx);
    tx.execute("UPDATE documents SET payload_json=replace(payload_json,'DE123456789','DE000000000') WHERE id=1",[]).unwrap();
    assert!(report(&tx, "2026-01-01", "2026-12-31").is_err());
}
