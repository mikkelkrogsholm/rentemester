use super::*;
use crate::sales::Artifact;
use rentemester_engine::fiscal_year::Date;
use rusqlite::{Connection, TransactionBehavior, params};
fn database() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Advice").unwrap();
    db.execute_batch("UPDATE companies SET address='Synthetic Seller Street',cvr='87654321',vat_period_type='quarter' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R','Synthetic debtor','asset','debit'),('S','Synthetic income','income','credit'),('V','Synthetic output VAT','vat','credit'),('B','Synthetic bank','asset','debit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');INSERT INTO bank_accounts(slug,name,ledger_account_no)VALUES('synthetic','Synthetic bank','B');").unwrap();
    db
}
fn invoice(tx: &Transaction<'_>, net: i32, legacy: bool) -> i64 {
    let input=serde_json::from_value(json!({"issueDate":"2026-10-18","buyer":{"name":"Synthetic Blue Buyer","address":"Synthetic street 2","vatOrCvr":"DK12345678"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":net}]})).unwrap();
    let mut payload = crate::sales::prepare(tx, &input, "2026-10-18T12:00:00Z", true).unwrap();
    if legacy {
        for key in ["taxClassification", "vatRate", "vatBase", "vatAmount"] {
            payload["lines"][0].as_object_mut().unwrap().remove(key);
        }
    }
    let bytes = serde_json::to_string_pretty(&payload).unwrap();
    let hash = crate::invoice_email::sha(bytes.as_bytes());
    let pdf_hash = crate::invoice_email::sha(&rentemester_engine::invoice_pdf::build(&payload));
    let value = crate::sales::register(
        tx,
        &payload,
        Artifact {
            filename: "synthetic.json",
            path: "synthetic.json",
            hash: &hash,
            mime: "application/json",
        },
        Artifact {
            filename: "synthetic.pdf",
            path: "synthetic.pdf",
            hash: &pdf_hash,
            mime: "application/pdf",
        },
        "synthetic",
    )
    .unwrap();
    let id = value["documentId"].as_i64().unwrap();
    crate::sales::post(
        tx,
        &crate::sales::Post {
            invoice_document_id: id,
            revenue_account_no: "S".into(),
            confirm: true,
        },
        "synthetic",
        Date::parse("2026-10-18").unwrap(),
    )
    .unwrap();
    id
}
fn purchase(tx: &Transaction<'_>, amount: i32, n: i32) -> i64 {
    let metadata=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","currency":"DKK","issueDate":"2026-10-18","invoiceNo":format!("PUR-{n}"),"deliveryDescription":"Synthetic purchase","amountIncVat":amount,"vatAmount":amount/5,"sender":{"name":"Synthetic Green Supplier","address":"Synthetic 1","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic Buyer","address":"Synthetic 2","vatOrCvr":"DK87654321"}})).unwrap();
    let value = crate::documents::register(
        tx,
        crate::documents::Original {
            filename: "synthetic.txt",
            hash: &format!("{n:064x}"),
            path: "synthetic.txt",
            mime: "text/plain",
        },
        &metadata,
        "synthetic",
        "2026-10-18",
    )
    .unwrap();
    value["document"]["id"].as_i64().unwrap()
}
fn bank(tx: &Transaction<'_>, csv: &str) {
    crate::bank::import(
        tx,
        csv,
        &rentemester_engine::bank::parse_with_profile(
            csv,
            if csv.starts_with("Dato;") {
                Some("danske-bank")
            } else {
                None
            },
        )
        .unwrap(),
        Some("synthetic"),
        "synthetic",
        "2026-10-18T12:00:00Z",
        "synthetic.csv",
    )
    .unwrap();
}
fn credit(tx: &Transaction<'_>, id: i64, amount: i32) -> i64 {
    let input=serde_json::from_value(json!({"invoiceDocumentId":id,"issueDate":"2026-10-18","reason":"Synthetic correction","grossAmount":amount,"confirm":true})).unwrap();
    let prepared = crate::credit_note::prepare(tx, &input, "2026-10-18T12:00:00Z").unwrap();
    let bytes = serde_json::to_string_pretty(&prepared.payload).unwrap();
    let hash = crate::invoice_email::sha(bytes.as_bytes());
    crate::credit_note::register_and_post(
        tx,
        prepared,
        Artifact {
            filename: "credit.json",
            path: "credit.json",
            hash: &hash,
            mime: "application/json",
        },
        "synthetic",
        Date::parse("2026-10-18").unwrap(),
    )
    .unwrap()["documentId"]
        .as_i64()
        .unwrap()
}
fn read(tx: &Transaction<'_>) -> Value {
    suggest(tx, &Input::default()).unwrap()
}
fn by_bank(value: &Value, id: i64) -> &Value {
    value["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["bankTransactionId"] == id)
        .unwrap()
}
#[test]
fn all_four_advisory_families_preserve_scores_extra_columns_bounds_and_no_effects() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let a = invoice(&tx, 160, false);
    let b = invoice(&tx, 200, false);
    bank(
        &tx,
        "date;text;amount;reference\n2026-10-18;Synthetic paid;250;paid\n",
    );
    crate::invoice_settlement::settle(
        &tx,
        &serde_json::from_value(
            json!({"invoiceDocumentId":b,"bankTransactionId":1,"confirm":true}),
        )
        .unwrap(),
        "synthetic",
        Date::parse("2026-10-18").unwrap(),
    )
    .unwrap();
    let c = credit(&tx, b, 100);
    let p = purchase(&tx, 125, 1);
    purchase(&tx, 50, 2);
    purchase(&tx, 50, 3);
    let no = |id| {
        tx.query_row::<String, _, _>("SELECT invoice_no FROM documents WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap()
    };
    let csv = format!(
        "Dato;Tekst;Beløb;Afsender;Besked\n2026-10-18;Synthetic incoming;200;Synthetic Blue Buyer;{}\n2026-10-18;Synthetic outgoing;-125;Synthetic Green Supplier;PUR-1\n2026-10-18;Synthetic refund;-100;Synthetic Blue Buyer;{} {}\n2026-10-18;Synthetic supplier;125;Synthetic Green Supplier;PUR-1 KREDIT\n2026-10-18;Unknown payment;-50;;\n",
        no(a),
        no(c),
        no(b)
    );
    bank(&tx, &csv);
    let before = tx.total_changes();
    let value = read(&tx);
    assert_eq!(tx.total_changes(), before);
    assert_eq!(value["count"], 5);
    assert_eq!(value["rows"][0]["bankTransactionId"], 6);
    for (id, kind, document) in [
        (2, "issued_invoice", a),
        (3, "purchase_sale", p),
        (4, "credit_note_refund", c),
        (5, "supplier_credit_refund", p),
    ] {
        let found = by_bank(&value, id)["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["kind"] == kind && v["documentId"] == document)
            .unwrap();
        assert!(!found["reasons"].as_array().unwrap().is_empty());
    }
    let refund = &by_bank(&value, 4)["suggestions"][0];
    assert_eq!(refund["confidence"], 1.2);
    assert_eq!(
        refund["reasons"][0],
        "amount match: 100 vs credit-note gross amount 100"
    );
    assert!(
        by_bank(&value, 6)["suggestions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        by_bank(&value, 6)["unsafeMatchReason"]
            .as_str()
            .unwrap()
            .starts_with("Flere bilag")
    );
    assert_eq!(
        suggest(
            &tx,
            &Input {
                max: Some(1),
                bank_transaction_id: None
            }
        )
        .unwrap()["count"],
        5
    );
    assert_eq!(
        suggest(
            &tx,
            &Input {
                max: Some(0),
                bank_transaction_id: None
            }
        )
        .unwrap()["ok"],
        false
    );
    assert_eq!(
        suggest(
            &tx,
            &Input {
                max: None,
                bank_transaction_id: Some(1)
            }
        )
        .unwrap()["errors"][0],
        "unmatched bank transaction 1 does not exist"
    );
    assert_eq!(
        suggest(
            &tx,
            &Input {
                max: None,
                bank_transaction_id: Some(999)
            }
        )
        .unwrap()["ok"],
        false
    );
}
#[test]
fn amount_only_currency_direction_stopwords_and_stable_document_order_are_conservative() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let p = purchase(&tx, 125, 1);
    purchase(&tx, 125, 2);
    bank(
        &tx,
        "date;text;amount;currency;amount_dkk;fx_rate_to_dkk\n2026-10-18;Unknown;-125;DKK;;\n2026-10-18;Unknown;125;DKK;;\n2026-10-18;Unknown;-125;EUR;-937.5;7.5\n2026-10-18;Synthetic Green Supplier;-125;DKK;;\n",
    );
    let value = read(&tx);
    assert!(
        by_bank(&value, 1)["unsafeMatchReason"]
            .as_str()
            .unwrap()
            .starts_with("Flere bilag")
    );
    assert!(by_bank(&value, 2)["unsafeMatchReason"].is_null());
    assert!(by_bank(&value, 3)["unsafeMatchReason"].is_null());
    let matched = by_bank(&value, 4)["suggestions"].as_array().unwrap();
    assert_eq!(matched.len(), 2);
    assert_eq!(matched[0]["documentId"], p);
    assert!(matched[0]["confidence"].as_f64().unwrap() >= 0.5);
    assert_eq!(
        tokens("Blue APS DKK Blue ÅÆØ GmbH AB AND Straße-42"),
        vec!["BLUE", "ÅÆØ", "STRASSE-42"]
    );
    assert!(read(&tx)["rows"].as_array().unwrap().iter().all(|v| {
        v["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["kind"] != "supplier_credit_refund")
    }));
}
#[test]
fn readonly_credit_verification_accepts_partial_and_full_native_and_legacy_credits() {
    for legacy in [false, true] {
        let mut db = database();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let id = invoice(&tx, 200, legacy);
        bank(
            &tx,
            "date;text;amount;reference\n2026-10-18;Synthetic receipt;250;paid\n",
        );
        crate::invoice_settlement::settle(
            &tx,
            &serde_json::from_value(
                json!({"invoiceDocumentId":id,"bankTransactionId":1,"confirm":true}),
            )
            .unwrap(),
            "synthetic",
            Date::parse("2026-10-18").unwrap(),
        )
        .unwrap();
        crate::credit_note::verify_existing(&tx, id).unwrap();
        credit(&tx, id, 100);
        crate::credit_note::verify_existing(&tx, id).unwrap();
        credit(&tx, id, 150);
        let before = tx.total_changes();
        crate::credit_note::verify_existing(&tx, id).unwrap();
        assert_eq!(tx.total_changes(), before);
        bank(
            &tx,
            "date;text;amount;reference\n2026-10-18;Synthetic Blue Buyer;-250;CN-2026-0001 2026-0001\n",
        );
        let value = read(&tx);
        assert!(
            by_bank(&value, 2)["suggestions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value["kind"] == "credit_note_refund"),
            "{value}"
        );
    }
}
#[test]
fn corrupt_prior_allocation_and_orphan_credit_never_produce_customer_refund_advice() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let id = invoice(&tx, 200, false);
    let c = credit(&tx, id, 250);
    let mut payload: Value = serde_json::from_str(
        &tx.query_row::<String, _, _>("SELECT payload_json FROM documents WHERE id=?1", [c], |r| {
            r.get(0)
        })
        .unwrap(),
    )
    .unwrap();
    payload["creditAllocations"][0]["originalAmount"] = json!("999.00");
    let changed = serde_json::to_string_pretty(&payload).unwrap();
    // Manufacture one corrupt stored fixture, then restore the exact guard
    // before exercising product admission and immutable-evidence checks.
    let guard:String=tx.query_row("SELECT sql FROM sqlite_schema WHERE type='trigger' AND name='documents_no_update_issued_invoice'",[],|r|r.get(0)).unwrap();
    tx.execute_batch("DROP TRIGGER documents_no_update_issued_invoice")
        .unwrap();
    tx.execute(
        "UPDATE documents SET payload_json=?1,sha256_hash=?2 WHERE id=?3",
        params![changed, crate::invoice_email::sha(changed.as_bytes()), c],
    )
    .unwrap();
    tx.execute_batch(&guard).unwrap();
    assert!(crate::credit_note::verify_existing(&tx, id).is_err());
    bank(
        &tx,
        "date;text;amount;reference\n2026-10-18;Synthetic Blue Buyer;-250;CN-2026-0001\n",
    );
    assert!(
        by_bank(&read(&tx), 1)["suggestions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    tx.execute("INSERT INTO documents(source,document_type,invoice_no,payment_details,sha256_hash,currency)VALUES('synthetic','credit_note','ORPHAN','2026-0001','synthetic','DKK')",[]).unwrap();
    assert!(crate::credit_note::verify_existing(&tx, id).is_err());
}
#[test]
fn reversed_original_posting_cannot_receive_advisory_invoice_matches() {
    let mut db = database();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let id = invoice(&tx, 100, false);
    bank(
        &tx,
        "date;text;amount;reference\n2026-10-18;Synthetic Blue Buyer;125;2026-0001\n",
    );
    assert!(
        !by_bank(&read(&tx), 1)["suggestions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // Recreate the canonical guard after introducing a synthetic corrupt/
    // historical status; no production reversal bypass is introduced.
    let guard:String=tx.query_row("SELECT sql FROM sqlite_schema WHERE type='trigger' AND name='journal_entries_no_update'",[],|r|r.get(0)).unwrap();
    tx.execute_batch("DROP TRIGGER journal_entries_no_update")
        .unwrap();
    tx.execute("UPDATE journal_entries SET status='reversed' WHERE id=(SELECT journal_entry_id FROM issued_invoice_postings WHERE invoice_document_id=?1)",[id]).unwrap();
    tx.execute_batch(&guard).unwrap();
    let before = tx.total_changes();
    let value = read(&tx);
    assert_eq!(tx.total_changes(), before);
    assert!(
        by_bank(&value, 1)["suggestions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
