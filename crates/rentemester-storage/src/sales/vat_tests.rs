use super::*;
const NOW: &str = "2026-10-20T10:00:00Z";

#[test]
fn shifted_span_invoice_and_credit_numbers_use_stable_filename_safe_identifiers() {
    let mut db = fixture(true);
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute("UPDATE companies SET fiscal_year_start_month=7,fiscal_year_label_strategy='span' WHERE id=1", []).unwrap();
    let (payload, id) = issue(
        &tx,
        input(
            json!([{"description":"Synthetic shifted service","quantity":1,"unitPriceExVat":100}]),
        ),
    );
    assert_eq!(payload["invoiceNumber"], "2026-27-0001");
    post_invoice(&tx, id).unwrap();
    let request: crate::credit_note::Input = serde_json::from_value(json!({"invoiceDocumentId":id,"issueDate":"2026-10-20","reason":"Synthetic shifted correction","confirm":true})).unwrap();
    let credit = crate::credit_note::prepare(&tx, &request, NOW).unwrap();
    assert_eq!(credit.payload["creditNoteNumber"], "CN-2026-27-0001");
    let raw = serde_json::to_string_pretty(&credit.payload).unwrap();
    let hash = crate::invoice_email::sha(raw.as_bytes());
    crate::credit_note::register_and_post(
        &tx,
        credit,
        Artifact {
            filename: "synthetic-shifted-credit.json",
            path: "synthetic-shifted-credit.json",
            hash: &hash,
            mime: "application/json",
        },
        "synthetic",
        Date::parse("2026-10-20").unwrap(),
    )
    .unwrap();
    let original: String = tx
        .query_row(
            "SELECT payload_json FROM documents WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(original, serde_json::to_string_pretty(&payload).unwrap());
    let (next, _) = issue(
        &tx,
        input(json!([{"description":"Synthetic next service","quantity":1,"unitPriceExVat":1}])),
    );
    assert_eq!(next["invoiceNumber"], "2026-27-0002");
}

fn fixture(output: bool) -> rusqlite::Connection {
    let mut control = rusqlite::Connection::open_in_memory().unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic VAT Seller").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET address='Synthetic Street 75',cvr='DK87654321',vat_period_type='quarter' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R75','Synthetic debtor','asset','debit'),('S75','Synthetic revenue','income','credit'),('V75','Synthetic output VAT','vat','credit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('debtors','R75',1,'synthetic');").unwrap();
    if output {
        tx.execute("INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('output_vat','V75',1,'synthetic')",[]).unwrap();
    }
    tx.commit().unwrap();
    db
}
fn input(lines: Value) -> Value {
    json!({"issueDate":"2026-10-20","buyer":{"name":"Synthetic VAT Buyer","address":"Synthetic Street 2","vatOrCvr":"DK12345678"},"lines":lines})
}
fn issue(tx: &Transaction<'_>, input: Value) -> (Value, i64) {
    let request: Input = serde_json::from_value(input).unwrap();
    let payload = prepare(tx, &request, NOW, true).unwrap();
    let id = register_snapshot(tx, &payload);
    (payload, id)
}
fn register_snapshot(tx: &Transaction<'_>, p: &Value) -> i64 {
    let raw = serde_json::to_string_pretty(p).unwrap();
    let hash = crate::invoice_email::sha(raw.as_bytes());
    let pdf = crate::invoice_email::sha(format!("synthetic-pdf-{}", p["invoiceNumber"]).as_bytes());
    register(
        tx,
        p,
        Artifact {
            filename: "synthetic.json",
            path: &format!("invoices/issued/{hash}.json"),
            hash: &hash,
            mime: "application/json",
        },
        Artifact {
            filename: "synthetic.pdf",
            path: &format!("invoices/issued/{pdf}.pdf"),
            hash: &pdf,
            mime: "application/pdf",
        },
        "synthetic",
    )
    .unwrap()["documentId"]
        .as_i64()
        .unwrap()
}
fn post_invoice(tx: &Transaction<'_>, id: i64) -> Result<Value, Error> {
    post(
        tx,
        &Post {
            invoice_document_id: id,
            revenue_account_no: "S75".into(),
            confirm: true,
        },
        "synthetic",
        Date::parse("2026-10-20").unwrap(),
    )
}
fn posted(tx: &Transaction<'_>, entry: i64) -> Vec<(String, Option<String>, String, String)> {
    let mut q=tx.prepare("SELECT a.account_no,l.vat_code,l.debit_amount,l.credit_amount FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE journal_entry_id=? ORDER BY l.id").unwrap();
    q.query_map([entry], |r| {
        Ok((
            r.get(0)?,
            r.get(1)?,
            legacy_money(r.get_ref(2)?).unwrap().decimal(),
            legacy_money(r.get_ref(3)?).unwrap().decimal(),
        ))
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}
#[test]
fn mixed_snapshot_posts_source_line_rounding_and_categories_without_aggregate_recompute() {
    let mut db = fixture(true);
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let base = json!({"description":"Synthetic taxable rounding","quantity":1,"unitPriceExVat":0.02,"taxClassification":"taxable","vatRate":0.25});
    let (p, id) = issue(
        &tx,
        input(
            json!([base.clone(),base.clone(),base,{"description":"Synthetic exempt","quantity":1,"unitPriceExVat":10,"taxClassification":"exempt","vatRate":0},{"description":"Synthetic domestic reverse","quantity":1,"unitPriceExVat":20,"taxClassification":"reverse_charge","reverseChargeBasis":"DK_MOMSLOVEN_§46_STK_1_NR_6"}]),
        ),
    );
    assert_eq!(p["totals"]["netAmount"], json!(30.06));
    assert_eq!(p["totals"]["vatAmount"], json!(0.03));
    assert_eq!(p["totals"]["grossAmount"], json!(30.09));
    assert!(p["totals"].get("vatRate").is_none());
    let original = serde_json::to_string_pretty(&p).unwrap();
    let result = post_invoice(&tx, id).unwrap();
    let rows = posted(&tx, result["entryId"].as_i64().unwrap());
    assert_eq!(rows.len(), 7);
    assert_eq!(
        rows.iter()
            .filter(|r| r.1.as_deref() == Some("DK_SALE_25"))
            .count(),
        3
    );
    assert!(
        rows.iter()
            .any(|r| r.1.as_deref() == Some("DK_SALE_EXEMPT") && r.3 == "10.00")
    );
    assert!(
        rows.iter()
            .any(|r| r.1.as_deref() == Some("DOMESTIC_REVERSE_CHARGE_EXEMPT") && r.3 == "20.00")
    );
    assert!(rows.iter().any(|r| r.0 == "V75" && r.3 == "0.03"));
    assert_eq!(
        tx.query_row("SELECT payload_json FROM documents WHERE id=?", [id], |r| r
            .get::<_, String>(0))
            .unwrap(),
        original
    );
    assert!(post_invoice(&tx, id).is_err());
    let report = crate::vat_report::read(&tx, "synthetic", "2026-10-20", None).unwrap();
    assert_eq!(report["vatReportErrors"], json!([]));
    assert_eq!(report["outputVat"], json!(0.03));
}
#[test]
fn exempt_and_domestic_reverse_post_without_output_role_and_retain_explicit_wording() {
    let mut db = fixture(false);
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let (_, exempt) = issue(
        &tx,
        input(
            json!([{"description":"Synthetic exempt","quantity":1,"unitPriceExVat":40,"taxClassification":"exempt","vatRate":0}]),
        ),
    );
    let r = post_invoice(&tx, exempt).unwrap();
    let rows = posted(&tx, r["entryId"].as_i64().unwrap());
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].1.as_deref(), Some("DK_SALE_EXEMPT"));
    let mut reverse =
        input(json!([{"description":"Synthetic reverse","quantity":1,"unitPriceExVat":60}]));
    reverse["vatTreatment"] = json!("domestic_reverse_charge");
    reverse["reverseChargeBasis"] = json!("DK_MOMSLOVEN_§46_STK_1_NR_7");
    reverse["reverseChargeNote"] = json!("Synthetic explicitly reviewed reverse charge wording");
    let (p, id) = issue(&tx, reverse);
    assert_eq!(p["vatTreatment"], "domestic_reverse_charge");
    assert_eq!(p["reverseChargeBasis"], "DK_MOMSLOVEN_§46_STK_1_NR_7");
    assert_eq!(
        p["reverseChargeNote"],
        "Synthetic explicitly reviewed reverse charge wording"
    );
    assert!(p["totals"].get("vatAmount").is_none());
    assert!(p["totals"].get("vatRate").is_none());
    assert_eq!(
        tx.query_row("SELECT vat_amount FROM documents WHERE id=?", [id], |r| r
            .get::<_, f64>(
            0
        ))
        .unwrap(),
        0.0
    );
    let r = post_invoice(&tx, id).unwrap();
    let rows = posted(&tx, r["entryId"].as_i64().unwrap());
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].1.as_deref(), Some("DOMESTIC_REVERSE_CHARGE_EXEMPT"));
}
#[test]
fn legacy_uniform_aggregate_snapshot_keeps_original_totals_and_posting_shape() {
    let mut db = fixture(true);
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let p = json!({"invoiceType":"full","vatTreatment":"standard","issueDate":"2026-10-20","dueDate":"2026-11-03","invoiceNumber":"LEGACY-75","status":"issued","issuedAt":NOW,"currency":"DKK","seller":{"name":"Synthetic VAT Seller","address":"Synthetic Street 75","vatOrCvr":"DK87654321"},"buyer":{"name":"Synthetic VAT Buyer","address":"Synthetic Street 2","vatOrCvr":"DK12345678"},"lines":[{"description":"Legacy one","quantity":1,"unitPriceExVat":0.02,"lineTotalExVat":0.02},{"description":"Legacy two","quantity":1,"unitPriceExVat":0.02,"lineTotalExVat":0.02},{"description":"Legacy three","quantity":1,"unitPriceExVat":0.02,"lineTotalExVat":0.02}],"totals":{"netAmount":0.06,"vatRate":0.25,"vatAmount":0.02,"grossAmount":0.08}});
    assert!(sales_invoice::project(&p).unwrap().legacy);
    let id = register_snapshot(&tx, &p);
    let r = post_invoice(&tx, id).unwrap();
    let rows = posted(&tx, r["entryId"].as_i64().unwrap());
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1].3, "0.06");
    assert_eq!(rows[2].3, "0.02");
    assert_eq!(
        tx.query_row("SELECT payload_json FROM documents WHERE id=?", [id], |r| r
            .get::<_, String>(0))
            .unwrap(),
        serde_json::to_string_pretty(&p).unwrap()
    );
}
#[test]
fn invalid_classification_and_missing_required_mapping_leave_no_posting() {
    let mut db = fixture(false);
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let wrong:Input=serde_json::from_value(input(json!([{"description":"Synthetic unclassified zero","quantity":1,"unitPriceExVat":10,"vatRate":0}]))).unwrap();
    assert!(prepare(&tx, &wrong, NOW, true).is_err());
    assert_eq!(
        crate::sequences::current(
            &tx,
            "issued_invoice",
            &crate::sequences::company_scope(Some("DK87654321"), 1, "2026"),
            0
        )
        .unwrap(),
        0
    );
    let (_, id) = issue(
        &tx,
        input(json!([{"description":"Synthetic standard","quantity":1,"unitPriceExVat":100}])),
    );
    assert!(post_invoice(&tx, id).is_err());
    assert_eq!(
        tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        tx.query_row("SELECT count(*) FROM issued_invoice_postings", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
