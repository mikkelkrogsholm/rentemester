use super::*;
use rusqlite::{Connection, params};
fn database() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE documents(id INTEGER PRIMARY KEY,document_type TEXT,invoice_no TEXT,invoice_date TEXT,currency TEXT,payload_json TEXT,sha256_hash TEXT);CREATE TABLE issued_invoice_postings(invoice_document_id INTEGER,journal_entry_id INTEGER,booked_gross_dkk NUMERIC);CREATE TABLE credit_note_postings(credit_note_document_id INTEGER,original_invoice_document_id INTEGER,journal_entry_id INTEGER,booked_gross_dkk NUMERIC);CREATE TABLE journal_entries(id INTEGER PRIMARY KEY,status TEXT,document_id INTEGER,transaction_date TEXT,currency TEXT,source_bank_transaction_id INTEGER,reversal_of_entry_id INTEGER,amount_foreign NUMERIC,amount_dkk NUMERIC,fx_rate_to_dkk NUMERIC);CREATE TABLE accounts(id INTEGER PRIMARY KEY,type TEXT);CREATE TABLE journal_lines(id INTEGER PRIMARY KEY,journal_entry_id INTEGER,account_id INTEGER,vat_code TEXT,debit_amount NUMERIC,credit_amount NUMERIC);INSERT INTO accounts VALUES(1,'income'),(2,'asset');").unwrap();
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
    let invoice = json!({"invoiceType":"full","invoiceNumber":"2026-0001","issueDate":"2026-08-02","issuedAt":"2026-08-02T12:00:00Z","currency":"EUR","vatTreatment":"foreign_reverse_charge","reverseChargeBasis":"EU_B2B_SERVICES","reverseChargeNote":"Reverse charge - customer accounts for VAT","lines":[{"description":"Synthetic EU service","quantity":1,"unitPriceExVat":100,"lineTotalExVat":100,"taxClassification":"reverse_charge","reverseChargeBasis":"EU_B2B_SERVICES","vatRate":0,"vatBase":100,"vatAmount":0,"vatBaseDkk":750,"vatAmountDkk":0}],"viesValidation":validation,"fxValuationVersion":1,"buyer":{"name":"Synthetic EU Buyer","vatOrCvr":"DE123456789"},"totals":{"netAmount":100,"vatAmount":0,"grossAmount":100,"fxRateToDkk":7.5,"netAmountDkk":750,"vatAmountDkk":0,"grossAmountDkk":750}});
    save(tx, 1, "issued_invoice", "2026-0001", "2026-08-02", &invoice);
    let credit = json!({"type":"credit_note","creditNoteNumber":"CN-2026-0001","originalInvoiceNumber":"2026-0001","originalInvoiceDocumentId":1,"issueDate":"2026-09-02","currency":"EUR","netAmount":20,"vatAmount":0,"grossAmount":20,"netAmountDkk":150,"vatAmountDkk":0,"grossAmountDkk":150,"fxRateToDkk":7.5,"journalFxRateToDkk":7.5,"fxValuationVersion":1,"buyer":invoice["buyer"]});
    save(tx, 2, "credit_note", "CN-2026-0001", "2026-09-02", &credit);
    tx.execute_batch("INSERT INTO issued_invoice_postings VALUES(1,1,750);INSERT INTO credit_note_postings VALUES(2,1,2,150);INSERT INTO journal_entries VALUES(1,'posted',1,'2026-08-02','EUR',NULL,NULL,100,750,7.5),(2,'posted',2,'2026-09-02','EUR',NULL,NULL,20,150,7.5);INSERT INTO journal_lines VALUES(1,1,1,'REVERSE_CHARGE_EXEMPT',0,750),(2,1,2,NULL,750,0),(3,2,1,'REVERSE_CHARGE_EXEMPT',150,0),(4,2,2,NULL,0,150);").unwrap();
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
    assert_eq!(august["totalValue"], 750.0);
    assert_eq!(august["invoiceCount"], 1);
    assert_eq!(august["creditNoteCount"], 0);
    let september = report(&tx, "2026-09-01", "2026-09-30").unwrap();
    assert_eq!(september["totalValue"], -150.0);
    assert_eq!(september["invoiceCount"], 0);
    assert_eq!(september["sources"][0]["netAmountDkk"], -150.0);
    let year = report(&tx, "2026-01-01", "2026-12-31").unwrap();
    assert_eq!(year["totalValue"], 600.0);
    assert_eq!(year["customers"][0]["totalValue"], 600.0);
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
        0.0
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

#[test]
fn foreign_journal_metadata_must_match_frozen_invoice() {
    for assignment in [
        "currency='USD'",
        "currency='DKK'",
        "amount_foreign=99",
        "amount_dkk=749",
        "fx_rate_to_dkk=7.49",
        "amount_foreign=NULL",
    ] {
        let mut db = database();
        let tx = db.transaction().unwrap();
        fixture(&tx);
        tx.execute(
            &format!("UPDATE journal_entries SET {assignment} WHERE id=1"),
            [],
        )
        .unwrap();
        assert!(
            report(&tx, "2026-08-01", "2026-08-31").is_err(),
            "{assignment}"
        );
    }
}
#[test]
fn foreign_frozen_line_allocation_is_checked_against_rate() {
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
    payload["lines"][0]["vatBaseDkk"] = json!(749);
    tx.execute("DELETE FROM documents WHERE id=1", []).unwrap();
    save(
        &tx,
        1,
        "issued_invoice",
        "2026-0001",
        "2026-08-02",
        &payload,
    );
    assert!(report(&tx, "2026-08-01", "2026-08-31").is_err());
}

/// Synthetic stored originals exercise the reader/poster; this is not a
/// TypeScript-workspace adoption or an authorization bypass in product code.
#[test]
fn source_foreign_bases_omitted_vat_and_aggregate_fx_support_post_credit_and_report() {
    use crate::sales::{self, Artifact};
    for currency in ["DKK", "EUR"] {
        for basis in [
            "DK_MOMSLOVEN_§46_STK_1_NR_3",
            "EU_MOMSDIREKTIV_ART_196",
            "EU_MOMSDIREKTIV_ART_199",
        ] {
            let mut control = Connection::open_in_memory().unwrap();
            let mut db = Connection::open_in_memory().unwrap();
            crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Seller").unwrap();
            let tx = db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            tx.execute_batch("UPDATE companies SET address='Synthetic Seller Street',cvr='87654321',vat_period_type='quarter' WHERE id=1; INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('R','Synthetic receivable','asset','debit'),('S','Synthetic sale','income','credit'),('V','Synthetic output VAT','vat','credit'); INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');").unwrap();
            let mut observation = json!({"countryCode":"DE","vatNumber":"123456789","normalized":"DE123456789","valid":true,"name":"Synthetic EU Buyer","address":null,"rawResponse":"{}"});
            crate::vies::store(
                &tx,
                &vies::record(&observation, "2026-08-01T12:00:00Z").unwrap(),
                "user:synthetic",
            )
            .unwrap();
            let mut source = json!({"issueDate":"2026-08-02","currency":currency,"vatTreatment":"foreign_reverse_charge","reverseChargeBasis":basis,"reverseChargeNote":"Synthetic reverse charge","buyer":{"name":"Synthetic EU Buyer","address":"Synthetic Buyer Street","vatOrCvr":"DE123456789"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100}]});
            if currency == "EUR" {
                source["fxRateToDkk"] = json!(7.5);
            }
            let input = serde_json::from_value(source).unwrap();
            let mut original = sales::prepare(&tx, &input, "2026-08-02T12:00:00Z", true).unwrap();
            original["totals"]
                .as_object_mut()
                .unwrap()
                .remove("vatAmount");
            original["totals"]
                .as_object_mut()
                .unwrap()
                .remove("vatAmountDkk");
            original
                .as_object_mut()
                .unwrap()
                .remove("fxValuationVersion");
            for key in [
                "taxClassification",
                "reverseChargeBasis",
                "vatRate",
                "vatBase",
                "vatAmount",
                "vatBaseDkk",
                "vatAmountDkk",
            ] {
                original["lines"][0].as_object_mut().unwrap().remove(key);
            }
            let bytes = serde_json::to_string_pretty(&original).unwrap();
            let hash = crate::invoice_email::sha(bytes.as_bytes());
            let issued = sales::register(
                &tx,
                &original,
                Artifact {
                    filename: "synthetic.json",
                    path: "synthetic.json",
                    hash: &hash,
                    mime: "application/json",
                },
                Artifact {
                    filename: "synthetic.pdf",
                    path: "synthetic.pdf",
                    hash: &"b".repeat(64),
                    mime: "application/pdf",
                },
                "synthetic",
            )
            .unwrap();
            let id = issued["documentId"].as_i64().unwrap();
            sales::post(
                &tx,
                &serde_json::from_value(
                    json!({"invoiceDocumentId":id,"revenueAccountNo":"S","confirm":true}),
                )
                .unwrap(),
                "synthetic",
                Date::parse("2026-08-02").unwrap(),
            )
            .unwrap();
            let expected = if currency == "EUR" { 750.0 } else { 100.0 };
            assert_eq!(
                report(&tx, "2026-08-01", "2026-08-31").unwrap()["totalValue"],
                expected
            );
            assert_eq!(
                sales::list(&tx, "synthetic", "2026-08-02", None).unwrap()["invoices"][0]["grossAmountDkk"],
                expected
            );
            let credit = crate::credit_note::prepare(&tx,&serde_json::from_value(json!({"invoiceDocumentId":id,"issueDate":"2026-09-02","reason":"Synthetic correction","grossAmount":20,"confirm":true})).unwrap(),"2026-09-02T12:00:00Z").unwrap();
            let credit_bytes = serde_json::to_string_pretty(&credit.payload).unwrap();
            crate::credit_note::register_and_post(
                &tx,
                credit,
                Artifact {
                    filename: "synthetic-credit.json",
                    path: "synthetic-credit.json",
                    hash: &crate::invoice_email::sha(credit_bytes.as_bytes()),
                    mime: "application/json",
                },
                "synthetic",
                Date::parse("2026-09-02").unwrap(),
            )
            .unwrap();
            assert_eq!(
                report(&tx, "2026-01-01", "2026-12-31").unwrap()["totalValue"],
                expected * 0.8
            );
            assert_eq!(
                report(&tx, "2026-09-01", "2026-09-30").unwrap()["totalValue"],
                -expected * 0.2
            );
            observation["valid"] = json!(false);
            crate::vies::store(
                &tx,
                &vies::record(&observation, "2026-09-03T12:00:00Z").unwrap(),
                "user:synthetic",
            )
            .unwrap();
            assert_eq!(
                report(&tx, "2026-01-01", "2026-12-31").unwrap()["totalValue"],
                expected * 0.8
            );
            assert!(sales::prepare(&tx, &input, "2026-09-03T12:00:00Z", false).is_err());
            let saved: (String, String) = tx
                .query_row(
                    "SELECT payload_json,sha256_hash FROM documents WHERE id=?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(saved, (bytes, hash));
            tx.execute(
                "UPDATE documents SET sha256_hash=?1 WHERE id=?2",
                params!["0".repeat(64), id],
            )
            .unwrap();
            assert!(report(&tx, "2026-01-01", "2026-12-31").is_err());
        }
    }
}

#[test]
fn snapshot_proof_hashes_original_bytes_without_reserializing_source_order() {
    let mut db = database();
    let tx = db.transaction().unwrap();
    fixture(&tx);
    let original: String = tx
        .query_row("SELECT payload_json FROM documents WHERE id=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    let payload: Value = serde_json::from_str(&original).unwrap();
    let original_bytes = serde_json::to_string(&payload).unwrap();
    assert_ne!(original_bytes, original);
    tx.execute(
        "UPDATE documents SET payload_json=?1,sha256_hash=?2 WHERE id=1",
        params![
            original_bytes,
            crate::invoice_email::sha(original_bytes.as_bytes())
        ],
    )
    .unwrap();
    assert_eq!(
        report(&tx, "2026-08-01", "2026-08-31").unwrap()["totalValue"],
        750.0
    );
    tx.execute(
        "UPDATE documents SET payload_json=payload_json||' ' WHERE id=1",
        [],
    )
    .unwrap();
    assert!(report(&tx, "2026-08-01", "2026-08-31").is_err());
}
