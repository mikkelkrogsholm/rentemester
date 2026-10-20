use super::*;
fn input() -> Input {
    serde_json::from_value(json!({"issueDate":"2025-08-01","currency":"EUR","fxRateToDkk":7.46,"vatTreatment":"foreign_reverse_charge","reverseChargeBasis":"EU_B2B_SERVICES","reverseChargeNote":"Reverse charge - customer accounts for VAT","buyer":{"name":"Synthetic EU buyer","address":"Synthetic street 2","vatOrCvr":"DE123456789"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100}]})).unwrap()
}
fn validation(valid: bool, at: &str) -> Value {
    rentemester_engine::vies::record(&json!({"countryCode":"DE","vatNumber":"123456789","normalized":"DE123456789","valid":valid,"name":"Synthetic EU buyer","address":null,"rawResponse":"{\"synthetic\":true}"}),at).unwrap()
}
#[test]
fn foreign_service_freezes_valid_vies_and_posts_only_dkk_without_rewriting_original() {
    let mut control = rusqlite::Connection::open_in_memory().unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic EU seller").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET address='Synthetic street 1',cvr='87654321',vat_period_type='quarter' WHERE id=1; INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('R','Synthetic receivable','asset','debit'),('S','Synthetic sale','income','credit'); INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('debtors','R',1,'synthetic');").unwrap();
    let request = input();
    let now = "2025-08-01T12:00:00Z";
    assert!(prepare(&tx, &request, now, true).is_err());
    crate::vies::store(&tx, &validation(false, now), "synthetic").unwrap();
    assert!(prepare(&tx, &request, now, true).is_err());
    let record = validation(true, now);
    crate::vies::store(&tx, &record, "synthetic").unwrap();
    let payload = prepare(&tx, &request, now, true).unwrap();
    assert_eq!(payload["invoiceNumber"], "2025-0001");
    assert_eq!(payload["currency"], "EUR");
    assert_eq!(payload["viesValidation"], record);
    assert_eq!(payload["totals"]["grossAmount"], 100.0);
    assert_eq!(payload["totals"]["grossAmountDkk"], 746.0);
    let registered = register(
        &tx,
        &payload,
        Artifact {
            filename: "eu.json",
            path: "eu.json",
            hash: &"a".repeat(64),
            mime: "application/json",
        },
        Artifact {
            filename: "eu.pdf",
            path: "eu.pdf",
            hash: &"b".repeat(64),
            mime: "application/pdf",
        },
        "synthetic",
    )
    .unwrap();
    // The issue confirmation is consumed directly by the existing web dialog.
    assert_eq!(registered["grossAmount"], 100.0);
    assert_eq!(registered["fxRateToDkk"], 7.46);
    assert_eq!(registered["netAmountDkk"], 746.0);
    assert_eq!(registered["vatAmountDkk"], 0.0);
    assert_eq!(registered["grossAmountDkk"], 746.0);
    let id = registered["documentId"].as_i64().unwrap();
    let before: String = tx
        .query_row(
            "SELECT payload_json FROM documents WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    let request = Post {
        invoice_document_id: id,
        revenue_account_no: "S".into(),
        confirm: true,
    };
    assert_eq!(
        post(
            &tx,
            &request,
            "synthetic",
            Date::parse("2025-08-01").unwrap()
        )
        .unwrap()["ok"],
        true
    );
    assert!(
        post(
            &tx,
            &request,
            "synthetic",
            Date::parse("2025-08-01").unwrap()
        )
        .is_err()
    );
    let posting: Money = tx
        .query_row(
            "SELECT booked_gross_dkk FROM issued_invoice_postings WHERE invoice_document_id=?1",
            [id],
            |r| legacy_money(r.get_ref(0)?).map_err(|_| rusqlite::Error::InvalidQuery),
        )
        .unwrap();
    assert_eq!(posting.decimal(), "746.00");
    let report = crate::cockpit::read(&tx, "synthetic", "2025-08-01", Some("2025"), None).unwrap();
    assert_eq!(report.trial_balance["balanced"], true);
    assert_eq!(report.income_statement["totalIncome"], 746.0);
    let listed = list(&tx, "synthetic", "2025-08-01", Some("2025")).unwrap();
    assert_eq!(listed["invoices"][0]["grossAmount"], 100.0);
    assert_eq!(listed["invoices"][0]["openBalanceDkk"], 746.0);
    assert_eq!(listed["totalGrossDkk"], 746.0);
    assert_eq!(
        tx.query_row::<String, _, _>("SELECT currency FROM documents WHERE id=?1", [id], |r| r
            .get(0))
            .unwrap(),
        "EUR"
    );
    assert_eq!(
        tx.query_row::<String, _, _>(
            "SELECT payload_json FROM documents WHERE id=?1",
            [id],
            |r| r.get(0)
        )
        .unwrap(),
        before
    );
    // Later invalid observations never rewrite the original issuance evidence.
    crate::vies::store(&tx, &validation(false, "2025-08-02T12:00:00Z"), "synthetic").unwrap();
    assert_eq!(
        tx.query_row::<String, _, _>(
            "SELECT payload_json FROM documents WHERE id=?1",
            [id],
            |r| r.get(0)
        )
        .unwrap(),
        before
    );
}
#[test]
fn future_expired_domestic_and_mismatched_vat_never_allocate_invoice_number() {
    let mut control = rusqlite::Connection::open_in_memory().unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic EU seller").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET address='Synthetic street 1',cvr='87654321',vat_period_type='quarter' WHERE id=1").unwrap();
    let request = input();
    let now = "2025-08-01T12:00:00Z";
    for at in ["2025-08-02T12:00:00Z", "2025-01-01T12:00:00Z"] {
        crate::vies::store(&tx, &validation(true, at), "synthetic").unwrap();
        assert!(prepare(&tx, &request, now, true).is_err());
    }
    crate::vies::store(&tx, &validation(true, now), "synthetic").unwrap();
    for vat in ["DK12345678", "GB123456789", "DE000000000", "DE12/345"] {
        let mut bad = input();
        bad.buyer.as_mut().unwrap().vat_or_cvr = Some(vat.into());
        assert!(prepare(&tx, &bad, now, true).is_err());
    }
    assert_eq!(
        prepare(&tx, &request, now, false).unwrap()["invoiceNumber"],
        "2025-0001"
    );
}
