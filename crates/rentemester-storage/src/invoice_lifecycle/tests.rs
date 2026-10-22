use super::*;
use rusqlite::TransactionBehavior;
#[test]
fn original_supplied_totals_lines_fx_terms_selectors_and_accounts_are_authoritative() {
    let mut db = crate::credit_note::tests::fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let customer:crate::sales::Customer=serde_json::from_value(json!({"name":"Synthetic card","address":"Synthetic card street","vatOrCvr":"DK12345678","paymentTermsDays":30,"defaultCurrency":"DKK"})).unwrap();
    let customer =
        crate::sales::create_customer(&tx, &customer, "synthetic").unwrap()["id"].clone();
    let mut body = json!({"confirm":true,"customerId":customer,"payload":{"invoiceType":"full","issueDate":"2026-10-18","lines":[{"description":"Synthetic service","quantity":2,"unitPriceExVat":50,"lineTotalExVat":100}],"totals":{"netAmount":100,"vatRate":0.25,"vatAmount":25,"grossAmount":125}}});
    validate(Kind::Issue, &body).unwrap();
    let changes = tx.total_changes();
    body["payload"]["totals"]["grossAmount"] = json!(126);
    assert!(prepare(&tx, Kind::Issue, &body, "2026-10-18T12:00:00Z").is_err());
    assert_eq!(changes, tx.total_changes());
    body["payload"]["totals"]["grossAmount"] = json!(125);
    body["payload"]["lines"][0]["lineTotalExVat"] = json!(99);
    assert!(prepare(&tx, Kind::Issue, &body, "2026-10-18T12:00:00Z").is_err());
    assert_eq!(changes, tx.total_changes());
    body["payload"]["lines"][0]["lineTotalExVat"] = json!(100);
    let (p, c, notes) = prepare(&tx, Kind::Issue, &body, "2026-10-18T12:00:00Z").unwrap();
    assert_eq!(p["buyer"]["name"], "Synthetic card");
    assert_eq!(p["dueDate"], "2026-11-17");
    assert_eq!(c["totals"]["grossAmount"], 125.0);
    assert_eq!(
        notes,
        vec!["Betalingsfrist 30 dage fra kundekortet — virksomhedens standard er 14 dage."]
    );
    body["payload"]["dueDate"] = json!("2026-10-25");
    let (p, _, notes) = prepare(&tx, Kind::Issue, &body, "2026-10-18T12:00:00Z").unwrap();
    assert_eq!(p["dueDate"], "2026-10-25");
    assert!(notes.is_empty());
    body["payload"]["currency"] = json!("EUR");
    body["payload"]["totals"]["fxRateToDkk"] = json!(7.5);
    body["payload"]["totals"]["netAmountDkk"] = json!(750);
    body["payload"]["totals"]["vatAmountDkk"] = json!(187.5);
    body["payload"]["totals"]["grossAmountDkk"] = json!(937.5);
    let (p, _, _) = prepare(&tx, Kind::Issue, &body, "2026-10-18T12:00:00Z").unwrap();
    assert_eq!(p["totals"]["grossAmountDkk"], 937.5);
    body["payload"]["totals"]["grossAmountDkk"] = json!(938);
    assert!(prepare(&tx, Kind::Issue, &body, "2026-10-18T12:00:00Z").is_err());
    let create = json!({"confirm":true,"payload":{"issueDate":"2026-10-18","buyer":{"name":"Synthetic essential buyer","address":"Synthetic street"},"lines":[{"description":"Synthetic essentials","quantity":2,"unitPriceExVat":40}]}});
    validate(Kind::Create, &create).unwrap();
    let (_, computed, _) = prepare(&tx, Kind::Create, &create, "2026-10-18T12:00:00Z").unwrap();
    assert_eq!(computed["totals"]["netAmount"], 80.0);
    assert_eq!(computed["totals"]["vatAmount"], 20.0);
    assert_eq!(computed["totals"]["grossAmount"], 100.0);
    assert_eq!(
        resolve_invoice(
            &tx,
            &json!({"documentId":9999}),
            "documentId",
            "invoiceNumber"
        )
        .unwrap_err()
        .code,
        "invoice_selector_not_found"
    );
    assert_eq!(
        bank_id(&tx, &json!({"bankTransactionId":9999}))
            .unwrap_err()
            .code,
        "bank_selector_not_found"
    );
    let number: String = tx
        .query_row("SELECT invoice_no FROM documents WHERE id=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        resolve_invoice(
            &tx,
            &json!({"documentId":1,"invoiceNumber":"wrong"}),
            "documentId",
            "invoiceNumber"
        )
        .unwrap(),
        1
    );
    assert_eq!(
        resolve_invoice(
            &tx,
            &json!({"invoiceNumber":format!(" {number} ")}),
            "documentId",
            "invoiceNumber"
        )
        .unwrap(),
        1
    );
    let csv = "date;text;amount;currency;reference\n2026-10-18;Synthetic receipt;125;DKK;receipt\n";
    crate::bank::import(
        &tx,
        csv,
        &rentemester_engine::bank::parse(csv).unwrap(),
        Some("synthetic"),
        "synthetic",
        "2026-10-18T12:00:00Z",
        "synthetic",
    )
    .unwrap();
    let bank = bank_id(&tx, &json!({"bankTransactionReference":"receipt"})).unwrap();
    assert_accounts(
        &tx,
        &json!({"bankAccountNo":"B","receivableAccountNo":"R"}),
        1,
        bank,
    )
    .unwrap();
    assert!(assert_accounts(&tx, &json!({"receivableAccountNo":"R2"}), 1, bank).is_err());
    assert!(assert_accounts(&tx, &json!({"bankAccountNo":"R"}), 1, bank).is_err());
}

#[test]
fn original_full_tax_rate_fallback_overrides_and_omitted_reverse_note_are_preserved() {
    let mut db = crate::credit_note::tests::fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let body = json!({"confirm":true,"payload":{"invoiceType":"full","issueDate":"2026-10-18","buyer":{"name":"Synthetic rate buyer","address":"Synthetic street","vatOrCvr":"DK12345678"},"lines":[{"description":"Synthetic 20 percent","quantity":1,"unitPriceExVat":100,"lineTotalExVat":100}],"totals":{"netAmount":100,"vatRate":0.2,"vatAmount":20,"grossAmount":120}}});
    let (payload, computed, _) = prepare(&tx, Kind::Issue, &body, "2026-10-18T12:00:00Z").unwrap();
    assert_eq!(payload["lines"][0]["vatRate"], 0.2);
    assert_eq!(computed["totals"]["vatAmount"], 20.0);
    assert_eq!(computed["totals"]["grossAmount"], 120.0);
    let before = tx.total_changes();
    let mut wrong = body.clone();
    wrong["payload"]["totals"]["vatAmount"] = json!(25);
    assert!(prepare(&tx, Kind::Issue, &wrong, "2026-10-18T12:00:00Z").is_err());
    assert_eq!(tx.total_changes(), before);
    let mut mixed = body.clone();
    mixed["payload"]["lines"][0]["vatRate"] = json!(0.25);
    mixed["payload"]["lines"].as_array_mut().unwrap().push(json!({"description":"Synthetic fallback","quantity":1,"unitPriceExVat":100,"lineTotalExVat":100}));
    mixed["payload"]["totals"] =
        json!({"netAmount":200,"vatRate":0.2,"vatAmount":45,"grossAmount":245});
    let (payload, _, _) = prepare(&tx, Kind::Issue, &mixed, "2026-10-18T12:00:00Z").unwrap();
    assert_eq!(payload["lines"][0]["vatRate"], 0.25);
    assert_eq!(payload["lines"][1]["vatRate"], 0.2);
    assert!(payload["totals"].get("vatRate").is_none());
    assert_eq!(payload["totals"]["vatAmount"], 45.0);
    let mut reverse = body;
    reverse["payload"]["vatTreatment"] = json!("domestic_reverse_charge");
    reverse["payload"]["reverseChargeBasis"] = json!("DK_MOMSLOVEN_§46_STK_1_NR_6");
    reverse["payload"]["totals"] = json!({"netAmount":100,"grossAmount":100});
    let (payload, _, _) = prepare(&tx, Kind::Issue, &reverse, "2026-10-18T12:00:00Z").unwrap();
    assert!(payload.get("reverseChargeNote").is_none());
    assert_eq!(payload["reverseChargeBasis"], "DK_MOMSLOVEN_§46_STK_1_NR_6");
    assert_eq!(payload["totals"]["grossAmount"], 100.0);
    let before = tx.total_changes();
    reverse["payload"]["totals"]["vatRate"] = json!(0.25);
    assert!(prepare(&tx, Kind::Issue, &reverse, "2026-10-18T12:00:00Z").is_err());
    assert_eq!(tx.total_changes(), before);
}
